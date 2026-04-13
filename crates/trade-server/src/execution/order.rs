use chrono::{DateTime, Utc};

use super::events::TimeInForce;
use crate::position::ExitMode;

#[derive(Debug, Clone, PartialEq)]
pub enum OrderType {
    // === Market Orders (AMM-style) ===
    /// Market buy order - purchase tokens using quote currency (e.g., SOL)
    MarketBuy {
        /// Quote amount in standard units (e.g., SOL, not lamports)
        quote_amount: f64,
    },
    /// Market sell order - sell tokens for quote currency
    MarketSell {
        /// Token amount in standard units, not base units
        token_amount: f64,
        /// Whether to fully close the position
        clear_position: bool,
    },

    // === Limit Orders (Orderbook-style) ===
    /// Limit buy order - place a bid at a specific price
    LimitBuy {
        /// Quote amount in standard units (e.g., USDC for Polymarket)
        quote_amount: f64,
        /// Maximum price willing to pay
        limit_price: f64,
        /// How long the order should remain active
        time_in_force: TimeInForce,
    },
    /// Limit sell order - place an ask at a specific price
    LimitSell {
        /// Token amount in standard units
        token_amount: f64,
        /// Minimum price willing to accept
        limit_price: f64,
        /// Whether to fully close the position when filled
        clear_position: bool,
        /// How long the order should remain active
        time_in_force: TimeInForce,
    },

    // === Cancel Orders ===
    /// Cancel an existing order by venue order ID.
    ///
    /// Used by the intent-based reconciliation engine to cancel orders
    /// that are no longer part of the desired order book state.
    Cancel {
        /// The venue-assigned order ID to cancel
        order_id: String,
    },

    // === Legacy Aliases (for backward compatibility) ===
    // These are kept for existing code that uses Buy/Sell directly
    /// Legacy alias for MarketBuy - use MarketBuy for new code
    #[deprecated(since = "0.2.0", note = "Use MarketBuy instead")]
    Buy {
        /// SOL amount in SOL, not in lamports
        sol_amount: f64,
    },
    /// Legacy alias for MarketSell - use MarketSell for new code
    #[deprecated(since = "0.2.0", note = "Use MarketSell instead")]
    Sell {
        /// Token amount in standard unit, not in base units
        token_amount: f64,
        clear_position: bool,
    },
}

impl OrderType {
    /// Returns true if this is a buy order (market or limit)
    #[allow(deprecated)]
    pub fn is_buy(&self) -> bool {
        matches!(
            self,
            OrderType::MarketBuy { .. } | OrderType::LimitBuy { .. } | OrderType::Buy { .. }
        )
    }

    /// Returns true if this is a sell order (market or limit)
    #[allow(deprecated)]
    pub fn is_sell(&self) -> bool {
        matches!(
            self,
            OrderType::MarketSell { .. } | OrderType::LimitSell { .. } | OrderType::Sell { .. }
        )
    }

    /// Returns true if this is a limit order
    pub fn is_limit(&self) -> bool {
        matches!(
            self,
            OrderType::LimitBuy { .. } | OrderType::LimitSell { .. }
        )
    }

    /// Returns true if this is a cancel order
    pub fn is_cancel(&self) -> bool {
        matches!(self, OrderType::Cancel { .. })
    }

    /// Returns the order ID if this is a cancel order
    pub fn cancel_order_id(&self) -> Option<&str> {
        match self {
            OrderType::Cancel { order_id } => Some(order_id),
            _ => None,
        }
    }

    /// Returns true if this is a market order
    #[allow(deprecated)]
    pub fn is_market(&self) -> bool {
        matches!(
            self,
            OrderType::MarketBuy { .. }
                | OrderType::MarketSell { .. }
                | OrderType::Buy { .. }
                | OrderType::Sell { .. }
        )
    }

    /// Returns the limit price if this is a limit order
    pub fn limit_price(&self) -> Option<f64> {
        match self {
            OrderType::LimitBuy { limit_price, .. } | OrderType::LimitSell { limit_price, .. } => {
                Some(*limit_price)
            }
            _ => None,
        }
    }

    /// Returns the time-in-force if this is a limit order
    pub fn time_in_force(&self) -> Option<TimeInForce> {
        match self {
            OrderType::LimitBuy { time_in_force, .. }
            | OrderType::LimitSell { time_in_force, .. } => Some(*time_in_force),
            _ => None,
        }
    }

