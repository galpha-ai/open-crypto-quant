//! Order status poller service.
//!
//! This module provides the core order polling functionality for monitoring
//! pending limit orders via the Polymarket REST API. Merge functionality
//! has been extracted to `MergeExecutor`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use chrono::Utc;
use polyfill_rs::ClobClient;
use rust_decimal::prelude::ToPrimitive;
use tokio::sync::{RwLock, watch};
use tracing::{debug, error, info, warn};

use crate::domain::SystemEvent;
use crate::event_coordinator::EventCoordinator;
use crate::execution::TimeInForce;
use crate::execution::events::LimitOrderEvent;
use crate::execution::lifecycle::{
    LifecycleCancelRequest, LifecycleEngine, LifecycleOrderRef, LifecyclePlaceRequest,
    LifecyclePlaceSuccess, LifecycleState, LifecycleTerminalEvidence, TerminalReason,
};
use crate::position::PositionManager;
use crate::position::position_query::ExchangePosition;

use super::balance_api;
use super::config::PollerConfig;
use super::metrics::PollerMetrics;
use super::rate_limiter::RateLimiter;
use super::types::{MonitoredOrder, PollCycleResult, PositionSnapshot, PositionSyncResult};

/// Service for polling order status from Polymarket API.
///
/// This struct focuses solely on order monitoring and position sync.
/// For merge/redemption operations, see `MergeExecutor`.
pub struct OrderStatusPoller {
    /// Polymarket CLOB client for REST API calls
    client: Arc<ClobClient>,

    /// Event coordinator for enqueueing fill events
    event_coordinator: Arc<dyn EventCoordinator>,

    /// Position manager for reconciliation
    position_manager: Arc<dyn PositionManager>,

    /// Orders being monitored: order_id -> MonitoredOrder
    monitored_orders: RwLock<HashMap<String, MonitoredOrder>>,

    /// Canonical lifecycle state for monitored live orders.
    lifecycle_engine: RwLock<LifecycleEngine>,

    /// Token bucket rate limiter
    rate_limiter: RateLimiter,

    /// Configuration
    config: PollerConfig,

    /// Metrics collector
    metrics: PollerMetrics,

    /// Shutdown signal
    shutdown: watch::Receiver<bool>,
}

impl OrderStatusPoller {
    fn lifecycle_order_ref(order_id: &str) -> LifecycleOrderRef {
        LifecycleOrderRef::ClientOrderId(order_id.to_string())
    }

    /// Create a new poller with the given client, position manager and config.
    pub fn new(
        client: Arc<ClobClient>,
        event_coordinator: Arc<dyn EventCoordinator>,
        position_manager: Arc<dyn PositionManager>,
        config: PollerConfig,
        shutdown: watch::Receiver<bool>,
    ) -> Self {
        let rate_limiter = RateLimiter::new(config.rate_limit_rps);
        Self {
            client,
            event_coordinator,
            position_manager,
            monitored_orders: RwLock::new(HashMap::new()),
            lifecycle_engine: RwLock::new(LifecycleEngine::new()),
            rate_limiter,
            config,
            metrics: PollerMetrics::default(),
            shutdown,
        }
    }

    /// Create a new poller with custom metrics.
    pub fn with_metrics(
        client: Arc<ClobClient>,
        event_coordinator: Arc<dyn EventCoordinator>,
        position_manager: Arc<dyn PositionManager>,
        config: PollerConfig,
        metrics: PollerMetrics,
        shutdown: watch::Receiver<bool>,
    ) -> Self {
        let rate_limiter = RateLimiter::new(config.rate_limit_rps);
        Self {
            client,
            event_coordinator,
            position_manager,
            monitored_orders: RwLock::new(HashMap::new()),
            lifecycle_engine: RwLock::new(LifecycleEngine::new()),
            rate_limiter,
            config,
            metrics,
            shutdown,
        }
    }

    /// Register an order for monitoring.
    /// Called by executor after successful order placement.
    pub async fn monitor_order(&self, order: MonitoredOrder) {
        self.register_lifecycle_order(&order).await;

        debug!(
            order_id = %order.order_id,
            mint = %order.mint,
            side = ?order.side,
            price = order.price,
            size = order.original_size,
            "Registering order for monitoring"
        );

        let mut orders = self.monitored_orders.write().await;
        orders.insert(order.order_id.clone(), order);
        self.metrics.set_monitored_orders(orders.len());
    }

