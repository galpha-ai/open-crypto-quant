//! Event processor for backtest execution.
//!
//! The `BacktestEventProcessor` handles routing and processing of events
//! during a backtest run, including signal routing and order execution.
//!
//! Note: Event capture is handled by the `CapturingEventCoordinator` decorator,
//! not by this processor. The processor focuses solely on event routing and execution.

use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use popeyes_trading_types::MarketDataEvent;
use tracing::{debug, warn};

use crate::{
    domain::SystemEvent,
    event_coordinator::EventCoordinator,
    execution::{
        ExecutionEvent, LimitOrderEvent, Order, OrderExecutor, OrderSide, OrderStatus, OrderType,
        RedemptionEvent, backtest::BacktestOrderExecutor, fill_event_to_execution_event,
    },
    position::{InMemoryPositionManager, PositionManager},
    signal::{SignalAction, SignalGenerator, TradableSignal},
};

use super::{BacktestEventCoordinator, BacktestMetrics};

/// Processes events during backtest execution.
///
/// This struct encapsulates the event handling logic that was previously
/// inline in `BacktestRunner::run()`, providing a cleaner separation of concerns.
///
/// Event capture is handled by the `CapturingEventCoordinator` decorator that wraps
/// the coordinator. This processor focuses on event routing and execution only.
pub struct BacktestEventProcessor<'a> {
    signal_generator: &'a mut Box<dyn SignalGenerator + Send>,
    position_manager: Arc<InMemoryPositionManager>,
    executor: &'a BacktestOrderExecutor,
    coordinator: &'a Arc<BacktestEventCoordinator>,
    persistence: Option<Arc<dyn crate::persistence::TradingEventPersistence>>,
}

impl<'a> BacktestEventProcessor<'a> {
    /// Create a new event processor.
    pub fn new(
        signal_generator: &'a mut Box<dyn SignalGenerator + Send>,
        position_manager: Arc<InMemoryPositionManager>,
        executor: &'a BacktestOrderExecutor,
        coordinator: &'a Arc<BacktestEventCoordinator>,
        persistence: Option<Arc<dyn crate::persistence::TradingEventPersistence>>,
    ) -> Self {
        Self {
            signal_generator,
            position_manager,
            executor,
            coordinator,
            persistence,
        }
    }

    /// Process a single event and update metrics.
    ///
    /// Events are automatically captured by the `CapturingEventCoordinator` decorator
    /// when they are delivered via `next_event()`.
    pub async fn process_event(
        &mut self,
        event: &SystemEvent,
        metrics: &mut BacktestMetrics,
    ) -> Result<()> {
        match event {
            SystemEvent::MarketData(MarketDataEvent::OrderbookSnapshot(snapshot)) => {
                self.handle_orderbook_snapshot(event, snapshot, metrics)
                    .await?;
            }
            SystemEvent::MarketData(MarketDataEvent::PolymarketTrade(trade)) => {
                self.handle_polymarket_trade(trade, metrics).await?;
            }
            SystemEvent::MarketData(MarketDataEvent::SpotPrice(spot_price)) => {
                self.handle_spot_price(event, spot_price, metrics).await?;
            }
            SystemEvent::Timer(timer_event) => {
                self.handle_timer(timer_event, metrics).await?;
            }
            SystemEvent::LimitOrder(limit_event) => {
                self.handle_limit_order(limit_event, metrics).await?;
            }
            SystemEvent::Execution(exec_event) => {
                self.handle_execution(exec_event).await?;
            }
            SystemEvent::Position(pos_event) => {
                debug!(event = ?pos_event, "Position event processed");
                // Forward to signal generator so strategies can update inventory state
                // Position events typically don't generate new signals, but the
                // generator needs to see them to update internal state
                let _signals = self.signal_generator.generate_signal(event).await?;
            }
            SystemEvent::Redemption(redemption_event) => {
                self.handle_redemption(redemption_event, metrics).await?;
            }
            _ => {
                debug!(event_type = event.event_type(), "Unhandled event type");
            }
        }

        Ok(())
    }

