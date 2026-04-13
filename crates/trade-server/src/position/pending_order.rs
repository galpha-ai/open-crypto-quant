//! Pending limit order tracking for intent-based position management.
//!
//! This module provides types for tracking limit orders that have been placed
//! on orderbook venues but not yet filled. This enables the reconciliation
//! engine to diff current order state against desired state.
//!
//! ## Order Lifecycle
//!
//! Orders go through the following states:
//!
//! 1. **In-Flight**: Order has been submitted but not yet confirmed by venue
//!    - Tracked in `in_flight_orders` map
//!    - Prevents duplicate orders during reconciliation
//!
//! 2. **Pending**: Order has been confirmed by venue (OrderPlaced event received)
//!    - Tracked in `pending_limit_orders` map
//!    - Subject to fills, cancellation, or expiry
//!
//! 3. **Completed**: Order has been fully filled, cancelled, or expired
//!    - Removed from tracking

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::execution::OrderSide;
use crate::position::ExitMode;

/// Internal tracking of a pending limit order.
///
/// Represents an order that has been placed on a venue's orderbook and is
/// awaiting fill or cancellation. Used by the reconciliation engine to
/// track current order state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingLimitOrder {
    /// Venue-assigned order ID
    pub order_id: String,
    /// Asset identifier (token mint address)
    pub mint: String,
    /// Market/condition ID for orderbook venues
    pub market: Option<String>,
    /// Order side (buy or sell)
    pub side: OrderSide,
    /// Limit price
    pub price: f64,
    /// Original order size
    pub original_size: f64,
    /// Remaining size after partial fills
    pub remaining_size: f64,
    /// When the order was placed
    pub placed_at: DateTime<Utc>,
    /// Signal ID that generated this order (for tracing)
    pub signal_id: Option<String>,
    /// Exit mode for positions created from fills of this order.
    /// Used to propagate StrategyManaged mode from intent-based signals.
    pub exit_mode: Option<ExitMode>,
}

impl PendingLimitOrder {
    /// Create a new PendingLimitOrder.
    pub fn new(
        order_id: String,
        mint: String,
        market: Option<String>,
        side: OrderSide,
        price: f64,
        size: f64,
        placed_at: DateTime<Utc>,
        signal_id: Option<String>,
    ) -> Self {
        Self {
            order_id,
            mint,
            market,
            side,
            price,
            original_size: size,
            remaining_size: size,
            placed_at,
            signal_id,
            exit_mode: None,
        }
    }

    /// Create a new PendingLimitOrder with an exit mode.
    pub fn with_exit_mode(mut self, exit_mode: Option<ExitMode>) -> Self {
        self.exit_mode = exit_mode;
        self
    }

    /// Returns true if this order has been partially filled.
    pub fn is_partially_filled(&self) -> bool {
        self.remaining_size < self.original_size
    }

    /// Returns the filled amount.
    pub fn filled_size(&self) -> f64 {
        self.original_size - self.remaining_size
    }

    /// Returns the fill percentage (0.0 to 1.0).
    pub fn fill_percentage(&self) -> f64 {
        if self.original_size == 0.0 {
            0.0
        } else {
            self.filled_size() / self.original_size
        }
    }

    /// Update remaining size after a partial fill.
    pub fn apply_partial_fill(&mut self, filled_amount: f64) {
        self.remaining_size = (self.remaining_size - filled_amount).max(0.0);
    }

    /// Check if this order matches a desired price level.
    ///
    /// Two levels match if both price and remaining size are equal within tolerance.
    /// Uses 0.01 tolerance to handle floating-point precision issues in price calculations.
    pub fn matches_level(&self, price: f64, size: f64) -> bool {
        (self.price - price).abs() < 0.01 && (self.remaining_size - size).abs() < 0.01
    }

    /// Check if this order has the same price as a desired level (ignoring size).
    pub fn same_price(&self, price: f64) -> bool {
        (self.price - price).abs() < 0.01
    }
}

