use std::collections::HashMap;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use popeyes_trading_types::{
    OrderbookSnapshotEvent, OrderbookUpdateEvent, PolymarketTradeEvent, TokenTradeEvent,
};

use crate::{
    execution::{
        events::{ExecutionEvent, LimitOrderEvent, OrderSide, RedemptionEvent},
        order::Order,
    },
    signal::{RedemptionAction, TradableSignal},
};

#[async_trait]
pub trait OrderExecutor: Send + Sync + 'static {
    // === Required methods ===

    /// Execute a market order and return execution event
    async fn execute_market_order(&self, order: Order) -> Result<ExecutionEvent>;

    /// Update internal state from trade events.
    /// Can be used to confirm trades.
    async fn handle_token_trade(&self, trade: &TokenTradeEvent) -> Result<()>;

    // === Optional methods with defaults ===

    /// Handle signal data for potential optimization
    async fn handle_signal(&self, _signal: &dyn TradableSignal) -> Result<()> {
        // Default implementation does nothing
        Ok(())
    }

    /// Execute a limit order on an orderbook venue.
    ///
    /// Returns a `LimitOrderEvent::OrderPlaced` on success, or an error if
    /// limit orders are not supported or the order was rejected.
    ///
    /// Default implementation returns an error indicating limit orders
    /// are not supported, allowing existing AMM executors to compile unchanged.
    async fn execute_limit_order(&self, _order: Order) -> Result<LimitOrderEvent> {
        Err(anyhow!("Limit orders not supported by this executor"))
    }

    /// Cancel an existing limit order by its venue-assigned order ID.
    ///
    /// Returns a `LimitOrderEvent::OrderCancelled` on success, or an error
    /// if cancellation is not supported or failed.
    ///
    /// Default implementation returns an error indicating cancellation
    /// is not supported.
    async fn cancel_order(&self, _order_id: &str) -> Result<LimitOrderEvent> {
        Err(anyhow!("Order cancellation not supported by this executor"))
    }

    /// Handle orderbook snapshot events for state tracking or fill detection.
    ///
    /// Called when an `OrderbookSnapshotEvent` is received from the event stream.
    /// Implementations can use this to maintain local orderbook state or detect fills.
    ///
    /// Default implementation does nothing.
    async fn handle_orderbook_snapshot(&self, _snapshot: &OrderbookSnapshotEvent) -> Result<()> {
        Ok(())
    }

    /// Handle orderbook update events for state tracking or fill detection.
    ///
    /// Called when an `OrderbookUpdateEvent` is received from the event stream.
    /// Implementations can use this to maintain local orderbook state or detect fills.
    ///
    /// Default implementation does nothing.
    async fn handle_orderbook_update(&self, _update: &OrderbookUpdateEvent) -> Result<()> {
        Ok(())
    }

    /// Returns whether this executor supports limit orders.
    ///
    /// Can be used by the position handler to determine whether to generate
    /// limit or market orders based on signal parameters.
    ///
    /// Default is false (AMM executors).
    fn supports_limit_orders(&self) -> bool {
        false
    }

    /// Execute a pair redemption operation.
    ///
    /// For binary markets, this redeems paired Up + Down tokens for the
    /// underlying quote currency (e.g., USDC).
    ///
    /// Default implementation returns an error indicating redemption
    /// is not supported by this executor.
    async fn execute_redemption(&self, _action: &RedemptionAction) -> Result<RedemptionEvent> {
        Err(anyhow!("Pair redemption not supported by this executor"))
    }

    /// Returns whether this executor supports pair redemption.
    ///
    /// Can be used to determine whether redemption policies should be enforced.
    ///
    /// Default is false.
    fn supports_redemption(&self) -> bool {
        false
    }

    /// Check for simulated fills from a trade event.
    ///
    /// This method is used by simulation executors (backtest, paper trading) to
    /// detect when pending limit orders should be filled based on market trade events.
    ///
    /// Trade-based fill logic:
    /// - BID orders (we're buying) fill when SELL trades cross at or below our bid price
    /// - ASK orders (we're selling) fill when BUY trades cross at or above our ask price
    ///
    /// # Arguments
    /// * `trade` - The Polymarket trade event to check against pending orders
    /// * `inventory` - Map of asset_id to current inventory. Used for inventory constraint
    ///   checking (e.g., preventing naked shorts).
    /// * `available_quote` - Available quote balance for buying. Used to prevent buy fills
    ///   that would exceed available funds.
    ///
    /// # Returns
    /// A vector of `LimitOrderEvent`s for any orders that were filled (partially or fully).
    /// Live executors return an empty vector since fills are detected via other mechanisms.
    ///
    /// # Default Implementation
    /// Returns an empty vector, suitable for live executors that don't simulate fills.
    async fn check_fills_from_trade(
        &self,
        _trade: &PolymarketTradeEvent,
        _inventory: &HashMap<String, f64>,
        _available_quote: f64,
    ) -> Vec<LimitOrderEvent> {
        vec![]
    }

    /// Returns whether this executor simulates fills from trade events.
    ///
    /// When true, the TradeServer will route trade events to `check_fills_from_trade()`
    /// for fill detection. This is used by backtest and paper trading executors.
    ///
    /// Live executors return false since they rely on venue-specific fill notifications.
    ///
    /// Default is false.
    fn simulates_fills(&self) -> bool {
        false
    }

    /// Returns whether this executor defers limit order lifecycle events.
    ///
    /// When true, the caller should not enqueue the immediate result of
    /// `execute_limit_order` or `cancel_order`, because the executor will
    /// emit the corresponding OrderPlaced/OrderCancelled events later (e.g. to
    /// model placement/cancellation latency in backtests).
    fn defers_limit_order_events(&self) -> bool {
        false
    }

    /// Returns whether this executor defers cancellation events until terminal venue evidence.
    ///
    /// This allows executors to acknowledge cancel commands immediately while emitting
    /// terminal cancellation events from poller/websocket evidence.
    ///
    /// By default this follows `defers_limit_order_events()`, which is suitable for
    /// simulation executors that defer both placement and cancellation lifecycle events.
    fn defers_cancel_order_events(&self) -> bool {
        self.defers_limit_order_events()
    }
}

