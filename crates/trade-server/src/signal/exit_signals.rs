//! Exit signal types for strategy-driven position exit management.
//!
//! This module provides signal types for CLOB (Central Limit Order Book) markets
//! where exit management is part of the trading strategy rather than infrastructure.
//!
//! ## Signal Types
//!
//! - [`ExitSignal`]: Request to close a position (market or limit order)
//! - [`ModifyOrderSignal`]: Request to modify an existing exit order (cancel + replace)
//! - [`CancelOrderSignal`]: Request to cancel an existing exit order
//!
//! ## Usage
//!
//! Signal generators can emit these signals to manage position exits:
//!
//! ```ignore
//! use trade_server::signal::{ExitSignal, ExitType, SignalMetadata};
//!
//! // Create a limit exit signal
//! let exit_signal = ExitSignal {
//!     signal_meta: SignalMetadata::new(),
//!     mint: "asset123".to_string(),
//!     market: Some("market456".to_string()),
//!     exit_type: ExitType::Limit {
//!         price: 0.75,
//!         time_in_force: TimeInForce::GoodTilCancelled,
//!     },
//!     reason: "take_profit_target_reached".to_string(),
//! };
//! ```

use std::any::Any;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::execution::TimeInForce;
use crate::notifier::Notifiable;

use super::{SignalAction, SignalMetadata, TradableSignal};

/// Specifies how a position should be exited.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExitType {
    /// Market sell (immediate, for AMM or urgent exits)
    Market,
    /// Limit sell at specified price
    Limit {
        /// The limit price for the exit order
        price: f64,
        /// How long the order should remain active
        time_in_force: TimeInForce,
    },
}

/// A signal that requests closing a position.
///
/// This signal can be emitted by strategy signal generators to manage
/// position exits, especially for CLOB markets where exit timing and
/// pricing is part of the strategy.
///
/// ## Example
///
/// ```ignore
/// let exit = ExitSignal {
///     signal_meta: SignalMetadata::new(),
///     mint: "token123".to_string(),
///     market: Some("market456".to_string()),
///     exit_type: ExitType::Limit {
///         price: 0.80,
///         time_in_force: TimeInForce::GoodTilCancelled,
///     },
///     reason: "take_profit".to_string(),
/// };
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct ExitSignal {
    /// Signal metadata (ID, timestamp)
    pub signal_meta: SignalMetadata,
    /// Asset identifier (token mint or asset_id)
    pub mint: String,
    /// Market/condition ID for orderbook venues (optional)
    pub market: Option<String>,
    /// How to execute the exit (market or limit)
    pub exit_type: ExitType,
    /// Human-readable reason for the exit
    pub reason: String,
}

impl TradableSignal for ExitSignal {
    fn signal_id(&self) -> &str {
        self.signal_meta.id()
    }

    fn signal_type(&self) -> &str {
        "exit"
    }

    fn get_mint(&self) -> Option<&str> {
        Some(&self.mint)
    }

    fn get_price(&self) -> Option<f64> {
        match &self.exit_type {
            ExitType::Limit { price, .. } => Some(*price),
            ExitType::Market => None,
        }
    }

    fn get_timestamp(&self) -> Option<DateTime<Utc>> {
        Some(self.signal_meta.timestamp())
    }

    fn get_slot(&self) -> Option<u64> {
        None
    }

    fn get_creator(&self) -> Option<solana_sdk::pubkey::Pubkey> {
        None
    }

    fn passes_filter(&self) -> bool {
        true // Exit signals should always pass filters
    }

    fn as_notifiable(&self) -> Option<Box<dyn Notifiable + Send + Sync>> {
        None // Exit signals don't need notifications by default
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn to_json(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::to_value(self)?)
    }

    // Orderbook methods
    fn get_market(&self) -> Option<&str> {
        self.market.as_deref()
    }

    fn get_limit_price(&self) -> Option<f64> {
        match &self.exit_type {
            ExitType::Limit { price, .. } => Some(*price),
            ExitType::Market => None,
        }
    }

    fn is_limit_order(&self) -> bool {
        matches!(self.exit_type, ExitType::Limit { .. })
    }

    fn get_time_in_force(&self) -> Option<TimeInForce> {
        match &self.exit_type {
            ExitType::Limit { time_in_force, .. } => Some(*time_in_force),
            ExitType::Market => None,
        }
    }

