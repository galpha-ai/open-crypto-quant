//! Event persistence for trading signals and positions.

use async_trait::async_trait;
use serde_json::Value;

pub mod file;
mod redis;

pub use file::FileTradingEventPersistence;
pub use redis::{RedisPersistenceConfig, RedisTradingEventPersistence};

#[async_trait]
pub trait TradingEventPersistence: Send + Sync {
    async fn persist_signal(&self, signal_json: Value) -> anyhow::Result<()>;
    async fn persist_position_closed(&self, event_json: Value) -> anyhow::Result<()>;
}
