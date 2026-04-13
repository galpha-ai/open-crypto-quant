use anyhow::Result;
use tx_sub_common::subscriber::{
    EventReceiver, ListSubscriberConfig, PubsubSubscriberConfig, RedisListReceiver,
    RedisPubsubReceiver, RedisStreamReceiver, StreamSubscriberConfig,
};

use crate::config::{RedisConfig, RedisSubscriberType};
use crate::event_source::{EventReceiverSource, EventSource};

/// Create an EventSource from Redis configuration.
///
/// This factory function creates the appropriate EventSource implementation based on
/// the `subscriber` configuration in `RedisConfig`. It supports three modes:
///
/// - **List** (default): Uses Redis LIST with BRPOP for simple queue semantics
/// - **Stream**: Uses Redis STREAM with consumer groups for at-least-once delivery
/// - **Pubsub**: Uses Redis PUBSUB for real-time broadcast (fire-and-forget)
///
/// # Example
///
/// ```ignore
/// let config = RedisConfig {
///     url: "redis://localhost:6379".to_string(),
///     token_events_key: "token_events".to_string(),
///     signal_persistence_key: None,
///     subscriber: RedisSubscriberType::Stream {
///         consumer_group: "trade-server".to_string(),
///         consumer_name: Some("instance-1".to_string()),
///         block_ms: Some(5000),
///         count: Some(10),
///     },
/// };
///
/// let event_source = create_event_source(&config).await?;
/// ```
pub async fn create_event_source(config: &RedisConfig) -> Result<Box<dyn EventSource>> {
    let receiver: Box<dyn EventReceiver> = match &config.subscriber {
        RedisSubscriberType::List { timeout_secs } => {
            tracing::info!(
                queue = %config.token_events_key,
                timeout_secs = ?timeout_secs,
                "Creating Redis LIST event source"
            );
            let receiver = RedisListReceiver::new(
                &config.url,
                ListSubscriberConfig {
                    name: config.token_events_key.clone(),
                    timeout_secs: *timeout_secs,
                },
            )
            .await?;
            Box::new(receiver)
        }
        RedisSubscriberType::Stream {
            consumer_group,
            consumer_name,
            block_ms,
            count,
        } => {
            tracing::info!(
                stream = %config.token_events_key,
                consumer_group = %consumer_group,
                consumer_name = ?consumer_name,
                block_ms = ?block_ms,
                count = ?count,
                "Creating Redis STREAM event source"
            );
            let receiver = RedisStreamReceiver::new(
                &config.url,
                StreamSubscriberConfig {
                    name: config.token_events_key.clone(),
                    consumer_group: consumer_group.clone(),
                    consumer_name: consumer_name.clone(),
                    block_ms: *block_ms,
                    count: *count,
                },
            )
            .await?;
            Box::new(receiver)
        }
        RedisSubscriberType::Pubsub { channels } => {
            tracing::info!(
                channels = ?channels,
                "Creating Redis PUBSUB event source"
            );
            let receiver = RedisPubsubReceiver::new(
                &config.url,
                PubsubSubscriberConfig {
                    channels: channels.clone(),
                },
            )
            .await?;
            Box::new(receiver)
        }
    };

    Ok(Box::new(EventReceiverSource::new(receiver)))
}
