use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicU64, Ordering},
    },
};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rand::rngs::StdRng;
use solana_sdk::signature::Signature;
use tokio::sync::Mutex;
use tracing::{debug, instrument};

use popeyes_trading_types::{OrderbookSnapshotEvent, PolymarketTradeEvent, TokenTradeEvent};

use crate::config::LatencySimulationConfig;

use crate::execution::{
    OrderExecutor,
    events::{ExecutionEvent, LimitOrderEvent, RedemptionEvent},
    lifecycle::LifecycleEngine,
    order::{Order, OrderType},
};
use crate::signal::RedemptionAction;

mod config;
mod fill_engine;
mod latency;
mod market_registry;
mod quote_lifecycle;
mod types;

pub use types::PendingBacktestOrder;
use types::{QuoteLaneKey, QuoteLaneState};

/// Backtest-specific executor that:
/// - Tracks last trade prices
/// - Applies configurable slippage for market orders
/// - Simulates limit order fills using trade-based fill logic
/// - Manages pending limit orders
/// - Optionally enforces inventory constraints on sell orders
/// - Optionally simulates order placement latency for realistic fill modeling
#[derive(Clone)]
pub struct BacktestOrderExecutor {
    /// Last trade price by mint (for market orders)
    last_prices: Arc<Mutex<HashMap<String, f64>>>,
    /// Slippage percentage for buy orders (e.g. 0.01 for 1%)
    buy_slippage: f64,
    /// Slippage percentage for sell orders (e.g. 0.01 for 1%)
    sell_slippage: f64,
    /// Pending limit orders awaiting fill (keyed by order_id)
    pending_orders: Arc<Mutex<HashMap<String, PendingBacktestOrder>>>,
    /// Counter for generating unique order IDs
    order_id_counter: Arc<AtomicU64>,
    /// If true, sell orders require positive inventory to fill.
    /// This prevents "naked shorting" which is not realistic for venues like Polymarket.
    enforce_inventory_constraints: bool,
    /// Optional latency simulation configuration for realistic fill modeling
    latency_config: Option<LatencySimulationConfig>,
    /// Optional deterministic RNG stream for latency sampling.
    /// Present only when `latency_config.seed` is set.
    latency_rng: Option<Arc<StdMutex<StdRng>>>,
    /// Per-lane quote lifecycle state for placement/cancellation latency modeling.
    quote_lanes: Arc<Mutex<HashMap<QuoteLaneKey, QuoteLaneState>>>,
    /// Order ID -> quote lane lookup for cancellation and fill bookkeeping.
    order_id_to_lane: Arc<Mutex<HashMap<String, QuoteLaneKey>>>,
    /// Current simulation time (set from incoming events).
    current_time: Arc<Mutex<Option<DateTime<Utc>>>>,
    /// Mapping from market id -> (asset_a, asset_b) observed in snapshots/trades.
    /// Used to simulate Polymarket CTF "mirroring" (YES/NO parity) fills across complementary assets.
    market_assets: Arc<Mutex<HashMap<String, (String, Option<String>)>>>,
    /// Mapping from asset_id -> complementary asset_id for the same market.
    complement_by_asset: Arc<Mutex<HashMap<String, String>>>,
    /// Canonical lifecycle authority for backtest limit-order state transitions.
    lifecycle_engine: Arc<StdMutex<LifecycleEngine>>,
}

impl BacktestOrderExecutor {
    /// Get the last price for a given token
    pub async fn last_prices(&self, mint: &str) -> Option<f64> {
        let prices = self.last_prices.lock().await;
        prices.get(mint).copied()
    }

    /// Generate a unique order ID for this backtest session.
    fn generate_order_id(&self) -> String {
        let id = self.order_id_counter.fetch_add(1, Ordering::SeqCst);
        format!("bt-order-{}", id)
    }

    /// Get all pending orders (for testing/inspection)
    pub async fn get_pending_orders(&self) -> Vec<PendingBacktestOrder> {
        let orders = self.pending_orders.lock().await;
        orders.values().cloned().collect()
    }

    /// Get a pending order by ID
    pub async fn get_pending_order(&self, order_id: &str) -> Option<PendingBacktestOrder> {
        let orders = self.pending_orders.lock().await;
        orders.get(order_id).cloned()
    }

    /// Process a Polymarket trade event and check for fills against pending orders.
    ///
    /// Trade-based fill logic:
    /// - BID orders (we're buying) fill when SELL trades cross at or below our bid price
    /// - ASK orders (we're selling) fill when BUY trades cross at or above our ask price
    ///
    /// # Arguments
    /// * `trade` - The Polymarket trade event to process
    /// * `inventory` - Map of asset_id to current inventory. Used for inventory constraint checking
    ///   when `enforce_inventory_constraints` is enabled.
    /// * `available_quote` - Available quote balance. Buy fills that would exceed this are skipped.
    ///
    /// # Returns
    /// A vector of `LimitOrderEvent`s for any orders that were filled (partially or fully).
    pub async fn handle_polymarket_trade(
        &self,
        trade: &PolymarketTradeEvent,
        inventory: &HashMap<String, f64>,
        available_quote: f64,
    ) -> Result<Vec<LimitOrderEvent>> {
        self.handle_polymarket_trade_with_fill_engine(trade, inventory, available_quote)
            .await
    }

    /// Returns whether inventory constraints are enforced.
    pub fn enforces_inventory_constraints(&self) -> bool {
        self.enforce_inventory_constraints
    }
}

