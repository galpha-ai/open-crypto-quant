use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::{
    domain::TimerEvent,
    execution::{ExecutionEvent, LimitOrderEvent, Order, OrderSide, RedemptionEvent},
    position::{
        ActiveExitOrder, ExitMode, Position, PositionEvent, errors::PositionError,
        pending_order::InFlightOrder, position_query::ExchangePosition,
    },
    signal::{OrderIntent, RedemptionAction, RedemptionPolicy, TradableSignal},
};

#[async_trait]
pub trait PositionManager: Send + Sync {
    // === Required methods ===

    async fn handle_execution(
        &self,
        event: &ExecutionEvent,
    ) -> Result<PositionEvent, PositionError>;
    async fn get_position(&self, mint: &str) -> Option<Position>;
    async fn update_price(
        &self,
        mint: &str,
        price: f64,
        timestamp: DateTime<Utc>,
    ) -> Result<PositionEvent, PositionError>;

    /// Handle a signal and potentially generate an order for execution
    /// Returns Ok(Some(Order)) if an order should be executed, Ok(None) if no action is needed
    async fn handle_signal(
        &self,
        signal: &dyn TradableSignal,
    ) -> Result<Option<Order>, PositionError>;

    /// Handle timer events for periodic position maintenance
    /// Returns Ok(()) if successful, Err(PositionError) if any issues occur
    async fn handle_timer(&self, event: &TimerEvent) -> Result<Vec<Order>, PositionError>;

    /// Get current available quote currency balance
    async fn get_available_quote(&self) -> f64;

    /// Get cumulative quote currency received from all sells
    async fn get_total_quote_received(&self) -> f64;

    /// Get cumulative quote currency spent on all buys
    async fn get_total_quote_spent(&self) -> f64;

    /// Get net cash flow (received - spent)
    /// This represents realized profit/loss from completed round-trips
    async fn get_net_cash_flow(&self) -> f64 {
        self.get_total_quote_received().await - self.get_total_quote_spent().await
    }

    /// Get total PnL including unrealized inventory value
    /// Formula: net_cash_flow + sum(position.amount * position.current_price)
    async fn get_total_pnl(&self) -> f64;

    /// Get total unrealized PnL from open positions
    /// Formula: sum(position.amount * (current_price - entry_price))
    async fn get_total_unrealized_pnl(&self) -> f64;

    /// Get total number of closed positions
    async fn get_total_closed_positions(&self) -> u32;

    /// Get number of winning trades
    async fn get_winning_trades(&self) -> u32;

    /// Get the number of currently open positions
    async fn get_open_position_count(&self) -> u32;

    /// Try mark a mint in pending sell state
    async fn try_mark_pending_sell(&self, mint: &str) -> bool;

    /// Try to mark a mint for buying.
    async fn try_mark_for_buying(&self, mint: &str) -> bool;

    /// Remove a position from the manager, typically used when an on-chain check reveals
    /// the position doesn't actually exist despite the manager thinking it does.
    /// Returns the removed position if it existed.
    async fn remove_position(&self, mint: &str) -> Result<Option<Position>, PositionError>;

    /// Get a list of all currently open positions according to the manager.
    async fn get_all_open_positions(&self) -> Vec<Position>;

    /// Clears the pending sell flag for a given mint. Should be called after a sell confirmation failure.
    async fn clear_pending_sell(&self, mint: &str) -> Result<(), PositionError>;

    /// Increments the sell failure counter for a given mint.
    async fn increment_sell_failure_count(&self, mint: &str) -> Result<(), PositionError>;

    /// Add an orphan position to the manager
    /// This is used when the position exists on-chain but is not tracked by the manager
    async fn add_orphan_position(&self, mint: String, amount: u64) -> Result<(), PositionError>;

    /// Get all mints that have ever been bought (for orphan position detection)
    /// Returns a list of all token IDs that were ever traded, even if positions are now closed
    async fn get_all_bought_mints(&self) -> Vec<String>;

    /// Synchronize available quote balance with actual balance from exchange.
    /// Called when balance drift is detected during polling.
    ///
    /// Default implementation does nothing (no-op for non-tracking managers).
    async fn sync_available_quote(&self, _actual_balance: f64) -> Result<(), PositionError> {
        Ok(())
    }

    // === Optional orderbook methods with defaults ===

