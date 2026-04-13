use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use prometheus::Registry;
use tracing::{debug, error, info};

use crate::{
    domain::TimerEvent,
    execution::{ExecutionEvent, LimitOrderEvent, Order, OrderSide, RedemptionEvent},
    position::{
        Position, PositionEvent, PositionManager, PositionUpdateSource,
        balance_tracker::calculate_pnl_pct,
        errors::PositionError,
        exit_strategy::ExitStrategy,
        handlers::{amm_signal, fills, reconciliation, redemption, timer_exit},
        metrics::PositionManagerMetrics,
        orderbook::{intent, limit_orders},
        pending_order::{InFlightOrder, PendingLimitOrder},
        position_query::ExchangePosition,
        reconciliation::ReconciliationEngine,
        state::PositionManagerState,
    },
    signal::{OrderIntent, RedemptionAction, RedemptionPolicy, TradableSignal},
};

pub struct InMemoryPositionManager {
    state: Mutex<PositionManagerState>,
    trade_amount_sol: f64,
    max_holding_period: chrono::Duration,
    metrics: Arc<PositionManagerMetrics>,
    max_open_positions: u32, // Maximum number of positions that can be open simultaneously
    exit_strategy: Arc<dyn ExitStrategy>,
    reconciliation_engine: ReconciliationEngine,
    min_quote_lifetime_ms: Option<u64>,
}

impl InMemoryPositionManager {
    // Getter for max_holding_period
    pub fn get_max_holding_period(&self) -> chrono::Duration {
        self.max_holding_period
    }

    // Getter for pending_sells
    pub fn get_pending_sells(&self) -> HashSet<String> {
        self.state.lock().unwrap().pending_sells.clone()
    }

    // Check if a mint is in pending_sells
    pub fn is_pending_sell(&self, mint: &str) -> bool {
        self.state.lock().unwrap().pending_sells.contains(mint)
    }

    pub fn new(
        trade_amount_sol: f64,
        initial_sol: f64,
        max_holding_period: chrono::Duration,
        max_open_positions: u32,
        registry: &Registry,
        exit_strategy: Arc<dyn ExitStrategy>,
    ) -> Self {
        Self::new_with_quote_lifetime(
            trade_amount_sol,
            initial_sol,
            max_holding_period,
            max_open_positions,
            None,
            registry,
            exit_strategy,
        )
    }

    pub fn new_with_quote_lifetime(
        trade_amount_sol: f64,
        initial_sol: f64,
        max_holding_period: chrono::Duration,
        max_open_positions: u32,
        min_quote_lifetime_ms: Option<u64>,
        registry: &Registry,
        exit_strategy: Arc<dyn ExitStrategy>,
    ) -> Self {
        tracing::info!(
            trade_amount_sol,
            initial_sol,
            max_holding_period_secs = max_holding_period.num_seconds(),
            max_open_positions,
            "Creating InMemoryPositionManager"
        );
        let state = PositionManagerState::new(initial_sol);

        let metrics = Arc::new(
            PositionManagerMetrics::new(registry)
                .expect("Failed to create position manager metrics"),
        );

        Self {
            state: Mutex::new(state),
            trade_amount_sol,
            max_holding_period,
            metrics,
            max_open_positions,
            exit_strategy,
            reconciliation_engine: ReconciliationEngine::new(),
            min_quote_lifetime_ms,
        }
    }

    fn with_state<R>(&self, f: impl FnOnce(&PositionManagerState) -> R) -> R {
        let state = self.state.lock().unwrap();
        f(&state)
    }

    fn with_state_mut<R>(&self, f: impl FnOnce(&mut PositionManagerState) -> R) -> R {
        let mut state = self.state.lock().unwrap();
        f(&mut state)
    }
}

#[async_trait]
impl PositionManager for InMemoryPositionManager {
    async fn get_available_quote(&self) -> f64 {
        self.state.lock().unwrap().available_quote
    }

    async fn get_total_quote_received(&self) -> f64 {
        self.state.lock().unwrap().total_quote_received
    }

    async fn get_total_quote_spent(&self) -> f64 {
        self.state.lock().unwrap().total_quote_spent
    }