    // Signal action methods
    fn signal_action(&self) -> SignalAction {
        SignalAction::Exit
    }

    fn references_position(&self) -> Option<&str> {
        Some(&self.mint)
    }

    fn references_order_id(&self) -> Option<&str> {
        None
    }
}

/// A signal to modify an existing exit order (cancel + replace).
///
/// This signal is used to adjust the price or size of an existing
/// limit exit order without fully cancelling and re-creating it.
///
/// ## Example
///
/// ```ignore
/// let modify = ModifyOrderSignal {
///     signal_meta: SignalMetadata::new(),
///     mint: "token123".to_string(),
///     order_id: "order456".to_string(),
///     new_price: 0.75,
///     new_size: None, // Keep original size
///     time_in_force: TimeInForce::GoodTilCancelled,
///     reason: "order_timeout_adjustment".to_string(),
/// };
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct ModifyOrderSignal {
    /// Signal metadata (ID, timestamp)
    pub signal_meta: SignalMetadata,
    /// Asset identifier (token mint or asset_id)
    pub mint: String,
    /// The venue order ID to modify
    pub order_id: String,
    /// New price for the order
    pub new_price: f64,
    /// New size for the order (None to keep original size)
    pub new_size: Option<f64>,
    /// Time-in-force for the replacement order
    pub time_in_force: TimeInForce,
    /// Human-readable reason for the modification
    pub reason: String,
}

impl TradableSignal for ModifyOrderSignal {
    fn signal_id(&self) -> &str {
        self.signal_meta.id()
    }

    fn signal_type(&self) -> &str {
        "modify_order"
    }

    fn get_mint(&self) -> Option<&str> {
        Some(&self.mint)
    }

    fn get_price(&self) -> Option<f64> {
        Some(self.new_price)
    }

    fn get_timestamp(&self) -> Option<DateTime<Utc>> {
        Some(self.signal_meta.timestamp())
    }

    fn get_slot(&self) -> Option<u64> {
        None
    }

    fn get_creator(&self) -> Option<solana_sdk::pubkey::Pubkey> {
        None
    }

    fn passes_filter(&self) -> bool {
        true
    }

    fn as_notifiable(&self) -> Option<Box<dyn Notifiable + Send + Sync>> {
        None
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn to_json(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::to_value(self)?)
    }

    // Orderbook methods
    fn get_limit_price(&self) -> Option<f64> {
        Some(self.new_price)
    }

    fn is_limit_order(&self) -> bool {
        true // Modify signals are always for limit orders
    }

    fn get_time_in_force(&self) -> Option<TimeInForce> {
        Some(self.time_in_force)
    }

    // Signal action methods
    fn signal_action(&self) -> SignalAction {
        SignalAction::ModifyOrder
    }

    fn references_position(&self) -> Option<&str> {
        Some(&self.mint)
    }

    fn references_order_id(&self) -> Option<&str> {
        Some(&self.order_id)
    }
}

/// A signal to cancel an existing exit order.
///
/// This signal cancels an active limit exit order, typically when
/// the strategy wants to change exit approach or market conditions
/// have changed significantly.
///
/// ## Example
///
/// ```ignore
/// let cancel = CancelOrderSignal {
///     signal_meta: SignalMetadata::new(),
///     mint: "token123".to_string(),
///     order_id: "order456".to_string(),
///     reason: "market_conditions_changed".to_string(),
/// };
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct CancelOrderSignal {
    /// Signal metadata (ID, timestamp)
    pub signal_meta: SignalMetadata,
    /// Asset identifier (token mint or asset_id)
    pub mint: String,
    /// The venue order ID to cancel
    pub order_id: String,
    /// Human-readable reason for the cancellation
    pub reason: String,
}

impl TradableSignal for CancelOrderSignal {
    fn signal_id(&self) -> &str {
        self.signal_meta.id()
    }

    fn signal_type(&self) -> &str {
        "cancel_order"
    }

    fn get_mint(&self) -> Option<&str> {
        Some(&self.mint)
    }

    fn get_price(&self) -> Option<f64> {
        None
    }

    fn get_timestamp(&self) -> Option<DateTime<Utc>> {
        Some(self.signal_meta.timestamp())
    }

    fn get_slot(&self) -> Option<u64> {
        None
    }

    fn get_creator(&self) -> Option<solana_sdk::pubkey::Pubkey> {
        None
    }

    fn passes_filter(&self) -> bool {
        true
    }

