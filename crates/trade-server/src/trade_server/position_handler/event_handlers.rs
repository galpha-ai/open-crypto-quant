//! Event handlers for position lifecycle events.
//!
//! Handles:
//! - Position events (created, closed, updated)
//! - Execution events (order filled, rejected)
//! - Timer events (time-based exits)
//! - Price updates

use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use tracing::{debug, error, info, instrument, warn};

use crate::{
    domain::{PositionClosedNotification, PositionCreatedNotification, SystemEvent, TimerEvent},
    event_coordinator::EventCoordinator,
    execution::{ExecutionEvent, Order, OrderExecutor, OrderStatus, OrderType},
    notifier::Notifier,
    persistence::TradingEventPersistence,
    position::{
        ExitReason, ExitStrategy, Position, PositionClosedEvent, PositionEvent, PositionManager,
    },
    trade_server::TradeServerMetrics,
};

use super::order_executor::spawn_market_order_execution;

/// Handles position lifecycle events (created, closed, updated).
pub struct PositionEventHandler {
    position_manager: Arc<dyn PositionManager>,
    order_executor: Arc<dyn OrderExecutor + Send + Sync>,
    signal_notifier: Arc<dyn Notifier>,
    event_coordinator: Arc<dyn EventCoordinator>,
    exit_strategy: Arc<dyn ExitStrategy>,
    metrics: Option<Arc<TradeServerMetrics>>,
    max_sell_failures: u32,
    persistence: Option<Arc<dyn TradingEventPersistence>>,
}

impl PositionEventHandler {
    pub fn new(
        position_manager: Arc<dyn PositionManager>,
        order_executor: Arc<dyn OrderExecutor + Send + Sync>,
        signal_notifier: Arc<dyn Notifier>,
        event_coordinator: Arc<dyn EventCoordinator>,
        exit_strategy: Arc<dyn ExitStrategy>,
        metrics: Option<Arc<TradeServerMetrics>>,
        max_sell_failures: u32,
        persistence: Option<Arc<dyn TradingEventPersistence>>,
    ) -> Self {
        Self {
            position_manager,
            order_executor,
            signal_notifier,
            event_coordinator,
            exit_strategy,
            metrics,
            max_sell_failures,
            persistence,
        }
    }

    /// Handle a position event (created, closed, or updated).
    #[instrument(skip(self), level = "trace")]
    pub async fn handle_position_event(&self, event: &PositionEvent) -> Result<()> {
        match event {
            PositionEvent::PositionCreated {
                position,
                available_quote: _,
            } => {
                self.handle_position_created(position).await?;
            }
            PositionEvent::PositionClosed {
                position,
                realized_pnl_sol,
                pnl_pct,
                holding_period,
                signal_id,
                exit_reason,
                available_quote: _,
            } => {
                self.handle_position_closed(
                    position,
                    *realized_pnl_sol,
                    *pnl_pct,
                    *holding_period,
                    signal_id.clone(),
                    exit_reason.clone(),
                )
                .await?;
            }
            PositionEvent::PositionUpdated {
                position,
                source: _,
                available_quote: _,
            } => {
                self.handle_position_updated(position).await?;
            }
        }
        self.update_position_metrics().await;
        Ok(())
    }

    /// Handle position created event.
    async fn handle_position_created(&self, position: &Position) -> Result<()> {
        info!(
            position = ?position,
            "Handling position creation"
        );

        // Create notification
        let notification = PositionCreatedNotification {
            mint: position.mint.clone(),
            amount: position.amount,
            entry_price: position.entry_price,
            entry_time: position.entry_time,
        };

        // Notify using the signal notifier in async task
        let notifier = self.signal_notifier.clone();
        tokio::spawn(async move {
            if let Err(e) = notifier.notify(&notification).await {
                error!(
                    error = ?e,
                    "Failed to notify position creation",
                );
            } else {
                debug!("Notified position creation");
            }
        });
        self.update_position_metrics().await;

        Ok(())
    }

    /// Handle position closed event.
    async fn handle_position_closed(
        &self,
        position: &Position,
        realized_pnl_sol: Option<f64>,
        pnl_pct: Option<f64>,
        holding_period: chrono::Duration,
        signal_id: Option<String>,
        exit_reason: ExitReason,
    ) -> Result<()> {
        info!(
            position = ?position,
            realized_pnl_sol = realized_pnl_sol,
            pnl_pct = pnl_pct,
            holding_period = ?holding_period,
            signal_id = ?signal_id,
            exit_reason = ?exit_reason,
            "Handling position closure"
        );

        // Persist position closed event if persistence is configured
        self.persist_position_closed(
            position,
            realized_pnl_sol,
            pnl_pct,
            holding_period,
            signal_id.clone(),
            exit_reason.clone(),
        )
        .await;

        // Create and send notification
        self.notify_position_closed(position, realized_pnl_sol, pnl_pct, holding_period)
            .await;

        self.update_position_metrics().await;

        Ok(())
    }

