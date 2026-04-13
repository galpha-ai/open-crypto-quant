//! Redis publisher for Polymarket orderbook events
//!
//! This module publishes individual price changes from price_change events
//! and full orderbook snapshots from book events to Redis queues, streams,
//! and/or pubsub channels.

use anyhow::Result;
use common::publisher::{RedisListPublisher, RedisPubsubPublisher, RedisPublisher, RedisStreamPublisher};
use popeyes_trading_types::{Event, MarketDataEvent, OrderbookSnapshotEvent, OrderbookSource, OrderbookUpdateEvent, OrderSummary, TradeSide};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::market_metadata_cache::MarketMetadataCache;
use crate::metrics::Metrics;
use crate::types::{ParsedBookEvent, ParsedPriceChangeEvent};

const BUFFER_MAX_SIZE: usize = 1000;
const FLUSH_RATE_LIMIT: usize = 100; // events per second
const FLUSH_INTERVAL_MS: u64 = 1000;

/// Redis publisher for Polymarket orderbook events
pub struct RedisOrderbookPublisher {
    /// Redis publishers for different targets
    publishers: Vec<Box<dyn RedisPublisher>>,

    /// Receiver for price change events
    price_change_rx: broadcast::Receiver<ParsedPriceChangeEvent>,

    /// Receiver for book (orderbook snapshot) events
    book_rx: broadcast::Receiver<ParsedBookEvent>,

    /// Metrics for tracking publishing
    metrics: Arc<Metrics>,

    /// Buffer for events during Redis unavailability
    buffer: Arc<Mutex<VecDeque<OrderbookUpdateEvent>>>,

    /// Market metadata cache for enriching orderbook events
    market_metadata_cache: Arc<MarketMetadataCache>,
}

impl RedisOrderbookPublisher {
    /// Create a new Redis orderbook publisher
    pub async fn new(
        config: &Config,
        price_change_rx: broadcast::Receiver<ParsedPriceChangeEvent>,
        book_rx: broadcast::Receiver<ParsedBookEvent>,
        metrics: Arc<Metrics>,
        market_metadata_cache: Arc<MarketMetadataCache>,
    ) -> Result<Self> {
        let mut publishers: Vec<Box<dyn RedisPublisher>> = Vec::new();

        // Add list publisher if orderbook queues are configured
        if let Some(ref orderbook_queues) = config.redis.orderbook_queues {
            if !orderbook_queues.is_empty() {
                let list_publisher = RedisListPublisher::new(
                    &config.redis.url,
                    orderbook_queues.clone()
                ).await?;
                publishers.push(Box::new(list_publisher));
                info!("Initialized RedisListPublisher for orderbook updates");
            }
        }

        // Add stream publisher if orderbook streams are configured
        if let Some(ref orderbook_streams) = config.redis.orderbook_streams {
            if !orderbook_streams.is_empty() {
                let stream_publisher = RedisStreamPublisher::new(
                    &config.redis.url,
                    orderbook_streams.clone()
                ).await?;
                publishers.push(Box::new(stream_publisher));
                info!("Initialized RedisStreamPublisher for orderbook updates");
            }
        }

        // Add pubsub publisher if configured
        if let Some(ref orderbook_pubsub) = config.redis.orderbook_pubsub {
            if !orderbook_pubsub.channels.is_empty() {
                let pubsub_publisher = RedisPubsubPublisher::new(
                    &config.redis.url,
                    orderbook_pubsub.clone()
                ).await?;
                publishers.push(Box::new(pubsub_publisher));
                info!("Initialized RedisPubsubPublisher for orderbook updates");
            }
        }

        if publishers.is_empty() {
            return Err(anyhow::anyhow!(
                "No orderbook Redis publishers configured"
            ));
        }

        Ok(Self {
            publishers,
            price_change_rx,
            book_rx,
            metrics,
            buffer: Arc::new(Mutex::new(VecDeque::new())),
            market_metadata_cache,
        })
    }

