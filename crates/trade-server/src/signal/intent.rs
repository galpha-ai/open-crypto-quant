//! Intent-based signal types for orderbook trading strategies.
//!
//! This module provides types for expressing desired order book state rather than
//! discrete actions. Strategies emit `OrderIntent` signals that describe the
//! desired quotes, and the PositionManager reconciles current state with desired
//! state to generate the necessary orders.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::execution::TimeInForce;
use crate::signal::redemption::RedemptionPolicy;

/// Desired order book state from strategy.
///
/// Instead of emitting discrete "place order" or "cancel order" signals,
/// strategies express their desired state via `OrderIntent`. The PositionManager
/// compares this against current pending orders and generates the necessary
/// actions (cancels and placements) to reach the desired state.
///
/// # Option Semantics
///
/// The `bids` and `asks` fields use `Option` semantics:
/// - `None`: No change to this side (preserve existing orders)
/// - `Some([])`: Cancel all orders on this side
/// - `Some([levels...])`: Desired state for this side (reconcile to match)
///
/// # Example
///
/// ```ignore
/// use trade_server::signal::OrderIntent;
///
/// // Express desire to have a single bid at 0.45 and a single ask at 0.55
/// let intent = OrderIntent {
///     mint: "token123".to_string(),
///     market: Some("market456".to_string()),
///     bids: Some(vec![QuoteLevel { price: 0.45, size: 100.0, time_in_force: TimeInForce::GoodTilCancelled }]),
///     asks: Some(vec![QuoteLevel { price: 0.55, size: 100.0, time_in_force: TimeInForce::GoodTilCancelled }]),
///     signal_id: "signal789".to_string(),
///     timestamp: Utc::now(),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderIntent {
    /// Token mint address (asset identifier)
    pub mint: String,
    /// Market/condition ID for orderbook venues (e.g., Polymarket condition_id)
    /// None for AMM venues where mint alone identifies the market
    pub market: Option<String>,
    /// Desired bid orders. None = unchanged, Some([]) = cancel all
    pub bids: Option<Vec<QuoteLevel>>,
    /// Desired ask orders. None = unchanged, Some([]) = cancel all
    pub asks: Option<Vec<QuoteLevel>>,
    /// Signal ID for tracing and debugging
    pub signal_id: String,
    /// Timestamp of intent generation
    pub timestamp: DateTime<Utc>,
    /// Optional debug context data from the signal (e.g., fair price, inventory state).
    /// Propagated to OrderPlaced events for debugging and analysis.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,

    /// Optional redemption policy for binary market pairs.
    /// When set, position manager will auto-redeem pairs that exceed the threshold.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redemption_policy: Option<RedemptionPolicy>,
}

impl OrderIntent {
    /// Create a new OrderIntent with specified bid and ask levels.
    pub fn new(
        mint: String,
        market: Option<String>,
        bids: Option<Vec<QuoteLevel>>,
        asks: Option<Vec<QuoteLevel>>,
        signal_id: String,
        timestamp: DateTime<Utc>,
        context: Option<serde_json::Value>,
    ) -> Self {
        Self {
            mint,
            market,
            bids,
            asks,
            signal_id,
            timestamp,
            context,
            redemption_policy: None,
        }
    }

    /// Create a new OrderIntent with a redemption policy.
    pub fn with_redemption_policy(mut self, policy: RedemptionPolicy) -> Self {
        self.redemption_policy = Some(policy);
        self
    }

    /// Create an intent to cancel all orders on both sides.
    pub fn cancel_all(
        mint: String,
        market: Option<String>,
        signal_id: String,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            mint,
            market,
            bids: Some(vec![]),
            asks: Some(vec![]),
            signal_id,
            timestamp,
            context: None,
            redemption_policy: None,
        }
    }

    /// Returns the total number of desired bid levels.
    pub fn bid_count(&self) -> usize {
        self.bids.as_ref().map(|b| b.len()).unwrap_or(0)
    }

    /// Returns the total number of desired ask levels.
    pub fn ask_count(&self) -> usize {
        self.asks.as_ref().map(|a| a.len()).unwrap_or(0)
    }

    /// Returns true if this intent would cancel all bids.
    pub fn cancels_all_bids(&self) -> bool {
        matches!(&self.bids, Some(levels) if levels.is_empty())
    }

    /// Returns true if this intent would cancel all asks.
    pub fn cancels_all_asks(&self) -> bool {
        matches!(&self.asks, Some(levels) if levels.is_empty())
    }
}

/// A single price level in desired order state.
///
/// Represents one order at a specific price and size that the strategy
/// desires to have on the book.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuoteLevel {
    /// Limit price for the order
    pub price: f64,
    /// Order size in base units
    pub size: f64,
    /// Time-in-force for the order
    pub time_in_force: TimeInForce,
}

