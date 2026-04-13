//! Intent-based signal handlers for orderbook market making.
//!
//! Intent signals express desired order book state rather than discrete actions.
//! The PositionManager reconciles current pending orders with the desired state
//! and returns the necessary actions (cancels and placements).
//!
//! ## Order Execution Phases
//!
//! Orders are executed in two phases to avoid liquidity issues:
//!
//! 1. **Phase 1 (Cancels)**: All cancel orders execute in parallel
//! 2. **Phase 2 (Placements)**: After cancels complete, all placements execute in parallel
//!
//! This ensures existing orders are cancelled before new orders are placed,
//! preventing rejection from venues when new orders would exceed available
//! balance or position limits.
//!
//! ## Race Condition Prevention
//!
//! Limit order placements are spawned asynchronously (fire-and-forget) to avoid
//! blocking the event loop during API calls. To prevent duplicate orders when
//! multiple intents arrive before the first order is confirmed:
//!
//! 1. Orders are registered as "in-flight" before execution starts
//! 2. `reconcile_intent()` considers both pending AND in-flight orders
//! 3. In-flight orders transition to pending when `OrderPlaced` event arrives
//! 4. In-flight orders are removed on execution failure
//!
//! This explicit lifecycle tracking ensures correct behavior regardless of
//! execution timing, without relying on rate-limiting heuristics.

use std::sync::Arc;

use anyhow::Result;
use futures::future::join_all;
use tokio::spawn;
use tracing::{debug, error, info, warn};

use crate::{
    domain::SystemEvent,
    event_coordinator::EventCoordinator,
    execution::{Order, OrderExecutor, OrderType, RedemptionEvent},
    position::PositionManager,
    signal::{RedemptionAction, TradableSignal},
};

use super::order_executor::{cancel_order_async, spawn_intent_limit_order_execution};

/// Handles intent-based signals for orderbook market making.
pub struct IntentHandler {
    position_manager: Arc<dyn PositionManager>,
    order_executor: Arc<dyn OrderExecutor + Send + Sync>,
    event_coordinator: Arc<dyn EventCoordinator>,
}

impl IntentHandler {
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

    /// Handle an intent-based signal (orderbook market making).
    ///
    /// Intent signals express desired order book state rather than discrete actions.
    /// The PositionManager reconciles current pending orders with the desired state
    /// and returns the necessary actions (cancels and placements).
    ///
    /// This approach:
    /// - Simplifies strategy logic (express desired state, not discrete actions)
    /// - Provides natural idempotency (same intent + same state = no-op)
    /// - Unifies order management across strategies
    pub async fn handle_intent_signal(&self, signal: &dyn TradableSignal) -> Result<()> {
        let intent = match signal.get_order_intent() {
            Some(intent) => intent,
            None => {
                error!(
                    signal_id = signal.signal_id(),
                    "Intent signal returned None from get_order_intent()"
                );
                return Ok(());
            }
        };

        debug!(
            signal_id = %intent.signal_id,
            mint = %intent.mint,
            bid_count = intent.bid_count(),
            ask_count = intent.ask_count(),
            has_redemption_policy = intent.redemption_policy.is_some(),
            "Processing intent signal"
        );

        // Update redemption policy if present and trigger reconciliation
        if let Some(policy) = &intent.redemption_policy {
            self.position_manager
                .update_redemption_policy(policy.clone());
            self.process_redemption_actions().await;
        }

        // Reconcile current state with desired intent
        let orders = match self.position_manager.reconcile_intent(&intent).await {
            Ok(orders) => orders,
            Err(e) => {
                error!(
                    err = ?e,
                    signal_id = %intent.signal_id,
                    mint = %intent.mint,
                    "Failed to reconcile intent"
                );
                return Err(e.into());
            }
        };

        // Count cancels and placements for logging
        let cancel_count = orders.iter().filter(|o| o.order_type.is_cancel()).count();
        let placement_count = orders.len() - cancel_count;

        debug!(
            signal_id = %intent.signal_id,
            mint = %intent.mint,
            cancels = cancel_count,
            placements = placement_count,
            "Reconciliation complete"
        );

        // Execute orders in two phases: cancels first, then placements.
        //
        // This ensures existing orders are cancelled before new orders are placed,
        // avoiding liquidity issues where placing new orders before cancelling
        // existing ones could exceed available balance or position limits.
        //
        // Within each phase, orders execute in parallel since they don't depend
        // on each other. Different markets also proceed independently.
        let (cancels, placements): (Vec<_>, Vec<_>) =
            orders.into_iter().partition(|o| o.order_type.is_cancel());

        // Phase 1: Execute all cancel orders in parallel
        let mut total_failures = 0;
        if !cancels.is_empty() {
            let cancel_futures: Vec<_> = cancels
                .into_iter()
                .map(|order| self.execute_reconciled_order_async(order))
                .collect();

            let cancel_results = join_all(cancel_futures).await;
            total_failures += cancel_results.iter().filter(|r| r.is_err()).count();
        }

        // Phase 2: Execute all placement orders in parallel (after cancels complete)
        if !placements.is_empty() {
            let placement_futures: Vec<_> = placements
                .into_iter()
                .map(|order| self.execute_reconciled_order_async(order))
                .collect();

            let placement_results = join_all(placement_futures).await;
            total_failures += placement_results.iter().filter(|r| r.is_err()).count();
        }

        // Log any failures (individual errors already logged in execute_reconciled_order_async)
        if total_failures > 0 {
            warn!(
                signal_id = %intent.signal_id,
                mint = %intent.mint,
                failures = total_failures,
                cancels = cancel_count,
                placements = placement_count,
                "Some orders from intent failed to execute"
            );
        }

        Ok(())
    }

