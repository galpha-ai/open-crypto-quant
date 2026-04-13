//! Types for the order status poller.

use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::execution::events::OrderSide;

/// An order being monitored for fills.
#[derive(Debug, Clone)]
pub struct MonitoredOrder {
    /// Polymarket order ID
    pub order_id: String,

    /// Token/asset ID (mint)
    pub mint: String,

    /// Market identifier (condition_id)
    pub market: Option<String>,

    /// Order side
    pub side: OrderSide,

    /// Limit price
    pub price: f64,

    /// Original order size
    pub original_size: f64,

    /// Last known filled amount (for delta detection)
    pub last_known_filled: f64,

    /// Signal ID that triggered this order
    pub signal_id: Option<String>,

    /// Time order was added to monitoring
    pub added_at: Instant,

    /// Last time this order was polled
    pub last_polled: Option<Instant>,

    /// Number of times polled
    pub poll_count: u32,

    /// Consecutive poll failures
    pub consecutive_failures: u32,
}

impl MonitoredOrder {
    /// Create a new monitored order.
    pub fn new(
        order_id: String,
        mint: String,
        market: Option<String>,
        side: OrderSide,
        price: f64,
        original_size: f64,
        signal_id: Option<String>,
    ) -> Self {
        Self {
            order_id,
            mint,
            market,
            side,
            price,
            original_size,
            last_known_filled: 0.0,
            signal_id,
            added_at: Instant::now(),
            last_polled: None,
            poll_count: 0,
            consecutive_failures: 0,
        }
    }

    /// Time since order was added.
    pub fn age(&self) -> Duration {
        self.added_at.elapsed()
    }

    /// Time since last poll (None if never polled).
    pub fn time_since_poll(&self) -> Option<Duration> {
        self.last_polled.map(|t| t.elapsed())
    }

    /// Remaining unfilled size.
    pub fn remaining_size(&self) -> f64 {
        self.original_size - self.last_known_filled
    }

    /// Whether order is fully filled.
    pub fn is_fully_filled(&self) -> bool {
        self.remaining_size() <= 0.0
    }

    /// Update after a successful poll.
    pub fn record_poll_success(&mut self, filled: f64) {
        self.last_polled = Some(Instant::now());
        self.poll_count += 1;
        self.consecutive_failures = 0;
        if filled > self.last_known_filled {
            self.last_known_filled = filled;
        }
    }

    /// Update after a failed poll.
    pub fn record_poll_failure(&mut self) {
        self.last_polled = Some(Instant::now());
        self.poll_count += 1;
        self.consecutive_failures += 1;
    }

    /// Calculate priority for polling.
    /// Lower values = higher priority.
    /// Prioritizes: newer orders, never-polled orders, healthy orders.
    pub fn poll_priority(&self) -> (Instant, Instant, u32) {
        (
            self.added_at,
            self.last_polled.unwrap_or(Instant::now()),
            self.consecutive_failures,
        )
    }
}

/// Result of a single poll cycle.
#[derive(Debug, Default, Clone)]
pub struct PollCycleResult {
    /// Number of orders polled
    pub orders_polled: usize,

    /// Fill events detected and enqueued
    pub fills_detected: usize,

    /// Orders completed (fully filled or cancelled)
    pub orders_completed: usize,

    /// Poll errors encountered
    pub errors: usize,

    /// Duration of the poll cycle
    pub duration: Duration,
}

impl PollCycleResult {
    /// Create a new poll cycle result.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a successful poll with optional fill.
    pub fn record_poll(&mut self, had_fill: bool) {
        self.orders_polled += 1;
        if had_fill {
            self.fills_detected += 1;
        }
    }

    /// Record a completed order.
    pub fn record_completion(&mut self) {
        self.orders_completed += 1;
    }

    /// Record a poll error.
    pub fn record_error(&mut self) {
        self.errors += 1;
    }

    /// Set the duration.
    pub fn set_duration(&mut self, duration: Duration) {
        self.duration = duration;
    }
}

/// Result of position sync.
#[derive(Debug, Clone)]
pub struct PositionSyncResult {
    /// Position snapshots from API
    pub positions: Vec<PositionSnapshot>,

    /// Available quote balance
    pub available_quote: f64,

    /// Timestamp of sync
    pub timestamp: DateTime<Utc>,

    /// Whether any drift was detected vs local state
    pub drift_detected: bool,
}