impl QuoteLevel {
    /// Create a new QuoteLevel.
    pub fn new(price: f64, size: f64, time_in_force: TimeInForce) -> Self {
        Self {
            price,
            size,
            time_in_force,
        }
    }

    /// Create a new QuoteLevel with GoodTilCancelled time-in-force.
    pub fn gtc(price: f64, size: f64) -> Self {
        Self {
            price,
            size,
            time_in_force: TimeInForce::GoodTilCancelled,
        }
    }

    /// Check if this level matches another level within tolerance.
    ///
    /// Two levels match if both price and size are equal within 1e-9 tolerance.
    pub fn matches(&self, other: &QuoteLevel) -> bool {
        (self.price - other.price).abs() < 1e-9 && (self.size - other.size).abs() < 1e-9
    }

    /// Check if this level has the same price as another (ignoring size).
    pub fn same_price(&self, other: &QuoteLevel) -> bool {
        (self.price - other.price).abs() < 1e-9
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_intent_new() {
        let timestamp = Utc::now();
        let intent = OrderIntent::new(
            "token123".to_string(),
            Some("market456".to_string()),
            Some(vec![QuoteLevel::gtc(0.45, 100.0)]),
            Some(vec![QuoteLevel::gtc(0.55, 100.0)]),
            "signal789".to_string(),
            timestamp,
            None,
        );

        assert_eq!(intent.mint, "token123");
        assert_eq!(intent.market, Some("market456".to_string()));
        assert_eq!(intent.bid_count(), 1);
        assert_eq!(intent.ask_count(), 1);
        assert_eq!(intent.signal_id, "signal789");
    }

    #[test]
    fn test_order_intent_cancel_all() {
        let timestamp = Utc::now();
        let intent = OrderIntent::cancel_all(
            "token123".to_string(),
            None,
            "signal789".to_string(),
            timestamp,
        );

        assert!(intent.cancels_all_bids());
        assert!(intent.cancels_all_asks());
        assert_eq!(intent.bid_count(), 0);
        assert_eq!(intent.ask_count(), 0);
    }

    #[test]
    fn test_order_intent_none_preserves() {
        let timestamp = Utc::now();
        let intent = OrderIntent {
            mint: "token123".to_string(),
            market: None,
            bids: None, // Preserve existing bids
            asks: Some(vec![QuoteLevel::gtc(0.55, 100.0)]),
            signal_id: "signal789".to_string(),
            timestamp,
            context: None,
            redemption_policy: None,
        };

        assert!(!intent.cancels_all_bids());
        assert!(!intent.cancels_all_asks());
        assert_eq!(intent.bid_count(), 0); // None returns 0 for count
        assert_eq!(intent.ask_count(), 1);
    }

    #[test]
    fn test_quote_level_matches() {
        let level1 = QuoteLevel::gtc(0.45, 100.0);
        let level2 = QuoteLevel::gtc(0.45, 100.0);
        let level3 = QuoteLevel::gtc(0.45, 100.0000000001); // Within 1e-9 tolerance
        let level4 = QuoteLevel::gtc(0.46, 100.0); // Different price
        let level5 = QuoteLevel::gtc(0.45, 100.0001); // Outside tolerance

        assert!(level1.matches(&level2));
        assert!(level1.matches(&level3));
        assert!(!level1.matches(&level4));
        assert!(!level1.matches(&level5)); // Difference is 0.0001 > 1e-9
    }

    #[test]
    fn test_quote_level_same_price() {
        let level1 = QuoteLevel::gtc(0.45, 100.0);
        let level2 = QuoteLevel::gtc(0.45, 200.0); // Same price, different size

        assert!(level1.same_price(&level2));
        assert!(!level1.matches(&level2)); // But not matching due to size
    }

    #[test]
    fn test_order_intent_serialization() {
        let timestamp = DateTime::from_timestamp(1700000000, 0).unwrap();
        let intent = OrderIntent::new(
            "token123".to_string(),
            Some("market456".to_string()),
            Some(vec![QuoteLevel::gtc(0.45, 100.0)]),
            None,
            "signal789".to_string(),
            timestamp,
            None,
        );

        let json = serde_json::to_string(&intent).unwrap();
        let deserialized: OrderIntent = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.mint, intent.mint);
        assert_eq!(deserialized.market, intent.market);
        assert_eq!(deserialized.signal_id, intent.signal_id);
        assert_eq!(deserialized.bid_count(), 1);
        assert!(deserialized.asks.is_none());
    }

    #[test]
    fn test_quote_level_serialization() {
        let level = QuoteLevel::new(0.65, 250.0, TimeInForce::ImmediateOrCancel);

        let json = serde_json::to_string(&level).unwrap();
        let deserialized: QuoteLevel = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.price, 0.65);
        assert_eq!(deserialized.size, 250.0);
        assert_eq!(deserialized.time_in_force, TimeInForce::ImmediateOrCancel);
    }
}