    /// Update position price with bid/ask spread.
    ///
    /// For orderbook markets, positions should be valued using the exit price:
    /// - Long positions: valued at bid (price you could sell at)
    /// - Short positions: valued at ask (price you would need to buy at)
    ///
    /// Default implementation uses mid price, which is suitable for display
    /// and for AMM markets where there's no spread.
    async fn update_price_with_spread(
        &self,
        mint: &str,
        bid: f64,
        ask: f64,
        timestamp: DateTime<Utc>,
    ) -> Result<PositionEvent, PositionError> {
        // Default: use mid price for AMM compatibility
        let mid = (bid + ask) / 2.0;
        self.update_price(mint, mid, timestamp).await
    }

    /// Track a pending limit order for a position.
    ///
    /// Used to track limit orders that have been placed but not yet filled.
    /// This allows the position manager to prevent duplicate orders and
    /// track order state.
    ///
    /// Default implementation does nothing (no-op).
    async fn add_pending_limit_order(&self, _order: &Order) -> Result<(), PositionError> {
        Ok(())
    }

    /// Get all pending limit orders for a given mint.
    ///
    /// Returns orders that have been placed but not yet filled or cancelled.
    ///
    /// Default implementation returns empty vector.
    async fn get_pending_limit_orders(&self, _mint: &str) -> Vec<Order> {
        vec![]
    }

    /// Remove a pending limit order by its venue order ID.
    ///
    /// Called when an order is filled, cancelled, or expired.
    /// Returns the removed order if it existed.
    ///
    /// Default implementation returns None (no-op).
    async fn remove_pending_limit_order(
        &self,
        _order_id: &str,
    ) -> Result<Option<Order>, PositionError> {
        Ok(None)
    }

    // === Active exit order management (for strategy-driven exits) ===

    /// Set an active exit order for a position.
    ///
    /// Used to track limit orders that have been placed to exit a position.
    /// Only one active exit order per position is supported.
    ///
    /// Default implementation does nothing (no-op).
    async fn set_active_exit_order(
        &self,
        _mint: &str,
        _order: ActiveExitOrder,
    ) -> Result<(), PositionError> {
        Ok(())
    }

    /// Clear the active exit order for a position.
    ///
    /// Called when an exit order is filled, cancelled, or expired.
    ///
    /// Default implementation does nothing (no-op).
    async fn clear_active_exit_order(&self, _mint: &str) -> Result<(), PositionError> {
        Ok(())
    }

    /// Get the active exit order for a position, if any.
    ///
    /// Returns the active exit order or None if no exit order is active.
    ///
    /// Default implementation returns None.
    async fn get_active_exit_order(&self, _mint: &str) -> Option<ActiveExitOrder> {
        None
    }

    /// Set the exit mode for a position.
    ///
    /// - `Automatic`: ExitStrategy controls timing, market orders generated
    /// - `StrategyManaged`: SignalGenerator manages exits via Exit signals
    ///
    /// Default implementation does nothing (no-op).
    async fn set_exit_mode(&self, _mint: &str, _mode: ExitMode) -> Result<(), PositionError> {
        Ok(())
    }

    // === In-flight order tracking (for race condition prevention) ===

    /// Register an order as in-flight before execution starts.
    ///
    /// This should be called immediately before spawning order execution
    /// to prevent duplicate orders from being generated by subsequent
    /// `reconcile_intent()` calls while execution is pending.
    ///
    /// # Arguments
    ///
    /// * `in_flight_id` - Unique ID for this in-flight order (typically UUID)
    /// * `order` - The in-flight order details
    ///
    /// # Default Implementation
    ///
    /// No-op. Override for orderbook venue support.
    fn register_in_flight_order(&self, _in_flight_id: String, _order: InFlightOrder) {
        // Default: do nothing
    }

    /// Remove an in-flight order by its ID.
    ///
    /// Called on execution failure to clean up the in-flight tracking.
    ///
    /// # Default Implementation
    ///
    /// No-op. Override for orderbook venue support.
    fn remove_in_flight_order(&self, _mint: &str, _in_flight_id: &str) {
        // Default: do nothing
    }

    /// Remove an in-flight order that matches the given order parameters.
    ///
    /// Called when `LimitOrderEvent::OrderPlaced` is received to transition
    /// the order from in-flight to pending state.
    ///
    /// # Default Implementation
    ///
    /// No-op. Override for orderbook venue support.
    fn remove_in_flight_order_by_level(
        &self,
        _mint: &str,
        _side: OrderSide,
        _price: f64,
        _size: f64,
    ) {
        // Default: do nothing
    }