    /// Run the publisher
    pub async fn run(mut self, cancellation_token: CancellationToken) -> Result<()> {
        info!("Redis orderbook publisher starting");

        let mut flush_ticker = interval(Duration::from_millis(FLUSH_INTERVAL_MS));

        loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    info!("Redis orderbook publisher shutting down, flushing buffer");
                    self.flush_buffer().await;
                    break;
                }
                recv_result = self.price_change_rx.recv() => {
                    match recv_result {
                        Ok(price_change_event) => {
                            // Calculate latency
                            let now_ms = chrono::Utc::now().timestamp_millis();
                            let latency_sec = (now_ms - price_change_event.timestamp) as f64 / 1000.0;
                            self.metrics
                                .orderbook_latency
                                .with_label_values(&[&price_change_event.market])
                                .observe(latency_sec);

                            // Try to flush buffer first
                            let buffer_len = {
                                let buf = self.buffer.lock().await;
                                buf.len()
                            };
                            if buffer_len > 0 {
                                debug!(buffer_size = buffer_len, "Flushing buffer before new events");
                                self.flush_buffer().await;
                            }

                            // Publish each price change individually
                            for price_change in &price_change_event.price_changes {
                                // Look up market metadata from cache
                                let market_metadata = self.market_metadata_cache.get(&price_change.asset_id).await;

                                let update_event = OrderbookUpdateEvent {
                                    asset_id: price_change.asset_id.clone(),
                                    market: price_change_event.market.clone(),
                                    price: price_change.price,
                                    size: price_change.size,
                                    side: price_change.side.clone(),
                                    hash: price_change.hash.clone(),
                                    best_bid: price_change.best_bid,
                                    best_ask: price_change.best_ask,
                                    timestamp: price_change_event.timestamp,
                                    observed_at: price_change_event.observed_at,
                                    source: OrderbookSource::Polymarket,
                                    market_metadata,
                                };

                                if let Err(e) = self.publish_event(&update_event).await {
                                    warn!(
                                        error = %e,
                                        asset_id = %update_event.asset_id,
                                        "Failed to publish orderbook update, buffering"
                                    );

                                    // Add to buffer
                                    let mut buffer = self.buffer.lock().await;
                                    if buffer.len() >= BUFFER_MAX_SIZE {
                                        buffer.pop_front();
                                        warn!("Buffer overflow, dropped oldest event");
                                    }
                                    buffer.push_back(update_event);
                                } else {
                                    // Update success metrics
                                    let side_str = match update_event.side {
                                        TradeSide::Buy => "BUY",
                                        TradeSide::Sell => "SELL",
                                    };
                                    self.metrics
                                        .price_changes_published
                                        .with_label_values(&[&update_event.market, side_str])
                                        .inc();

                                    // Track removals (size=0)
                                    if update_event.size == 0.0 {
                                        self.metrics
                                            .orderbook_removals
                                            .with_label_values(&[&update_event.market, side_str])
                                            .inc();
                                    }

                                    // Update best bid/ask gauges
                                    self.metrics
                                        .best_bid
                                        .with_label_values(&[&update_event.market, &update_event.asset_id])
                                        .set(update_event.best_bid);
                                    self.metrics
                                        .best_ask
                                        .with_label_values(&[&update_event.market, &update_event.asset_id])
                                        .set(update_event.best_ask);
                                }
                            }

                            // Update received counter
                            self.metrics
                                .price_changes_received
                                .with_label_values(&[&price_change_event.market])
                                .inc();
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(
                                skipped = skipped,
                                "Orderbook publisher lagged, events skipped"
                            );
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            info!("Price change channel closed, shutting down");
                            break;
                        }
                    }
                }
                book_result = self.book_rx.recv() => {
                    match book_result {
                        Ok(book_event) => {
                            // Calculate latency
                            let now_ms = chrono::Utc::now().timestamp_millis();
                            let latency_sec = (now_ms - book_event.timestamp) as f64 / 1000.0;
                            self.metrics
                                .book_snapshot_latency
                                .with_label_values(&[&book_event.market])
                                .observe(latency_sec);

                            // Update received counter
                            self.metrics
                                .book_snapshots_received
                                .with_label_values(&[&book_event.market])
                                .inc();

                            // Look up market metadata from cache
                            let market_metadata = self.market_metadata_cache.get(&book_event.asset_id).await;

                            // Convert to OrderbookSnapshotEvent
                            let snapshot_event = OrderbookSnapshotEvent {
                                asset_id: book_event.asset_id.clone(),
                                market: book_event.market.clone(),
                                bids: book_event.bids.iter().map(|b| OrderSummary {
                                    price: b.price,
                                    size: b.size,
                                }).collect(),
                                asks: book_event.asks.iter().map(|a| OrderSummary {
                                    price: a.price,
                                    size: a.size,
                                }).collect(),
                                hash: book_event.hash.clone(),
                                timestamp: book_event.timestamp,
                                observed_at: book_event.observed_at,
                                source: OrderbookSource::Polymarket,
                                market_metadata,
                            };

                            // Publish the snapshot
                            if let Err(e) = self.publish_snapshot(&snapshot_event).await {
                                warn!(
                                    error = %e,
                                    asset_id = %snapshot_event.asset_id,
                                    market = %snapshot_event.market,
                                    "Failed to publish book snapshot"
                                );
                            } else {
                                self.metrics
                                    .book_snapshots_published
                                    .with_label_values(&[&snapshot_event.market])
                                    .inc();
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(
                                skipped = skipped,
                                "Book publisher lagged, events skipped"
                            );
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            info!("Book channel closed");
                            // Don't break here - price_change channel may still be active
                        }
                    }
                }
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

        info!("Flushing buffered events before shutdown");
        self.flush_buffer().await;

        info!("Redis orderbook publisher shut down successfully");
        Ok(())
    }

