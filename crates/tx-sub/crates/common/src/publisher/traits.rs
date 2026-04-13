use anyhow::Result;
use async_trait::async_trait;
use popeyes_trading_types::Event;

#[async_trait]
pub trait RedisPublisher: Send + Sync {
    async fn publish(&self, event: Event) -> Result<()>;

    /// Publish a pre-serialized JSON string directly.
    /// Useful for publishing events that don't fit the Event enum.
    async fn publish_raw(&self, json: String) -> Result<()>;

    fn name(&self) -> &str;
}
