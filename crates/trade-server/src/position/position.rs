use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use solana_sdk::signature::Signature;

use crate::position::events::OrderSide;

/// Active exit order tracking for CLOB markets
///
/// Tracks a limit order that has been placed to exit a position.
/// Only one active exit order per position is supported.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveExitOrder {
    /// Venue-assigned order ID
    pub order_id: String,
    /// Limit price for the order
    pub price: f64,
    /// Order size (token amount)
    pub size: f64,
    /// When the order was placed
    pub placed_at: DateTime<Utc>,
    /// Order side (typically Sell for long positions)
    pub side: OrderSide,
}

/// Exit management mode for a position
///
/// Determines how exit decisions are made:
/// - Automatic: ExitStrategy controls timing, market orders generated automatically
/// - StrategyManaged: SignalGenerator emits Exit signals with custom logic
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ExitMode {
    /// ExitStrategy decides exit timing, market orders generated (current default behavior)
    #[default]
    Automatic,
    /// SignalGenerator manages exits via Exit signals (for CLOB markets)
    StrategyManaged,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Position {
    pub mint: String,
    pub amount: f64,
    pub entry_price: Option<f64>,
    pub current_price: Option<f64>,
    pub current_price_updated_time: DateTime<Utc>,
    pub pnl_pct: Option<f64>,       // Percentage PnL
    pub entry_time: DateTime<Utc>,  // Time when the position was initiated (first buy)
    pub entry_slot: u64,            // Slot when the first buy transaction was confirmed
    pub entry_signature: Signature, // Signature of the first buy
    pub signal_id: Option<String>,  // ID of the signal that triggered this position
    pub sell_failure_count: u32,    // Track consecutive sell confirmation failures

    // === Orderbook/CLOB extensions ===
    /// Active exit order for this position (for CLOB markets)
    pub active_exit_order: Option<ActiveExitOrder>,
    /// Exit mode: automatic (ExitStrategy) or strategy-managed
    pub exit_mode: ExitMode,
}
