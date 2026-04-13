//! Redis publisher for Polymarket trade events
//!
//! This module consumes parsed PolymarketTradeEvent instances and publishes them
//! to Redis using multiple publishers (LIST, STREAM, PUBSUB).

use anyhow::Result;
use common::publisher::{RedisListPublisher, RedisPubsubPublisher, RedisPublisher, RedisStreamPublisher};
use popeyes_trading_types::{Event, MarketDataEvent, PolymarketTradeEvent, TradeSide};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::metrics::Metrics;

const BUFFER_MAX_SIZE: usize = 1000;
const FLUSH_RATE_LIMIT: usize = 100; // events per second
const FLUSH_INTERVAL_MS: u64 = 1000; // 1 second

/// Redis publisher that consumes PolymarketTradeEvent and publishes to Redis
pub struct RedisTradePublisher {
    /// Redis publishers for different targets
    publishers: Vec<Box<dyn RedisPublisher>>,
    /// Receiver for parsed trade events
    trade_rx: broadcast::Receiver<PolymarketTradeEvent>,
    /// Metrics for tracking publishing
    metrics: Arc<Metrics>,
    /// Buffer for events during Redis unavailability
    buffer: Arc<Mutex<VecDeque<PolymarketTradeEvent>>>,
}

impl RedisTradePublisher {
    /// Create a new Redis trade publisher
    pub async fn new(
        config: &Config,
        trade_rx: broadcast::Receiver<PolymarketTradeEvent>,
        metrics: Arc<Metrics>,
    ) -> Result<Self> {
        let mut publishers: Vec<Box<dyn RedisPublisher>> = Vec::new();

        // Add list publisher if trade_queues are configured
        if !config.redis.trade_queues.is_empty() {
            let list_publisher =
                RedisListPublisher::new(&config.redis.url, config.redis.trade_queues.clone()).await?;
            publishers.push(Box::new(list_publisher));
            info!("Initialized RedisListPublisher for Polymarket trades");
        }

        // Add stream publisher if trade_streams are configured
        if let Some(stream_configs) = &config.redis.trade_streams {
            if !stream_configs.is_empty() {
                let stream_publisher = RedisStreamPublisher::new(&config.redis.url, stream_configs.clone()).await?;
                publishers.push(Box::new(stream_publisher));
                info!("Initialized RedisStreamPublisher for Polymarket trades");
            }
        }

        // Add pubsub publisher if trade_pubsub is configured
        if let Some(pubsub_config) = &config.redis.trade_pubsub {
            if !pubsub_config.channels.is_empty() {
                let pubsub_publisher =
                    RedisPubsubPublisher::new(&config.redis.url, pubsub_config.clone()).await?;
                publishers.push(Box::new(pubsub_publisher));
                info!("Initialized RedisPubsubPublisher for Polymarket trades");
            }
        }

        if publishers.is_empty() {
            return Err(anyhow::anyhow!(
                "No Redis publishers configured. Please configure at least one queue, stream, or pubsub channel."
            ));
        }

        Ok(Self {
            publishers,
            trade_rx,
            metrics,
            buffer: Arc::new(Mutex::new(VecDeque::new())),
        })
    }

