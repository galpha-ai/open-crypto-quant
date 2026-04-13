//! Order execution helpers for position handler.
//!
//! This module extracts common async spawn + execute + enqueue patterns
//! used throughout the position handler to reduce code duplication.

use std::sync::Arc;

use chrono::Utc;
use tokio::spawn;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::{
    domain::SystemEvent,
    event_coordinator::EventCoordinator,
    execution::{LimitOrderEvent, Order, OrderExecutor, OrderSide},
    position::{PositionManager, pending_order::InFlightOrder},
};

/// Execute a market order and enqueue the resulting execution event.
///
/// On failure, optionally clears the pending sell flag and increments failure count.
pub fn spawn_market_order_execution(
    executor: Arc<dyn OrderExecutor + Send + Sync>,
    event_coordinator: Arc<dyn EventCoordinator>,
    order: Order,
    position_manager: Option<Arc<dyn PositionManager>>,
    handle_failure: bool,
) {
    let mint = order.mint.clone();
    let order_for_logging = order.clone();

    spawn(async move {
        match executor.execute_market_order(order).await {
            Ok(execution_event) => {
                if let Err(e) = event_coordinator
                    .enqueue_event(SystemEvent::Execution(execution_event))
                    .await
                {
                    error!(
                        err = ?e,
                        order = ?order_for_logging,
                        "Failed to enqueue execution event"
                    );
                }
            }
            Err(e) => {
                error!(
                    err = ?e,
                    order = ?order_for_logging,
                    "Failed to execute market order"
                );

                if handle_failure {
                    if let Some(pm) = position_manager {
                        // Clear the pending sell flag
                        if let Err(clear_err) = pm.clear_pending_sell(&mint).await {
                            error!(
                                mint = mint,
                                err = ?clear_err,
                                "Failed to clear pending sell flag after market order failure"
                            );
                        } else {
                            info!(
                                mint = mint,
                                "Cleared pending sell flag after market order failure"
                            );
                        }

                        // Increment the sell failure count
                        if let Err(inc_err) = pm.increment_sell_failure_count(&mint).await {
                            error!(
                                mint = mint,
                                err = ?inc_err,
                                "Failed to increment sell failure count after market order failure"
                            );
                        } else {
                            info!(
                                mint = mint,
                                "Incremented sell failure count after market order failure"
                            );
                        }
                    }
                }
            }
        }
    });
}

/// Execute a limit order and enqueue the resulting limit order event.
///
/// On failure, optionally clears the pending sell flag.
pub fn spawn_limit_order_execution(
    executor: Arc<dyn OrderExecutor + Send + Sync>,
    event_coordinator: Arc<dyn EventCoordinator>,
    order: Order,
    position_manager: Option<Arc<dyn PositionManager>>,
) {
    let mint = order.mint.clone();
    let signal_id = order.signal_id.clone();
    let order_for_logging = order.clone();

    spawn(async move {
        match executor.execute_limit_order(order).await {
            Ok(limit_event) => {
                // Log success for OrderPlaced
                if let LimitOrderEvent::OrderPlaced {
                    order_id,
                    price,
                    size,
                    ..
                } = &limit_event
                {
                    info!(
                        mint = mint,
                        order_id = order_id,
                        price = price,
                        size = size,
                        "Limit order placed successfully"
                    );
                }

                if !executor.defers_limit_order_events() {
                    if let Err(e) = event_coordinator
                        .enqueue_event(SystemEvent::LimitOrder(limit_event))
                        .await
                    {
                        error!(
                            err = ?e,
                            mint = mint,
                            signal_id = ?signal_id,
                            "Failed to enqueue limit order event"
                        );
                    }
                } else {
                    debug!(
                        mint = mint,
                        signal_id = ?signal_id,
                        "Deferring limit order event enqueue"
                    );
                }
            }
            Err(e) => {
                error!(
                    err = ?e,
                    order = ?order_for_logging,
                    "Failed to execute limit order"
                );

                // Clear pending sell flag on failure
                if let Some(pm) = position_manager {
                    if let Err(clear_err) = pm.clear_pending_sell(&mint).await {
                        error!(
                            err = ?clear_err,
                            mint = mint,
                            "Failed to clear pending sell flag after limit order failure"
                        );
                    }
                }
            }
        }
    });
}