    // === Intent-based reconciliation methods (for orderbook market making) ===

    /// Reconcile current order state with desired intent.
    ///
    /// Compares the desired order state (from `OrderIntent`) against current
    /// pending orders and returns the orders needed to reach the desired state.
    /// Orders are returned in cancel-first order (cancels before new placements).
    ///
    /// This method is called when processing intent-based signals
    /// (`is_intent_signal() = true`).
    ///
    /// # Arguments
    ///
    /// * `intent` - The desired order book state from the strategy
    ///
    /// # Returns
    ///
    /// A vector of `Order` objects representing the actions needed:
    /// - `OrderType::Cancel` for orders that need to be cancelled
    /// - `OrderType::LimitBuy`/`LimitSell` for new orders to place
    ///
    /// # Default Implementation
    ///
    /// Returns empty vector (no-op for AMM-only managers).
    async fn reconcile_intent(&self, _intent: &OrderIntent) -> Result<Vec<Order>, PositionError> {
        Ok(vec![])
    }

    /// Handle limit order lifecycle events from the venue.
    ///
    /// Updates internal pending order tracking based on venue events.
    /// This keeps the PositionManager's view of pending orders in sync
    /// with the actual state at the venue.
    ///
    /// # Event Handling
    ///
    /// | Event | Action |
    /// |-------|--------|
    /// | `OrderPlaced` | Add to pending orders map |
    /// | `OrderPartiallyFilled` | Update `remaining_size` |
    /// | `OrderFilled` | Remove from pending orders |
    /// | `OrderCancelled` | Remove from pending orders |
    /// | `OrderExpired` | Remove from pending orders |
    /// | `OrderRejected` | Do not add (was never pending) |
    ///
    /// # Default Implementation
    ///
    /// No-op. Override for orderbook venue support.
    async fn handle_limit_order_event(
        &self,
        _event: &LimitOrderEvent,
    ) -> Result<(), PositionError> {
        Ok(())
    }

    // === Position reconciliation methods ===

    /// Reconcile position with exchange data.
    ///
    /// Updates the local position state to match the authoritative exchange
    /// position. This is called by the `PositionReconciler` when drift is
    /// detected between local and exchange state.
    ///
    /// # Arguments
    ///
    /// * `exchange_pos` - The authoritative position data from the exchange
    ///
    /// # Returns
    ///
    /// A `PositionEvent::PositionUpdated` with `PositionUpdateSource::Reconciliation`
    /// containing the drift information.
    ///
    /// # Default Implementation
    ///
    /// Returns an error. Implementations that support reconciliation must override.
    async fn reconcile_position(
        &self,
        _exchange_pos: &ExchangePosition,
    ) -> Result<PositionEvent, PositionError> {
        Err(PositionError::ReconciliationFailed(
            "reconcile_position not implemented".to_string(),
        ))
    }

    // === Redemption policy methods (for binary market pairs) ===

    /// Update the redemption policy for a market.
    ///
    /// Called when processing orderbook intents that include a policy.
    /// The policy specifies the maximum unredeemed pair value to hold.
    ///
    /// # Default Implementation
    ///
    /// No-op. Override for binary market support.
    fn update_redemption_policy(&self, _policy: RedemptionPolicy) {
        // Default: do nothing
    }

    /// Reconcile current positions against redemption policies.
    ///
    /// Returns redemption actions for any policies that are violated.
    /// Should be called:
    /// - When a new policy arrives (via update_redemption_policy)
    /// - When positions change (after fills)
    ///
    /// # Default Implementation
    ///
    /// Returns empty vector (no-op).
    fn reconcile_redemption_policies(&self) -> Vec<RedemptionAction> {
        vec![]
    }

    /// Handle a completed redemption by updating positions.
    ///
    /// Called when a redemption event is received. Updates the positions
    /// for both Up and Down assets to reflect the redemption.
    ///
    /// # Default Implementation
    ///
    /// No-op.
    async fn handle_redemption(
        &self,
        _event: &RedemptionEvent,
    ) -> Result<Vec<PositionEvent>, PositionError> {
        Ok(vec![])
    }

    /// Get the current position quantity for an asset.
    ///
    /// Returns 0.0 if no position exists.
    ///
    /// # Default Implementation
    ///
    /// Returns 0.0.
    fn get_position_quantity(&self, _asset_id: &str) -> f64 {
        0.0
    }
}
