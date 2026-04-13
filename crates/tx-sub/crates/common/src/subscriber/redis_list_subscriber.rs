use std::pin::Pin;
use std::time::Duration;

use anyhow::{Context, Result};
use async_stream::try_stream;
use async_trait::async_trait;
use futures::Stream;
use popeyes_trading_types::Event;
use redis::{aio::ConnectionManager, Client};
use tracing::{debug, trace};

use super::config::ListSubscriberConfig;
use super::traits::{EventReceiver, ReceivedEvent, RedisSubscriber};

/// Redis list subscriber using BRPOP.
///
/// Consumes events from a Redis list queue. Messages are removed from the
/// queue on pop, so acknowledgment is a no-op.
pub struct RedisListSubscriber {
    conn_manager: ConnectionManager,
    config: ListSubscriberConfig,
}

impl RedisListSubscriber {
    /// Create a new list subscriber.
    pub async fn new(redis_url: &str, config: ListSubscriberConfig) -> Result<Self> {
        let redis_client = Client::open(redis_url)?;
        let conn_manager = ConnectionManager::new(redis_client).await?;

        Ok(Self {
            conn_manager,
            config,
        })
    }
}

#[async_trait]
impl RedisSubscriber for RedisListSubscriber {
    async fn subscribe(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ReceivedEvent>> + Send + '_>>> {
        let mut conn = self.conn_manager.clone();
        let queue_name = self.config.name.clone();
        let timeout = self.config.timeout_secs.unwrap_or(0) as f64;

        let stream = try_stream! {
            loop {
                // BRPOP returns Option<(queue_name, value)>
                let result: Option<(String, String)> = redis::cmd("BRPOP")
                    .arg(&queue_name)
                    .arg(timeout)
                    .query_async(&mut conn)
                    .await
                    .with_context(|| format!("Failed to BRPOP from queue {}", queue_name))?;

                if let Some((source, json_str)) = result {
                    trace!(
                        queue = %source,
                        data_len = json_str.len(),
                        "Received event from Redis list"
                    );

                    let event: Event = serde_json::from_str(&json_str)
                        .with_context(|| format!("Failed to deserialize Event from queue {}", source))?;

                    yield ReceivedEvent {
                        event,
                        stream_id: None,
                        source,
                    };
                }
                // If result is None (timeout), loop continues
            }
        };

        Ok(Box::pin(stream))
    }

    async fn acknowledge(&self, _event: &ReceivedEvent) -> Result<()> {
        // No-op for list subscriber - message is removed on BRPOP
        Ok(())
    }

    fn name(&self) -> &str {
        "RedisListSubscriber"
    }
}

/// Pull-based Redis list receiver using BRPOP.
///
/// Provides a simpler pull-based interface for consuming events from a Redis list.
/// Messages are removed atomically on pop, so no acknowledgment is needed.
pub struct RedisListReceiver {
    conn_manager: ConnectionManager,
    config: ListSubscriberConfig,
}

impl RedisListReceiver {
    /// Create a new list receiver.
    pub async fn new(redis_url: &str, config: ListSubscriberConfig) -> Result<Self> {
        let redis_client = Client::open(redis_url)?;
        let conn_manager = ConnectionManager::new(redis_client).await?;

        Ok(Self {
            conn_manager,
            config,
        })
    }

    /// Internal method to fetch a single event with a specified timeout.
    async fn fetch_event(&self, timeout_secs: u64) -> Result<Option<ReceivedEvent>> {
        let mut conn = self.conn_manager.clone();
        let queue_name = &self.config.name;

        let result: Option<(String, String)> = redis::cmd("BRPOP")
            .arg(queue_name)
            .arg(timeout_secs)
            .query_async(&mut conn)
            .await
            .with_context(|| format!("Failed to BRPOP from queue {}", queue_name))?;

        match result {
            Some((source, json_str)) => {
                trace!(
                    queue = %source,
                    data_len = json_str.len(),
                    "Received event from Redis list"
                );

                let event: Event = serde_json::from_str(&json_str)
                    .with_context(|| {
                        format!("Failed to deserialize Event from queue {}", source)
                    })?;

                Ok(Some(ReceivedEvent {
                    event,
                    stream_id: None,
                    source,
                }))
            }
            None => Ok(None),
        }
    }
}

#[async_trait]
impl EventReceiver for RedisListReceiver {
    async fn next_event(&self) -> Result<ReceivedEvent> {
        // Block forever (timeout=0) until an event is available
        loop {
            if let Some(event) = self.fetch_event(0).await? {
                return Ok(event);
            }
            // BRPOP with timeout=0 should block forever, but just in case
        }
    }

    async fn try_next_event(&self, timeout: Duration) -> Result<Option<ReceivedEvent>> {
        // Convert Duration to seconds (minimum 1 second for Redis BRPOP)
        let timeout_secs = timeout.as_secs().max(1);
        self.fetch_event(timeout_secs).await
    }

    fn name(&self) -> &str {
        "RedisListReceiver"
    }
}