    /// Handle an orderbook snapshot event.
    async fn handle_orderbook_snapshot(
        &mut self,
        event: &SystemEvent,
        snapshot: &popeyes_trading_types::OrderbookSnapshotEvent,
        metrics: &mut BacktestMetrics,
    ) -> Result<()> {
        metrics.total_snapshots += 1;

        // Let executor handle orderbook update (for price tracking)
        self.executor.handle_orderbook_snapshot(snapshot).await?;

        // Generate signals
        let signals = self.signal_generator.generate_signal(event).await?;

        for signal in signals {
            metrics.total_signals += 1;

            // Persist signal for analysis if persistence is configured
            if let Some(persistence) = &self.persistence {
                if let Ok(signal_json) = signal.to_json() {
                    let _ = persistence.persist_signal(signal_json).await;
                }
            }

            // Route signal based on type and action
            let orders = self.route_signal(signal.as_ref()).await;

            // Execute all resulting orders
            for order in orders {
                self.execute_order(order, metrics).await?;
            }
        }

        Ok(())
    }

    /// Handle a spot price event.
    async fn handle_spot_price(
        &mut self,
        event: &SystemEvent,
        _spot_price: &popeyes_trading_types::SpotPriceUpdate,
        metrics: &mut BacktestMetrics,
    ) -> Result<()> {
        // Forward spot price events to signal generator for strategies that use spot data
        // (e.g., BS model probability calculations).
        // Most strategies won't generate signals from spot prices (they just update internal state),
        // but if they do, we need to route and execute them.
        let signals = self.signal_generator.generate_signal(event).await?;

        for signal in signals {
            metrics.total_signals += 1;

            // Persist signal for analysis if persistence is configured
            if let Some(persistence) = &self.persistence {
                if let Ok(signal_json) = signal.to_json() {
                    let _ = persistence.persist_signal(signal_json).await;
                }
            }

            // Route signal based on type and action
            let orders = self.route_signal(signal.as_ref()).await;

            // Execute all resulting orders
            for order in orders {
                self.execute_order(order, metrics).await?;
            }
        }

        Ok(())
    }

    /// Handle a Polymarket trade event.
    async fn handle_polymarket_trade(
        &mut self,
        trade: &popeyes_trading_types::PolymarketTradeEvent,
        metrics: &mut BacktestMetrics,
    ) -> Result<()> {
        metrics.total_trades += 1;

        // Build inventory map from current positions for inventory constraint checking
        let inventory: std::collections::HashMap<String, f64> = self
            .position_manager
            .get_all_open_positions()
            .await
            .into_iter()
            .map(|p| (p.mint.clone(), p.amount))
            .collect();

        // Get available quote for Layer 2 quote balance enforcement
        let available_quote = self.position_manager.get_available_quote().await;

        // Process trade through executor for fill simulation using trait method.
        // Note: latency simulation may emit lifecycle events (OrderPlaced/OrderCancelled)
        // in addition to fill events.
        let limit_order_events = self
            .executor
            .check_fills_from_trade(trade, &inventory, available_quote)
            .await;

        for event in limit_order_events {
            match &event {
                LimitOrderEvent::OrderPartiallyFilled { side, .. } => {
                    metrics.total_fills += 1;
                    match side {
                        OrderSide::Buy => metrics.total_bid_fills += 1,
                        OrderSide::Sell => metrics.total_ask_fills += 1,
                    }
                }
                LimitOrderEvent::OrderPlaced { .. } => {
                    metrics.total_orders_placed += 1;
                }
                LimitOrderEvent::OrderCancelled { .. } => {
                    metrics.total_cancels += 1;
                }
                LimitOrderEvent::OrderExpired { .. } => {
                    metrics.total_cancels += 1;
                }
                LimitOrderEvent::OrderRejected { .. } => {
                    // Rejections are typically client- or venue-side failures to accept the order.
                    // Count them as cancels for aggregate backtest reporting.
                    metrics.total_cancels += 1;
                }
            }

            // Convert limit order fill to execution event for position update
            // using shared helper function
            if let Some(exec_event) = fill_event_to_execution_event(&event) {
                // Handle execution event in position manager
                match self.position_manager.handle_execution(&exec_event).await {
                    Ok(pos_event) => {
                        // Enqueue to coordinator so signal generator receives inventory updates
                        // (Event will be captured by the CapturingEventCoordinator)
                        self.coordinator
                            .enqueue_event(SystemEvent::Position(pos_event))
                            .await?;
                    }
                    Err(e) => {
                        debug!(error = %e, "Failed to update position from fill");
                    }
                }
            }

            // Enqueue limit order event for priority processing
            self.coordinator.enqueue_limit_order_events(vec![event]);
        }

        // After processing fills, check if any redemption policies are violated
        // and execute necessary redemptions
        self.process_redemption_actions(metrics).await?;

        Ok(())
    }

