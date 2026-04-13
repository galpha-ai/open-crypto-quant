//! Redis-based persistence for production trading.

use async_trait::async_trait;
use redis::{Client, aio::ConnectionManager};
use serde::Deserialize;
use serde_json::Value;
use tracing::info;

use super::TradingEventPersistence;

#[derive(Debug, Clone, Deserialize)]
pub struct RedisPersistenceConfig {
    pub redis_url: String,
    pub signal_queue_name: String,
    pub position_queue_name: String,
    pub max_queue_length: usize,
}

pub struct RedisTradingEventPersistence {
    conn_manager: ConnectionManager,
    signal_queue_name: String,
    position_queue_name: String,
    max_queue_length: isize,
}

impl RedisTradingEventPersistence {
    pub async fn new(config: RedisPersistenceConfig) -> anyhow::Result<Self> {
        let redis_client = Client::open(config.redis_url)?;
        let conn_manager = ConnectionManager::new(redis_client).await?;

        Ok(Self {
            conn_manager,
            signal_queue_name: config.signal_queue_name,
            position_queue_name: config.position_queue_name,
            max_queue_length: config.max_queue_length as isize,
        })
    }
}

#[async_trait]
impl TradingEventPersistence for RedisTradingEventPersistence {
    async fn persist_signal(&self, signal_json: Value) -> anyhow::Result<()> {
        let json_str = serde_json::to_string(&signal_json)
            .map_err(|e| anyhow::anyhow!("Failed to serialize signal: {}", e))?;

        let mut conn = self.conn_manager.clone();
        let mut pipe = redis::pipe();

        pipe.lpush(&self.signal_queue_name, &json_str).ltrim(
            &self.signal_queue_name,
            0,
            self.max_queue_length - 1,
        );

        pipe.exec_async(&mut conn)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to persist signal to Redis: {}", e))?;

        info!(
            queue_name = self.signal_queue_name,
            "Successfully persisted signal to Redis queue"
        );

        Ok(())
    }

    async fn persist_position_closed(&self, event_json: Value) -> anyhow::Result<()> {
        let json_str = serde_json::to_string(&event_json)
            .map_err(|e| anyhow::anyhow!("Failed to serialize position closed event: {}", e))?;

        let mut conn = self.conn_manager.clone();
        let mut pipe = redis::pipe();

        pipe.lpush(&self.position_queue_name, &json_str).ltrim(
            &self.position_queue_name,
            0,
            self.max_queue_length - 1,
        );

        pipe.exec_async(&mut conn).await.map_err(|e| {
            anyhow::anyhow!("Failed to persist position closed event to Redis: {}", e)
        })?;

        info!(
            queue_name = self.position_queue_name,
            "Successfully persisted position closed event to Redis queue"
        );

        Ok(())
    }
}
