//! Redis-based event collector for paper trading event persistence.
//!
//! The `RedisEventCollector` implements the `EventCollector` trait and publishes
//! events to Redis instead of storing them in memory. This enables real-time
//! event streaming to downstream consumers during paper trading.
//!
//! Events are published asynchronously via a bounded channel to avoid blocking
//! the trading event loop. A background task handles the actual Redis publishing.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio::sync::mpsc;
use tracing::{debug, error, trace, warn};
use tx_sub_common::publisher::RedisPublisher;

use crate::domain::SystemEvent;

use super::collector::{CollectedEvent, EventCollector, convert_system_event_to_collected};
use super::filter::EventFilter;

/// Configuration for the Redis event collector.
#[derive(Debug, Clone)]
pub struct RedisEventCollectorConfig {
    /// Session identifier for correlation
    pub session_id: String,

    /// Event filter configuration
    pub filter: EventFilter,

    /// Channel buffer size (events dropped if buffer is full)
    pub channel_buffer_size: usize,
}

impl Default for RedisEventCollectorConfig {
    fn default() -> Self {
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            filter: EventFilter::default(),
            channel_buffer_size: 10_000,
        }
    }
}

/// Redis-based event collector that publishes events to Redis.
///
/// Events are sent to a background task via a bounded channel for
/// non-blocking publishing. If the channel is full, events are dropped
/// to avoid blocking the trading event loop.
pub struct RedisEventCollector {
    /// Sender for the event channel
    tx: mpsc::Sender<CollectedEvent>,

    /// Event filter
    filter: EventFilter,

    /// Session ID for logging
    session_id: String,

    /// Count of recorded events (for len/is_empty)
    event_count: AtomicUsize,

    /// Count of dropped events due to channel backpressure
    dropped_count: AtomicUsize,

    /// Next sequence number to assign
    next_sequence: AtomicU64,
}

impl RedisEventCollector {
    /// Create a new Redis event collector with the given publisher.
    ///
    /// Spawns a background task to handle event publishing.
    pub fn new<P: RedisPublisher + 'static>(
        publisher: Arc<P>,
        config: RedisEventCollectorConfig,
    ) -> Self {
        let (tx, rx) = mpsc::channel(config.channel_buffer_size);

        // Spawn background publisher task
        let session_id = config.session_id.clone();
        tokio::spawn(Self::publisher_task(rx, publisher, session_id.clone()));

        debug!(
            session_id = %config.session_id,
            buffer_size = config.channel_buffer_size,
            "Created RedisEventCollector"
        );

        Self {
            tx,
            filter: config.filter,
            session_id: config.session_id,
            event_count: AtomicUsize::new(0),
            dropped_count: AtomicUsize::new(0),
            next_sequence: AtomicU64::new(0),
        }
    }

    /// Background task that drains the channel and publishes to Redis.
    async fn publisher_task<P: RedisPublisher + 'static>(
        mut rx: mpsc::Receiver<CollectedEvent>,
        publisher: Arc<P>,
        session_id: String,
    ) {
        let mut published_count: u64 = 0;
        let mut error_count: u64 = 0;

        debug!(session_id = %session_id, "Redis publisher task started");

        while let Some(event) = rx.recv().await {
            match serde_json::to_string(&event) {
                Ok(json) => {
                    if let Err(e) = publisher.publish_raw(json).await {
                        error_count += 1;
                        if error_count <= 10 || error_count % 100 == 0 {
                            error!(
                                error = %e,
                                error_count,
                                event_type = %event.event_type,
                                "Failed to publish event to Redis"
                            );
                        }
                    } else {
                        published_count += 1;
                        trace!(
                            event_type = %event.event_type,
                            published_count,
                            "Published event to Redis"
                        );
                    }
                }
                Err(e) => {
                    error_count += 1;
                    error!(
                        error = %e,
                        event_type = %event.event_type,
                        "Failed to serialize event"
                    );
                }
            }
        }

        debug!(
            session_id = %session_id,
            published_count,
            error_count,
            "Redis publisher task completed"
        );
    }

    /// Get the count of dropped events due to channel backpressure.
    pub fn dropped_count(&self) -> usize {
        self.dropped_count.load(Ordering::Relaxed)
    }

    /// Get the session ID.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

impl EventCollector for RedisEventCollector {
    fn record(&self, event: &SystemEvent) {
        self.record_with_logical_time(event, None);
    }

    fn record_with_logical_time(&self, event: &SystemEvent, logical_time: Option<DateTime<Utc>>) {
        // Apply filter
        if !self.filter.should_record(event) {
            return;
        }

        let sequence_id = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        let collected = convert_system_event_to_collected(
            event,
            logical_time,
            Some(&self.session_id),
            sequence_id,
        );

        // Non-blocking send - drop if channel is full
        match self.tx.try_send(collected) {
            Ok(()) => {
                self.event_count.fetch_add(1, Ordering::Relaxed);
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                let dropped = self.dropped_count.fetch_add(1, Ordering::Relaxed) + 1;
                if dropped <= 10 || dropped % 1000 == 0 {
                    warn!(
                        dropped_count = dropped,
                        session_id = %self.session_id,
                        "Event dropped due to channel backpressure"
                    );
                }
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                warn!(
                    session_id = %self.session_id,
                    "Event channel closed, event dropped"
                );
            }
        }
    }

    fn events(&self) -> Vec<CollectedEvent> {
        // RedisEventCollector streams events, doesn't store them
        Vec::new()
    }