    /// Run the publisher (receive events, publish to Redis)
    ///
    /// # Arguments
    /// * `cancellation_token` - Token to signal shutdown
    pub async fn run(mut self, cancellation_token: CancellationToken) -> Result<()> {
        info!("Redis trade publisher starting");

        // Start periodic buffer flush ticker
        let mut flush_ticker = interval(Duration::from_millis(FLUSH_INTERVAL_MS));

        loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    info!("Redis trade publisher shutting down, flushing buffer");
                    self.flush_buffer().await;
                    break;
                }
                // Receive new events
                recv_result = self.trade_rx.recv() => {
                    match recv_result {
                        Ok(trade_event) => {
                            // Calculate latency
                            let now_ms = chrono::Utc::now().timestamp_millis();
                            let latency_sec = (now_ms - trade_event.timestamp) as f64 / 1000.0;
                            self.metrics
                                .trade_latency
                                .with_label_values(&[&trade_event.market])
                                .observe(latency_sec);

                            // Try to flush buffer first if it has events
                            let buffer_len = {
                                let buf = self.buffer.lock().await;
                                buf.len()
                            };
                            if buffer_len > 0 {
                                debug!(buffer_size = buffer_len, "Attempting to flush buffer before publishing new event");
                                self.flush_buffer().await;
                            }

                            // Try to publish event
                            if let Err(e) = self.publish_event(&trade_event).await {
                                warn!(
                                    error = %e,
                                    asset_id = %trade_event.asset_id,
                                    "Failed to publish event, buffering"
                                );

                                // Add to buffer
                                let mut buffer = self.buffer.lock().await;
                                if buffer.len() >= BUFFER_MAX_SIZE {
                                    // Drop oldest event
                                    buffer.pop_front();
                                    warn!(
                                        buffer_size = BUFFER_MAX_SIZE,
                                        "Buffer overflow, dropped oldest event"
                                    );
                                }
                                buffer.push_back(trade_event);
                            } else {
                                // Increment success metrics
                                let side_str = match trade_event.side {
                                    TradeSide::Buy => "BUY",
                                    TradeSide::Sell => "SELL",
                                };
                                self.metrics
                                    .trades_published
                                    .with_label_values(&[&trade_event.market, side_str])
                                    .inc();
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(
                                skipped = skipped,
                                "Publisher lagged behind parser, some events skipped"
                            );
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            info!("Trade channel closed, publisher shutting down");
                            break;
                        }
                    }
                }
                // Periodic buffer flush
                _ = flush_ticker.tick() => {
                    let buffer_len = {
                        let buf = self.buffer.lock().await;
                        buf.len()
                    };
                    if buffer_len > 0 {
                        debug!(buffer_size = buffer_len, "Periodic buffer flush");
                        self.flush_buffer().await;
                    }
                }
            }
        }

        // Flush buffer on shutdown
        info!("Flushing buffered events before shutdown");
        self.flush_buffer().await;

        info!("Redis trade publisher shut down successfully");
        Ok(())
    }

    /// Publish a single event to all configured Redis targets
    async fn publish_event(&self, event: &PolymarketTradeEvent) -> Result<()> {
        let wrapped_event = Event::MarketData(MarketDataEvent::PolymarketTrade(event.clone()));

        let mut last_error = None;
        let mut success_count = 0;

        for publisher in &self.publishers {
            match publisher.publish(wrapped_event.clone()).await {
                Ok(_) => {
                    debug!(publisher = publisher.name(), "Published event to Redis");
                    success_count += 1;
                }
                Err(e) => {
                    let queue_name = publisher.name();
                    warn!(
                        error = %e,
                        publisher = queue_name,
                        "Failed to publish event"
                    );
                    self.metrics
                        .redis_publish_failures
                        .with_label_values(&[queue_name])
                        .inc();
                    last_error = Some(e);
                }
            }
        }

        // Return error only if all publishers failed
        if success_count == 0 {
            if let Some(e) = last_error {
                return Err(e);
            }
        }

        Ok(())
    }

    /// Flush buffered events to Redis
    async fn flush_buffer(&self) {
        let mut buffer = self.buffer.lock().await;
        let mut flushed = 0;

        while let Some(event) = buffer.pop_front() {
            if let Err(e) = self.publish_event(&event).await {
                warn!(
                    error = %e,
                    "Failed to flush buffered event, re-buffering"
                );
                // Put it back at the front
                buffer.push_front(event);
                break;
            }
            flushed += 1;

            // Rate limiting
            if flushed >= FLUSH_RATE_LIMIT {
                break;
            }
        }

        if flushed > 0 {
            info!(flushed = flushed, remaining = buffer.len(), "Flushed buffered events");
        }
    }
}