    async fn register_lifecycle_order(&self, order: &MonitoredOrder) {
        let mut lifecycle = self.lifecycle_engine.write().await;
        let order_ref = Self::lifecycle_order_ref(order.order_id.as_str());
        if lifecycle.get(&order_ref).is_some() {
            return;
        }

        if let Err(err) = lifecycle.record_place_request(LifecyclePlaceRequest {
            lifecycle_id: Some(format!("live-{}", order.order_id)),
            client_order_id: order.order_id.clone(),
            mint: order.mint.clone(),
            market: order.market.clone(),
            side: order.side,
            price: order.price,
            size: order.original_size,
            time_in_force: TimeInForce::GoodTilCancelled,
            signal_id: order.signal_id.clone(),
        }) {
            warn!(
                order_id = %order.order_id,
                err = ?err,
                "Failed to register lifecycle place request for monitored order"
            );
            return;
        }

        if let Err(err) = lifecycle.record_place_success(LifecyclePlaceSuccess {
            order_ref,
            venue_order_id: order.order_id.clone(),
        }) {
            warn!(
                order_id = %order.order_id,
                err = ?err,
                "Failed to register lifecycle place success for monitored order"
            );
        }
    }

    /// Record a cancel request intent from command-path acknowledgement.
    pub async fn record_cancel_request(&self, order_id: &str) {
        let mut lifecycle = self.lifecycle_engine.write().await;
        if let Err(err) = lifecycle.record_cancel_request(LifecycleCancelRequest {
            order_ref: Self::lifecycle_order_ref(order_id),
        }) {
            warn!(
                order_id = %order_id,
                err = ?err,
                "Failed to record lifecycle cancel request for monitored order"
            );
        }
    }

    async fn should_emit_terminal_cancellation(&self, order_id: &str) -> bool {
        let mut lifecycle = self.lifecycle_engine.write().await;
        let order_ref = Self::lifecycle_order_ref(order_id);

        let previous_state = lifecycle.get(&order_ref).map(|order| order.state);
        if matches!(previous_state, Some(LifecycleState::Terminal(_))) {
            return false;
        }

        match lifecycle.record_terminal_evidence(LifecycleTerminalEvidence {
            order_ref,
            terminal_reason: TerminalReason::Cancelled,
            venue_order_id: Some(order_id.to_string()),
        }) {
            Ok(_) => true,
            Err(err) => {
                warn!(
                    order_id = %order_id,
                    err = ?err,
                    "Failed to record lifecycle terminal cancellation evidence"
                );
                false
            }
        }
    }

    /// Stop monitoring an order.
    /// Called by executor before/after cancel, or when order fully filled.
    pub async fn unmonitor_order(&self, order_id: &str) -> Option<MonitoredOrder> {
        debug!(order_id = %order_id, "Unmonitoring order");

        let mut orders = self.monitored_orders.write().await;
        let removed = orders.remove(order_id);
        self.metrics.set_monitored_orders(orders.len());

        if removed.is_some() {
            debug!(order_id = %order_id, "Order removed from monitoring");
        }

        removed
    }

    /// Get current monitored order count.
    pub async fn monitored_count(&self) -> usize {
        self.monitored_orders.read().await.len()
    }

    /// Get a snapshot of monitored orders (for debugging/API).
    pub async fn get_monitored_orders(&self) -> Vec<MonitoredOrder> {
        self.monitored_orders
            .read()
            .await
            .values()
            .cloned()
            .collect()
    }