    /// Publish a single orderbook update event
    async fn publish_event(&self, event: &OrderbookUpdateEvent) -> Result<()> {
        // Convert to Event::MarketData(MarketDataEvent::OrderbookUpdate)
        let market_event = Event::MarketData(MarketDataEvent::OrderbookUpdate(event.clone()));
        self.publish_market_event(market_event).await
    }

    /// Publish an orderbook snapshot event
    async fn publish_snapshot(&self, event: &OrderbookSnapshotEvent) -> Result<()> {
        // Convert to Event::MarketData(MarketDataEvent::OrderbookSnapshot)
        let market_event = Event::MarketData(MarketDataEvent::OrderbookSnapshot(event.clone()));
        self.publish_market_event(market_event).await
    }

    /// Publish an Event to all configured Redis publishers
    async fn publish_market_event(&self, market_event: Event) -> Result<()> {
        let mut last_error = None;
        let mut success_count = 0;

        for publisher in &self.publishers {
            match publisher.publish(market_event.clone()).await {
                Ok(_) => {
                    debug!(publisher = publisher.name(), "Published to Redis");
                    success_count += 1;
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        publisher = publisher.name(),
                        "Failed to publish"
                    );
                    self.metrics
                        .redis_publish_failures
                        .with_label_values(&[publisher.name()])
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

    /// Flush buffered events
    async fn flush_buffer(&self) {
        let mut buffer = self.buffer.lock().await;
        let mut flushed = 0;

        while let Some(event) = buffer.pop_front() {
            if let Err(e) = self.publish_event(&event).await {
                warn!(error = %e, "Failed to flush buffered event, re-buffering");
                buffer.push_front(event);
                break;
            }
            flushed += 1;

            if flushed >= FLUSH_RATE_LIMIT {
                break;
            }
        }

        if flushed > 0 {
            info!(flushed = flushed, remaining = buffer.len(), "Flushed buffered events");
        }
    }
}
