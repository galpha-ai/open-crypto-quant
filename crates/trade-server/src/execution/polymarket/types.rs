//! Types for the Polymarket executor.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::execution::events::{OrderSide, TimeInForce};

/// A pending limit order awaiting fill.
///
/// Tracks the state of a limit order that has been placed on Polymarket
/// but not yet fully filled or cancelled.
#[derive(Debug, Clone)]
pub struct PendingOrder {
    /// Unique order ID assigned by Polymarket
    pub order_id: String,

    /// Asset identifier (token_id)
    pub mint: String,

    /// Market/condition ID
    pub market: Option<String>,

    /// Order side (Buy or Sell)
    pub side: OrderSide,

    /// Limit price
    pub price: f64,

    /// Original order size
    pub original_size: f64,

    /// Remaining unfilled size
    pub remaining_size: f64,

    /// Amount filled so far (tracked for WebSocket updates)
    pub last_known_filled: f64,

    /// When the order was placed
    pub placed_at: DateTime<Utc>,

    /// Time-in-force constraint
    pub time_in_force: TimeInForce,

    /// Signal ID that originated this order
    pub signal_id: Option<String>,
}

impl PendingOrder {
    /// Create a new pending order
    pub fn new(
        order_id: String,
        mint: String,
        market: Option<String>,
        side: OrderSide,
        price: f64,
        size: f64,
        time_in_force: TimeInForce,
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
            last_known_filled: 0.0,
            placed_at: Utc::now(),
            time_in_force,
            signal_id,
        }
    }

    /// Check if the order is fully filled
    pub fn is_filled(&self) -> bool {
        self.remaining_size <= 0.0
    }

    /// Update the filled amount and return the new fill size
    pub fn update_filled(&mut self, total_filled: f64) -> f64 {
        let new_fill = total_filled - self.last_known_filled;
        if new_fill > 0.0 {
            self.last_known_filled = total_filled;
            self.remaining_size = self.original_size - total_filled;
        }
        new_fill
    }
}

/// Response from the User Channel WebSocket for order updates
/// TODO: Used for WebSocket fill detection (not yet implemented)
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct UserOrderUpdate {
    /// Order ID
    pub id: String,

    /// Update type: PLACEMENT, UPDATE, CANCELLATION
    #[serde(rename = "type")]
    pub update_type: String,

    /// Order status: LIVE, MATCHED, CANCELLED
    #[serde(default)]
    pub status: Option<String>,

    /// Total amount filled
    #[serde(default)]
    pub size_matched: Option<f64>,

    /// Original order size
    #[serde(default)]
    pub original_size: Option<f64>,

    /// Order price
    #[serde(default)]
    pub price: Option<f64>,

    /// Order side
    #[serde(default)]
    pub side: Option<String>,

    /// Asset ID
    #[serde(default)]
    pub asset_id: Option<String>,

    /// Associated trade IDs
    #[serde(default)]
    pub associate_trades: Option<Vec<String>>,
}

/// Response from the User Channel WebSocket for trade events
/// TODO: Used for WebSocket fill detection (not yet implemented)
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct UserTradeUpdate {
    /// Taker order ID
    #[serde(default)]
    pub taker_order_id: Option<String>,

    /// Maker orders involved
    #[serde(default)]
    pub maker_orders: Option<Vec<MakerOrderFill>>,

    /// Trade status
    #[serde(default)]
    pub status: Option<String>,

    /// Trade price
    #[serde(default)]
    pub price: Option<f64>,

    /// Trade size
    #[serde(default)]
    pub size: Option<f64>,
}

/// Fill information for a maker order
/// TODO: Used for WebSocket fill detection (not yet implemented)
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct MakerOrderFill {
    /// Order ID
    pub order_id: String,

    /// Amount matched
    pub matched_amount: f64,
}

/// Position data from Polymarket Data API
#[derive(Debug, Clone, Deserialize)]
pub struct DataApiPosition {
    /// Asset/token ID
    pub asset: String,

    /// Position size
    pub size: f64,

    /// Average entry price (if available)
    #[serde(rename = "avgPrice")]
    pub avg_price: Option<f64>,

    /// Current price
    #[serde(rename = "curPrice")]
    pub cur_price: Option<f64>,

    /// Profit/Loss
    pub pnl: Option<f64>,

    /// Market title
    pub title: Option<String>,

    /// Outcome (Up/Down)
    pub outcome: Option<String>,

    /// Condition ID
    #[serde(rename = "conditionId")]
    pub condition_id: Option<String>,
}

/// Metrics for the Polymarket executor
#[derive(Debug, Clone, Default, Serialize)]
pub struct PolymarketMetrics {
    /// Total orders placed
    pub orders_placed: u64,

    /// Total orders filled
    pub orders_filled: u64,

    /// Total orders cancelled
    pub orders_cancelled: u64,

    /// Total orders rejected
    pub orders_rejected: u64,

    /// WebSocket reconnection count
    pub ws_reconnects: u64,

    /// API error count
    pub api_errors: u64,
}