    /// Handle a timer event.
    async fn handle_timer(
        &mut self,
        timer_event: &crate::domain::TimerEvent,
        metrics: &mut BacktestMetrics,
    ) -> Result<()> {
        // Check for position exits on timer
        match self.position_manager.handle_timer(timer_event).await {
            Ok(orders) => {
                for order in orders {
                    // Execute exit orders as market orders
                    match self.executor.execute_market_order(order).await {
                        Ok(exec_event) => {
                            metrics.total_orders_placed += 1;
                            metrics.total_fills += 1;

                            // Handle execution event
                            match self.position_manager.handle_execution(&exec_event).await {
                                Ok(pos_event) => {
                                    // Enqueue to coordinator so signal generator receives inventory updates
                                    // (Event will be captured by the CapturingEventCoordinator)
                                    if let Err(e) = self
                                        .coordinator
                                        .enqueue_event(SystemEvent::Position(pos_event))
                                        .await
                                    {
                                        debug!(error = %e, "Failed to enqueue position event");
                                    }
                                }
                                Err(e) => {
                                    debug!(error = %e, "Failed to update position from timer exit");
                                }
                            }

                            // Enqueue execution event for processing
                            // (Event will be captured by the CapturingEventCoordinator)
                            if let Err(e) = self
                                .coordinator
                                .enqueue_event(SystemEvent::Execution(exec_event))
                                .await
                            {
                                debug!(error = %e, "Failed to enqueue execution event");
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "Failed to execute timer exit order");
                        }
                    }
                }
            }
            Err(e) => {
                debug!(error = %e, "Timer handler error");
            }
        }

        Ok(())
    }

    /// Handle a limit order event.
    async fn handle_limit_order(
        &mut self,
        limit_event: &LimitOrderEvent,
        _metrics: &mut BacktestMetrics,
    ) -> Result<()> {
        if let Err(e) = self
            .position_manager
            .handle_limit_order_event(limit_event)
            .await
        {
            debug!(error = %e, "Failed to handle limit order event");
        }

        Ok(())
    }

    /// Handle an execution event.
    async fn handle_execution(&mut self, exec_event: &ExecutionEvent) -> Result<()> {
        match self.position_manager.handle_execution(exec_event).await {
            Ok(pos_event) => {
                // Enqueue position event so signal generator can update inventory state
                // (Event will be captured by the CapturingEventCoordinator)
                if let Err(e) = self
                    .coordinator
                    .enqueue_event(SystemEvent::Position(pos_event))
                    .await
                {
                    debug!(error = %e, "Failed to enqueue position event");
                }
            }
            Err(e) => {
                debug!(error = %e, "Failed to update position from execution");
            }
        }

        Ok(())
    }

    // =========================================================================
    // Signal Routing Methods
    // =========================================================================

    /// Route a signal based on its type and action.
    ///
    /// This mirrors the routing logic in `PositionHandler::handle_signal`:
    /// - Intent-based signals: Reconcile desired state with current orders
    /// - Entry signals: Create new positions
    /// - Exit signals: Close existing positions
    /// - ModifyOrder signals: Cancel and replace existing orders
    /// - CancelOrder signals: Cancel existing orders
    async fn route_signal(&self, signal: &dyn TradableSignal) -> Vec<Order> {
        // Let executor cache the signal (for Polymarket signal routing)
        if let Err(e) = self.executor.handle_signal(signal).await {
            warn!(error = %e, "Failed to cache signal in executor");
        }

        // Check for intent-based signals first (orderbook market making)
        if signal.is_intent_signal() {
            return self.handle_intent_signal(signal).await;
        }

        // Route based on signal action
        match signal.signal_action() {
            SignalAction::Entry => self.handle_entry_signal(signal).await,
            SignalAction::Exit => self.handle_exit_signal(signal).await,
            SignalAction::ModifyOrder => self.handle_modify_order_signal(signal).await,
            SignalAction::CancelOrder => self.handle_cancel_order_signal(signal).await,
        }
    }