    /// Returns whether this order should clear the position
    pub fn clear_position(&self) -> Option<bool> {
        #[allow(deprecated)]
        match self {
            OrderType::MarketSell { clear_position, .. }
            | OrderType::LimitSell { clear_position, .. }
            | OrderType::Sell { clear_position, .. } => Some(*clear_position),
            _ => None,
        }
    }

    /// Returns the quote amount (SOL amount for buy orders)
    pub fn quote_amount(&self) -> Option<f64> {
        #[allow(deprecated)]
        match self {
            OrderType::MarketBuy { quote_amount } | OrderType::LimitBuy { quote_amount, .. } => {
                Some(*quote_amount)
            }
            OrderType::Buy { sol_amount } => Some(*sol_amount),
            _ => None,
        }
    }

    /// Returns the token amount (for sell orders)
    pub fn token_amount(&self) -> Option<f64> {
        #[allow(deprecated)]
        match self {
            OrderType::MarketSell { token_amount, .. }
            | OrderType::LimitSell { token_amount, .. }
            | OrderType::Sell { token_amount, .. } => Some(*token_amount),
            _ => None,
        }
    }
}

/// Represents a trading order that can be executed by an OrderExecutor.
///
/// Supports both AMM-style market orders and orderbook-style limit orders.
#[derive(Debug, Clone, PartialEq)]
pub struct Order {
    /// Asset identifier (token mint address for Solana, asset_id for Polymarket)
    pub mint: String,
    /// Market or condition identifier for orderbook venues (e.g., Polymarket condition_id)
    /// None for AMM venues where mint alone identifies the market
    pub market: Option<String>,
    /// The type of order (market buy/sell or limit buy/sell)
    pub order_type: OrderType,
    /// Expected execution price (for market orders) or limit price reference
    pub price: Option<f64>,
    /// Current status of the order
    pub status: OrderStatus,
    /// When the order was created
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Slot number from the signal that triggered this order
    pub signal_slot: Option<u64>,
    /// ID of the signal that triggered this order
    pub signal_id: Option<String>,
    /// DEX type for AMM orders (PumpFun, Bonk, etc.)
    pub dex_type: Option<crate::execution::solana::tx_constructor::DexType>,
    /// External order ID assigned by the venue (for limit orders and order tracking)
    pub venue_order_id: Option<String>,
    /// Exit mode for positions created from this order.
    /// None means use the default (Automatic). Used to propagate StrategyManaged mode
    /// from intent-based signals through to position creation.
    pub exit_mode: Option<ExitMode>,
    /// Arbitrary debug context data from the signal (e.g., fair price, inventory, orderbook state).
    /// Propagated to OrderPlaced events for debugging and analysis.
    pub context: Option<serde_json::Value>,
}

impl Order {
    /// Create a new market buy order (convenience constructor for backward compatibility)
    #[allow(deprecated)]
    pub fn new_buy(
        mint: String,
        sol_amount: f64,
        timestamp: DateTime<Utc>,
        signal_id: Option<String>,
        signal_slot: Option<u64>,
    ) -> Self {
        Order {
            mint,
            market: None,
            order_type: OrderType::Buy { sol_amount },
            price: None,
            status: OrderStatus::Pending,
            timestamp,
            signal_slot,
            signal_id,
            dex_type: None,
            venue_order_id: None,
            exit_mode: None,
            context: None,
        }
    }

    /// Create a new market sell order (convenience constructor for backward compatibility)
    #[allow(deprecated)]
    pub fn new_sell(
        mint: String,
        token_amount: f64,
        clear_position: bool,
        timestamp: DateTime<Utc>,
        signal_id: Option<String>,
        signal_slot: Option<u64>,
    ) -> Self {
        Order {
            mint,
            market: None,
            order_type: OrderType::Sell {
                token_amount,
                clear_position,
            },
            price: None,
            status: OrderStatus::Pending,
            timestamp,
            signal_slot,
            signal_id,
            dex_type: None,
            venue_order_id: None,
            exit_mode: None,
            context: None,
        }
    }

    /// Create a new limit buy order for orderbook venues
    pub fn new_limit_buy(
        mint: String,
        market: Option<String>,
        quote_amount: f64,
        limit_price: f64,
        time_in_force: TimeInForce,
        timestamp: DateTime<Utc>,
        signal_id: Option<String>,
        signal_slot: Option<u64>,
    ) -> Self {
        Order {
            mint,
            market,
            order_type: OrderType::LimitBuy {
                quote_amount,
                limit_price,
                time_in_force,
            },
            price: Some(limit_price),
            status: OrderStatus::Pending,
            timestamp,
            signal_slot,
            signal_id,
            dex_type: None,
            venue_order_id: None,
            exit_mode: None,
            context: None,
        }
    }

