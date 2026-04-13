//! Configuration loading and validation for Polymarket subscriber

use anyhow::{Context, Result};
use common::config::{MetricsConfig, PubsubConfig, QueueConfig, StreamConfig};
use serde::Deserialize;
use std::path::Path;

/// Top-level configuration structure
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Polymarket-specific configuration
    pub polymarket: PolymarketConfig,
    /// Redis configuration
    pub redis: PolymarketRedisConfig,
    /// Metrics server configuration
    pub metrics: MetricsConfig,
}

/// Polymarket-specific Redis configuration
#[derive(Debug, Clone, Deserialize)]
pub struct PolymarketRedisConfig {
    /// Redis connection URL
    pub url: String,

    /// Trade event targets
    pub trade_queues: Vec<QueueConfig>,
    pub trade_streams: Option<Vec<StreamConfig>>,
    pub trade_pubsub: Option<PubsubConfig>,

    /// Orderbook event targets (optional)
    pub orderbook_queues: Option<Vec<QueueConfig>>,
    pub orderbook_streams: Option<Vec<StreamConfig>>,
    pub orderbook_pubsub: Option<PubsubConfig>,
}

/// Polymarket WebSocket and subscription configuration
#[derive(Debug, Clone, Deserialize)]
pub struct PolymarketConfig {
    /// WebSocket endpoint URL (must be wss://)
    pub wss_endpoint: String,
    /// List of asset IDs to subscribe to (256-bit integers as decimal strings)
    /// Used when market_discovery.enabled = false
    #[serde(default)]
    pub assets: Vec<String>,
    /// Market discovery configuration (optional)
    pub market_discovery: Option<MarketDiscoveryConfig>,
    /// Health monitoring configuration (optional)
    pub health_monitoring: Option<HealthMonitoringConfig>,
}

/// Market discovery configuration for automatic asset ID subscription
#[derive(Debug, Clone, Deserialize)]
pub struct MarketDiscoveryConfig {
    /// Enable automatic market discovery (if false, use manual assets list)
    pub enabled: bool,
    /// Polymarket Gamma API base URL
    pub api_base_url: String,
    /// Tag ID to filter markets (21 = Crypto)
    pub tag_id: u32,
    /// Ticker patterns to match (e.g., ["btc-updown-15m-", "eth-updown-15m-"])
    pub ticker_patterns: Vec<String>,
    /// Discovery interval in seconds (fetch markets periodically)
    pub discovery_interval_secs: u64,
    /// Maximum number of asset IDs to subscribe (safety limit)
    pub max_subscriptions: usize,
    /// HTTP request timeout in seconds
    pub api_timeout_secs: u64,
    /// Number of retry attempts for failed API requests
    pub api_retry_attempts: u32,
    /// Backoff duration between retries in milliseconds
    pub api_retry_backoff_ms: u64,
}

/// Health monitoring configuration for detecting silent rate limiting
#[derive(Debug, Clone, Deserialize)]
pub struct HealthMonitoringConfig {
    /// Enable health monitoring
    pub enabled: bool,
    /// Health check interval in seconds
    pub check_interval_secs: u64,
    /// Minimum events per minute (exit if below this threshold)
    pub min_events_per_minute: u64,
    /// Event tracking window in seconds (rolling window for rate calculation)
    pub event_tracking_window_secs: u64,
}

impl Config {
    /// Load configuration from YAML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .context("Failed to read config file")?;

        // TODO: Implement environment variable substitution
        let config: Config = serde_yaml::from_str(&content)
            .context("Failed to parse config YAML")?;

        config.validate()?;
        Ok(config)
    }

    /// Validate configuration values
    fn validate(&self) -> Result<()> {
        // Validate WSS endpoint
        anyhow::ensure!(
            self.polymarket.wss_endpoint.starts_with("wss://"),
            "wss_endpoint must start with 'wss://'"
        );

        // Check if market discovery is enabled
        let discovery_enabled = self.polymarket.market_discovery
            .as_ref()
            .map(|d| d.enabled)
            .unwrap_or(false);

        if discovery_enabled {
            // Validate market discovery configuration
            let discovery = self.polymarket.market_discovery.as_ref().unwrap();

            // Validate API base URL is HTTPS
            anyhow::ensure!(
                discovery.api_base_url.starts_with("https://"),
                "market_discovery.api_base_url must start with 'https://'"
            );

            // Validate at least one ticker pattern
            anyhow::ensure!(
                !discovery.ticker_patterns.is_empty(),
                "At least one ticker pattern must be specified when market_discovery.enabled = true"
            );

            // Validate max_subscriptions
            anyhow::ensure!(
                discovery.max_subscriptions > 0 && discovery.max_subscriptions <= 10000,
                "market_discovery.max_subscriptions must be between 1 and 10000"
            );

            // Validate discovery interval
            anyhow::ensure!(
                discovery.discovery_interval_secs >= 60,
                "market_discovery.discovery_interval_secs must be >= 60 to avoid API abuse"
            );
        } else {
            // Manual mode: validate at least one asset configured
            anyhow::ensure!(
                !self.polymarket.assets.is_empty(),
                "At least one asset ID must be configured when market_discovery.enabled = false"
            );

            // Validate asset IDs are non-empty
            for asset_id in &self.polymarket.assets {
                anyhow::ensure!(
                    !asset_id.is_empty(),
                    "Asset IDs must be non-empty strings"
                );
            }
        }

        // Validate health monitoring configuration if provided
        if let Some(health) = &self.polymarket.health_monitoring {
            if health.enabled {
                anyhow::ensure!(
                    health.check_interval_secs >= 10,
                    "health_monitoring.check_interval_secs must be >= 10"
                );
                // Note: min_events_per_minute is u64 so always >= 0
            }
        }

        // Validate metrics port (if specified)
        if let Some(port) = self.metrics.port {
            anyhow::ensure!(
                port >= 1024,
                "Metrics port must be >= 1024"
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_config_validation() {
        // TODO: Add configuration validation tests
    }
}