    /// Handle an intent-based signal by reconciling with current order state.
    ///
    /// This also processes redemption policies attached to the intent signal,
    /// mirroring the behavior of the live `IntentHandler`.
    async fn handle_intent_signal(&self, signal: &dyn TradableSignal) -> Vec<Order> {
        let intent = match signal.get_order_intent() {
            Some(intent) => intent,
            None => {
                warn!(
                    signal_id = signal.signal_id(),
                    "Intent signal returned None from get_order_intent()"
                );
                return vec![];
            }
        };

        debug!(
            signal_id = %intent.signal_id,
            mint = %intent.mint,
            bid_count = intent.bid_count(),
            ask_count = intent.ask_count(),
            has_redemption_policy = intent.redemption_policy.is_some(),
            "Processing intent signal in backtest"
        );

        // Update redemption policy if present
        if let Some(policy) = &intent.redemption_policy {
            self.position_manager
                .update_redemption_policy(policy.clone());
        }

        match self.position_manager.reconcile_intent(&intent).await {
            Ok(orders) => {
                let cancel_count = orders.iter().filter(|o| o.order_type.is_cancel()).count();
                let placement_count = orders.len() - cancel_count;
                debug!(
                    signal_id = %intent.signal_id,
                    cancels = cancel_count,
                    placements = placement_count,
                    "Reconciliation complete"
                );
                orders
            }
            Err(e) => {
                warn!(
                    error = %e,
                    signal_id = %intent.signal_id,
                    "Failed to reconcile intent"
                );
                vec![]
            }
        }
    }

    /// Handle an entry signal (open a new position).
    async fn handle_entry_signal(&self, signal: &dyn TradableSignal) -> Vec<Order> {
        let mint = match signal.get_mint() {
            Some(mint) => mint,
            None => {
                debug!("Signal does not have a valid mint, skipping");
                return vec![];
            }
        };

        // Check if we've already bought this mint
        if !self.position_manager.try_mark_for_buying(mint).await {
            debug!(
                mint,
                signal_id = signal.signal_id(),
                "Mint has already been bought before, skipping signal"
            );
            return vec![];
        }

        // Generate order from position manager
        match self.position_manager.handle_signal(signal).await {
            Ok(Some(order)) => {
                debug!(
                    mint = order.mint,
                    signal_id = signal.signal_id(),
                    order_type = ?order.order_type,
                    "Generated entry order"
                );
                vec![order]
            }
            Ok(None) => vec![],
            Err(e) => {
                debug!(error = %e, "Entry signal did not result in order");
                vec![]
            }
        }
    }

    /// Handle an exit signal (close an existing position).
    async fn handle_exit_signal(&self, signal: &dyn TradableSignal) -> Vec<Order> {
        let mint = match signal.references_position() {
            Some(mint) => mint.to_string(),
            None => {
                warn!(
                    signal_id = signal.signal_id(),
                    "Exit signal missing position reference (mint)"
                );
                return vec![];
            }
        };

        // Get the position to determine amount to sell
        let position = match self.position_manager.get_position(&mint).await {
            Some(p) => p,
            None => {
                debug!(
                    mint = mint,
                    signal_id = signal.signal_id(),
                    "No position found for exit signal"
                );
                return vec![];
            }
        };

        // Skip if position has zero amount
        if position.amount <= 0.0 {
            debug!(
                mint = mint,
                "Position has zero amount, skipping exit signal"
            );
            return vec![];
        }

        // Check if we can mark this position for selling
        if !self.position_manager.try_mark_pending_sell(&mint).await {
            debug!(
                mint = mint,
                "Sell operation already in progress, skipping exit signal"
            );
            return vec![];
        }

        // Create the exit order based on signal parameters
        let order = if signal.is_limit_order() {
            let limit_price = signal
                .get_limit_price()
                .unwrap_or_else(|| position.current_price.unwrap_or(0.0));
            let time_in_force = signal.get_time_in_force().unwrap_or_default();

            debug!(
                mint = mint,
                signal_id = signal.signal_id(),
                limit_price = limit_price,
                "Creating limit exit order"
            );

            Order::new_limit_sell(
                mint,
                signal.get_market().map(String::from),
                position.amount,
                limit_price,
                true, // clear_position
                time_in_force,
                signal.get_timestamp().unwrap_or_else(Utc::now),
                Some(signal.signal_id().to_string()),
                signal.get_slot(),
            )
        } else {
            debug!(
                mint = mint,
                signal_id = signal.signal_id(),
                "Creating market exit order"
            );

            #[allow(deprecated)]
            Order {
                mint,
                market: signal.get_market().map(String::from),
                order_type: OrderType::MarketSell {
                    token_amount: position.amount,
                    clear_position: true,
                },
                price: position.current_price,
                status: OrderStatus::Pending,
                timestamp: signal.get_timestamp().unwrap_or_else(Utc::now),
                signal_slot: signal.get_slot(),
                signal_id: Some(signal.signal_id().to_string()),
                dex_type: None,
                venue_order_id: None,
                exit_mode: None,
                context: signal.get_context(),
            }
        };

        vec![order]
    }

