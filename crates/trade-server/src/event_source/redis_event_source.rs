use anyhow::{Context, Result};
use async_trait::async_trait;
use popeyes_trading_types::{TokenEvent /* , TokenEventJson*/};
use redis::aio::MultiplexedConnection;

use crate::{domain::SystemEvent, event_source::EventSource};

pub struct RedisEventSource {
    connection: MultiplexedConnection,
    queue_key: String,
}

impl RedisEventSource {
    pub async fn new(redis_url: &str, queue_key: String) -> Result<Self> {
        let client = redis::Client::open(redis_url)?;
        let connection = client.get_multiplexed_async_connection().await?;

        tracing::debug!("Connected to Redis at {}", redis_url);

        Ok(RedisEventSource {
            connection,
            queue_key,
        })
    }
}

#[async_trait]
impl EventSource for RedisEventSource {
    async fn try_next_event(&self) -> Result<Option<SystemEvent>> {
        // Use RPOP to get an event without blocking
        let result: Option<String> = redis::cmd("RPOP")
            .arg(&self.queue_key)
            .query_async(&mut self.connection.clone())
            .await?;

        match result {
            Some(json_str) => {
                tracing::debug!(
                    raw_json = %json_str,
                    "Received event from Redis with raw json string",
                );
                let json_event: TokenEvent = serde_json::from_str(json_str.as_str())
                    .with_context(|| format!("converting {} to TokenEvent", json_str.as_str()))?;
                let event: TokenEvent = json_event.into();
                Ok(Some(SystemEvent::Token(event)))
            }
            None => Ok(None),
        }
    }

    async fn next_event(&self) -> Result<Option<SystemEvent>> {
        // Use BRPOP with a timeout of 1 second
        let result: Option<(String, String)> = redis::cmd("BRPOP")
            .arg(&self.queue_key)
            .arg(1)
            .query_async(&mut self.connection.clone())
            .await?;

        match result {
            Some((_, json_str)) => {
                tracing::debug!(
                    "Received event from Redis with raw json string: {}",
                    json_str
                );
                let json_event: TokenEvent = serde_json::from_str(json_str.as_str())
                    .with_context(|| format!("converting {} to TokenEvent", json_str.as_str()))?;
                let event: TokenEvent = json_event.into();
                Ok(Some(SystemEvent::Token(event)))
            }
            None => Ok(None),
        }
    }
}
