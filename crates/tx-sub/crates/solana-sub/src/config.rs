use std::{fs, path::Path};

use anyhow::Result;
use serde::Deserialize;

// Re-export common types for convenience
pub use common::config::{MetricsConfig, PubsubConfig, QueueConfig, RedisConfig, StreamConfig};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    pub grpc: GrpcConfig,
    pub pumpfun: PumpfunConfig,
    pub bonk: Option<BonkConfig>,
    pub metrics: Option<MetricsConfig>,
    pub redis: RedisConfig,
    pub persist_tx_redis: Option<PersistTxRedisConfig>,
    pub test: Option<TestConfig>,
    pub stats_monitor: Option<StatsMonitorConfig>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct PersistTxRedisConfig {
    pub enabled: bool,
    pub environment: String,
    pub ttl_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct ClickhouseConfig {
    pub url: String,
    pub user: String,
    pub password: String,
    pub db: String,
    pub table: String,
    pub batch_size: usize,
}

#[derive(Debug, Deserialize)]
pub struct GrpcConfig {
    pub endpoint: String,
    pub x_token: Option<String>,
    pub program_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct PumpfunConfig {
    pub program_id: String,
    pub rpc_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BonkConfig {
    pub program_id: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TestConfig {
    pub event_count: usize,
    pub print_format: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StatsMonitorConfig {
    pub enabled: bool,
    pub window_seconds: Option<u64>,
    pub log_interval_seconds: Option<u64>,
    pub inactivity_timeout_seconds: Option<u64>,
}

impl AppConfig {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let config_str = fs::read_to_string(path)?;
        let config = serde_yaml::from_str(&config_str)?;
        Ok(config)
    }
}
