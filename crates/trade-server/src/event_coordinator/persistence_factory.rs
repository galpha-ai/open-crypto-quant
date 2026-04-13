//! Factory for creating event persistence components from configuration.
//!
//! This module provides utilities to construct `RedisEventCollector` instances
//! from `EventPersistenceConfig`, handling publisher creation and session ID
//! placeholder substitution.
//!
//! # Example
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use trade_server::config::PaperTradingConfig;
//! use trade_server::event_coordinator::{
//!     create_event_coordinator_from_config,
//!     wrap_coordinator_with_event_persistence,
//! };
//!
//! // Create base event coordinator
//! let coordinator = Arc::new(
//!     create_event_coordinator_from_config(&config.redis, timer_frequency).await?
//! );
//!
//! // Wrap with event persistence if configured
//! let coordinator = wrap_coordinator_with_event_persistence(
//!     coordinator,
//!     &paper_config,
//! ).await?;
//!
//! // Use coordinator with TradeServer
//! let trade_server = TradeServer::new(coordinator, ...);
//! ```

use std::sync::Arc;

use anyhow::{Context, Result};
use tracing::info;
use tx_sub_common::config::{PubsubConfig, QueueConfig, StreamConfig};
use tx_sub_common::publisher::{
    RedisListPublisher, RedisPublisher, RedisPubsubPublisher, RedisStreamPublisher,
};

use crate::config::{EventPersistenceConfig, EventPersistenceMode, PaperTradingConfig};

use super::{
    CapturingEventCoordinator, EventCollector, EventCoordinator, EventFilter, RedisEventCollector,
    RedisEventCollectorConfig,
};

/// Create a RedisEventCollector from configuration.
///
/// This function:
/// 1. Uses the explicit session_id from config, or generates a unique one
/// 2. Creates the appropriate Redis publisher based on mode
/// 3. Configures the event filter
/// 4. Returns a ready-to-use collector
///
/// # Arguments
/// * `config` - Event persistence configuration
///
/// # Returns
/// A configured `RedisEventCollector` instance
pub async fn create_event_collector(
    config: &EventPersistenceConfig,
) -> Result<RedisEventCollector> {
    let session_id = config
        .session_id
        .clone()
        .unwrap_or_else(generate_session_id);
    create_event_collector_with_session(config, session_id).await
}

/// Create a RedisEventCollector with a specific session ID.
///
/// Useful for testing or when you want to control the session ID.
/// The session_id is embedded in each event for correlation, not in the queue name.
pub async fn create_event_collector_with_session(
    config: &EventPersistenceConfig,
    session_id: String,
) -> Result<RedisEventCollector> {
    let key = &config.key;

    info!(
        session_id = %session_id,
        mode = ?config.mode,
        key = %key,
        max_length = config.max_length,
        "Creating event persistence collector"
    );

    // Create the appropriate publisher based on mode
    let collector = match &config.mode {
        EventPersistenceMode::List => {
            let publisher = create_list_publisher(&config.redis_url, &key, config.max_length)
                .await
                .context("Failed to create Redis list publisher")?;
            create_collector(Arc::new(publisher), config, session_id)
        }
        EventPersistenceMode::Stream => {
            let publisher =
                create_stream_publisher(&config.redis_url, &key, Some(config.max_length))
                    .await
                    .context("Failed to create Redis stream publisher")?;
            create_collector(Arc::new(publisher), config, session_id)
        }
        EventPersistenceMode::Pubsub => {
            let publisher = create_pubsub_publisher(&config.redis_url, &key)
                .await
                .context("Failed to create Redis pubsub publisher")?;
            create_collector(Arc::new(publisher), config, session_id)
        }
    };

    Ok(collector)
}

/// Create a Redis list publisher.
async fn create_list_publisher(
    redis_url: &str,
    key: &str,
    max_length: usize,
) -> Result<RedisListPublisher> {
    let queues = vec![QueueConfig {
        name: key.to_string(),
        max_length,
    }];

    RedisListPublisher::new(redis_url, queues).await
}

/// Create a Redis stream publisher.
async fn create_stream_publisher(
    redis_url: &str,
    key: &str,
    max_length: Option<usize>,
) -> Result<RedisStreamPublisher> {
    let streams = vec![StreamConfig {
        name: key.to_string(),
        max_length,
        consumer_group: None,
    }];

    RedisStreamPublisher::new(redis_url, streams).await
}

/// Create a Redis pubsub publisher.
async fn create_pubsub_publisher(redis_url: &str, channel: &str) -> Result<RedisPubsubPublisher> {
    let pubsub_config = PubsubConfig {
        channels: vec![channel.to_string()],
    };

    RedisPubsubPublisher::new(redis_url, pubsub_config).await
}

/// Create the collector with the given publisher.
fn create_collector<P: RedisPublisher + 'static>(
    publisher: Arc<P>,
    config: &EventPersistenceConfig,
    session_id: String,
) -> RedisEventCollector {
    let filter = EventFilter::from(config.filter.clone());

    let collector_config = RedisEventCollectorConfig {
        session_id,
        filter,
        channel_buffer_size: config.channel_buffer_size,
    };

    RedisEventCollector::new(publisher, collector_config)
}

