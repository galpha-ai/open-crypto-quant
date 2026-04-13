//! Configuration loading and validation for spot price subscriber

use anyhow::{Context, Result};
use common::config::{MetricsConfig, PubsubConfig, QueueConfig, StreamConfig};
use serde::Deserialize;
use std::path::Path;

/// Top-level configuration structure
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Spot price specific configuration
    pub spot_price: SpotPriceConfig,
    /// Redis configuration
    pub redis: SpotPriceRedisConfig,
    /// Metrics server configuration
    pub metrics: MetricsConfig,
}

/// Spot price WebSocket and subscription configuration
#[derive(Debug, Clone, Deserialize)]
pub struct SpotPriceConfig {
    /// Data source: "rtds" (Polymarket) or "binance"
    #[serde(default = "default_data_source")]
    pub data_source: String,
    /// WebSocket endpoint URL (must be wss://)
    pub wss_endpoint: String,
    /// List of cryptocurrency symbols to subscribe to (e.g., ["BTCUSDT", "ETHUSDT"])
    pub symbols: Vec<String>,
    /// Health monitoring configuration (optional)
    pub health_monitoring: Option<HealthMonitoringConfig>,
}

fn default_data_source() -> String {
    "rtds".to_string()
}

/// Health monitoring configuration
#[derive(Debug, Clone, Deserialize)]
pub struct HealthMonitoringConfig {
    /// Enable health monitoring (if false, skip monitoring)
    pub enabled: bool,
    /// Health check interval in seconds (minimum 10)
    pub check_interval_secs: u64,
    /// Minimum spot price updates per minute to consider healthy
    pub min_updates_per_minute: u64,
    /// Event tracking window in seconds (for rate calculation)
    pub event_tracking_window_secs: u64,
}

/// Spot price specific Redis configuration
#[derive(Debug, Clone, Deserialize)]
pub struct SpotPriceRedisConfig {
    /// Redis connection URL
    pub url: String,
    /// Redis LIST queues for spot price events (LPUSH with LTRIM)
    pub queues: Vec<QueueConfig>,
    /// Redis STREAM configurations (XADD with XTRIM) - optional
    pub streams: Option<Vec<StreamConfig>>,
    /// Redis PUBSUB configuration - optional
    pub pubsub: Option<PubsubConfig>,
}

impl Config {
    /// Load configuration from YAML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .context("Failed to read config file")?;

        // Expand environment variables
        let expanded = shellexpand::env(&content)
            .context("Failed to expand environment variables")?;

        let config: Config = serde_yaml::from_str(&expanded)
            .context("Failed to parse config YAML")?;

        config.validate()?;
        Ok(config)
    }

    /// Validate configuration
    fn validate(&self) -> Result<()> {
        // Validate WebSocket endpoint
        anyhow::ensure!(
            self.spot_price.wss_endpoint.starts_with("wss://"),
            "wss_endpoint must start with wss://"
        );

        // Validate at least one symbol configured
        anyhow::ensure!(
            !self.spot_price.symbols.is_empty(),
            "At least one symbol must be configured"
        );

        // Validate symbol format (alphanumeric only)
        for symbol in &self.spot_price.symbols {
            anyhow::ensure!(
                symbol.chars().all(|c| c.is_ascii_alphanumeric()),
                "Invalid symbol format: '{}' (must be alphanumeric)", symbol
            );
        }

        // Validate health monitoring config if enabled
        if let Some(ref health_config) = self.spot_price.health_monitoring {
            if health_config.enabled {
                anyhow::ensure!(
                    health_config.check_interval_secs >= 10,
                    "check_interval_secs must be at least 10 seconds"
                );

                anyhow::ensure!(
                    health_config.event_tracking_window_secs >= 30,
                    "event_tracking_window_secs must be at least 30 seconds"
                );
            }
        }

        // Validate at least one Redis target configured
        let has_queue = !self.redis.queues.is_empty();
        let has_stream = self.redis.streams.as_ref().map_or(false, |s| !s.is_empty());
        let has_pubsub = self.redis.pubsub.is_some();

        anyhow::ensure!(
            has_queue || has_stream || has_pubsub,
            "At least one Redis target (queue, stream, or pubsub) must be configured"
        );

        // Validate metrics port
        if let Some(port) = self.metrics.port {
            anyhow::ensure!(
                port >= 1024,
                "Metrics port must be >= 1024, got {}", port
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_config() {
        let config = Config {
            spot_price: SpotPriceConfig {
                wss_endpoint: "wss://ws-live-data.polymarket.com".to_string(),
                symbols: vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()],
                health_monitoring: Some(HealthMonitoringConfig {
                    enabled: true,
                    check_interval_secs: 30,
                    min_updates_per_minute: 4,
                    event_tracking_window_secs: 60,
                }),
            },
            redis: SpotPriceRedisConfig {
                url: "redis://localhost:6379".to_string(),
                queues: vec![QueueConfig {
                    name: "spot_price:updates".to_string(),
                    max_length: 10000,
                }],
                streams: None,
                pubsub: None,
            },
            metrics: MetricsConfig {
                port: Some(9093),
            },
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_invalid_wss_endpoint() {
        let config = Config {
            spot_price: SpotPriceConfig {
                wss_endpoint: "ws://insecure-endpoint".to_string(),
                symbols: vec!["BTCUSDT".to_string()],
                health_monitoring: None,
            },
            redis: SpotPriceRedisConfig {
                url: "redis://localhost:6379".to_string(),
                queues: vec![QueueConfig {
                    name: "test".to_string(),
                    max_length: 100,
                }],
                streams: None,
                pubsub: None,
            },
            metrics: MetricsConfig { port: Some(9093) },
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_empty_symbols() {
        let config = Config {
            spot_price: SpotPriceConfig {
                wss_endpoint: "wss://valid-endpoint".to_string(),
                symbols: vec![],
                health_monitoring: None,
            },
            redis: SpotPriceRedisConfig {
                url: "redis://localhost:6379".to_string(),
                queues: vec![QueueConfig {
                    name: "test".to_string(),
                    max_length: 100,
                }],
                streams: None,
                pubsub: None,
            },
            metrics: MetricsConfig { port: Some(9093) },
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_no_redis_targets() {
        let config = Config {
            spot_price: SpotPriceConfig {
                wss_endpoint: "wss://valid-endpoint".to_string(),
                symbols: vec!["BTCUSDT".to_string()],
                health_monitoring: None,
            },
            redis: SpotPriceRedisConfig {
                url: "redis://localhost:6379".to_string(),
                queues: vec![],
                streams: None,
                pubsub: None,
            },
            metrics: MetricsConfig { port: Some(9093) },
        };

        assert!(config.validate().is_err());
    }
}
