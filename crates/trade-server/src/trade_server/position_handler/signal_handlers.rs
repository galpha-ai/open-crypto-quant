//! Signal handlers for position management.
//!
//! Handles:
//! - Entry signals (open new positions)
//! - Exit signals (close existing positions)
//! - Modify order signals (cancel + replace)
//! - Cancel order signals

use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use tracing::{debug, error, info, warn};

use crate::{
    event_coordinator::EventCoordinator,
    execution::{Order, OrderExecutor, OrderStatus, OrderType},
    position::{PositionError, PositionManager},
    signal::TradableSignal,
};

use super::order_executor::{
    spawn_cancel_order, spawn_limit_order_execution, spawn_market_order_execution,
    spawn_modify_order,
};

/// Handles signal-based position operations.
pub struct SignalHandler {
    position_manager: Arc<dyn PositionManager>,
    order_executor: Arc<dyn OrderExecutor + Send + Sync>,
    event_coordinator: Arc<dyn EventCoordinator>,
}

impl SignalHandler {
    pub fn new(
        position_manager: Arc<dyn PositionManager>,
        order_executor: Arc<dyn OrderExecutor + Send + Sync>,
        event_coordinator: Arc<dyn EventCoordinator>,
    ) -> Self {
        Self {
            position_manager,
            order_executor,
            event_coordinator,
        }
    }

    /// Handle an entry signal (open a new position).
    ///
    /// This is the original handle_signal behavior, preserved for backward compatibility.
    pub async fn handle_entry_signal(&self, signal: &dyn TradableSignal) -> Result<()> {
        // Get the mint from the signal
        let mint = match signal.get_mint() {
            Some(mint) => mint,
            None => {
                debug!("Signal does not have a valid mint, skipping");
                return Ok(());
            }
        };

        // Check if we've already bought this mint before
        if !self.position_manager.try_mark_for_buying(mint).await {
            info!(
                mint,
                signal_id = signal.signal_id(),
                "Mint has already been bought before, skipping signal",
            );
            return Ok(());
        }

        // Generate order from position manager
        match self.position_manager.handle_signal(signal).await {
            Ok(Some(order)) => {
                info!(
                    mint = order.mint,
                    signal_id = signal.signal_id(),
                    order_type = ?order.order_type,
                    price = order.price,
                    "Generated order",
                );

                // Execute the order
                spawn_market_order_execution(
                    Arc::clone(&self.order_executor),
                    Arc::clone(&self.event_coordinator),
                    order,
                    None,  // No position manager needed for entry
                    false, // Don't handle failure for entry signals
                );

                Ok(())
            }
            Ok(None) => {
                debug!("Position manager returned no order for signal");
                Ok(())
            }
            Err(e) => match e {
                PositionError::InsufficientSolBalance { .. } => {
                    debug!(
                        "Insufficient SOL balance, skipping order generation: {:?}",
                        e
                    );
                    Ok(())
                }
                PositionError::MaxOpenPositionsReached(limit) => {
                    debug!(
                        "Maximum open positions limit ({}) reached, skipping order generation",
                        limit
                    );
                    Ok(())
                }
                _ => {
                    error!("Error generating order from signal: {:?}", e);
                    Err(e.into())
                }
            },
        }
    }