    /// Persist position closed event to storage.
    async fn persist_position_closed(
        &self,
        position: &Position,
        realized_pnl_sol: Option<f64>,
        pnl_pct: Option<f64>,
        holding_period: chrono::Duration,
        signal_id: Option<String>,
        exit_reason: ExitReason,
    ) {
        if let Some(persistence) = &self.persistence {
            if let (
                Some(pnl_pct_val),
                Some(realized_pnl_sol_val),
                Some(entry_price),
                Some(current_price),
            ) = (
                pnl_pct,
                realized_pnl_sol,
                position.entry_price,
                position.current_price,
            ) {
                // Generate a unique position ID based on mint and entry time
                let position_id = format!("{}-{}", position.mint, position.entry_time.timestamp());

                let closed_event = PositionClosedEvent {
                    position_id,
                    signal_id: signal_id.unwrap_or_default(),
                    mint: position.mint.clone(),
                    opened_at: position.entry_time,
                    closed_at: Utc::now(),
                    buy_price: entry_price,
                    sell_price: current_price,
                    quantity: position.amount,
                    pnl_sol: realized_pnl_sol_val,
                    pnl_percentage: pnl_pct_val,
                    exit_reason,
                    holding_duration_seconds: holding_period.num_seconds(),
                };

                match serde_json::to_value(&closed_event) {
                    Ok(event_json) => {
                        let persistence = persistence.clone();
                        tokio::spawn(async move {
                            if let Err(e) = persistence.persist_position_closed(event_json).await {
                                error!("Failed to persist position closed event: {:?}", e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Failed to serialize position closed event: {:?}", e);
                    }
                }
            }
        }
    }

    /// Send notification for position closed.
    async fn notify_position_closed(
        &self,
        position: &Position,
        realized_pnl_sol: Option<f64>,
        pnl_pct: Option<f64>,
        holding_period: chrono::Duration,
    ) {
        let net_cash_flow = self.position_manager.get_net_cash_flow().await;
        let total_closed = self.position_manager.get_total_closed_positions().await;
        let notification = PositionClosedNotification {
            mint: position.mint.clone(),
            realized_pnl_sol,
            pnl_pct,
            holding_period,
            exit_price: position.current_price,
            current_quote_balance: self.position_manager.get_available_quote().await,
            net_cash_flow,
            total_closed_positions: total_closed,
            win_rate_pct: {
                let wins = self.position_manager.get_winning_trades().await;
                if total_closed > 0 {
                    (wins as f64 / total_closed as f64) * 100.0
                } else {
                    0.0
                }
            },
            avg_pnl_per_trade: {
                if total_closed > 0 {
                    net_cash_flow / total_closed as f64
                } else {
                    0.0
                }
            },
        };

        // Notify using the signal notifier in async task
        let notifier = self.signal_notifier.clone();
        tokio::spawn(async move {
            if let Err(e) = notifier.notify(&notification).await {
                error!(
                    error = ?e,
                    "Failed to notify position closure",
                );
            } else {
                debug!("Notified position closure");
            }
        });
    }

    /// Handle position updated event - check exit conditions.
    async fn handle_position_updated(&self, position: &Position) -> Result<()> {
        debug!(
            position = ?position,
            "Handling position update",
        );

        // Check if position should exit based on exit strategy AND we have a non-zero position
        if !self.exit_strategy.should_exit(position, Utc::now()) || position.amount <= 0.0 {
            return Ok(());
        }

        let exit_reason = self.exit_strategy.get_exit_reason(position, Utc::now());

        // Check max sell failures BEFORE attempting to mark
        if position.sell_failure_count >= self.max_sell_failures {
            error!(
                mint = position.mint,
                position = ?position,
                failure_count = position.sell_failure_count,
                limit = self.max_sell_failures,
                "Skipping exit trigger due to max sell failures reached."
            );
            return Ok(());
        }

        // Check if we can mark this position for selling
        if !self
            .position_manager
            .try_mark_pending_sell(&position.mint)
            .await
        {
            debug!(
                mint = position.mint,
                "Sell operation already in progress, skipping concurrent trigger"
            );
            return Ok(());
        }

        info!(
            position = ?position,
            exit_reason = ?exit_reason,
            current_pnl_pct = position.pnl_pct,
            "Exit condition met, generating sell order"
        );

        // Generate sell order
        #[allow(deprecated)]
        let sell_order = Order {
            mint: position.mint.clone(),
            market: None,
            order_type: OrderType::Sell {
                token_amount: position.amount,
                clear_position: true,
            },
            price: position.current_price,
            status: OrderStatus::Pending,
            timestamp: position.current_price_updated_time,
            signal_slot: None,
            signal_id: None,
            dex_type: None,
            venue_order_id: None,
            exit_mode: None,
            context: None,
        };

        // Execute sell order
        spawn_market_order_execution(
            Arc::clone(&self.order_executor),
            Arc::clone(&self.event_coordinator),
            sell_order,
            Some(Arc::clone(&self.position_manager)),
            true, // handle_failure
        );

        Ok(())
    }

    /// Handle an execution event (order filled or rejected).
    pub async fn handle_execution_event(&self, event: &ExecutionEvent) -> Result<()> {
        match event {
            ExecutionEvent::OrderFilled {
                mint,
                token_amount_change,
                quote_amount_change,
                price,
                timestamp,
                slippage,
                clear_position,
                force_position_clear,
                execution_latency_in_slots,
                signal_id: _,
                confirmed_slot: _,
                confirmed_signature: _,
                exit_mode: _,
            } => {
                info!(
                    mint = mint,
                    token_amount_change = token_amount_change,
                    quote_amount_change = quote_amount_change,
                    price = price,
                    timestamp = timestamp.to_rfc3339(),
                    slippage = slippage,
                    clear_position = clear_position,
                    force_position_clear = force_position_clear,
                    "Handling OrderFilled execution event"
                );

                // Update position based on filled order
                match self.position_manager.handle_execution(event).await {
                    Ok(position_event) => {
                        // Enqueue position update event
                        if let Err(e) = self
                            .event_coordinator
                            .enqueue_event(SystemEvent::Position(position_event))
                            .await
                        {
                            error!("Failed to enqueue position event: {:?}", e);
                        }
                    }
                    Err(e) => {
                        error!("Error handling execution in position manager: {:?}", e);
                    }
                }

                // Track slot latency if metrics are enabled
                if let Some(slots) = execution_latency_in_slots {
                    info!(
                        mint = mint,
                        clear_position = clear_position,
                        latency_in_slots = slots,
                        "Calculated execution latency in position handler (signal slot -> filled slot)",
                    );

                    if let Some(metrics) = &self.metrics {
                        metrics.signal_execution_slot_latency.observe(*slots as f64);
                    }
                }
            }
            ExecutionEvent::OrderRejected { mint, reason } => {
                error!("Order rejected for {}: {}", mint, reason);
            }
        }
        Ok(())
    }

    /// Handle a timer event for time-based exits.
    pub async fn handle_timer_event(&self, timer_event: &TimerEvent) -> Result<()> {
        // Get orders from position manager
        let orders = self.position_manager.handle_timer(timer_event).await?;

        // Execute returned orders
        for order in orders {
            let mint = order.mint.clone();

            if order.order_type.is_sell() {
                // Check max sell failures BEFORE attempting to mark
                if let Some(pos) = self.position_manager.get_position(&mint).await {
                    if pos.sell_failure_count >= self.max_sell_failures {
                        warn!(
                            mint = mint,
                            count = pos.sell_failure_count,
                            limit = self.max_sell_failures,
                            "Max sell failures reached for timer-based sell, skipping"
                        );
                        continue;
                    }
                } else {
                    error!(
                        mint = mint,
                        "Could not find position to check failure count for timer sell"
                    );
                    continue;
                }

                // Check if we can mark this position for selling
                if !self.position_manager.try_mark_pending_sell(&mint).await {
                    debug!(
                        mint,
                        "Sell operation already in progress, skipping concurrent trigger",
                    );
                    continue;
                }
            }

            spawn_market_order_execution(
                Arc::clone(&self.order_executor),
                Arc::clone(&self.event_coordinator),
                order,
                Some(Arc::clone(&self.position_manager)),
                true, // handle_failure
            );
        }
        Ok(())
    }

    /// Update price for a position and enqueue the resulting event.
    pub async fn update_price(
        &self,
        mint: &str,
        price: f64,
        timestamp: DateTime<Utc>,
    ) -> Result<()> {
        if let Ok(position_event) = self
            .position_manager
            .update_price(mint, price, timestamp)
            .await
        {
            self.event_coordinator
                .enqueue_event(SystemEvent::Position(position_event))
                .await?;
        }
        Ok(())
    }

    /// Update position-related metrics.
    pub async fn update_position_metrics(&self) {
        if let Some(metrics) = &self.metrics {
            // Get current open positions count
            let open_positions = self.position_manager.get_open_position_count().await as f64;
            metrics.open_positions.set(open_positions);

            // Get available quote balance
            metrics
                .available_quote
                .set(self.position_manager.get_available_quote().await);

            // Get net cash flow
            metrics
                .net_cash_flow
                .set(self.position_manager.get_net_cash_flow().await);

            // Get total PnL
            metrics
                .total_pnl
                .set(self.position_manager.get_total_pnl().await);

            // Get total unrealized PnL
            metrics
                .total_unrealized_pnl
                .set(self.position_manager.get_total_unrealized_pnl().await);

            // Get quote received and spent
            metrics
                .total_quote_received
                .set(self.position_manager.get_total_quote_received().await);
            metrics
                .total_quote_spent
                .set(self.position_manager.get_total_quote_spent().await);
        }
    }
}