    /// Create a new limit sell order for orderbook venues
    pub fn new_limit_sell(
        mint: String,
        market: Option<String>,
        token_amount: f64,
        limit_price: f64,
        clear_position: bool,
        time_in_force: TimeInForce,
        timestamp: DateTime<Utc>,
        signal_id: Option<String>,
        signal_slot: Option<u64>,
    ) -> Self {
        Order {
            mint,
            market,
            order_type: OrderType::LimitSell {
                token_amount,
                limit_price,
                clear_position,
                time_in_force,
            },
            price: Some(limit_price),
            status: OrderStatus::Pending,
            timestamp,
            signal_slot,
            signal_id,
            dex_type: None,
            venue_order_id: None,
            exit_mode: None,
            context: None,
        }
    }

    /// Create a new cancel order for orderbook venues
    ///
    /// Used by the intent-based reconciliation engine to cancel orders
    /// that are no longer part of the desired order book state.
    pub fn new_cancel(
        mint: String,
        market: Option<String>,
        order_id: String,
        timestamp: DateTime<Utc>,
        signal_id: Option<String>,
    ) -> Self {
        Order {
            mint,
            market,
            order_type: OrderType::Cancel {
                order_id: order_id.clone(),
            },
            price: None,
            status: OrderStatus::Pending,
            timestamp,
            signal_slot: None,
            signal_id,
            dex_type: None,
            venue_order_id: Some(order_id),
            exit_mode: None,
            context: None,
        }
    }

    /// Returns true if this is a limit order
    pub fn is_limit_order(&self) -> bool {
        self.order_type.is_limit()
    }

    /// Returns true if this is a market order
    pub fn is_market_order(&self) -> bool {
        self.order_type.is_market()
    }

    /// Returns true if this is a cancel order
    pub fn is_cancel_order(&self) -> bool {
        self.order_type.is_cancel()
    }

    /// Set the exit mode for this order (builder pattern).
    ///
    /// Used to propagate StrategyManaged mode from intent-based signals
    /// through to position creation.
    pub fn with_exit_mode(mut self, exit_mode: ExitMode) -> Self {
        self.exit_mode = Some(exit_mode);
        self
    }

