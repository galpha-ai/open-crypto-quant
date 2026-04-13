use std::pin::Pin;
use std::time::Duration;

use anyhow::{Context, Result};
use async_stream::try_stream;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use popeyes_trading_types::Event;
use redis::Client;
use tokio::sync::mpsc;
use tracing::{debug, error, trace};

use super::config::PubsubSubscriberConfig;
use super::traits::{EventReceiver, ReceivedEvent, RedisSubscriber};

/// Redis pubsub subscriber using SUBSCRIBE.
///
/// Fire-and-forget semantics - messages are not persisted and acknowledgment
/// is a no-op.
pub struct RedisPubsubSubscriber {
    redis_url: String,
    config: PubsubSubscriberConfig,
}

impl RedisPubsubSubscriber {
    /// Create a new pubsub subscriber.
    pub async fn new(redis_url: &str, config: PubsubSubscriberConfig) -> Result<Self> {
        // Validate we can connect
        let redis_client = Client::open(redis_url)?;
        let _ = redis_client.get_connection()?;

        Ok(Self {
            redis_url: redis_url.to_string(),
            config,
        })
    }
}

#[async_trait]
impl RedisSubscriber for RedisPubsubSubscriber {
    async fn subscribe(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ReceivedEvent>> + Send + '_>>> {
        let redis_client = Client::open(self.redis_url.as_str())?;
        let mut pubsub = redis_client.get_async_pubsub().await?;

        // Subscribe to all configured channels
        for channel in &self.config.channels {
            pubsub
                .subscribe(channel)
                .await
                .with_context(|| format!("Failed to subscribe to channel {}", channel))?;
            debug!(channel = %channel, "Subscribed to Redis pubsub channel");
        }

        let stream = try_stream! {
            loop {
                let msg = pubsub.on_message().next().await;

                if let Some(msg) = msg {
                    let channel: String = msg.get_channel_name().to_string();
                    let payload: String = msg.get_payload()
                        .with_context(|| format!("Failed to get payload from channel {}", channel))?;

                    trace!(
                        channel = %channel,
                        data_len = payload.len(),
                        "Received event from Redis pubsub"
                    );

                    let event: Event = serde_json::from_str(&payload)
                        .with_context(|| format!("Failed to deserialize Event from channel {}", channel))?;

                    yield ReceivedEvent {
                        event,
                        stream_id: None,
                        source: channel,
                    };
                }
            }
        };

        Ok(Box::pin(stream))
    }

    async fn acknowledge(&self, _event: &ReceivedEvent) -> Result<()> {
        // No-op for pubsub subscriber - fire-and-forget semantics
        Ok(())
    }

    fn name(&self) -> &str {
        "RedisPubsubSubscriber"
    }
}

/// Pull-based Redis pubsub receiver using SUBSCRIBE.
///
/// Internally maintains an async channel that receives messages from the pubsub
/// connection. Fire-and-forget semantics - messages are not persisted.
pub struct RedisPubsubReceiver {
    /// Channel receiver for incoming events
    receiver: tokio::sync::Mutex<mpsc::Receiver<Result<ReceivedEvent>>>,
}

impl RedisPubsubReceiver {
    /// Create a new pubsub receiver.
    ///
    /// This spawns a background task that subscribes to the configured channels
    /// and forwards messages to an internal channel.
    pub async fn new(redis_url: &str, config: PubsubSubscriberConfig) -> Result<Self> {
        let redis_client = Client::open(redis_url)?;
        let mut pubsub = redis_client.get_async_pubsub().await?;

        // Subscribe to all configured channels
        for channel in &config.channels {
            pubsub
                .subscribe(channel)
                .await
                .with_context(|| format!("Failed to subscribe to channel {}", channel))?;
            debug!(channel = %channel, "Subscribed to Redis pubsub channel");
        }

        // Create a channel to forward messages
        let (tx, rx) = mpsc::channel::<Result<ReceivedEvent>>(100);

        // Spawn a background task to read from pubsub and forward to channel
        tokio::spawn(async move {
            loop {
                let msg = pubsub.on_message().next().await;

                match msg {
                    Some(msg) => {
                        let channel: String = msg.get_channel_name().to_string();
                        let payload_result: Result<String, _> = msg.get_payload();

                        let event_result = match payload_result {
                            Ok(payload) => {
                                trace!(
                                    channel = %channel,
                                    data_len = payload.len(),
                                    "Received event from Redis pubsub"
                                );

                                match serde_json::from_str::<Event>(&payload) {
                                    Ok(event) => Ok(ReceivedEvent {
                                        event,
                                        stream_id: None,
                                        source: channel,
                                    }),
                                    Err(e) => Err(anyhow::anyhow!(
                                        "Failed to deserialize Event: {}",
                                        e
                                    )),
                                }
                            }
                            Err(e) => {
                                Err(anyhow::anyhow!("Failed to get payload from channel: {}", e))
                            }
                        };

                        if tx.send(event_result).await.is_err() {
                            // Receiver dropped, exit the loop
                            debug!("Pubsub receiver channel closed, stopping background task");
                            break;
                        }
                    }
                    None => {
                        // Stream ended, this shouldn't happen normally
                        error!("Pubsub message stream ended unexpectedly");
                        break;
                    }
                }
            }
        });

        Ok(Self {
            receiver: tokio::sync::Mutex::new(rx),
        })
    }
}

#[async_trait]
impl EventReceiver for RedisPubsubReceiver {
    async fn next_event(&self) -> Result<ReceivedEvent> {
        let mut rx = self.receiver.lock().await;
        match rx.recv().await {
            Some(result) => result,
            None => Err(anyhow::anyhow!("Pubsub channel closed")),
        }
    }

    async fn try_next_event(&self, timeout: Duration) -> Result<Option<ReceivedEvent>> {
        let mut rx = self.receiver.lock().await;
        match tokio::time::timeout(timeout, rx.recv()).await {
            Ok(Some(result)) => result.map(Some),
            Ok(None) => Err(anyhow::anyhow!("Pubsub channel closed")),
            Err(_) => Ok(None), // Timeout
        }
    }

    fn name(&self) -> &str {
        "RedisPubsubReceiver"
    }
}
