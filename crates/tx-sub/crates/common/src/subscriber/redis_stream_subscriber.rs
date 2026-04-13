use std::pin::Pin;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use async_stream::try_stream;
use async_trait::async_trait;
use futures::Stream;
use popeyes_trading_types::Event;
use redis::{aio::ConnectionManager, streams::StreamReadReply, Client, Value};
use tokio::sync::Mutex;
use tracing::{debug, info, trace, warn};

use super::config::StreamSubscriberConfig;
use super::traits::{EventReceiver, ReceivedEvent, RedisSubscriber};

/// Redis stream subscriber using XREADGROUP with consumer groups.
///
/// Provides at-least-once delivery semantics. Events must be acknowledged
/// via the `acknowledge` method after processing.
pub struct RedisStreamSubscriber {
    conn_manager: ConnectionManager,
    config: StreamSubscriberConfig,
}

impl RedisStreamSubscriber {
    /// Create a new stream subscriber.
    ///
    /// This will attempt to create the consumer group if it doesn't exist.
    pub async fn new(redis_url: &str, config: StreamSubscriberConfig) -> Result<Self> {
        let redis_client = Client::open(redis_url)?;
        let conn_manager = ConnectionManager::new(redis_client).await?;

        let subscriber = Self {
            conn_manager,
            config,
        };

        subscriber.ensure_consumer_group().await?;

        Ok(subscriber)
    }

    /// Ensure the consumer group exists, creating it if necessary.
    async fn ensure_consumer_group(&self) -> Result<()> {
        let mut conn = self.conn_manager.clone();

        // Try to create the consumer group
        // XGROUP CREATE stream group $ MKSTREAM
        // $ means start reading from new messages only
        let result: Result<(), redis::RedisError> = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(&self.config.name)
            .arg(&self.config.consumer_group)
            .arg("$")
            .arg("MKSTREAM")
            .query_async(&mut conn)
            .await;

        match result {
            Ok(()) => {
                info!(
                    stream = %self.config.name,
                    group = %self.config.consumer_group,
                    "Created consumer group"
                );
            }
            Err(e) => {
                // BUSYGROUP means the group already exists, which is fine
                if e.to_string().contains("BUSYGROUP") {
                    debug!(
                        stream = %self.config.name,
                        group = %self.config.consumer_group,
                        "Consumer group already exists"
                    );
                } else {
                    return Err(anyhow!(
                        "Failed to create consumer group {} for stream {}: {}",
                        self.config.consumer_group,
                        self.config.name,
                        e
                    ));
                }
            }
        }

        Ok(())
    }

    /// Parse a stream entry into a ReceivedEvent.
    fn parse_stream_entry(
        &self,
        stream_name: &str,
        entry_id: &str,
        fields: &[(String, Value)],
    ) -> Result<ReceivedEvent> {
        // Find the 'data' field which contains the JSON
        let data = fields
            .iter()
            .find(|(k, _)| k == "data")
            .ok_or_else(|| anyhow!("Stream entry missing 'data' field"))?;

        let json_str = match &data.1 {
            Value::BulkString(bytes) => String::from_utf8_lossy(bytes).to_string(),
            Value::SimpleString(s) => s.clone(),
            _ => return Err(anyhow!("Unexpected data field type: {:?}", data.1)),
        };

        let event: Event = serde_json::from_str(&json_str)
            .with_context(|| format!("Failed to deserialize Event from stream entry {}", entry_id))?;

        Ok(ReceivedEvent {
            event,
            stream_id: Some(entry_id.to_string()),
            source: stream_name.to_string(),
        })
    }
}