impl PositionSyncResult {
    /// Create a new position sync result.
    pub fn new(
        positions: Vec<PositionSnapshot>,
        available_quote: f64,
        drift_detected: bool,
    ) -> Self {
        Self {
            positions,
            available_quote,
            timestamp: Utc::now(),
            drift_detected,
        }
    }
}

/// Snapshot of a position from API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionSnapshot {
    /// Token/asset ID
    pub asset_id: String,

    /// Current position size
    pub amount: f64,

    /// Average entry price (if available)
    pub avg_price: Option<f64>,
}

impl PositionSnapshot {
    /// Create a new position snapshot.
    pub fn new(asset_id: String, amount: f64, avg_price: Option<f64>) -> Self {
        Self {
            asset_id,
            amount,
            avg_price,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monitored_order_new() {
        let order = MonitoredOrder::new(
            "order-123".to_string(),
            "token-abc".to_string(),
            Some("market-xyz".to_string()),
            OrderSide::Buy,
            0.5,
            100.0,
            Some("signal-1".to_string()),
        );

        assert_eq!(order.order_id, "order-123");
        assert_eq!(order.mint, "token-abc");
        assert_eq!(order.market, Some("market-xyz".to_string()));
        assert_eq!(order.side, OrderSide::Buy);
        assert_eq!(order.price, 0.5);
        assert_eq!(order.original_size, 100.0);
        assert_eq!(order.last_known_filled, 0.0);
        assert_eq!(order.poll_count, 0);
        assert_eq!(order.consecutive_failures, 0);
        assert!(order.last_polled.is_none());
    }

    #[test]
    fn test_monitored_order_remaining_size() {
        let mut order = MonitoredOrder::new(
            "order-123".to_string(),
            "token-abc".to_string(),
            None,
            OrderSide::Buy,
            0.5,
            100.0,
            None,
        );

        assert_eq!(order.remaining_size(), 100.0);
        assert!(!order.is_fully_filled());

        order.last_known_filled = 50.0;
        assert_eq!(order.remaining_size(), 50.0);
        assert!(!order.is_fully_filled());

        order.last_known_filled = 100.0;
        assert_eq!(order.remaining_size(), 0.0);
        assert!(order.is_fully_filled());
    }

    #[test]
    fn test_monitored_order_poll_success() {
        let mut order = MonitoredOrder::new(
            "order-123".to_string(),
            "token-abc".to_string(),
            None,
            OrderSide::Buy,
            0.5,
            100.0,
            None,
        );

        order.record_poll_success(25.0);
        assert_eq!(order.poll_count, 1);
        assert_eq!(order.consecutive_failures, 0);
        assert_eq!(order.last_known_filled, 25.0);
        assert!(order.last_polled.is_some());

        order.record_poll_success(50.0);
        assert_eq!(order.poll_count, 2);
        assert_eq!(order.last_known_filled, 50.0);
    }

    #[test]
    fn test_monitored_order_poll_failure() {
        let mut order = MonitoredOrder::new(
            "order-123".to_string(),
            "token-abc".to_string(),
            None,
            OrderSide::Buy,
            0.5,
            100.0,
            None,
        );

        order.record_poll_failure();
        assert_eq!(order.poll_count, 1);
        assert_eq!(order.consecutive_failures, 1);

        order.record_poll_failure();
        assert_eq!(order.poll_count, 2);
        assert_eq!(order.consecutive_failures, 2);

        // Success resets consecutive failures
        order.record_poll_success(0.0);
        assert_eq!(order.consecutive_failures, 0);
    }

    #[test]
    fn test_poll_cycle_result() {
        let mut result = PollCycleResult::new();

        result.record_poll(false);
        assert_eq!(result.orders_polled, 1);
        assert_eq!(result.fills_detected, 0);

        result.record_poll(true);
        assert_eq!(result.orders_polled, 2);
        assert_eq!(result.fills_detected, 1);

        result.record_completion();
        assert_eq!(result.orders_completed, 1);

        result.record_error();
        assert_eq!(result.errors, 1);
    }

    #[test]
    fn test_position_snapshot() {
        let snapshot = PositionSnapshot::new("asset-123".to_string(), 100.0, Some(0.65));

        assert_eq!(snapshot.asset_id, "asset-123");
        assert_eq!(snapshot.amount, 100.0);
        assert_eq!(snapshot.avg_price, Some(0.65));
    }
}