/// Execute a limit order from intent reconciliation with in-flight tracking.
///
/// This function:
/// 1. Registers the order as in-flight before execution starts
/// 2. On success: The in-flight order will be removed when OrderPlaced event is handled
/// 3. On failure: Removes the in-flight order to allow retry
///
/// This prevents the race condition where duplicate orders are placed because
/// `pending_limit_orders` is not updated until the OrderPlaced event flows through
/// the event loop.
pub fn spawn_intent_limit_order_execution(
    executor: Arc<dyn OrderExecutor + Send + Sync>,
    event_coordinator: Arc<dyn EventCoordinator>,
    order: Order,
    position_manager: Arc<dyn PositionManager>,
) {
    let mint = order.mint.clone();
    let signal_id = order.signal_id.clone();
    let order_for_logging = order.clone();

    // Extract order details for in-flight tracking
    let (side, price, size) = match &order.order_type {
        crate::execution::OrderType::LimitBuy {
            quote_amount,
            limit_price,
            ..
        } => {
            let size = quote_amount / limit_price;
            (OrderSide::Buy, *limit_price, size)
        }
        crate::execution::OrderType::LimitSell {
            token_amount,
            limit_price,
            ..
        } => (OrderSide::Sell, *limit_price, *token_amount),
        _ => {
            error!(
                order = ?order_for_logging,
                "spawn_intent_limit_order_execution called with non-limit order"
            );
            return;
        }
    };

    // Generate unique ID and register in-flight order BEFORE spawning
    let in_flight_id = Uuid::new_v4().to_string();
    let in_flight_order = InFlightOrder::new(
        in_flight_id.clone(),
        mint.clone(),
        order.market.clone(),
        side,
        price,
        size,
        Utc::now(),
        signal_id.clone(),
    );
    position_manager.register_in_flight_order(in_flight_id.clone(), in_flight_order);

    spawn(async move {
        match executor.execute_limit_order(order).await {
            Ok(limit_event) => {
                // Log success for OrderPlaced
                if let LimitOrderEvent::OrderPlaced {
                    order_id,
                    price,
                    size,
                    ..
                } = &limit_event
                {
                    info!(
                        mint = mint,
                        order_id = order_id,
                        price = price,
                        size = size,
                        in_flight_id = %in_flight_id,
                        "Intent limit order placed successfully"
                    );
                }

                // Enqueue the event - the in-flight order will be removed when
                // handle_limit_order_event processes the OrderPlaced event
                if !executor.defers_limit_order_events() {
                    if let Err(e) = event_coordinator
                        .enqueue_event(SystemEvent::LimitOrder(limit_event))
                        .await
                    {
                        error!(
                            err = ?e,
                            mint = mint,
                            signal_id = ?signal_id,
                            "Failed to enqueue limit order event"
                        );
                    }
                } else {
                    debug!(
                        mint = mint,
                        signal_id = ?signal_id,
                        in_flight_id = %in_flight_id,
                        "Deferring intent limit order event enqueue"
                    );
                }
            }
            Err(e) => {
                error!(
                    err = ?e,
                    order = ?order_for_logging,
                    in_flight_id = %in_flight_id,
                    "Failed to execute intent limit order"
                );

                // Remove in-flight order on failure to allow retry
                position_manager.remove_in_flight_order(&mint, &in_flight_id);
            }
        }
    });
}

/// Cancel an order and enqueue the resulting cancel event.
pub fn spawn_cancel_order(
    executor: Arc<dyn OrderExecutor + Send + Sync>,
    event_coordinator: Arc<dyn EventCoordinator>,
    order_id: String,
    mint: Option<String>,
    position_manager: Option<Arc<dyn PositionManager>>,
    clear_pending_on_success: bool,
) {
    spawn(async move {
        match executor.cancel_order(&order_id).await {
            Ok(cancel_event) => {
                if executor.defers_cancel_order_events() {
                    debug!(order_id = order_id, "Deferring cancel order event enqueue");
                } else if !executor.defers_limit_order_events() {
                    if let Err(e) = event_coordinator
                        .enqueue_event(SystemEvent::LimitOrder(cancel_event))
                        .await
                    {
                        error!(
                            err = ?e,
                            order_id = order_id,
                            "Failed to enqueue cancel order event"
                        );
                    } else {
                        debug!(order_id = order_id, "Cancel order executed successfully");
                    }
                }

                // Clear pending sell flag if requested
                if clear_pending_on_success {
                    if let (Some(pm), Some(m)) = (position_manager, mint.as_ref()) {
                        if let Err(clear_err) = pm.clear_pending_sell(m).await {
                            error!(
                                err = ?clear_err,
                                mint = m,
                                "Failed to clear pending sell after cancel"
                            );
                        } else {
                            info!(
                                mint = m,
                                order_id = order_id,
                                "Exit order cancelled successfully"
                            );
                        }
                    }
                }
            }
            Err(e) => {
                error!(
                    err = ?e,
                    order_id = order_id,
                    "Failed to cancel order"
                );
            }
        }
    });
}

