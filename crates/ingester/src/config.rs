use serde::Deserialize;
use crate::error::{IngesterError, Result};

#[derive(Debug, Deserialize)]
pub struct IngesterAppConfig {
    #[allow(dead_code)]
    pub metrics: Option<tx_sub::config::MetricsConfig>,
    pub redis: tx_sub::config::RedisConfig,
    pub clickhouse: tx_sub::config::ClickhouseConfig,
}

impl IngesterAppConfig {
    pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let config_str = std::fs::read_to_string(path)?;
        let mut config: Self = serde_yaml::from_str(&config_str)
            .map_err(|e| IngesterError::Config(format!("Failed to parse config: {}", e)))?;

        // Override ClickHouse username from environment variable if present
        if let Ok(user) = std::env::var("CLICKHOUSE_USER") {
            config.clickhouse.user = user;
        }

        // Override ClickHouse password from environment variable if present
        if let Ok(password) = std::env::var("CLICKHOUSE_PASSWORD") {
            config.clickhouse.password = password;
        }

        Ok(config)
    }
}