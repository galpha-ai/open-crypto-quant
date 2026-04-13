use thiserror::Error;

#[derive(Error, Debug)]
pub enum IngesterError {
    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),
    
    #[error("ClickHouse error: {0}")]
    ClickHouse(#[from] clickhouse::error::Error),
    
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, IngesterError>;