    async fn get_total_pnl(&self) -> f64 {
        let state = self.state.lock().unwrap();
        let net_cash_flow = state.total_quote_received - state.total_quote_spent;

        // Calculate inventory mark-to-market value
        let inventory_mtm: f64 = state
            .positions
            .values()
            .filter_map(|p| p.current_price.map(|price| p.amount * price))
            .sum();

        net_cash_flow + inventory_mtm
    }

    async fn get_total_unrealized_pnl(&self) -> f64 {
        let state = self.state.lock().unwrap();
        state
            .positions
            .values()
            .filter_map(|p| match (p.current_price, p.entry_price) {
                (Some(current), Some(entry)) => Some(p.amount * (current - entry)),
                _ => None,
            })
            .sum()
    }

    async fn get_total_closed_positions(&self) -> u32 {
        self.state.lock().unwrap().total_closed_positions
    }

    async fn get_winning_trades(&self) -> u32 {
        self.state.lock().unwrap().winning_trades
    }

    async fn get_open_position_count(&self) -> u32 {
        self.state.lock().unwrap().positions.len() as u32
    }

    async fn try_mark_for_buying(&self, mint: &str) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.bought_mints.contains(mint) {
            false // Already bought this mint
        } else {
            state.bought_mints.insert(mint.to_string());
            true // Successfully marked for buying
        }
    }

    async fn remove_position(&self, mint: &str) -> Result<Option<Position>, PositionError> {
        let mut state = self.state.lock().unwrap();
        Ok(state.positions.remove(mint))
    }

    async fn get_all_open_positions(&self) -> Vec<Position> {
        let state = self.state.lock().unwrap();
        state.positions.values().cloned().collect()
    }

    async fn handle_execution(
        &self,
        event: &ExecutionEvent,
    ) -> Result<PositionEvent, PositionError> {
        self.with_state_mut(|state| {
            fills::handle_execution(
                state,
                event,
                self.exit_strategy.as_ref(),
                self.metrics.as_ref(),
            )
        })
    }

    async fn get_position(&self, mint: &str) -> Option<Position> {
        self.state.lock().unwrap().positions.get(mint).cloned()
    }

    async fn update_price(
        &self,
        mint: &str,
        price: f64,
        timestamp: DateTime<Utc>,
    ) -> Result<PositionEvent, PositionError> {
        self.metrics.update_price_calls.with_label_values(&[]).inc();
        let mut state = self.state.lock().unwrap();
        if let Some(position) = state.positions.get_mut(mint) {
            position.current_price = Some(price);
            position.current_price_updated_time = timestamp;
            position.pnl_pct = calculate_pnl_pct(position);
            Ok(PositionEvent::PositionUpdated {
                position: position.clone(),
                source: PositionUpdateSource::PriceUpdate,
                available_quote: state.available_quote,
            })
        } else {
            Err(PositionError::PositionNotFound(mint.to_string()))
        }
    }

    async fn handle_signal(
        &self,
        signal: &dyn TradableSignal,
    ) -> Result<Option<Order>, PositionError> {
        self.with_state(|state| {
            amm_signal::handle_signal(
                state,
                self.trade_amount_sol,
                self.max_open_positions,
                signal,
            )
        })
    }

    #[tracing::instrument(skip(self, event), level = "trace")]
    async fn handle_timer(&self, event: &TimerEvent) -> Result<Vec<Order>, PositionError> {
        self.metrics
            .timer_events_handled
            .with_label_values(&[])
            .inc();
        let positions_to_sell = self.with_state(|state| {
            timer_exit::collect_positions_to_sell(state, self.exit_strategy.as_ref(), event)
        });
        timer_exit::build_exit_orders(positions_to_sell, self.exit_strategy.as_ref(), event)
    }

    // New atomic method that checks and updates pending_sells status
    async fn try_mark_pending_sell(&self, mint: &str) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.pending_sells.contains(mint) {
            false // Already pending, can't add
        } else {
            state.pending_sells.insert(mint.to_string());
            true // Successfully marked as pending
        }
    }

    async fn clear_pending_sell(&self, mint: &str) -> Result<(), PositionError> {
        let mut state = self.state.lock().unwrap();
        if state.pending_sells.remove(mint) {
            info!(mint, "Cleared pending sell flag");
            Ok(())
        } else {
            // This might happen if it was already cleared or never existed
            info!(
                mint,
                "Attempted to clear pending sell flag, but it was not set"
            );
            Ok(()) // Not an error state, just means it wasn't pending
        }
    }

    async fn increment_sell_failure_count(&self, mint: &str) -> Result<(), PositionError> {
        let mut state = self.state.lock().unwrap();
        if let Some(position) = state.positions.get_mut(mint) {
            position.sell_failure_count += 1;
            info!(
                mint,
                new_count = position.sell_failure_count,
                "Incremented sell failure count"
            );
            Ok(())
        } else {
            error!(
                mint,
                "Attempted to increment sell failure count for non-existent position"
            );
            Err(PositionError::PositionNotFound(mint.to_string()))
        }
    }

    async fn add_orphan_position(&self, mint: String, amount: u64) -> Result<(), PositionError> {
        let timestamp = chrono::Utc::now();
        let position = Position {
            mint: mint.clone(),
            amount: amount as f64, // Convert from u64 to f64
            entry_price: None,     // Unknown entry price
            current_price: None,   // Unknown current price
            current_price_updated_time: timestamp,
            pnl_pct: None,                                                // Unknown PnL
            entry_time: timestamp, // Use current time as entry time
            entry_slot: 0,         // Unknown entry slot
            entry_signature: solana_sdk::signature::Signature::default(), // Default signature
            signal_id: None,       // No signal ID for orphan positions
            sell_failure_count: 0, // Initialize failure count
            active_exit_order: None, // No active exit order
            exit_mode: crate::position::ExitMode::Automatic, // Default to automatic
        };

        info!(
            mint = mint,
            amount = amount,
            "Adding orphan position to manager"
        );

        let mut state = self.state.lock().unwrap();
        // Mark mint as bought to prevent repeat buys
        state.bought_mints.insert(mint.clone());
        // Add position to managed positions
        state.positions.insert(mint, position);

        // Update metrics
        self.metrics
            .orphan_positions_added
            .with_label_values(&[])
            .inc();

        Ok(())
    }

    async fn get_all_bought_mints(&self) -> Vec<String> {
        let state = self.state.lock().unwrap();
        state.bought_mints.iter().cloned().collect()
    }

    async fn sync_available_quote(&self, actual_balance: f64) -> Result<(), PositionError> {
        let mut state = self.state.lock().unwrap();
        let previous_balance = state.available_quote;

        state.available_quote = actual_balance;

        info!(
            previous_balance = previous_balance,
            actual_balance = actual_balance,
            diff = actual_balance - previous_balance,
            "Synchronized available_quote with exchange balance"
        );

        Ok(())
    }

    async fn set_active_exit_order(
        &self,
        mint: &str,
        order: crate::position::ActiveExitOrder,
    ) -> Result<(), PositionError> {
        let mut state = self.state.lock().unwrap();
        if let Some(position) = state.positions.get_mut(mint) {
            info!(
                mint = mint,
                order_id = %order.order_id,
                price = order.price,
                size = order.size,
                "Setting active exit order for position"
            );
            position.active_exit_order = Some(order);
            Ok(())
        } else {
            Err(PositionError::PositionNotFound(mint.to_string()))
        }
    }

    async fn clear_active_exit_order(&self, mint: &str) -> Result<(), PositionError> {
        let mut state = self.state.lock().unwrap();
        if let Some(position) = state.positions.get_mut(mint) {
            if position.active_exit_order.is_some() {
                info!(mint = mint, "Clearing active exit order for position");
            }
            position.active_exit_order = None;
            Ok(())
        } else {
            Err(PositionError::PositionNotFound(mint.to_string()))
        }
    }

    async fn get_active_exit_order(&self, mint: &str) -> Option<crate::position::ActiveExitOrder> {
        let state = self.state.lock().unwrap();
        state
            .positions
            .get(mint)
            .and_then(|pos| pos.active_exit_order.clone())
    }

    async fn set_exit_mode(
        &self,
        mint: &str,
        mode: crate::position::ExitMode,
    ) -> Result<(), PositionError> {
        let mut state = self.state.lock().unwrap();
        if let Some(position) = state.positions.get_mut(mint) {
            info!(
                mint = mint,
                exit_mode = ?mode,
                "Setting exit mode for position"
            );
            position.exit_mode = mode;
            Ok(())
        } else {
            Err(PositionError::PositionNotFound(mint.to_string()))
        }
    }

    // === In-flight order tracking ===

    fn register_in_flight_order(&self, in_flight_id: String, order: InFlightOrder) {
        let mut state = self.state.lock().unwrap();
        debug!(
            in_flight_id = %in_flight_id,
            mint = %order.mint,
            side = ?order.side,
            price = order.price,
            size = order.size,
            "Registering in-flight order"
        );
        state.add_in_flight_order(order.mint.clone(), in_flight_id, order);
    }

    fn remove_in_flight_order(&self, mint: &str, in_flight_id: &str) {
        let mut state = self.state.lock().unwrap();
        if let Some(order) = state.remove_in_flight_order(mint, in_flight_id) {
            debug!(
                in_flight_id = %in_flight_id,
                mint = %mint,
                side = ?order.side,
                price = order.price,
                "Removed in-flight order by ID"
            );
        }
    }

    fn remove_in_flight_order_by_level(&self, mint: &str, side: OrderSide, price: f64, size: f64) {
        let mut state = self.state.lock().unwrap();
        if let Some(order) = state.remove_in_flight_order_by_level(mint, side, price, size) {
            debug!(
                in_flight_id = %order.id,
                mint = %mint,
                side = ?side,
                price = price,
                size = size,
                "Removed in-flight order by level match"
            );
        }
    }

    // === Intent-based reconciliation methods ===

    async fn reconcile_intent(&self, intent: &OrderIntent) -> Result<Vec<Order>, PositionError> {
        self.with_state_mut(|state| {
            intent::reconcile_intent(
                state,
                &self.reconciliation_engine,
                self.min_quote_lifetime_ms,
                intent,
            )
        })
    }

    async fn handle_limit_order_event(&self, event: &LimitOrderEvent) -> Result<(), PositionError> {
        self.with_state_mut(|state| limit_orders::handle_limit_order_event(state, event))
    }

    // === Position reconciliation methods ===

    async fn reconcile_position(
        &self,
        exchange_pos: &ExchangePosition,
    ) -> Result<PositionEvent, PositionError> {
        self.with_state_mut(|state| reconciliation::reconcile_position(state, exchange_pos))
    }

    // === Redemption policy methods ===

    fn update_redemption_policy(&self, policy: RedemptionPolicy) {
        self.with_state_mut(|state| redemption::update_redemption_policy(state, policy));
    }

    fn reconcile_redemption_policies(&self) -> Vec<RedemptionAction> {
        self.with_state(|state| redemption::reconcile_redemption_policies(state))
    }

    async fn handle_redemption(
        &self,
        event: &RedemptionEvent,
    ) -> Result<Vec<PositionEvent>, PositionError> {
        self.with_state_mut(|state| redemption::handle_redemption(state, event))
    }

    fn get_position_quantity(&self, asset_id: &str) -> f64 {
        self.with_state(|state| {
            state
                .positions
                .get(asset_id)
                .map(|p| p.amount)
                .unwrap_or(0.0)
        })
    }
}

impl InMemoryPositionManager {
    /// Get all pending orders for a given mint.
    pub fn get_pending_orders_for_mint(&self, mint: &str) -> HashMap<String, PendingLimitOrder> {
        self.with_state(|state| {
            state
                .pending_limit_orders
                .get(mint)
                .cloned()
                .unwrap_or_default()
        })
    }

    /// Add a pending order for testing purposes.
    /// This method is only available in test mode and allows tests to set up
    /// the pending order state directly.
    #[cfg(test)]
    pub fn add_pending_order_for_testing(
        &self,
        mint: &str,
        order_id: &str,
        pending_order: PendingLimitOrder,
    ) {
        self.with_state_mut(|state| {
            let mint_orders = state
                .pending_limit_orders
                .entry(mint.to_string())
                .or_default();
            mint_orders.insert(order_id.to_string(), pending_order);
        });
    }
}