#[async_trait]
impl OrderExecutor for BacktestOrderExecutor {
    #[instrument(skip(self))]
    async fn execute_market_order(&self, order: Order) -> Result<ExecutionEvent> {
        let price = {
            let prices = self.last_prices.lock().await;
            prices
                .get(&order.mint)
                .ok_or_else(|| anyhow!("No price data for {}", order.mint))?
                .clone()
        };

        // Apply slippage based on order direction
        #[allow(deprecated)]
        let (amount, executed_price, clear_position, slippage) = match &order.order_type {
            OrderType::Buy { sol_amount }
            | OrderType::MarketBuy {
                quote_amount: sol_amount,
            } => (
                *sol_amount,
                price * (1.0 + self.buy_slippage), // Buy slippage
                false,
                self.buy_slippage,
            ),
            OrderType::Sell {
                token_amount,
                clear_position,
            }
            | OrderType::MarketSell {
                token_amount,
                clear_position,
            } => (
                -*token_amount,                     // Negative amount for sell
                price * (1.0 - self.sell_slippage), // Sell slippage
                *clear_position,
                self.sell_slippage,
            ),
            OrderType::LimitBuy {
                quote_amount,
                limit_price,
                ..
            } => (*quote_amount, *limit_price, false, 0.0),
            OrderType::LimitSell {
                token_amount,
                limit_price,
                clear_position,
                ..
            } => (-*token_amount, *limit_price, *clear_position, 0.0),
            OrderType::Cancel { order_id } => {
                // Cancel orders are not market orders - they should be handled by cancel_order()
                return Err(anyhow!(
                    "Cancel orders should not be passed to execute_market_order, use cancel_order() instead: {}",
                    order_id
                ));
            }
        };

        Ok(ExecutionEvent::OrderFilled {
            mint: order.mint,
            token_amount_change: amount,
            quote_amount_change: None,
            price: Some(executed_price),
            timestamp: order.timestamp,
            slippage: Some(slippage),
            clear_position,
            force_position_clear: false,
            execution_latency_in_slots: None,
            signal_id: None,
            confirmed_slot: Some(0), // Placeholder for backtest
            confirmed_signature: Some(Signature::default()), // Placeholder for backtest
            exit_mode: order.exit_mode,
        })
    }

    #[instrument(skip(self))]
    async fn handle_token_trade(&self, trade: &TokenTradeEvent) -> Result<()> {
        if let Some(price) = trade.spot_price() {
            let mut prices = self.last_prices.lock().await;
            prices.insert(trade.base_token_mint().to_string(), price);
        }
        Ok(())
    }

    async fn handle_orderbook_snapshot(&self, snapshot: &OrderbookSnapshotEvent) -> Result<()> {
        self.record_market_asset(&snapshot.market, &snapshot.asset_id)
            .await;
        Ok(())
    }

    /// Execute a limit order by adding it to the pending orders map.
    ///
    /// For ImmediateOrCancel (IOC) orders, this implementation does not attempt
    /// immediate fill checking - it simply places the order and relies on
    /// subsequent trade events to trigger fills.
    #[instrument(skip(self))]
    async fn execute_limit_order(&self, order: Order) -> Result<LimitOrderEvent> {
        self.execute_limit_order_with_lifecycle(order).await
    }

    /// Cancel a pending limit order by its order ID.
    #[instrument(skip(self))]
    async fn cancel_order(&self, order_id: &str) -> Result<LimitOrderEvent> {
        self.cancel_order_with_lifecycle(order_id).await
    }

    /// Returns true - the backtest executor supports limit orders.
    fn supports_limit_orders(&self) -> bool {
        true
    }

    /// Execute a pair redemption operation in backtest mode.
    ///
    /// For binary markets, this simulates redeeming paired Up + Down tokens for
    /// the underlying quote currency. Each pair redeems for exactly $1.00.
    ///
    /// The backtest executor simply returns a successful redemption event.
    /// The PositionManager handles the actual position and balance updates
    /// when it processes the RedemptionEvent.
    #[instrument(skip(self))]
    async fn execute_redemption(&self, action: &RedemptionAction) -> Result<RedemptionEvent> {
        debug!(
            market = %action.market,
            up_asset = %action.up_asset_id,
            down_asset = %action.down_asset_id,
            quantity = action.quantity,
            "Executing backtest redemption"
        );

        // In binary markets, 1 Up + 1 Down = $1.00
        let quote_received = action.quantity;

        Ok(RedemptionEvent::RedemptionCompleted {
            market: action.market.clone(),
            up_asset_id: action.up_asset_id.clone(),
            down_asset_id: action.down_asset_id.clone(),
            quantity: action.quantity,
            quote_received,
            timestamp: action.timestamp,
        })
    }

    /// Returns true - the backtest executor supports pair redemption.
    fn supports_redemption(&self) -> bool {
        true
    }

    /// Check for simulated fills from a Polymarket trade event.
    ///
    /// This implements the `OrderExecutor` trait method by delegating to
    /// `handle_polymarket_trade`. The backtest executor simulates order fills
    /// based on market trade events crossing pending limit order prices.
    async fn check_fills_from_trade(
        &self,
        trade: &PolymarketTradeEvent,
        inventory: &HashMap<String, f64>,
        available_quote: f64,
    ) -> Vec<LimitOrderEvent> {
        self.handle_polymarket_trade(trade, inventory, available_quote)
            .await
            .unwrap_or_default()
    }

    /// Returns true - the backtest executor simulates fills from trade events.
    fn simulates_fills(&self) -> bool {
        true
    }

    fn defers_limit_order_events(&self) -> bool {
        self.latency_config.is_some()
    }
}