/// Tracking for an order that has been submitted but not yet confirmed by venue.
///
/// In-flight orders are used to prevent the race condition where multiple intents
/// arrive before the first order is recorded in `pending_limit_orders`. By tracking
/// orders as soon as they are submitted (before execution completes), the
/// reconciliation engine can avoid generating duplicate orders.
///
/// ## Lifecycle
///
/// 1. Created when `spawn_limit_order_execution()` is called
/// 2. Removed when `LimitOrderEvent::OrderPlaced` is received (order moves to pending)
/// 3. Removed on execution failure
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InFlightOrder {
    /// Unique identifier for this in-flight order (UUID generated at submission)
    pub id: String,
    /// Asset identifier (token mint address)
    pub mint: String,
    /// Market/condition ID for orderbook venues
    pub market: Option<String>,
    /// Order side (buy or sell)
    pub side: OrderSide,
    /// Limit price
    pub price: f64,
    /// Order size
    pub size: f64,
    /// When the order was submitted
    pub submitted_at: DateTime<Utc>,
    /// Signal ID that generated this order (for tracing)
    pub signal_id: Option<String>,
}

impl InFlightOrder {
    /// Create a new InFlightOrder.
    pub fn new(
        id: String,
        mint: String,
        market: Option<String>,
        side: OrderSide,
        price: f64,
        size: f64,
        submitted_at: DateTime<Utc>,
        signal_id: Option<String>,
    ) -> Self {
        Self {
            id,
            mint,
            market,
            side,
            price,
            size,
            submitted_at,
            signal_id,
        }
    }

    /// Check if this in-flight order matches a desired price level.
    ///
    /// Two levels match if both price and size are equal within tolerance.
    /// Uses 0.01 tolerance to handle floating-point precision issues in price calculations.
    pub fn matches_level(&self, price: f64, size: f64) -> bool {
        (self.price - price).abs() < 0.01 && (self.size - size).abs() < 0.01
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pending_limit_order_new() {
        let timestamp = Utc::now();
        let order = PendingLimitOrder::new(
            "order123".to_string(),
            "token456".to_string(),
            Some("market789".to_string()),
            OrderSide::Buy,
            0.45,
            100.0,
            timestamp,
            Some("signal101".to_string()),
        );

        assert_eq!(order.order_id, "order123");
        assert_eq!(order.mint, "token456");
        assert_eq!(order.market, Some("market789".to_string()));
        assert_eq!(order.side, OrderSide::Buy);
        assert_eq!(order.price, 0.45);
        assert_eq!(order.original_size, 100.0);
        assert_eq!(order.remaining_size, 100.0);
        assert!(!order.is_partially_filled());
    }

    #[test]
    fn test_partial_fill() {
        let timestamp = Utc::now();
        let mut order = PendingLimitOrder::new(
            "order123".to_string(),
            "token456".to_string(),
            None,
            OrderSide::Sell,
            0.55,
            100.0,
            timestamp,
            None,
        );

        assert!(!order.is_partially_filled());
        assert_eq!(order.filled_size(), 0.0);
        assert_eq!(order.fill_percentage(), 0.0);

        order.apply_partial_fill(30.0);

        assert!(order.is_partially_filled());
        assert_eq!(order.filled_size(), 30.0);
        assert_eq!(order.remaining_size, 70.0);
        assert!((order.fill_percentage() - 0.3).abs() < 1e-9);

        order.apply_partial_fill(70.0);

        assert_eq!(order.remaining_size, 0.0);
        assert_eq!(order.fill_percentage(), 1.0);
    }

    #[test]
    fn test_matches_level() {
        let timestamp = Utc::now();
        let order = PendingLimitOrder::new(
            "order123".to_string(),
            "token456".to_string(),
            None,
            OrderSide::Buy,
            0.45,
            100.0,
            timestamp,
            None,
        );

        assert!(order.matches_level(0.45, 100.0));
        assert!(order.matches_level(0.4500000001, 100.0)); // Within tolerance
        assert!(!order.matches_level(0.46, 100.0)); // Different price
        assert!(!order.matches_level(0.45, 99.0)); // Different size
    }

    #[test]
    fn test_same_price() {
        let timestamp = Utc::now();
        let order = PendingLimitOrder::new(
            "order123".to_string(),
            "token456".to_string(),
            None,
            OrderSide::Buy,
            0.45,
            100.0,
            timestamp,
            None,
        );

        assert!(order.same_price(0.45));
        assert!(order.same_price(0.4500000001)); // Within tolerance
        assert!(!order.same_price(0.46));
    }

    #[test]
    fn test_serialization() {
        let timestamp = DateTime::from_timestamp(1700000000, 0).unwrap();
        let order = PendingLimitOrder::new(
            "order123".to_string(),
            "token456".to_string(),
            Some("market789".to_string()),
            OrderSide::Buy,
            0.65,
            250.0,
            timestamp,
            Some("signal101".to_string()),
        );

        let json = serde_json::to_string(&order).unwrap();
        let deserialized: PendingLimitOrder = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.order_id, order.order_id);
        assert_eq!(deserialized.mint, order.mint);
        assert_eq!(deserialized.market, order.market);
        assert_eq!(deserialized.side, order.side);
        assert_eq!(deserialized.price, order.price);
        assert_eq!(deserialized.original_size, order.original_size);
        assert_eq!(deserialized.remaining_size, order.remaining_size);
        assert_eq!(deserialized.placed_at, order.placed_at);
        assert_eq!(deserialized.signal_id, order.signal_id);
    }

