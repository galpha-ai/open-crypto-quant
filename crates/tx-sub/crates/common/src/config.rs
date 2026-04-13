use serde::Deserialize;

/// Redis connection and queue configuration
#[derive(Debug, Deserialize, Clone)]
pub struct RedisConfig {
    pub url: String,
    pub queues: Vec<QueueConfig>,
    pub streams: Option<Vec<StreamConfig>>,
    pub pubsub: Option<PubsubConfig>,
}

/// Configuration for a Redis list queue
#[derive(Debug, Deserialize, Clone)]
pub struct QueueConfig {
    pub name: String,
    pub max_length: usize,
}

/// Configuration for a Redis stream
#[derive(Debug, Deserialize, Clone)]
pub struct StreamConfig {
    pub name: String,
    pub max_length: Option<usize>,
    pub consumer_group: Option<String>,
}

/// Configuration for Redis pub/sub channels
#[derive(Debug, Deserialize, Clone)]
pub struct PubsubConfig {
    pub channels: Vec<String>,
}

/// ClickHouse connection and ingestion configuration
#[derive(Debug, Deserialize, Clone)]
pub struct ClickhouseConfig {
    pub url: String,
    pub user: String,
    pub password: String,
    pub db: String,
    pub table: String,
    pub batch_size: usize,
}

/// Metrics server configuration
#[derive(Debug, Deserialize, Clone)]
pub struct MetricsConfig {
    pub port: Option<u16>,
}