#[async_trait]
impl RedisSubscriber for RedisStreamSubscriber {
    async fn subscribe(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ReceivedEvent>> + Send + '_>>> {
        let mut conn = self.conn_manager.clone();
        let stream_name = self.config.name.clone();
        let consumer_group = self.config.consumer_group.clone();
        let consumer_name = self.config.consumer_name();
        let block_ms = self.config.block_ms();
        let count = self.config.count();

        let stream = try_stream! {
            loop {
                // XREADGROUP GROUP group consumer [COUNT count] [BLOCK ms] STREAMS stream >
                // The ">" ID means read only new messages that were never delivered
                let result: StreamReadReply = redis::cmd("XREADGROUP")
                    .arg("GROUP")
                    .arg(&consumer_group)
                    .arg(&consumer_name)
                    .arg("COUNT")
                    .arg(count)
                    .arg("BLOCK")
                    .arg(block_ms)
                    .arg("STREAMS")
                    .arg(&stream_name)
                    .arg(">")
                    .query_async(&mut conn)
                    .await
                    .with_context(|| format!("Failed to XREADGROUP from stream {}", stream_name))?;

                for stream_key in result.keys {
                    for entry in stream_key.ids {
                        let entry_id = entry.id.clone();

                        // Convert entry.map to Vec<(String, Value)>
                        let fields: Vec<(String, Value)> = entry.map.into_iter().collect();

                        match self.parse_stream_entry(&stream_key.key, &entry_id, &fields) {
                            Ok(received_event) => {
                                trace!(
                                    stream = %stream_key.key,
                                    entry_id = %entry_id,
                                    "Received event from Redis stream"
                                );
                                yield received_event;
                            }
                            Err(e) => {
                                warn!(
                                    stream = %stream_key.key,
                                    entry_id = %entry_id,
                                    error = %e,
                                    "Failed to parse stream entry, skipping"
                                );
                                // Acknowledge to prevent re-delivery of unparseable messages
                                let _: Result<i64, _> = redis::cmd("XACK")
                                    .arg(&stream_name)
                                    .arg(&consumer_group)
                                    .arg(&entry_id)
                                    .query_async(&mut conn)
                                    .await;
                            }
                        }
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }

    async fn acknowledge(&self, event: &ReceivedEvent) -> Result<()> {
        let stream_id = event
            .stream_id
            .as_ref()
            .ok_or_else(|| anyhow!("Cannot acknowledge event without stream_id"))?;

        let mut conn = self.conn_manager.clone();

        let acked: i64 = redis::cmd("XACK")
            .arg(&self.config.name)
            .arg(&self.config.consumer_group)
            .arg(stream_id)
            .query_async(&mut conn)
            .await
            .with_context(|| {
                format!(
                    "Failed to XACK entry {} in stream {}",
                    stream_id, self.config.name
                )
            })?;

        if acked == 1 {
            debug!(
                stream = %self.config.name,
                entry_id = %stream_id,
                "Acknowledged stream entry"
            );
        } else {
            warn!(
                stream = %self.config.name,
                entry_id = %stream_id,
                "XACK returned {}, entry may have already been acknowledged",
                acked
            );
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "RedisStreamSubscriber"
    }
}

/// Pull-based Redis stream receiver using XREADGROUP with consumer groups.
///
/// Provides at-least-once delivery semantics with automatic acknowledgment.
/// When `next_event()` or `try_next_event()` is called, the previously received
/// event is automatically acknowledged before fetching the next one.
pub struct RedisStreamReceiver {
    conn_manager: ConnectionManager,
    config: StreamSubscriberConfig,
    /// Tracks the last event ID for auto-acknowledgment
    last_event_id: Mutex<Option<String>>,
}

impl RedisStreamReceiver {
    /// Create a new stream receiver.
    ///
    /// This will attempt to create the consumer group if it doesn't exist.
    pub async fn new(redis_url: &str, config: StreamSubscriberConfig) -> Result<Self> {
        let redis_client = Client::open(redis_url)?;
        let conn_manager = ConnectionManager::new(redis_client).await?;

        let receiver = Self {
            conn_manager,
            config,
            last_event_id: Mutex::new(None),
        };

        receiver.ensure_consumer_group().await?;

        Ok(receiver)
    }

    /// Ensure the consumer group exists, creating it if necessary.
    async fn ensure_consumer_group(&self) -> Result<()> {
        let mut conn = self.conn_manager.clone();

        let result: Result<(), redis::RedisError> = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(&self.config.name)
            .arg(&self.config.consumer_group)
            .arg("$")
            .arg("MKSTREAM")
            .query_async(&mut conn)
            .await;

        match result {
            Ok(()) => {
                info!(
                    stream = %self.config.name,
                    group = %self.config.consumer_group,
                    "Created consumer group"
                );
            }
            Err(e) => {
                if e.to_string().contains("BUSYGROUP") {
                    debug!(
                        stream = %self.config.name,
                        group = %self.config.consumer_group,
                        "Consumer group already exists"
                    );
                } else {
                    return Err(anyhow!(
                        "Failed to create consumer group {} for stream {}: {}",
                        self.config.consumer_group,
                        self.config.name,
                        e
                    ));
                }
            }
        }

        Ok(())
    }

    /// Acknowledge the last received event if one exists.
    async fn ack_last_event(&self) -> Result<()> {
        let mut last_id = self.last_event_id.lock().await;
        if let Some(ref entry_id) = *last_id {
            let mut conn = self.conn_manager.clone();

            let acked: i64 = redis::cmd("XACK")
                .arg(&self.config.name)
                .arg(&self.config.consumer_group)
                .arg(entry_id)
                .query_async(&mut conn)
                .await
                .with_context(|| {
                    format!(
                        "Failed to XACK entry {} in stream {}",
                        entry_id, self.config.name
                    )
                })?;

            if acked == 1 {
                debug!(
                    stream = %self.config.name,
                    entry_id = %entry_id,
                    "Auto-acknowledged stream entry"
                );
            }

            *last_id = None;
        }
        Ok(())
    }

    /// Parse a stream entry into a ReceivedEvent.
    fn parse_stream_entry(
        &self,
        stream_name: &str,
        entry_id: &str,
        fields: &[(String, Value)],
    ) -> Result<ReceivedEvent> {
        let data = fields
            .iter()
            .find(|(k, _)| k == "data")
            .ok_or_else(|| anyhow!("Stream entry missing 'data' field"))?;

        let json_str = match &data.1 {
            Value::BulkString(bytes) => String::from_utf8_lossy(bytes).to_string(),
            Value::SimpleString(s) => s.clone(),
            _ => return Err(anyhow!("Unexpected data field type: {:?}", data.1)),
        };

        let event: Event = serde_json::from_str(&json_str).with_context(|| {
            format!(
                "Failed to deserialize Event from stream entry {}",
                entry_id
            )
        })?;

        Ok(ReceivedEvent {
            event,
            stream_id: Some(entry_id.to_string()),
            source: stream_name.to_string(),
        })
    }

    /// Internal method to fetch events with a specified block timeout.
    async fn fetch_event(&self, block_ms: u64) -> Result<Option<ReceivedEvent>> {
        let mut conn = self.conn_manager.clone();
        let stream_name = &self.config.name;
        let consumer_group = &self.config.consumer_group;
        let consumer_name = self.config.consumer_name();

        let result: StreamReadReply = redis::cmd("XREADGROUP")
            .arg("GROUP")
            .arg(consumer_group)
            .arg(&consumer_name)
            .arg("COUNT")
            .arg(1) // Only fetch 1 event at a time for pull-based interface
            .arg("BLOCK")
            .arg(block_ms)
            .arg("STREAMS")
            .arg(stream_name)
            .arg(">")
            .query_async(&mut conn)
            .await
            .with_context(|| format!("Failed to XREADGROUP from stream {}", stream_name))?;

        for stream_key in result.keys {
            for entry in stream_key.ids {
                let entry_id = entry.id.clone();
                let fields: Vec<(String, Value)> = entry.map.into_iter().collect();

                match self.parse_stream_entry(&stream_key.key, &entry_id, &fields) {
                    Ok(received_event) => {
                        trace!(
                            stream = %stream_key.key,
                            entry_id = %entry_id,
                            "Received event from Redis stream"
                        );
                        return Ok(Some(received_event));
                    }
                    Err(e) => {
                        warn!(
                            stream = %stream_key.key,
                            entry_id = %entry_id,
                            error = %e,
                            "Failed to parse stream entry, auto-acknowledging to skip"
                        );
                        // Acknowledge unparseable messages to prevent re-delivery
                        let _: Result<i64, _> = redis::cmd("XACK")
                            .arg(stream_name)
                            .arg(consumer_group)
                            .arg(&entry_id)
                            .query_async(&mut conn)
                            .await;
                    }
                }
            }
        }

        Ok(None)
    }
}

#[async_trait]
impl EventReceiver for RedisStreamReceiver {
    async fn next_event(&self) -> Result<ReceivedEvent> {
        // Acknowledge the previous event before fetching the next one
        self.ack_last_event().await?;

        // Block indefinitely until an event is available
        loop {
            // Use a long block timeout and retry if no event (handles edge cases)
            if let Some(event) = self.fetch_event(30000).await? {
                // Store the event ID for acknowledgment on next call
                let mut last_id = self.last_event_id.lock().await;
                *last_id = event.stream_id.clone();
                return Ok(event);
            }
        }
    }

    async fn try_next_event(&self, timeout: Duration) -> Result<Option<ReceivedEvent>> {
        // Acknowledge the previous event before fetching the next one
        self.ack_last_event().await?;

        let block_ms = timeout.as_millis() as u64;
        if let Some(event) = self.fetch_event(block_ms).await? {
            // Store the event ID for acknowledgment on next call
            let mut last_id = self.last_event_id.lock().await;
            *last_id = event.stream_id.clone();
            return Ok(Some(event));
        }

        Ok(None)
    }

    fn name(&self) -> &str {
        "RedisStreamReceiver"
    }
}