    /// Handle a modify order signal (cancel existing + place new).
    async fn handle_modify_order_signal(&self, signal: &dyn TradableSignal) -> Vec<Order> {
        let order_id = match signal.references_order_id() {
            Some(id) => id.to_string(),
            None => {
                warn!(
                    signal_id = signal.signal_id(),
                    "ModifyOrder signal missing order_id reference"
                );
                return vec![];
            }
        };

        let mint = match signal.references_position() {
            Some(mint) => mint.to_string(),
            None => {
                warn!(
                    signal_id = signal.signal_id(),
                    "ModifyOrder signal missing position reference (mint)"
                );
                return vec![];
            }
        };

        // Get the position to determine current amount
        let position = match self.position_manager.get_position(&mint).await {
            Some(p) => p,
            None => {
                debug!(
                    mint = mint,
                    signal_id = signal.signal_id(),
                    "No position found for modify order signal"
                );
                return vec![];
            }
        };

        let new_price = match signal.get_limit_price() {
            Some(price) => price,
            None => {
                warn!(
                    signal_id = signal.signal_id(),
                    "ModifyOrder signal missing new price"
                );
                return vec![];
            }
        };

        debug!(
            mint = mint,
            order_id = order_id,
            new_price = new_price,
            signal_id = signal.signal_id(),
            "Modifying exit order in backtest"
        );

        let time_in_force = signal.get_time_in_force().unwrap_or_default();
        let timestamp = signal.get_timestamp().unwrap_or_else(Utc::now);

        // Return cancel order followed by new limit order
        let cancel_order = Order::new_cancel(
            mint.clone(),
            signal.get_market().map(String::from),
            order_id,
            timestamp,
            Some(signal.signal_id().to_string()),
        );

        let new_order = Order::new_limit_sell(
            mint,
            signal.get_market().map(String::from),
            position.amount,
            new_price,
            true, // clear_position
            time_in_force,
            timestamp,
            Some(signal.signal_id().to_string()),
            signal.get_slot(),
        );

        vec![cancel_order, new_order]
    }

    /// Handle a cancel order signal.
    async fn handle_cancel_order_signal(&self, signal: &dyn TradableSignal) -> Vec<Order> {
        let order_id = match signal.references_order_id() {
            Some(id) => id.to_string(),
            None => {
                warn!(
                    signal_id = signal.signal_id(),
                    "CancelOrder signal missing order_id reference"
                );
                return vec![];
            }
        };

        let mint = match signal.references_position() {
            Some(mint) => mint.to_string(),
            None => {
                warn!(
                    signal_id = signal.signal_id(),
                    "CancelOrder signal missing position reference (mint)"
                );
                return vec![];
            }
        };

        debug!(
            mint = mint,
            order_id = order_id,
            signal_id = signal.signal_id(),
            "Cancelling order in backtest"
        );

        // Clear the pending sell flag since we're cancelling
        if let Err(e) = self.position_manager.clear_pending_sell(&mint).await {
            debug!(error = %e, mint = mint, "Failed to clear pending sell on cancel");
        }

        let cancel_order = Order::new_cancel(
            mint,
            signal.get_market().map(String::from),
            order_id,
            signal.get_timestamp().unwrap_or_else(Utc::now),
            Some(signal.signal_id().to_string()),
        );

        vec![cancel_order]
    }

