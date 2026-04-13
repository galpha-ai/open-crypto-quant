//! Position query abstraction for exchange integrations.
//!
//! This module provides a trait for querying positions from external exchanges,
//! enabling periodic reconciliation between local position state and the
//! authoritative exchange state.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;

/// Represents a position returned from an exchange.
///
/// This is a normalized representation of position data that can be
/// populated from any exchange's API response.
#[derive(Debug, Clone)]
pub struct ExchangePosition {
    /// Asset identifier (mint, token_id, etc.)
    pub asset_id: String,
    /// Market identifier (for exchanges with multiple markets per asset)
    pub market_id: Option<String>,
    /// Net position amount (positive = long, negative = short)
    pub amount: f64,
    /// Average entry price if available
    pub entry_price: Option<f64>,
    /// Timestamp of the position data
    pub timestamp: DateTime<Utc>,
}

impl ExchangePosition {
    /// Create a new ExchangePosition.
    pub fn new(
        asset_id: String,
        market_id: Option<String>,
        amount: f64,
        entry_price: Option<f64>,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            asset_id,
            market_id,
            amount,
            entry_price,
            timestamp,
        }
    }
}

/// Errors that can occur when querying positions.
#[derive(Debug, Error)]
pub enum PositionQueryError {
    /// Error from the exchange API.
    #[error("Exchange API error: {0}")]
    ApiError(String),

    /// Rate limited by the exchange.
    #[error("Rate limited, retry after {retry_after_ms}ms")]
    RateLimited {
        /// Milliseconds to wait before retrying.
        retry_after_ms: u64,
    },

    /// Network error communicating with the exchange.
    #[error("Network error: {0}")]
    NetworkError(String),

    /// Requested asset not found.
    #[error("Asset not found: {0}")]
    AssetNotFound(String),

    /// Authentication failed.
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
}

/// Trait for querying positions from an exchange.
///
/// Implement this trait for each exchange integration to enable
/// position reconciliation.
///
/// # Example
///
/// ```ignore
/// struct MyExchangeQuerier { /* ... */ }
///
/// #[async_trait]
/// impl PositionQuerier for MyExchangeQuerier {
///     async fn query_position(&self, asset_id: &str) -> Result<Option<ExchangePosition>, PositionQueryError> {
///         // Call exchange API...
///     }
///
///     async fn query_all_positions(&self) -> Result<Vec<ExchangePosition>, PositionQueryError> {
///         // Call exchange API...
///     }
///
///     fn exchange_name(&self) -> &str {
///         "my_exchange"
///     }
/// }
/// ```
#[async_trait]
pub trait PositionQuerier: Send + Sync {
    /// Query position for a specific asset.
    ///
    /// Returns `Ok(None)` if no position exists for the asset.
    async fn query_position(
        &self,
        asset_id: &str,
    ) -> Result<Option<ExchangePosition>, PositionQueryError>;

    /// Query all open positions.
    ///
    /// Returns a vector of all positions currently held on the exchange.
    async fn query_all_positions(&self) -> Result<Vec<ExchangePosition>, PositionQueryError>;

    /// Exchange identifier for logging/metrics.
    fn exchange_name(&self) -> &str;
}

/// No-op position querier for testing or when reconciliation is disabled.
///
/// Always returns empty results.
pub struct NoOpPositionQuerier;

#[async_trait]
impl PositionQuerier for NoOpPositionQuerier {
    async fn query_position(
        &self,
        _asset_id: &str,
    ) -> Result<Option<ExchangePosition>, PositionQueryError> {
        Ok(None)
    }

    async fn query_all_positions(&self) -> Result<Vec<ExchangePosition>, PositionQueryError> {
        Ok(vec![])
    }

    fn exchange_name(&self) -> &str {
        "noop"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exchange_position_new() {
        let pos = ExchangePosition::new(
            "token123".to_string(),
            Some("market456".to_string()),
            100.0,
            Some(0.5),
            Utc::now(),
        );

        assert_eq!(pos.asset_id, "token123");
        assert_eq!(pos.market_id, Some("market456".to_string()));
        assert_eq!(pos.amount, 100.0);
        assert_eq!(pos.entry_price, Some(0.5));
    }

    #[tokio::test]
    async fn test_noop_querier() {
        let querier = NoOpPositionQuerier;

        assert_eq!(querier.exchange_name(), "noop");
        assert!(querier.query_position("any").await.unwrap().is_none());
        assert!(querier.query_all_positions().await.unwrap().is_empty());
    }
}
