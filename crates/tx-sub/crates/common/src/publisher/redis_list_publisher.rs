use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use popeyes_trading_types::Event;
use redis::{Client, aio::ConnectionManager};
use tracing::debug;

use super::traits::RedisPublisher;
use crate::config::QueueConfig;

pub struct RedisListPublisher {
    conn_manager: ConnectionManager,
    queues: Vec<QueueConfig>,
}

impl RedisListPublisher {
    pub async fn new(redis_url: &str, queues: Vec<QueueConfig>) -> Result<Self> {
        let redis_client = Client::open(redis_url)?;
        let conn_manager = ConnectionManager::new(redis_client).await?;

        Ok(Self {
            conn_manager,
            queues,
        })
    }
}

#[async_trait]
impl RedisPublisher for RedisListPublisher {
    async fn publish(&self, event: Event) -> Result<()> {
        let json_str = self.convert_event_format(&event)?;
        self.publish_raw(json_str).await
    }

    async fn publish_raw(&self, json: String) -> Result<()> {
        let mut conn = self.conn_manager.clone();
        let mut pipe = redis::pipe();

        for queue in &self.queues {
            pipe.lpush(&queue.name, &json)
                .ltrim(&queue.name, 0, queue.max_length as isize - 1);

            debug!(queue_name = queue.name, "Added queue to Redis pipeline");
        }

        pipe.exec_async(&mut conn).await?;

        debug!("Successfully pushed event to all Redis queues");
        debug!(json, "Pushed event data");

        Ok(())
    }

    fn name(&self) -> &str {
        "RedisListPublisher"
    }
}

impl RedisListPublisher {
    fn convert_event_format(&self, event: &Event) -> Result<String> {
        serde_json::to_string(event)
            .with_context(|| format!("Failed to serialize event with payload: {:?}", event))
            .map_err(|e| anyhow!("{:#?}", e))
    }
}
