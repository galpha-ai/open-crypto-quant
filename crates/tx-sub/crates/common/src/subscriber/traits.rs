use std::pin::Pin;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use futures::Stream;
use popeyes_trading_types::Event;

/// A received event from a Redis subscriber with metadata for acknowledgment.
#[derive(Debug, Clone)]
pub struct ReceivedEvent {
    /// The parsed Event (can be Token or MarketData variant)
    pub event: Event,
    /// Stream entry ID for XACK (only set for stream subscribers)
    pub stream_id: Option<String>,
    /// Source queue/stream/channel name
    pub source: String,
}

/// Trait for Redis subscribers that consume TokenEvent data.
#[async_trait]
pub trait RedisSubscriber: Send + Sync {
    /// Subscribe and return a stream of events.
    async fn subscribe(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ReceivedEvent>> + Send + '_>>>;

    /// Acknowledge an event (required for streams, no-op for lists/pubsub).
    async fn acknowledge(&self, event: &ReceivedEvent) -> Result<()>;

    /// Get the subscriber name for logging/metrics.
    fn name(&self) -> &str;
}

/// Pull-based event receiver trait for consumers that prefer polling over streams.
///
/// This complements the stream-based `RedisSubscriber` trait by providing a simpler
/// pull-based interface that matches common consumption patterns like event loops.
///
/// For stream-based sources, implementations automatically handle acknowledgment
/// of the previous event when the next event is requested, providing at-least-once
/// delivery semantics.
#[async_trait]
pub trait EventReceiver: Send + Sync {
    /// Blocks until the next event is available.
    ///
    /// For stream-based sources, automatically acknowledges the previous event
    /// before returning the next one.
    async fn next_event(&self) -> Result<ReceivedEvent>;

    /// Non-blocking poll with timeout.
    ///
    /// Returns `Ok(None)` if no event is available within the timeout.
    /// For stream-based sources, automatically acknowledges the previous event.
    async fn try_next_event(&self, timeout: Duration) -> Result<Option<ReceivedEvent>>;

    /// Returns receiver name for logging/metrics.
    fn name(&self) -> &str;
}