    /// Run the polling loop (spawned as background task).
    /// Polls orders and optionally syncs positions on configured intervals.
    pub async fn run(self: Arc<Self>) -> Result<()> {
        info!(
            poll_interval_ms = self.config.poll_interval.as_millis(),
            batch_size = self.config.batch_size,
            rate_limit_rps = self.config.rate_limit_rps,
            "Starting order status poller"
        );

        let mut last_position_sync = Instant::now();

        loop {
            // Check for shutdown
            if *self.shutdown.borrow() {
                info!("Order status poller shutting down");
                break;
            }

            // Run poll cycle
            if let Err(e) = self.poll_cycle().await {
                warn!(error = %e, "Poll cycle failed");
            }

            // Position sync if enabled and interval elapsed
            if self.config.enable_position_sync
                && last_position_sync.elapsed() >= self.config.position_sync_interval
            {
                match self.sync_positions().await {
                    Ok(result) => {
                        if result.drift_detected {
                            warn!(
                                positions = result.positions.len(),
                                available_quote = result.available_quote,
                                "Position drift detected during sync"
                            );
                        }
                        last_position_sync = Instant::now();
                    }
                    Err(e) => {
                        warn!(error = %e, "Position sync failed");
                    }
                }
            }

            // Wait for next cycle
            let mut shutdown_rx = self.shutdown.clone();
            tokio::select! {
                _ = tokio::time::sleep(self.config.poll_interval) => {}
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        info!("Order status poller received shutdown signal");
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Execute a single poll cycle (for testing or manual trigger).
    pub async fn poll_cycle(&self) -> Result<PollCycleResult> {
        let start = Instant::now();
        let mut result = PollCycleResult::new();

        // Get orders to poll
        let orders_to_poll = self.select_orders_to_poll().await;

        if orders_to_poll.is_empty() {
            return Ok(result);
        }

        debug!(count = orders_to_poll.len(), "Starting poll cycle");

        // Poll each order
        for order_id in orders_to_poll {
            // Acquire rate limit token
            let wait_time = self.rate_limiter.acquire().await;
            if !wait_time.is_zero() {
                self.metrics.record_rate_limit_wait(wait_time.as_secs_f64());
            }

            // Poll the order
            match self.poll_single_order(&order_id).await {
                Ok(poll_result) => {
                    result.record_poll(poll_result.had_fill);
                    if poll_result.completed {
                        result.record_completion();
                    }
                }
                Err(e) => {
                    warn!(order_id = %order_id, error = %e, "Failed to poll order");
                    result.record_error();
                }
            }
        }

        // Prune stale orders
        self.prune_stale_orders().await;

        // Record metrics
        result.set_duration(start.elapsed());
        self.metrics.record_poll_cycle(
            result.duration.as_secs_f64(),
            result.orders_polled,
            result.fills_detected,
        );

        debug!(
            orders_polled = result.orders_polled,
            fills_detected = result.fills_detected,
            orders_completed = result.orders_completed,
            errors = result.errors,
            duration_ms = result.duration.as_millis(),
            "Poll cycle complete"
        );

        Ok(result)
    }

    /// Select orders to poll in this cycle.
    async fn select_orders_to_poll(&self) -> Vec<String> {
        let orders = self.monitored_orders.read().await;

        // Filter orders that need polling
        let mut candidates: Vec<_> = orders.values().filter(|o| self.should_poll(o)).collect();

        // Sort by priority
        candidates.sort_by_key(|o| o.poll_priority());

        // Take batch_size
        candidates
            .into_iter()
            .take(self.config.batch_size)
            .map(|o| o.order_id.clone())
            .collect()
    }

    /// Check if an order should be polled.
    fn should_poll(&self, order: &MonitoredOrder) -> bool {
        // Check if enough time has passed since last poll
        let min_interval = if order.consecutive_failures > 0 {
            self.config.calculate_backoff(order.consecutive_failures)
        } else {
            self.config.poll_interval
        };

        match order.time_since_poll() {
            Some(elapsed) => elapsed >= min_interval,
            None => true, // Never polled
        }
    }

    /// Poll a single order and process the result.
    async fn poll_single_order(&self, order_id: &str) -> Result<SinglePollResult> {
        // Get current order state
        let order = {
            let orders = self.monitored_orders.read().await;
            orders.get(order_id).cloned()
        };

        let order = match order {
            Some(o) => o,
            None => {
                // Order was unmonitored while we were preparing to poll
                return Ok(SinglePollResult {
                    had_fill: false,
                    completed: true,
                });
            }
        };

        // Query order from API
        match self.client.get_order(order_id).await {
            Ok(api_order) => {
                info!(
                    order_id = %order_id,
                    status = %api_order.status,
                    size_matched = %api_order.size_matched,
                    original_size = %api_order.original_size,
                    price = %api_order.price,
                    side = ?api_order.side,
                    order_type = ?api_order.order_type,
                    asset_id = %api_order.asset_id,
                    "Polled order from API"
                );

                let size_matched = api_order.size_matched.to_f64().unwrap_or(0.0);
                let had_fill = size_matched > order.last_known_filled;
                let order_status = api_order.status.to_uppercase();

                // Check if order is cancelled (Polymarket uses "CANCELED")
                if order_status == "CANCELED" {
                    info!(
                        order_id = %order_id,
                        mint = %order.mint,
                        size_matched = size_matched,
                        "Order cancelled externally"
                    );

                    // Emit any remaining fill before cancellation
                    if had_fill {
                        let fill_event = self.create_fill_event(&order, size_matched);
                        self.enqueue_event(fill_event).await?;
                    }

                    if self.should_emit_terminal_cancellation(order_id).await {
                        // Emit cancellation event
                        let cancel_event = LimitOrderEvent::OrderCancelled {
                            order_id: order_id.to_string(),
                            reason: Some("Cancelled externally".to_string()),
                            timestamp: Utc::now(),
                        };
                        self.enqueue_event(cancel_event).await?;
                    } else {
                        debug!(
                            order_id = %order_id,
                            "Suppressed duplicate terminal cancellation event"
                        );
                    }
                    self.unmonitor_order(order_id).await;

                    return Ok(SinglePollResult {
                        had_fill,
                        completed: true,
                    });
                }

                // Check if order is invalid (market expired or order invalidated)
                if order_status == "INVALID" {
                    info!(
                        order_id = %order_id,
                        mint = %order.mint,
                        size_matched = size_matched,
                        "Order marked INVALID (market likely expired)"
                    );

                    // Emit any remaining fill before marking invalid
                    if had_fill {
                        let fill_event = self.create_fill_event(&order, size_matched);
                        self.enqueue_event(fill_event).await?;
                    }

                    if self.should_emit_terminal_cancellation(order_id).await {
                        // Emit cancellation event for INVALID orders
                        let cancel_event = LimitOrderEvent::OrderCancelled {
                            order_id: order_id.to_string(),
                            reason: Some("Order invalid (market expired)".to_string()),
                            timestamp: Utc::now(),
                        };
                        self.enqueue_event(cancel_event).await?;
                    } else {
                        debug!(
                            order_id = %order_id,
                            "Suppressed duplicate terminal invalid-order cancellation event"
                        );
                    }
                    self.unmonitor_order(order_id).await;

                    return Ok(SinglePollResult {
                        had_fill,
                        completed: true,
                    });
                }

                // Detect fill
                if had_fill {
                    let fill_event = self.create_fill_event(&order, size_matched);
                    self.enqueue_event(fill_event).await?;
                }

                // Update order state
                let completed = {
                    let mut orders = self.monitored_orders.write().await;
                    if let Some(o) = orders.get_mut(order_id) {
                        o.record_poll_success(size_matched);
                        // Check if fully filled or matched status
                        if o.is_fully_filled() || order_status == "MATCHED" {
                            orders.remove(order_id);
                            self.metrics.set_monitored_orders(orders.len());
                            info!(
                                order_id = %order_id,
                                mint = %order.mint,
                                total_filled = size_matched,
                                status = %order_status,
                                "Order completed"
                            );
                            true
                        } else {
                            false
                        }
                    } else {
                        true // Already removed
                    }
                };

                Ok(SinglePollResult {
                    had_fill,
                    completed,
                })
            }
            Err(e) => {
                let error_str = e.to_string();

                // Log the full error for debugging
                warn!(
                    order_id = %order_id,
                    mint = %order.mint,
                    error = %e,
                    error_debug = ?e,
                    "Failed to poll order from API"
                );

                // Check for 404/NOT_FOUND - order was deleted or expired
                if error_str.contains("404") || error_str.contains("not found") {
                    info!(
                        order_id = %order_id,
                        mint = %order.mint,
                        "Order not found (deleted/expired), emitting cancellation"
                    );

                    if self.should_emit_terminal_cancellation(order_id).await {
                        let cancel_event = LimitOrderEvent::OrderCancelled {
                            order_id: order_id.to_string(),
                            reason: Some("Order not found (deleted/expired)".to_string()),
                            timestamp: Utc::now(),
                        };
                        self.enqueue_event(cancel_event).await?;
                    } else {
                        debug!(
                            order_id = %order_id,
                            "Suppressed duplicate terminal not-found cancellation event"
                        );
                    }
                    self.unmonitor_order(order_id).await;

                    return Ok(SinglePollResult {
                        had_fill: false,
                        completed: true,
                    });
                }

                // Other error - record failure
                {
                    let mut orders = self.monitored_orders.write().await;
                    if let Some(o) = orders.get_mut(order_id) {
                        o.record_poll_failure();

                        if o.consecutive_failures >= self.config.max_consecutive_failures {
                            error!(
                                order_id = %order_id,
                                failures = o.consecutive_failures,
                                "Max consecutive failures reached, unmonitoring order"
                            );
                            orders.remove(order_id);
                            self.metrics.set_monitored_orders(orders.len());
                        }
                    }
                }

                // Categorize error for metrics
                let error_type = if error_str.contains("429") || error_str.contains("rate limit") {
                    "rate_limit"
                } else if error_str.contains("timeout") {
                    "timeout"
                } else {
                    "other"
                };
                self.metrics.record_api_error(error_type);

                Err(e.into())
            }
        }
    }

    /// Create a fill event from order and new fill amount.
    fn create_fill_event(&self, order: &MonitoredOrder, size_matched: f64) -> LimitOrderEvent {
        let new_fill = size_matched - order.last_known_filled;
        let remaining = order.original_size - size_matched;

        info!(
            order_id = %order.order_id,
            mint = %order.mint,
            side = ?order.side,
            filled_size = new_fill,
            remaining_size = remaining,
            "Fill detected"
        );

        LimitOrderEvent::OrderPartiallyFilled {
            order_id: order.order_id.clone(),
            mint: order.mint.clone(),
            side: order.side,
            filled_size: new_fill,
            remaining_size: remaining,
            fill_price: order.price,
            timestamp: Utc::now(),
            signal_id: order.signal_id.clone(),
            exit_mode: None,
        }
    }

    /// Enqueue an event to the event coordinator.
    async fn enqueue_event(&self, event: LimitOrderEvent) -> Result<()> {
        self.event_coordinator
            .enqueue_event(SystemEvent::LimitOrder(event))
            .await
    }

    /// Prune orders that are too old.
    async fn prune_stale_orders(&self) {
        let mut orders = self.monitored_orders.write().await;
        let max_age = self.config.max_order_age;

        let stale_ids: Vec<_> = orders
            .values()
            .filter(|o| o.age() > max_age)
            .map(|o| o.order_id.clone())
            .collect();

        for order_id in stale_ids {
            if let Some(order) = orders.remove(&order_id) {
                warn!(
                    order_id = %order_id,
                    mint = %order.mint,
                    age_secs = order.age().as_secs(),
                    "Pruned stale order"
                );
            }
        }

        if !orders.is_empty() {
            self.metrics.set_monitored_orders(orders.len());
        }
    }

    /// Get USDC balance from API.
    pub async fn get_usdc_balance(&self) -> Result<f64> {
        // Acquire rate limit token
        let wait_time = self.rate_limiter.acquire().await;
        if !wait_time.is_zero() {
            self.metrics.record_rate_limit_wait(wait_time.as_secs_f64());
        }

        balance_api::get_usdc_balance(&self.client).await
    }

    /// Get token balance from API.
    pub async fn get_token_balance(&self, token_id: &str) -> Result<f64> {
        // Acquire rate limit token
        let wait_time = self.rate_limiter.acquire().await;
        if !wait_time.is_zero() {
            self.metrics.record_rate_limit_wait(wait_time.as_secs_f64());
        }

        balance_api::get_token_balance(&self.client, token_id).await
    }

    /// Sync positions/balances from API (reconciliation).
    ///
    /// Fetches all open positions from position manager and compares with
    /// exchange balances to detect and reconcile drift. Also checks bought_mints
    /// to detect orphan positions (tokens we traded but no longer track).
    pub async fn sync_positions(&self) -> Result<PositionSyncResult> {
        // Get open positions from position manager
        let positions = self.position_manager.get_all_open_positions().await;
        let open_mints: std::collections::HashSet<String> =
            positions.iter().map(|p| p.mint.clone()).collect();

        let mut expected_positions: Vec<(String, f64)> =
            positions.into_iter().map(|p| (p.mint, p.amount)).collect();

        // Check bought_mints for orphan positions (traded but not in open_positions)
        let bought_mints = self.position_manager.get_all_bought_mints().await;
        for mint in bought_mints {
            if !open_mints.contains(&mint) {
                // This token was traded before but is not in open_positions
                // Add with expected amount=0 so sync will detect if there's actual balance
                debug!(
                    mint = %mint,
                    "Adding orphan candidate from bought_mints with expected amount=0"
                );
                expected_positions.push((mint, 0.0));
            }
        }

        let expected_quote = self.position_manager.get_available_quote().await;

        self.sync_positions_with_expected(expected_positions, Some(expected_quote))
            .await
    }

    /// Sync positions with expected values for drift detection and reconciliation.
    ///
    /// When drift is detected and a position manager is configured, this method
    /// will automatically reconcile the position to match the exchange state.
    ///
    /// # Arguments
    /// * `expected_positions` - List of (token_id, expected_balance) pairs
    /// * `expected_quote` - Expected USDC balance (optional)
    ///
    /// # Returns
    /// PositionSyncResult with actual values and drift detection
    pub async fn sync_positions_with_expected(
        &self,
        expected_positions: Vec<(String, f64)>,
        expected_quote: Option<f64>,
    ) -> Result<PositionSyncResult> {
        debug!(
            expected_positions = expected_positions.len(),
            expected_quote = ?expected_quote,
            "Starting position sync"
        );

        let mut positions = Vec::new();
        let mut drift_detected = false;

        // Get USDC balance
        let available_quote = self.get_usdc_balance().await?;

        // Check quote drift
        if let Some(expected) = expected_quote {
            let diff = (available_quote - expected).abs();
            // Allow small tolerance for rounding (0.01 USDC)
            if diff > 0.01 {
                warn!(
                    expected = expected,
                    actual = available_quote,
                    diff = diff,
                    "Quote balance drift detected"
                );
                drift_detected = true;

                // Reconcile quote balance with actual balance from exchange
                if let Err(e) = self
                    .position_manager
                    .sync_available_quote(available_quote)
                    .await
                {
                    error!(
                        expected = expected,
                        actual = available_quote,
                        error = %e,
                        "Failed to sync available_quote"
                    );
                } else {
                    info!(
                        expected = expected,
                        actual = available_quote,
                        diff = diff,
                        "Quote balance synchronized successfully"
                    );
                }
            }
        }

        // Get token balances and reconcile drift
        for (token_id, expected_balance) in expected_positions {
            let actual_balance = match self.get_token_balance(&token_id).await {
                Ok(b) => b,
                Err(e) => {
                    warn!(
                        token_id = %token_id,
                        error = %e,
                        "Failed to get token balance"
                    );
                    self.metrics.record_api_error("balance_query");
                    continue;
                }
            };

            positions.push(PositionSnapshot::new(
                token_id.clone(),
                actual_balance,
                None,
            ));

            // Check position drift
            let diff = (actual_balance - expected_balance).abs();
            // Allow small tolerance for rounding
            if diff > 0.000001 {
                warn!(
                    token_id = %token_id,
                    expected = expected_balance,
                    actual = actual_balance,
                    diff = diff,
                    "Position drift detected"
                );
                drift_detected = true;

                // Reconcile position with exchange data
                let exchange_pos = ExchangePosition::new(
                    token_id.clone(),
                    None, // market_id not available from balance API
                    actual_balance,
                    None, // entry_price not available from balance API
                    Utc::now(),
                );

                match self
                    .position_manager
                    .reconcile_position(&exchange_pos)
                    .await
                {
                    Ok(position_event) => {
                        info!(
                            token_id = %token_id,
                            expected = expected_balance,
                            actual = actual_balance,
                            "Position reconciled successfully"
                        );
                        // Emit the position event
                        if let Err(e) = self
                            .event_coordinator
                            .enqueue_event(SystemEvent::Position(position_event))
                            .await
                        {
                            error!(
                                token_id = %token_id,
                                error = %e,
                                "Failed to enqueue position reconciliation event"
                            );
                        }
                    }
                    Err(e) => {
                        error!(
                            token_id = %token_id,
                            error = %e,
                            "Failed to reconcile position"
                        );
                    }
                }
            }
        }

        debug!(
            available_quote = available_quote,
            positions = positions.len(),
            drift_detected = drift_detected,
            "Position sync complete"
        );

        self.metrics.record_position_sync(drift_detected);

        Ok(PositionSyncResult::new(
            positions,
            available_quote,
            drift_detected,
        ))
    }

    /// Get the rate limiter (for testing).
    #[cfg(test)]
    pub fn rate_limiter(&self) -> &RateLimiter {
        &self.rate_limiter
    }

    /// Get the config (for testing).
    #[cfg(test)]
    pub fn config(&self) -> &PollerConfig {
        &self.config
    }
}

/// Result of polling a single order.
struct SinglePollResult {
    had_fill: bool,
    completed: bool,
}