/// Cancel an order and return the result (async, non-spawning version).
///
/// Unlike `spawn_cancel_order`, this function awaits completion and returns
/// the result. Use this when you need to wait for cancellation to complete.
pub async fn cancel_order_async(
    executor: Arc<dyn OrderExecutor + Send + Sync>,
    event_coordinator: Arc<dyn EventCoordinator>,
    order_id: String,
) -> Result<(), anyhow::Error> {
    match executor.cancel_order(&order_id).await {
        Ok(cancel_event) => {
            if executor.defers_cancel_order_events() {
                debug!(order_id = order_id, "Deferring cancel order event enqueue");
            } else if !executor.defers_limit_order_events() {
                event_coordinator
                    .enqueue_event(SystemEvent::LimitOrder(cancel_event))
                    .await
                    .map_err(|e| {
                        error!(
                            err = ?e,
                            order_id = order_id,
                            "Failed to enqueue cancel order event"
                        );
                        e
                    })?;
                debug!(order_id = order_id, "Cancel order executed successfully");
            }
            Ok(())
        }
        Err(e) => {
            error!(
                err = ?e,
                order_id = order_id,
                "Failed to cancel order"
            );
            Err(e)
        }
    }
}

/// Cancel an existing order and place a replacement order.
///
/// Used for order modification (cancel + replace pattern).
pub fn spawn_modify_order(
    executor: Arc<dyn OrderExecutor + Send + Sync>,
    event_coordinator: Arc<dyn EventCoordinator>,
    old_order_id: String,
    new_order: Order,
    position_manager: Arc<dyn PositionManager>,
) {
    let mint = new_order.mint.clone();

    spawn(async move {
        // Step 1: Cancel existing order
        match executor.cancel_order(&old_order_id).await {
            Ok(cancel_event) => {
                if executor.defers_cancel_order_events() {
                    debug!(
                        order_id = old_order_id,
                        "Deferring cancel event enqueue for modify"
                    );
                } else if !executor.defers_limit_order_events() {
                    // Enqueue cancellation event
                    if let Err(e) = event_coordinator
                        .enqueue_event(SystemEvent::LimitOrder(cancel_event))
                        .await
                    {
                        error!(err = ?e, "Failed to enqueue cancel event for modify");
                    }
                }

                // Step 2: Place new order at updated price
                match executor.execute_limit_order(new_order).await {
                    Ok(limit_event) => {
                        if !executor.defers_limit_order_events() {
                            if let Err(e) = event_coordinator
                                .enqueue_event(SystemEvent::LimitOrder(limit_event.clone()))
                                .await
                            {
                                error!(err = ?e, "Failed to enqueue new limit order event");
                            }
                        } else {
                            debug!(
                                mint = mint,
                                "Deferring limit order event enqueue for modify"
                            );
                        }

                        if let LimitOrderEvent::OrderPlaced {
                            order_id: new_id,
                            price,
                            ..
                        } = &limit_event
                        {
                            info!(
                                mint = mint,
                                old_order_id = old_order_id,
                                new_order_id = new_id,
                                new_price = price,
                                "Exit order modified successfully"
                            );
                        }
                    }
                    Err(e) => {
                        error!(
                            err = ?e,
                            mint = mint,
                            "Failed to place replacement order after cancel"
                        );
                        // Clear pending sell since the modify failed
                        if let Err(clear_err) = position_manager.clear_pending_sell(&mint).await {
                            error!(err = ?clear_err, "Failed to clear pending sell after modify failure");
                        }
                    }
                }
            }
            Err(e) => {
                error!(
                    err = ?e,
                    order_id = old_order_id,
                    "Failed to cancel order for modification"
                );
            }
        }
    });
}