    fn as_notifiable(&self) -> Option<Box<dyn Notifiable + Send + Sync>> {
        None
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn to_json(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::to_value(self)?)
    }

    // Signal action methods
    fn signal_action(&self) -> SignalAction {
        SignalAction::CancelOrder
    }

    fn references_position(&self) -> Option<&str> {
        Some(&self.mint)
    }

    fn references_order_id(&self) -> Option<&str> {
        Some(&self.order_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exit_signal_market() {
        let signal = ExitSignal {
            signal_meta: SignalMetadata::new(),
            mint: "token123".to_string(),
            market: None,
            exit_type: ExitType::Market,
            reason: "stop_loss".to_string(),
        };

        assert_eq!(signal.signal_type(), "exit");
        assert_eq!(signal.signal_action(), SignalAction::Exit);
        assert_eq!(signal.get_mint(), Some("token123"));
        assert!(!signal.is_limit_order());
        assert_eq!(signal.get_limit_price(), None);
        assert!(signal.passes_filter());
    }

    #[test]
    fn test_exit_signal_limit() {
        let signal = ExitSignal {
            signal_meta: SignalMetadata::new(),
            mint: "token123".to_string(),
            market: Some("market456".to_string()),
            exit_type: ExitType::Limit {
                price: 0.75,
                time_in_force: TimeInForce::GoodTilCancelled,
            },
            reason: "take_profit".to_string(),
        };

        assert_eq!(signal.signal_type(), "exit");
        assert_eq!(signal.signal_action(), SignalAction::Exit);
        assert!(signal.is_limit_order());
        assert_eq!(signal.get_limit_price(), Some(0.75));
        assert_eq!(signal.get_price(), Some(0.75));
        assert_eq!(signal.get_market(), Some("market456"));
        assert_eq!(
            signal.get_time_in_force(),
            Some(TimeInForce::GoodTilCancelled)
        );
    }

    #[test]
    fn test_modify_order_signal() {
        let signal = ModifyOrderSignal {
            signal_meta: SignalMetadata::new(),
            mint: "token123".to_string(),
            order_id: "order456".to_string(),
            new_price: 0.80,
            new_size: Some(50.0),
            time_in_force: TimeInForce::ImmediateOrCancel,
            reason: "price_adjustment".to_string(),
        };

        assert_eq!(signal.signal_type(), "modify_order");
        assert_eq!(signal.signal_action(), SignalAction::ModifyOrder);
        assert_eq!(signal.references_order_id(), Some("order456"));
        assert_eq!(signal.references_position(), Some("token123"));
        assert!(signal.is_limit_order());
        assert_eq!(signal.get_limit_price(), Some(0.80));
        assert_eq!(
            signal.get_time_in_force(),
            Some(TimeInForce::ImmediateOrCancel)
        );
    }

    #[test]
    fn test_cancel_order_signal() {
        let signal = CancelOrderSignal {
            signal_meta: SignalMetadata::new(),
            mint: "token123".to_string(),
            order_id: "order789".to_string(),
            reason: "market_changed".to_string(),
        };

        assert_eq!(signal.signal_type(), "cancel_order");
        assert_eq!(signal.signal_action(), SignalAction::CancelOrder);
        assert_eq!(signal.references_order_id(), Some("order789"));
        assert_eq!(signal.references_position(), Some("token123"));
        assert!(!signal.is_limit_order());
        assert_eq!(signal.get_limit_price(), None);
    }

    #[test]
    fn test_exit_type_serialization() {
        let market = ExitType::Market;
        let json = serde_json::to_string(&market).unwrap();
        assert!(json.contains("Market"));

        let limit = ExitType::Limit {
            price: 0.65,
            time_in_force: TimeInForce::FillOrKill,
        };
        let json = serde_json::to_string(&limit).unwrap();
        assert!(json.contains("0.65"));
        assert!(json.contains("FillOrKill"));
    }

    #[test]
    fn test_signal_to_json() {
        let signal = ExitSignal {
            signal_meta: SignalMetadata::new(),
            mint: "token_mint".to_string(),
            market: None,
            exit_type: ExitType::Market,
            reason: "test".to_string(),
        };

        let json = signal.to_json().unwrap();
        assert_eq!(json["mint"], "token_mint");
        assert_eq!(json["reason"], "test");
        assert_eq!(json["exit_type"], "Market");
    }
}
