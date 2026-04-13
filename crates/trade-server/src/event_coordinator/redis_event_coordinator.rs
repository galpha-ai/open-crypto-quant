use anyhow::Result;
use chrono::TimeDelta;

use super::GenericEventCoordinator;
use crate::config::RedisConfig;
use crate::event_source::{RedisEventSource, create_event_source};

/// Create an event coordinator from Redis URL and queue key.
///
/// This is the legacy factory that uses the default LIST-based subscriber.
/// For new code, prefer `create_event_coordinator_from_config` which supports
/// all subscriber types (List, Stream, Pubsub).
pub async fn create_redis_event_coordinator(
    redis_url: &str,
    queue_key: String,
    frequency: TimeDelta,
) -> Result<GenericEventCoordinator> {
    let redis_source = RedisEventSource::new(redis_url, queue_key).await?;
    Ok(GenericEventCoordinator::new(
        Box::new(redis_source),
        frequency,
    ))
}

/// Create an event coordinator from Redis configuration.
///
/// This factory creates the appropriate event source based on the subscriber
/// configuration in `RedisConfig`, supporting:
/// - **List** (default): Uses Redis LIST with BRPOP
/// - **Stream**: Uses Redis STREAM with consumer groups
/// - **Pubsub**: Uses Redis PUBSUB for real-time broadcast
///
/// # Example
///
/// ```ignore
/// use trade_server::config::RedisConfig;
/// use trade_server::event_coordinator::create_event_coordinator_from_config;
///
/// let config = RedisConfig {
///     url: "redis://localhost:6379".to_string(),
///     token_events_key: "token_events".to_string(),
///     signal_persistence_key: None,
///     subscriber: Default::default(), // Uses List mode
/// };
///
/// let event_coordinator = create_event_coordinator_from_config(
///     &config,
///     chrono::Duration::seconds(60),
/// ).await?;
/// ```
pub async fn create_event_coordinator_from_config(
    config: &RedisConfig,
    frequency: TimeDelta,
) -> Result<GenericEventCoordinator> {
    let event_source = create_event_source(config).await?;
    Ok(GenericEventCoordinator::new(event_source, frequency))
}