    // === InFlightOrder tests ===

    #[test]
    fn test_in_flight_order_new() {
        let timestamp = Utc::now();
        let order = InFlightOrder::new(
            "flight123".to_string(),
            "token456".to_string(),
            Some("market789".to_string()),
            OrderSide::Buy,
            0.45,
            100.0,
            timestamp,
            Some("signal101".to_string()),
        );

        assert_eq!(order.id, "flight123");
        assert_eq!(order.mint, "token456");
        assert_eq!(order.market, Some("market789".to_string()));
        assert_eq!(order.side, OrderSide::Buy);
        assert_eq!(order.price, 0.45);
        assert_eq!(order.size, 100.0);
        assert_eq!(order.submitted_at, timestamp);
        assert_eq!(order.signal_id, Some("signal101".to_string()));
    }

    #[test]
    fn test_in_flight_order_matches_level() {
        let timestamp = Utc::now();
        let order = InFlightOrder::new(
            "flight123".to_string(),
            "token456".to_string(),
            None,
            OrderSide::Buy,
            0.45,
            100.0,
            timestamp,
            None,
        );

        assert!(order.matches_level(0.45, 100.0));
        assert!(order.matches_level(0.4500000001, 100.0)); // Within tolerance
        assert!(!order.matches_level(0.46, 100.0)); // Different price
        assert!(!order.matches_level(0.45, 99.0)); // Different size
    }

    #[test]
    fn test_in_flight_order_serialization() {
        let timestamp = DateTime::from_timestamp(1700000000, 0).unwrap();
        let order = InFlightOrder::new(
            "flight123".to_string(),
            "token456".to_string(),
            Some("market789".to_string()),
            OrderSide::Sell,
            0.55,
            200.0,
            timestamp,
            Some("signal101".to_string()),
        );

        let json = serde_json::to_string(&order).unwrap();
        let deserialized: InFlightOrder = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, order.id);
        assert_eq!(deserialized.mint, order.mint);
        assert_eq!(deserialized.market, order.market);
        assert_eq!(deserialized.side, order.side);
        assert_eq!(deserialized.price, order.price);
        assert_eq!(deserialized.size, order.size);
        assert_eq!(deserialized.submitted_at, order.submitted_at);
        assert_eq!(deserialized.signal_id, order.signal_id);
    }
}
