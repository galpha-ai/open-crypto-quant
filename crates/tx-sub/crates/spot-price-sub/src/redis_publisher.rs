//! Redis publisher for spot price updates

use anyhow::Result;
use chrono::Utc;
use common::publisher::{RedisListPublisher, RedisPubsubPublisher, RedisStreamPublisher, RedisPublisher};
use popeyes_trading_types::{Event, MarketDataEvent, SpotPriceUpdate};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::config::SpotPriceRedisConfig;
use crate::metrics::Metrics;

/// Maximum buffer size for failed publishes
const BUFFER_MAX_SIZE: usize = 1000;

/// Redis publisher for spot price updates
pub struct RedisSpotPricePublisher {
    /// Spot price update receiver
    spot_price_rx: broadcast::Receiver<SpotPriceUpdate>,
    /// List publisher
    list_publisher: Option<Arc<RedisListPublisher>>,
    /// Stream publisher
    stream_publisher: Option<Arc<RedisStreamPublisher>>,
    /// Pubsub publisher
    pubsub_publisher: Option<Arc<RedisPubsubPublisher>>,
    /// Event buffer for failed publishes
    buffer: VecDeque<SpotPriceUpdate>,
    /// Metrics
    metrics: Arc<Metrics>,
}

impl RedisSpotPricePublisher {
    /// Create a new Redis spot price publisher
    pub async fn new(
        spot_price_rx: broadcast::Receiver<SpotPriceUpdate>,
        redis_config: &SpotPriceRedisConfig,
        metrics: Arc<Metrics>,
    ) -> Result<Self> {
        // Create list publisher if queues configured
        let list_publisher = if !redis_config.queues.is_empty() {
            Some(Arc::new(
                RedisListPublisher::new(&redis_config.url, redis_config.queues.clone()).await?,
            ))
        } else {
            None
        };

        // Create stream publisher if streams configured
        let stream_publisher = if let Some(ref streams) = redis_config.streams {
            if !streams.is_empty() {
                Some(Arc::new(
                    RedisStreamPublisher::new(&redis_config.url, streams.clone()).await?,
                ))
            } else {
                None
            }
        } else {
            None
        };

        // Create pubsub publisher if configured
        let pubsub_publisher = if let Some(ref pubsub_config) = redis_config.pubsub {
            Some(Arc::new(
                RedisPubsubPublisher::new(&redis_config.url, pubsub_config.clone()).await?,
            ))
        } else {
            None
        };

        Ok(Self {
            spot_price_rx,
            list_publisher,
            stream_publisher,
            pubsub_publisher,
            buffer: VecDeque::new(),
            metrics,
        })
    }

    /// Start the Redis publisher (consumes self)
    pub async fn start(mut self) -> Result<()> {
        tracing::info!("Redis spot price publisher starting");

        loop {
            tokio::select! {
                result = self.spot_price_rx.recv() => {
                    match result {
                        Ok(update) => {
                            // Try to flush buffer first if not empty
                            if !self.buffer.is_empty() {
                                self.try_flush_buffer().await;
                            }

                            // Publish the new update
                            self.publish_update(update).await;
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(lagged = n, "Redis publisher lagged behind parser");
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            tracing::info!("Spot price update channel closed, shutting down");
                            break;
                        }
                    }
                }
            }
        }

        // Flush remaining buffer before shutdown
        tracing::info!(buffered = self.buffer.len(), "Flushing buffer before shutdown");
        self.try_flush_buffer().await;

        tracing::info!("Redis spot price publisher shut down");
        Ok(())
    }

    /// Publish a single spot price update
    async fn publish_update(&mut self, update: SpotPriceUpdate) {
        // Convert to Event::MarketData(MarketDataEvent::SpotPrice)
        let event = Event::MarketData(MarketDataEvent::SpotPrice(update.clone()));

        // Publish to all targets
        let mut success = false;

        // Publish to lists
        if let Some(ref publisher) = self.list_publisher {
            if let Err(e) = publisher.publish(event.clone()).await {
                tracing::warn!(
                    error = %e,
                    "Failed to publish to Redis lists"
                );
                self.metrics
                    .redis_publish_failures
                    .with_label_values(&["list", "queues"])
                    .inc();
            } else {
                success = true;
            }
        }

        // Publish to streams
        if let Some(ref publisher) = self.stream_publisher {
            if let Err(e) = publisher.publish(event.clone()).await {
                tracing::warn!(
                    error = %e,
                    "Failed to publish to Redis streams"
                );
                self.metrics
                    .redis_publish_failures
                    .with_label_values(&["stream", "streams"])
                    .inc();
            } else {
                success = true;
            }
        }

        // Publish to pubsub
        if let Some(ref publisher) = self.pubsub_publisher {
            if let Err(e) = publisher.publish(event).await {
                tracing::warn!(
                    error = %e,
                    "Failed to publish to Redis pubsub"
                );
                self.metrics
                    .redis_publish_failures
                    .with_label_values(&["pubsub", "channel"])
                    .inc();
            } else {
                success = true;
            }
        }

        if success {
            // Update metrics
            self.metrics
                .spot_prices_published
                .with_label_values(&[&update.symbol])
                .inc();

            // Calculate and record latency
            let now = Utc::now();
            let latency_secs = (now.timestamp_millis() - update.timestamp.timestamp_millis()) as f64
                / 1000.0;
            self.metrics
                .spot_price_latency
                .with_label_values(&[&update.symbol])
                .observe(latency_secs);

            tracing::debug!(
                symbol = %update.symbol,
                price = update.price,
                latency_ms = (latency_secs * 1000.0) as i64,
                "Published spot price update"
            );
        } else {
            // All publishes failed, buffer the event
            self.buffer_event(update);
        }
    }

    /// Buffer an event that failed to publish
    fn buffer_event(&mut self, update: SpotPriceUpdate) {
        if self.buffer.len() >= BUFFER_MAX_SIZE {
            // Drop oldest event
            self.buffer.pop_front();
            self.metrics.buffer_overflows.inc();
            tracing::warn!("Buffer overflow, dropping oldest spot price update");
        }

        self.buffer.push_back(update);
        self.metrics.buffer_size.set(self.buffer.len() as i64);
    }

    /// Try to flush buffered events
    async fn try_flush_buffer(&mut self) {
        if self.buffer.is_empty() {
            return;
        }

        tracing::info!(buffer_size = self.buffer.len(), "Attempting to flush buffer");

        let mut flushed = 0;
        let max_flush = 100.min(self.buffer.len());

        for _ in 0..max_flush {
            if let Some(update) = self.buffer.pop_front() {
                self.publish_update(update).await;
                flushed += 1;
            }
        }

        self.metrics.buffer_size.set(self.buffer.len() as i64);
        tracing::info!(flushed = flushed, remaining = self.buffer.len(), "Buffer flush complete");
    }
}