    /// Set the context for this order (builder pattern).
    ///
    /// Used to propagate debug context data from signals through to
    /// OrderPlaced events for debugging and analysis.
    pub fn with_context(mut self, context: serde_json::Value) -> Self {
        self.context = Some(context);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum OrderStatus {
    Pending,
    Filled(DateTime<Utc>),
    Rejected(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_type_is_buy() {
        assert!(OrderType::MarketBuy { quote_amount: 1.0 }.is_buy());
        assert!(
            OrderType::LimitBuy {
                quote_amount: 1.0,
                limit_price: 0.5,
                time_in_force: TimeInForce::GoodTilCancelled
            }
            .is_buy()
        );
        assert!(
            !OrderType::MarketSell {
                token_amount: 1.0,
                clear_position: false
            }
            .is_buy()
        );
    }

    #[test]
    fn test_order_type_is_sell() {
        assert!(
            OrderType::MarketSell {
                token_amount: 1.0,
                clear_position: false
            }
            .is_sell()
        );
        assert!(
            OrderType::LimitSell {
                token_amount: 1.0,
                limit_price: 0.5,
                clear_position: true,
                time_in_force: TimeInForce::GoodTilCancelled
            }
            .is_sell()
        );
        assert!(!OrderType::MarketBuy { quote_amount: 1.0 }.is_sell());
    }

    #[test]
    fn test_order_type_is_limit() {
        assert!(
            OrderType::LimitBuy {
                quote_amount: 1.0,
                limit_price: 0.5,
                time_in_force: TimeInForce::GoodTilCancelled
            }
            .is_limit()
        );
        assert!(
            OrderType::LimitSell {
                token_amount: 1.0,
                limit_price: 0.5,
                clear_position: false,
                time_in_force: TimeInForce::ImmediateOrCancel
            }
            .is_limit()
        );
        assert!(!OrderType::MarketBuy { quote_amount: 1.0 }.is_limit());
        assert!(
            !OrderType::MarketSell {
                token_amount: 1.0,
                clear_position: false
            }
            .is_limit()
        );
    }

    #[test]
    fn test_order_type_limit_price() {
        assert_eq!(
            OrderType::LimitBuy {
                quote_amount: 1.0,
                limit_price: 0.75,
                time_in_force: TimeInForce::GoodTilCancelled
            }
            .limit_price(),
            Some(0.75)
        );
        assert_eq!(
            OrderType::MarketBuy { quote_amount: 1.0 }.limit_price(),
            None
        );
    }

    #[test]
    fn test_order_type_clear_position() {
        assert_eq!(
            OrderType::MarketSell {
                token_amount: 1.0,
                clear_position: true
            }
            .clear_position(),
            Some(true)
        );
        assert_eq!(
            OrderType::MarketSell {
                token_amount: 1.0,
                clear_position: false
            }
            .clear_position(),
            Some(false)
        );
        assert_eq!(
            OrderType::MarketBuy { quote_amount: 1.0 }.clear_position(),
            None
        );
    }

    #[test]
    fn test_order_new_limit_buy() {
        let order = Order::new_limit_buy(
            "token123".to_string(),
            Some("market456".to_string()),
            100.0,
            0.65,
            TimeInForce::ImmediateOrCancel,
            Utc::now(),
            Some("signal789".to_string()),
            Some(12345),
        );

        assert_eq!(order.mint, "token123");
        assert_eq!(order.market, Some("market456".to_string()));
        assert!(order.is_limit_order());
        assert!(!order.is_market_order());
        assert_eq!(order.price, Some(0.65));
        assert_eq!(order.status, OrderStatus::Pending);
    }

    #[test]
    fn test_order_new_limit_sell() {
        let order = Order::new_limit_sell(
            "token123".to_string(),
            None,
            50.0,
            0.80,
            true,
            TimeInForce::FillOrKill,
            Utc::now(),
            None,
            None,
        );

        assert_eq!(order.mint, "token123");
        assert_eq!(order.market, None);
        assert!(order.is_limit_order());
        assert_eq!(order.order_type.clear_position(), Some(true));
    }

    #[allow(deprecated)]
    #[test]
    fn test_legacy_order_type_still_works() {
        // Verify that deprecated Buy/Sell variants still work for backward compatibility
        let buy = OrderType::Buy { sol_amount: 1.0 };
        assert!(buy.is_buy());
        assert!(buy.is_market());
        assert!(!buy.is_limit());

        let sell = OrderType::Sell {
            token_amount: 100.0,
            clear_position: true,
        };
        assert!(sell.is_sell());
        assert!(sell.is_market());
        assert!(sell.clear_position().unwrap_or(false));
    }

    #[test]
    fn test_order_type_cancel() {
        let cancel = OrderType::Cancel {
            order_id: "order123".to_string(),
        };

        assert!(cancel.is_cancel());
        assert!(!cancel.is_buy());
        assert!(!cancel.is_sell());
        assert!(!cancel.is_limit());
        assert!(!cancel.is_market());
        assert_eq!(cancel.cancel_order_id(), Some("order123"));
        assert_eq!(cancel.limit_price(), None);
        assert_eq!(cancel.time_in_force(), None);
        assert_eq!(cancel.clear_position(), None);
        assert_eq!(cancel.quote_amount(), None);
        assert_eq!(cancel.token_amount(), None);
    }

    #[test]
    fn test_order_type_cancel_order_id_for_non_cancel() {
        let market_buy = OrderType::MarketBuy {
            quote_amount: 100.0,
        };
        assert_eq!(market_buy.cancel_order_id(), None);

        let limit_sell = OrderType::LimitSell {
            token_amount: 50.0,
            limit_price: 0.55,
            clear_position: true,
            time_in_force: TimeInForce::GoodTilCancelled,
        };
        assert_eq!(limit_sell.cancel_order_id(), None);
    }

    #[test]
    fn test_order_new_cancel() {
        let order = Order::new_cancel(
            "token123".to_string(),
            Some("market456".to_string()),
            "order789".to_string(),
            Utc::now(),
            Some("signal101".to_string()),
        );

        assert_eq!(order.mint, "token123");
        assert_eq!(order.market, Some("market456".to_string()));
        assert!(order.is_cancel_order());
        assert!(!order.is_limit_order());
        assert!(!order.is_market_order());
        assert_eq!(order.price, None);
        assert_eq!(order.status, OrderStatus::Pending);
        assert_eq!(order.venue_order_id, Some("order789".to_string()));
        assert_eq!(order.signal_id, Some("signal101".to_string()));
        assert_eq!(order.order_type.cancel_order_id(), Some("order789"));
    }
}