    /// Execute an order and handle the resulting events.
    ///
    /// Limit order events are enqueued via `enqueue_limit_order_events` for priority
    /// processing, and will be captured when returned by `next_event()`.
    /// Other events are enqueued via `enqueue_event` and captured when dequeued.
    async fn execute_order(&self, order: Order, metrics: &mut BacktestMetrics) -> Result<()> {
        // Handle cancel orders
        if order.order_type.is_cancel() {
            if let Some(order_id) = order.order_type.cancel_order_id() {
                match self.executor.cancel_order(order_id).await {
                    Ok(cancel_event) => {
                        // Some executors (e.g. backtest latency simulation) defer lifecycle events
                        // until a later time (e.g. cancellation effective time). In that case,
                        // do not enqueue the immediate return value here.
                        if !self.executor.defers_limit_order_events() {
                            metrics.total_cancels += 1;
                            self.coordinator
                                .enqueue_limit_order_events(vec![cancel_event]);
                        }
                    }
                    Err(e) => {
                        debug!(error = %e, order_id = order_id, "Failed to cancel order");
                    }
                }
            }
            return Ok(());
        }

        // Handle limit orders
        if self.executor.supports_limit_orders() && order.order_type.is_limit() {
            match self.executor.execute_limit_order(order).await {
                Ok(limit_event) => {
                    // Some executors (e.g. backtest latency simulation) defer lifecycle events
                    // (OrderPlaced/OrderCancelled) until a later time. In that case, do not
                    // enqueue the immediate return value here; the executor will emit the
                    // lifecycle events via `check_fills_from_trade()`.
                    if !self.executor.defers_limit_order_events() {
                        metrics.total_orders_placed += 1;
                        // Enqueue for priority processing (captured when returned by next_event)
                        self.coordinator
                            .enqueue_limit_order_events(vec![limit_event]);
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Failed to execute limit order");
                }
            }
            return Ok(());
        }

        // Handle market orders
        match self.executor.execute_market_order(order).await {
            Ok(exec_event) => {
                metrics.total_orders_placed += 1;
                metrics.total_fills += 1;

                // Update position from execution
                match self.position_manager.handle_execution(&exec_event).await {
                    Ok(pos_event) => {
                        // Enqueue to coordinator so signal generator receives inventory updates
                        // (Event will be captured by the CapturingEventCoordinator)
                        self.coordinator
                            .enqueue_event(SystemEvent::Position(pos_event))
                            .await?;
                    }
                    Err(e) => {
                        debug!(error = %e, "Failed to update position from market order");
                    }
                }

                // Enqueue execution event for processing
                // (Event will be captured by the CapturingEventCoordinator)
                self.coordinator
                    .enqueue_event(SystemEvent::Execution(exec_event))
                    .await?;
            }
            Err(e) => {
                warn!(error = %e, "Failed to execute market order");
            }
        }

        Ok(())
    }

    // =========================================================================
    // Redemption Methods
    // =========================================================================

    /// Process any pending redemption actions.
    ///
    /// Called after position updates (fills) to check if redemption policy
    /// constraints are violated and execute necessary redemptions.
    async fn process_redemption_actions(&self, metrics: &mut BacktestMetrics) -> Result<()> {
        // Check if executor supports redemption
        if !self.executor.supports_redemption() {
            return Ok(());
        }

        // Get any redemption actions needed
        let actions = self.position_manager.reconcile_redemption_policies();
        if actions.is_empty() {
            return Ok(());
        }

        debug!(
            action_count = actions.len(),
            "Processing redemption actions in backtest"
        );

        for action in actions {
            debug!(
                market = %action.market,
                up_asset = %action.up_asset_id,
                down_asset = %action.down_asset_id,
                quantity = action.quantity,
                "Executing backtest redemption"
            );

            match self.executor.execute_redemption(&action).await {
                Ok(event) => {
                    metrics.total_redemptions += 1;
                    metrics.total_redemption_value += action.quantity; // 1 pair = $1

                    // Enqueue the redemption event for processing
                    self.coordinator
                        .enqueue_event(SystemEvent::Redemption(event))
                        .await?;
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        market = %action.market,
                        quantity = action.quantity,
                        "Redemption execution failed in backtest"
                    );

                    // Enqueue failure event
                    let failure_event = RedemptionEvent::RedemptionFailed {
                        market: action.market,
                        reason: e.to_string(),
                        timestamp: action.timestamp,
                    };
                    self.coordinator
                        .enqueue_event(SystemEvent::Redemption(failure_event))
                        .await?;
                }
            }
        }

        Ok(())
    }

    /// Handle a redemption event.
    ///
    /// Updates position manager state when redemptions complete.
    async fn handle_redemption(
        &mut self,
        event: &RedemptionEvent,
        _metrics: &mut BacktestMetrics,
    ) -> Result<()> {
        match self.position_manager.handle_redemption(event).await {
            Ok(pos_events) => {
                // Enqueue position events so signal generator can update inventory state
                for pos_event in pos_events {
                    self.coordinator
                        .enqueue_event(SystemEvent::Position(pos_event))
                        .await?;
                }
            }
            Err(e) => {
                warn!(error = %e, "Failed to handle redemption event");
            }
        }

        // Forward to signal generator so strategies can see redemption events
        let event_clone = SystemEvent::Redemption(event.clone());
        let _signals = self.signal_generator.generate_signal(&event_clone).await?;

        Ok(())
    }
}