    /// Handle an exit signal (close an existing position).
    ///
    /// Supports both market exits (immediate) and limit exits (placed on orderbook).
    pub async fn handle_exit_signal(&self, signal: &dyn TradableSignal) -> Result<()> {
        let mint = match signal.references_position() {
            Some(mint) => mint.to_string(),
            None => {
                error!(
                    signal_id = signal.signal_id(),
                    "Exit signal missing position reference (mint)"
                );
                return Ok(());
            }
        };

        // Get the position to determine amount to sell
        let position = match self.position_manager.get_position(&mint).await {
            Some(p) => p,
            None => {
                warn!(
                    mint = mint,
                    signal_id = signal.signal_id(),
                    "No position found for exit signal"
                );
                return Ok(());
            }
        };

        // Skip if position has zero amount
        if position.amount <= 0.0 {
            debug!(
                mint = mint,
                "Position has zero amount, skipping exit signal"
            );
            return Ok(());
        }

        // Check if we can mark this position for selling
        if !self.position_manager.try_mark_pending_sell(&mint).await {
            debug!(
                mint = mint,
                "Sell operation already in progress, skipping exit signal"
            );
            return Ok(());
        }

        // Create the exit order based on signal parameters
        let order = if signal.is_limit_order() {
            let limit_price = signal
                .get_limit_price()
                .unwrap_or_else(|| position.current_price.unwrap_or(0.0));
            let time_in_force = signal.get_time_in_force().unwrap_or_default();

            info!(
                mint = mint,
                signal_id = signal.signal_id(),
                limit_price = limit_price,
                time_in_force = ?time_in_force,
                "Creating limit exit order"
            );

            Order::new_limit_sell(
                mint.clone(),
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
            info!(
                mint = mint,
                signal_id = signal.signal_id(),
                "Creating market exit order"
            );

            #[allow(deprecated)]
            Order {
                mint: mint.clone(),
                market: signal.get_market().map(String::from),
                order_type: OrderType::Sell {
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

        // Execute the order
        if signal.is_limit_order() && self.order_executor.supports_limit_orders() {
            spawn_limit_order_execution(
                Arc::clone(&self.order_executor),
                Arc::clone(&self.event_coordinator),
                order,
                Some(Arc::clone(&self.position_manager)),
            );
        } else {
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

    /// Handle a modify order signal (cancel existing order and place new one).
    ///
    /// This is used to adjust the price or size of an existing limit exit order.
    pub async fn handle_modify_order_signal(&self, signal: &dyn TradableSignal) -> Result<()> {
        let order_id = match signal.references_order_id() {
            Some(id) => id.to_string(),
            None => {
                error!(
                    signal_id = signal.signal_id(),
                    "Modify order signal missing order_id reference"
                );
                return Ok(());
            }
        };

        let mint = match signal.references_position() {
            Some(mint) => mint.to_string(),
            None => {
                error!(
                    signal_id = signal.signal_id(),
                    "Modify order signal missing position reference (mint)"
                );
                return Ok(());
            }
        };

        // Get the position to determine current amount
        let position = match self.position_manager.get_position(&mint).await {
            Some(p) => p,
            None => {
                warn!(
                    mint = mint,
                    signal_id = signal.signal_id(),
                    "No position found for modify order signal"
                );
                return Ok(());
            }
        };

        let new_price = match signal.get_limit_price() {
            Some(price) => price,
            None => {
                error!(
                    signal_id = signal.signal_id(),
                    "Modify order signal missing new price"
                );
                return Ok(());
            }
        };

        info!(
            mint = mint,
            order_id = order_id,
            new_price = new_price,
            signal_id = signal.signal_id(),
            "Modifying exit order"
        );

        let time_in_force = signal.get_time_in_force().unwrap_or_default();
        let signal_id = signal.signal_id().to_string();
        let signal_slot = signal.get_slot();
        let timestamp = signal.get_timestamp().unwrap_or_else(Utc::now);
        let market = signal.get_market().map(String::from);

        // Create the new order
        let new_order = Order::new_limit_sell(
            mint,
            market,
            position.amount,
            new_price,
            true, // clear_position
            time_in_force,
            timestamp,
            Some(signal_id),
            signal_slot,
        );

        spawn_modify_order(
            Arc::clone(&self.order_executor),
            Arc::clone(&self.event_coordinator),
            order_id,
            new_order,
            Arc::clone(&self.position_manager),
        );

        Ok(())
    }

    /// Handle a cancel order signal (cancel an existing exit order).
    pub async fn handle_cancel_order_signal(&self, signal: &dyn TradableSignal) -> Result<()> {
        let order_id = match signal.references_order_id() {
            Some(id) => id.to_string(),
            None => {
                error!(
                    signal_id = signal.signal_id(),
                    "Cancel order signal missing order_id reference"
                );
                return Ok(());
            }
        };

        let mint = match signal.references_position() {
            Some(mint) => mint.to_string(),
            None => {
                error!(
                    signal_id = signal.signal_id(),
                    "Cancel order signal missing position reference (mint)"
                );
                return Ok(());
            }
        };

        info!(
            mint = mint,
            order_id = order_id,
            signal_id = signal.signal_id(),
            "Cancelling exit order"
        );

        spawn_cancel_order(
            Arc::clone(&self.order_executor),
            Arc::clone(&self.event_coordinator),
            order_id,
            Some(mint),
            Some(Arc::clone(&self.position_manager)),
            true, // clear_pending_on_success
        );

        Ok(())
    }
}