/// Convert a limit order fill event to an execution event.
///
/// This helper function provides a single implementation of the fill-to-execution
/// conversion logic used by both TradeServer (for paper trading) and backtest.
/// It extracts fill information from `LimitOrderEvent::OrderPartiallyFilled` and
/// creates the corresponding `ExecutionEvent::OrderFilled`.
///
/// # Arguments
/// * `fill_event` - The limit order fill event to convert
///
/// # Returns
/// `Some(ExecutionEvent)` if the input was a fill event, `None` otherwise.
pub fn fill_event_to_execution_event(fill_event: &LimitOrderEvent) -> Option<ExecutionEvent> {
    if let LimitOrderEvent::OrderPartiallyFilled {
        order_id: _,
        mint,
        side,
        filled_size,
        remaining_size: _,
        fill_price,
        timestamp,
        signal_id,
        exit_mode,
    } = fill_event
    {
        Some(ExecutionEvent::OrderFilled {
            mint: mint.clone(),
            token_amount_change: match side {
                OrderSide::Buy => *filled_size,
                OrderSide::Sell => -*filled_size,
            },
            quote_amount_change: Some(match side {
                OrderSide::Buy => -filled_size * fill_price,
                OrderSide::Sell => filled_size * fill_price,
            }),
            price: Some(*fill_price),
            timestamp: *timestamp,
            slippage: Some(0.0),
            clear_position: false,
            force_position_clear: false,
            execution_latency_in_slots: None,
            signal_id: signal_id.clone(),
            confirmed_slot: None,
            confirmed_signature: None,
            exit_mode: *exit_mode,
        })
    } else {
        None
    }
}

pub struct NoopOrderExecutor;

#[async_trait]
impl OrderExecutor for NoopOrderExecutor {
    async fn execute_market_order(&self, order: Order) -> Result<ExecutionEvent> {
        Ok(ExecutionEvent::OrderRejected {
            mint: order.mint,
            reason: "No-op executor".to_string(),
        })
    }

    async fn handle_token_trade(&self, _trade: &TokenTradeEvent) -> Result<()> {
        Ok(())
    }
}