    /// Execute a single order from reconciliation (async, awaited version).
    ///
    /// Returns a Result to allow the caller to track success/failure.
    /// Handles cancel orders, limit orders, and market orders appropriately.
    async fn execute_reconciled_order_async(&self, order: Order) -> Result<()> {
        let mint = order.mint.clone();
        let signal_id = order.signal_id.clone();

        match &order.order_type {
            OrderType::Cancel { order_id } => {
                cancel_order_async(
                    Arc::clone(&self.order_executor),
                    Arc::clone(&self.event_coordinator),
                    order_id.clone(),
                )
                .await?;
            }
            OrderType::LimitBuy { .. } | OrderType::LimitSell { .. } => {
                if self.order_executor.supports_limit_orders() {
                    // Use spawn to avoid blocking the event loop during API calls.
                    // In-flight tracking prevents duplicate orders from race conditions.
                    spawn_intent_limit_order_execution(
                        Arc::clone(&self.order_executor),
                        Arc::clone(&self.event_coordinator),
                        order,
                        Arc::clone(&self.position_manager),
                    );
                } else {
                    warn!(
                        mint = mint,
                        signal_id = ?signal_id,
                        "Executor does not support limit orders, skipping reconciled order"
                    );
                }
            }
            _ => {
                // Market orders from reconciliation are not expected in normal flow
                // but handle them for completeness
                self.execute_market_order_from_reconciliation(order).await;
            }
        }

        Ok(())
    }

    /// Execute a market order from reconciliation (rare case).
    async fn execute_market_order_from_reconciliation(&self, order: Order) {
        let mint = order.mint.clone();
        let order_for_logging = order.clone();
        let executor = Arc::clone(&self.order_executor);
        let event_coordinator = Arc::clone(&self.event_coordinator);

        spawn(async move {
            match executor.execute_market_order(order).await {
                Ok(execution_event) => {
                    if let Err(e) = event_coordinator
                        .enqueue_event(SystemEvent::Execution(execution_event))
                        .await
                    {
                        error!(
                            err = ?e,
                            mint = mint,
                            "Failed to enqueue market order event from reconciliation"
                        );
                    }
                }
                Err(e) => {
                    error!(
                        err = ?e,
                        order = ?order_for_logging,
                        "Failed to execute market order from reconciliation"
                    );
                }
            }
        });
    }

    // === Redemption policy methods ===

    /// Check for redemption policy violations and execute necessary redemptions.
    ///
    /// Called when a redemption policy is updated or after position changes.
    pub async fn process_redemption_actions(&self) {
        // Check if executor supports redemption
        if !self.order_executor.supports_redemption() {
            return;
        }

        // Get any redemption actions needed
        let actions = self.position_manager.reconcile_redemption_policies();
        if actions.is_empty() {
            return;
        }

        info!(
            action_count = actions.len(),
            "Processing redemption actions"
        );

        // Execute redemptions in parallel
        let redemption_futures: Vec<_> = actions
            .into_iter()
            .map(|action| self.execute_redemption(action))
            .collect();

        let results = join_all(redemption_futures).await;
        let failures = results.iter().filter(|r| r.is_err()).count();

        if failures > 0 {
            warn!(failures = failures, "Some redemption actions failed");
        }
    }

    /// Execute a single redemption action and enqueue the resulting event.
    async fn execute_redemption(&self, action: RedemptionAction) -> Result<()> {
        info!(
            market = %action.market,
            up_asset = %action.up_asset_id,
            down_asset = %action.down_asset_id,
            quantity = action.quantity,
            "Executing redemption"
        );

        match self.order_executor.execute_redemption(&action).await {
            Ok(event) => {
                // Enqueue the redemption event
                if let Err(e) = self
                    .event_coordinator
                    .enqueue_event(SystemEvent::Redemption(event))
                    .await
                {
                    error!(
                        err = ?e,
                        market = %action.market,
                        "Failed to enqueue redemption event"
                    );
                    return Err(e);
                }
                Ok(())
            }
            Err(e) => {
                error!(
                    err = ?e,
                    market = %action.market,
                    quantity = action.quantity,
                    "Redemption execution failed"
                );

                // Enqueue a failure event
                let failure_event = RedemptionEvent::RedemptionFailed {
                    market: action.market,
                    reason: e.to_string(),
                    timestamp: action.timestamp,
                };

                if let Err(enqueue_err) = self
                    .event_coordinator
                    .enqueue_event(SystemEvent::Redemption(failure_event))
                    .await
                {
                    error!(
                        err = ?enqueue_err,
                        "Failed to enqueue redemption failure event"
                    );
                }

                Err(e)
            }
        }
    }
}