    fn export(&self, _path: &Path) -> Result<()> {
        // No-op for Redis collector - events are streamed continuously
        debug!(
            session_id = %self.session_id,
            event_count = self.event_count.load(Ordering::Relaxed),
            dropped_count = self.dropped_count.load(Ordering::Relaxed),
            "RedisEventCollector export called (no-op)"
        );
        Ok(())
    }

    fn len(&self) -> usize {
        self.event_count.load(Ordering::Relaxed)
    }

    fn is_empty(&self) -> bool {
        self.event_count.load(Ordering::Relaxed) == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::TimerEvent;
    use crate::execution::{LimitOrderEvent, OrderSide};
    use async_trait::async_trait;
    use std::sync::atomic::AtomicU64;
    use tokio::sync::Mutex;
    use tx_sub_common::types::Event as TxSubEvent;

    /// Mock Redis publisher for testing
    struct MockPublisher {
        published: Mutex<Vec<String>>,
        publish_count: AtomicU64,
    }

    impl MockPublisher {
        fn new() -> Self {
            Self {
                published: Mutex::new(Vec::new()),
                publish_count: AtomicU64::new(0),
            }
        }

        async fn get_published(&self) -> Vec<String> {
            self.published.lock().await.clone()
        }
    }

    #[async_trait]
    impl RedisPublisher for MockPublisher {
        async fn publish(&self, _event: TxSubEvent) -> anyhow::Result<()> {
            Ok(())
        }

        async fn publish_raw(&self, json: String) -> anyhow::Result<()> {
            self.published.lock().await.push(json);
            self.publish_count.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        fn name(&self) -> &str {
            "mock"
        }
    }

    #[tokio::test]
    async fn test_redis_collector_filters_events() {
        let publisher = Arc::new(MockPublisher::new());
        let config = RedisEventCollectorConfig {
            session_id: "test".to_string(),
            filter: EventFilter::paper_trading_default(),
            channel_buffer_size: 100,
        };

        let collector = RedisEventCollector::new(Arc::clone(&publisher), config);

        // Timer event should be filtered out
        let timer_event = SystemEvent::Timer(TimerEvent {
            timestamp: Utc::now(),
        });
        collector.record(&timer_event);

        // LimitOrder event should be recorded
        let order_event = SystemEvent::LimitOrder(LimitOrderEvent::OrderPlaced {
            order_id: "test".to_string(),
            mint: "test".to_string(),
            market: Some("test".to_string()),
            price: 0.5,
            size: 100.0,
            side: OrderSide::Buy,
            timestamp: Utc::now(),
            signal_id: None,
            context: None,
        });
        collector.record(&order_event);

        // Wait for publisher task to process
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Only 1 event should have been published (the order event)
        assert_eq!(collector.len(), 1);
    }

    #[tokio::test]
    async fn test_redis_collector_publishes_to_redis() {
        let publisher = Arc::new(MockPublisher::new());
        let config = RedisEventCollectorConfig {
            session_id: "test".to_string(),
            filter: EventFilter::allow_all(),
            channel_buffer_size: 100,
        };

        let collector = RedisEventCollector::new(Arc::clone(&publisher), config);

        // Record an event
        let timer_event = SystemEvent::Timer(TimerEvent {
            timestamp: Utc::now(),
        });
        collector.record(&timer_event);

        // Wait for publisher task to process
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Check that event was published
        let published = publisher.get_published().await;
        assert_eq!(published.len(), 1);

        // Verify the published JSON
        let event: CollectedEvent = serde_json::from_str(&published[0]).unwrap();
        assert_eq!(event.event_type, "Timer");
    }

    #[test]
    fn test_convert_limit_order_event_without_session_id() {
        let event = SystemEvent::LimitOrder(LimitOrderEvent::OrderPlaced {
            order_id: "ord123".to_string(),
            mint: "asset123".to_string(),
            market: Some("market456".to_string()),
            price: 0.55,
            size: 100.0,
            side: OrderSide::Buy,
            timestamp: Utc::now(),
            signal_id: Some("sig789".to_string()),
            context: None,
        });

        let collected = convert_system_event_to_collected(&event, None, None, 42);

        assert_eq!(collected.event_type, "LimitOrder.OrderPlaced");
        assert_eq!(collected.data["order_id"], "ord123");
        assert_eq!(collected.data["mint"], "asset123");
        assert_eq!(collected.data["price"], 0.55);
        assert!(collected.logical_time.is_none());
        assert!(collected.session_id.is_none());
        assert_eq!(collected.sequence_id, 42);
    }

    #[test]
    fn test_convert_limit_order_event_with_session_id() {
        let event = SystemEvent::LimitOrder(LimitOrderEvent::OrderPlaced {
            order_id: "ord123".to_string(),
            mint: "asset123".to_string(),
            market: Some("market456".to_string()),
            price: 0.55,
            size: 100.0,
            side: OrderSide::Buy,
            timestamp: Utc::now(),
            signal_id: Some("sig789".to_string()),
            context: None,
        });

        let collected =
            convert_system_event_to_collected(&event, None, Some("test-session-123"), 99);

        assert_eq!(collected.event_type, "LimitOrder.OrderPlaced");
        assert_eq!(collected.data["order_id"], "ord123");
        assert_eq!(collected.session_id, Some("test-session-123".to_string()));
        assert_eq!(collected.sequence_id, 99);
    }
}
