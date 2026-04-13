//! Error types for backtest operations.

use thiserror::Error;

/// Errors that can occur during backtest operations.
#[derive(Error, Debug)]
pub enum BacktestError {
    /// Failed to read or parse Parquet file
    #[error("Failed to read Parquet file: {0}")]
    ParquetReadError(String),

    /// Parquet schema doesn't match expected columns
    #[error("Invalid schema: {0}")]
    InvalidSchemaError(String),

    /// No events found in Parquet file
    #[error("No events found in data: {0}")]
    EmptyDataError(String),

    /// Failed to merge events chronologically
    #[error("Timeline error: {0}")]
    TimelineError(String),

    /// Data completeness check failed
    #[error("Data completeness check failed: {incomplete_count} incomplete markets ({details})")]
    DataCompletenessError {
        /// Number of incomplete markets
        incomplete_count: usize,
        /// Details about incomplete markets
        details: String,
    },

    /// IO error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// JSON parsing error
    #[error("JSON parsing error: {0}")]
    JsonError(#[from] serde_json::Error),
}

impl From<parquet::errors::ParquetError> for BacktestError {
    fn from(err: parquet::errors::ParquetError) -> Self {
        BacktestError::ParquetReadError(err.to_string())
    }
}

impl From<arrow::error::ArrowError> for BacktestError {
    fn from(err: arrow::error::ArrowError) -> Self {
        BacktestError::ParquetReadError(err.to_string())
    }
}
