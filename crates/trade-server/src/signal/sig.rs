use std::{any::Any, fmt::Debug};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::execution::TimeInForce;
use crate::notifier::Notifiable;
use crate::signal::intent::OrderIntent;

/// Indicates what action a signal represents.
///
/// This enum allows signals to express their action, enabling the position handler
/// to route them to appropriate handlers for entry, exit, or order management operations.
///
/// Note: This is distinct from "intent-based signals" (`is_intent_signal()` / `OrderIntent`)
/// which express desired order book state rather than discrete actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SignalAction {
    /// Open a new position (current behavior, default for backward compatibility)
    #[default]
    Entry,
    /// Close an existing position
    Exit,
    /// Modify an existing order (cancel + replace)
    ModifyOrder,
    /// Cancel an existing order
    CancelOrder,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SignalMetadata {
    id: String,
    timestamp: DateTime<Utc>,
}

impl SignalMetadata {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
        }
    }

    /// Create metadata with a specific timestamp.
    ///
    /// Use this when the timestamp should be derived from an external source
    /// (e.g., event timestamp in backtest mode) rather than wall-clock time.
    pub fn with_timestamp(timestamp: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}

pub trait TradableSignal: Send + Sync + Debug {
    // === Required methods ===
    fn signal_id(&self) -> &str;
    fn signal_type(&self) -> &str;
    fn get_mint(&self) -> Option<&str>;
    fn get_price(&self) -> Option<f64>;
    fn get_timestamp(&self) -> Option<DateTime<Utc>>;
    fn get_slot(&self) -> Option<u64>;
    fn get_creator(&self) -> Option<solana_sdk::pubkey::Pubkey>;
    fn passes_filter(&self) -> bool;
    fn as_notifiable(&self) -> Option<Box<dyn Notifiable + Send + Sync>>;
    fn as_any(&self) -> &dyn Any;
    fn to_json(&self) -> anyhow::Result<serde_json::Value>;

    // === Optional orderbook methods with defaults ===

    /// Returns the market/condition ID for orderbook venues.
    /// None for AMM venues where mint alone identifies the market.
    fn get_market(&self) -> Option<&str> {
        None
    }

    /// Returns the best bid price if available.
    /// Defaults to get_price() for AMM signals.
    fn get_bid_price(&self) -> Option<f64> {
        self.get_price()
    }

    /// Returns the best ask price if available.
    /// Defaults to get_price() for AMM signals.
    fn get_ask_price(&self) -> Option<f64> {
        self.get_price()
    }

    /// Returns the limit price for limit order signals.
    /// None for market order signals.
    fn get_limit_price(&self) -> Option<f64> {
        None
    }

    /// Returns true if this signal should result in a limit order.
    /// False by default (market orders).
    fn is_limit_order(&self) -> bool {
        false
    }

    /// Returns the time-in-force for limit order signals.
    /// None for market order signals.
    fn get_time_in_force(&self) -> Option<TimeInForce> {
        None
    }

    // === Signal Action methods (for strategy-driven exit management) ===

    /// Returns the action of this signal.
    ///
    /// This determines how the signal is routed:
    /// - `Entry`: Creates a new position (default, backward compatible)
    /// - `Exit`: Closes an existing position
    /// - `ModifyOrder`: Cancels and replaces an existing order
    /// - `CancelOrder`: Cancels an existing order
    fn signal_action(&self) -> SignalAction {
        SignalAction::Entry
    }

    /// Returns the mint/asset ID of the position this signal references.
    ///
    /// For Exit/ModifyOrder/CancelOrder signals, this identifies which position
    /// the operation applies to. Defaults to `get_mint()` for convenience.
    fn references_position(&self) -> Option<&str> {
        self.get_mint()
    }

    /// Returns the order ID this signal references.
    ///
    /// For ModifyOrder and CancelOrder signals, this identifies which
    /// existing order to modify or cancel. Returns None by default.
    fn references_order_id(&self) -> Option<&str> {
        None
    }

    // === Intent-based signal methods (for orderbook market making) ===

    /// Returns true if this is an intent-based signal requiring reconciliation.
    ///
    /// Intent-based signals express desired order book state rather than discrete
    /// actions. The PositionManager reconciles current order state with desired
    /// state and generates the necessary orders (cancels and placements).
    ///
    /// Default: false (existing action-based signals)
    fn is_intent_signal(&self) -> bool {
        false
    }

    /// Returns the desired order state for intent-based signals.
    ///
    /// Only called when `is_intent_signal()` returns true. Returns `None`
    /// for action-based signals.
    ///
    /// The returned `OrderIntent` describes the desired bid and ask orders.
    /// The PositionManager will diff this against current pending orders
    /// and generate cancels/placements to reach the desired state.
    fn get_order_intent(&self) -> Option<OrderIntent> {
        None
    }

    /// Returns optional context data for debugging/logging.
    ///
    /// Override this method in signal implementations to provide arbitrary
    /// debug context (e.g., fair price calculation, inventory state, orderbook
    /// snapshot) that will be propagated to OrderPlaced events.
    ///
    /// This context is purely for debugging and analysis - it does not affect
    /// order execution or position management.
    fn get_context(&self) -> Option<serde_json::Value> {
        None
    }
}