/// Generate a unique session ID.
///
/// Format: `{hostname}-{timestamp}-{uuid_short}`
fn generate_session_id() -> String {
    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());

    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let uuid_short = &uuid::Uuid::new_v4().to_string()[..8];

    format!("{}-{}-{}", hostname, timestamp, uuid_short)
}

/// Wrap an event coordinator with event persistence for paper trading.
///
/// If `paper_config.event_persistence` is configured, wraps the coordinator
/// with a `CapturingEventCoordinator` that publishes events to Redis.
/// Otherwise, returns the coordinator unchanged.
///
/// # Arguments
/// * `coordinator` - The base event coordinator to wrap
/// * `paper_config` - Paper trading configuration (may contain event persistence settings)
///
/// # Returns
/// An `Arc<dyn EventCoordinator>` - either the original coordinator or a wrapped one
///
/// # Example
///
/// ```rust,ignore
/// let base_coordinator = Arc::new(
///     create_event_coordinator_from_config(&config.redis, timer_frequency).await?
/// );
///
/// let coordinator = wrap_coordinator_with_event_persistence(
///     base_coordinator,
///     &paper_trading_config,
/// ).await?;
///
/// let trade_server = TradeServer::new(coordinator, ...);
/// ```
pub async fn wrap_coordinator_with_event_persistence<C: EventCoordinator + 'static>(
    coordinator: Arc<C>,
    paper_config: &PaperTradingConfig,
) -> Result<Arc<dyn EventCoordinator>> {
    match &paper_config.event_persistence {
        Some(persistence_config) => {
            let collector = create_event_collector(persistence_config)
                .await
                .context("Failed to create event collector for paper trading")?;

            info!(
                session_id = %collector.session_id(),
                "Wrapping coordinator with event persistence"
            );

            let capturing_coordinator = CapturingEventCoordinator::new(
                coordinator,
                Arc::new(collector) as Arc<dyn EventCollector>,
            );

            Ok(Arc::new(capturing_coordinator) as Arc<dyn EventCoordinator>)
        }
        None => {
            // No event persistence configured, return original coordinator
            Ok(coordinator as Arc<dyn EventCoordinator>)
        }
    }
}

/// Result of wrapping a coordinator with event persistence.
///
/// Provides access to both the coordinator and the collector (if enabled).
pub struct WrappedCoordinator {
    /// The event coordinator (may be wrapped with capturing)
    pub coordinator: Arc<dyn EventCoordinator>,

    /// The Redis event collector (if event persistence is enabled)
    pub collector: Option<Arc<RedisEventCollector>>,
}

/// Wrap an event coordinator with event persistence, returning both the coordinator and collector.
///
/// This is useful when you need access to the collector for metrics or inspection.
///
/// # Arguments
/// * `coordinator` - The base event coordinator to wrap
/// * `paper_config` - Paper trading configuration (may contain event persistence settings)
///
/// # Returns
/// A `WrappedCoordinator` containing the coordinator and optional collector
pub async fn wrap_coordinator_with_event_persistence_ext<C: EventCoordinator + 'static>(
    coordinator: Arc<C>,
    paper_config: &PaperTradingConfig,
) -> Result<WrappedCoordinator> {
    match &paper_config.event_persistence {
        Some(persistence_config) => {
            let collector = create_event_collector(persistence_config)
                .await
                .context("Failed to create event collector for paper trading")?;

            info!(
                session_id = %collector.session_id(),
                "Wrapping coordinator with event persistence"
            );

            let collector = Arc::new(collector);
            let capturing_coordinator = CapturingEventCoordinator::new(
                coordinator,
                Arc::clone(&collector) as Arc<dyn EventCollector>,
            );

            Ok(WrappedCoordinator {
                coordinator: Arc::new(capturing_coordinator) as Arc<dyn EventCoordinator>,
                collector: Some(collector),
            })
        }
        None => {
            // No event persistence configured, return original coordinator
            Ok(WrappedCoordinator {
                coordinator: coordinator as Arc<dyn EventCoordinator>,
                collector: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EventFilterConfig;

    #[test]
    fn test_generate_session_id() {
        let id1 = generate_session_id();
        let id2 = generate_session_id();

        // Should be unique
        assert_ne!(id1, id2);

        // Should contain timestamp-like pattern
        assert!(id1.contains('-'));
    }

    #[test]
    fn test_default_event_key_is_fixed() {
        let config = EventPersistenceConfig::default();
        // Key should be a fixed value, not containing session_id placeholder
        assert_eq!(config.key, "paper_trading_events");
        assert!(!config.key.contains("{session_id}"));
    }

    #[test]
    fn test_explicit_session_id_from_config() {
        let config = EventPersistenceConfig {
            redis_url: "redis://localhost:6379".to_string(),
            mode: EventPersistenceMode::List,
            key: "paper_trading_events".to_string(),
            max_length: 1000,
            filter: EventFilterConfig::default(),
            channel_buffer_size: 100,
            session_id: Some("my-explicit-session".to_string()),
        };

        assert_eq!(config.session_id, Some("my-explicit-session".to_string()));
    }
}
