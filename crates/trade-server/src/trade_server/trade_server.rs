use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use popeyes_trading_types::{MarketDataEvent, TokenEvent};
use prometheus::Registry;
use tracing::{debug, error, info, warn};

use crate::{
    api::{ApiServer, core_handler::CoreApiHandler},
    domain::SystemEvent,
    event_coordinator::{EventCoordinator, EventCoordinatorError},
    execution::{ExecutionEvent, OrderExecutor, fill_event_to_execution_event},
    notifier::Notifier,
    orderbook_tracker::OrderbookTracker,
    persistence::TradingEventPersistence,
    position::{ExitStrategy, PositionManager},
    signal::{SignalGenerator, TradableSignal},
    trade_server::{PositionHandler, TradeServerMetrics, event_monitor::EventMonitor},
};

pub struct TradeServer {
    event_coordinator: Arc<dyn EventCoordinator>,
    signal_generators: Vec<Box<dyn SignalGenerator>>,
    signal_notifier: Arc<dyn Notifier>,
    position_handler: PositionHandler,
    order_executor: Arc<dyn OrderExecutor + Send + Sync>,
    metrics: Arc<TradeServerMetrics>,
    persistence: Option<Arc<dyn TradingEventPersistence>>,
    event_monitor: EventMonitor,
    orderbook_tracker: OrderbookTracker,
    max_latency_ms: i64,
}

impl TradeServer {
    pub fn new(
        event_coordinator: Arc<dyn EventCoordinator>,
        signal_generators: Vec<Box<dyn SignalGenerator>>,
        signal_notifier: Arc<dyn Notifier>,
        position_manager: Arc<dyn PositionManager>,
        order_executor: Arc<dyn OrderExecutor + Send + Sync>,
        exit_strategy: Arc<dyn ExitStrategy>,
        max_sell_failures: u32,
        max_latency_ms: i64,
        registry: Registry,
    ) -> Self {
        Self::with_persistence(
            event_coordinator,
            signal_generators,
            signal_notifier,
            position_manager,
            order_executor,
            exit_strategy,
            max_sell_failures,
            max_latency_ms,
            registry,
            None,
        )
    }

    pub fn with_persistence(
        event_coordinator: Arc<dyn EventCoordinator>,
        signal_generators: Vec<Box<dyn SignalGenerator>>,
        signal_notifier: Arc<dyn Notifier>,
        position_manager: Arc<dyn PositionManager>,
        order_executor: Arc<dyn OrderExecutor + Send + Sync>,
        exit_strategy: Arc<dyn ExitStrategy>,
        max_sell_failures: u32,
        max_latency_ms: i64,
        registry: Registry,
        persistence: Option<Arc<dyn TradingEventPersistence>>,
    ) -> Self {
        let metrics = Arc::new(TradeServerMetrics::new(&registry));

        let position_handler = PositionHandler::with_persistence(
            Arc::clone(&position_manager),
            Arc::clone(&order_executor),
            Arc::clone(&signal_notifier),
            Arc::clone(&event_coordinator),
            Arc::clone(&exit_strategy),
            Some(Arc::clone(&metrics)),
            max_sell_failures,
            persistence.clone(),
        );

        let event_monitor = EventMonitor::new(Duration::from_secs(10));

        Self {
            event_coordinator,
            signal_generators,
            signal_notifier,
            position_handler,
            order_executor,
            metrics,
            persistence,
            event_monitor,
            orderbook_tracker: OrderbookTracker::new(),
            max_latency_ms,
        }
    }

    /// Runs the main trade server loop that:
    /// 1. Fetches the next event from the event coordinator
    /// 2. Processes the event by:
    ///    - Handling Position and Execution events directly
    ///    - Generating and processing signals from all signal generators
    /// 3. For each generated signal:
    ///    - Notifies about the signal
    ///    - Generates and executes orders through the position manager
    ///    - Enqueues as a new SystemEvent::Signal
    /// 4. Handles errors at each step with appropriate logging
    /// 5. On NoMoreEvents error:
    ///    - Gracefully exits the loop
    ///
    /// The loop continues until there are no more events or an unrecoverable error occurs.
    /// Returns Ok(()) when the event stream is exhausted or Err if an unrecoverable error occurs.
    pub async fn run(&mut self) -> Result<()> {
        // Initialize position metrics
        self.position_handler.update_position_metrics().await;

        // Start event monitoring with 10 second logging interval
        self.event_monitor
            .spawn_logging_task(Duration::from_secs(10));
        info!("Started event monitoring with 10 second reporting interval");

        loop {
            match self.event_coordinator.next_event().await {
                Ok(event) => {
                    let start_time = std::time::Instant::now();

                    // Record event for monitoring
                    self.event_monitor
                        .record_event(event.event_type().to_string())
                        .await;

                    // Track token events before processing
                    if let SystemEvent::Token(token_event) = &event {
                        match token_event {
                            TokenEvent::Buy(_) => {
                                self.metrics.token_events.with_label_values(&["buy"]).inc();
                            }
                            TokenEvent::Sell(_) => {
                                self.metrics.token_events.with_label_values(&["sell"]).inc();
                            }
                            TokenEvent::Create(_) => {
                                self.metrics
                                    .token_events
                                    .with_label_values(&["create"])
                                    .inc();
                            }
                            TokenEvent::Swap(_) => {
                                self.metrics.token_events.with_label_values(&["swap"]).inc();
                            }
                        }
                    }

                    // Track market data events separately
                    if let SystemEvent::MarketData(market_data_event) = &event {
                        match market_data_event {
                            MarketDataEvent::OrderbookUpdate(_) => {
                                self.metrics
                                    .token_events
                                    .with_label_values(&["orderbook_update"])
                                    .inc();
                            }
                            MarketDataEvent::OrderbookSnapshot(_) => {
                                self.metrics
                                    .token_events
                                    .with_label_values(&["orderbook_snapshot"])
                                    .inc();
                            }
                            MarketDataEvent::SpotPrice(_) => {
                                self.metrics
                                    .token_events
                                    .with_label_values(&["spot_price"])
                                    .inc();
                            }
                            MarketDataEvent::PolymarketTrade(_) => {
                                self.metrics
                                    .token_events
                                    .with_label_values(&["polymarket_trade"])
                                    .inc();
                            }
                        }
                    }

                    self.process_event(event).await?;
                    let duration_ms = start_time.elapsed().as_millis() as f64;
                    self.metrics.execution_latency.observe(duration_ms);
                }
                Err(e) => {
                    if self.handle_coordinator_error(e).await? {
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    /// Process a single system event
    async fn process_event(&mut self, event: SystemEvent) -> Result<()> {
        // Log event latency if available
        if let Some(latency_ms) = event.latency_ms() {
            debug!(
                event_type = %event.event_type(),
                latency_ms = latency_ms,
                "Event latency"
            );

            // Drop stale market data events based on configured max_latency_ms
            // Only filter MarketData and Token events, not internal events like LimitOrder/Execution
            if latency_ms > self.max_latency_ms {
                match &event {
                    SystemEvent::MarketData(_) | SystemEvent::Token(_) => {
                        debug!(
                            event_type = %event.event_type(),
                            latency_ms = latency_ms,
                            max_latency_ms = self.max_latency_ms,
                            "Dropping stale event"
                        );
                        self.metrics.stale_events_dropped.inc();
                        return Ok(());
                    }
                    _ => {
                        // Don't drop internal events (LimitOrder, Execution, Position, etc.)
                    }
                }
            }
        }

        // Transform orderbook events through the tracker and update price data
        let event = self.process_orderbook_events(event).await?;

        // Update price data from trade events
        if let SystemEvent::Token(token_event) = &event {
            if let TokenEvent::Buy(trade) | TokenEvent::Sell(trade) = token_event {
                self.order_executor.handle_token_trade(trade).await?;

                // Update position prices from trade events
                if let Some(price) = trade.spot_price() {
                    // Use trade.timestamp() which handles all trade event variants
                    let timestamp = trade.timestamp();
                    self.position_handler
                        .update_price(&trade.base_token_mint(), price, timestamp)
                        .await?;
                }
            }
        }

        // Handle specialized events first
        match &event {
            SystemEvent::Position(position_event) => {
                self.position_handler
                    .handle_position_event(position_event)
                    .await?;
            }
            SystemEvent::Timer(timer_event) => {
                self.position_handler
                    .handle_timer_event(timer_event)
                    .await?;
            }
            SystemEvent::Execution(execution_event) => {
                // Track execution events
                match execution_event {
                    ExecutionEvent::OrderFilled { .. } => {
                        self.metrics
                            .orders_executed
                            .with_label_values(&["market_order", "filled"])
                            .inc();
                    }
                    ExecutionEvent::OrderRejected { .. } => {
                        self.metrics
                            .orders_executed
                            .with_label_values(&["market_order", "rejected"])
                            .inc();
                    }
                }

                self.position_handler
                    .handle_execution_event(execution_event)
                    .await?;
            }
            SystemEvent::LimitOrder(limit_order_event) => {
                // Track limit order events with detailed metrics
                use crate::execution::LimitOrderEvent;
                match &limit_order_event {
                    LimitOrderEvent::OrderPlaced {
                        order_id,
                        mint,
                        price,
                        size,
                        side,
                        ..
                    } => {
                        // Track in orders_executed for overall tracking
                        self.metrics
                            .orders_executed
                            .with_label_values(&["limit_order", "placed"])
                            .inc();
                        // Track in limit_orders_placed with venue
                        self.metrics
                            .limit_orders_placed
                            .with_label_values(&["default", "success"])
                            .inc();
                        info!(
                            order_id = order_id,
                            mint = mint,
                            price = price,
                            size = size,
                            side = ?side,
                            "Limit order placed"
                        );
                    }
                    LimitOrderEvent::OrderPartiallyFilled {
                        order_id,
                        mint,
                        side,
                        filled_size,
                        fill_price,
                        ..
                    } => {
                        self.metrics
                            .orders_executed
                            .with_label_values(&["limit_order", "partial_fill"])
                            .inc();
                        info!(
                            order_id = order_id,
                            mint = mint,
                            side = ?side,
                            filled_size = filled_size,
                            fill_price = fill_price,
                            "Limit order partially filled"
                        );
                    }
                    LimitOrderEvent::OrderCancelled {
                        order_id, reason, ..
                    } => {
                        let is_cancel_ack = reason
                            .as_deref()
                            .is_some_and(|value| value.contains("awaiting confirmation"));
                        if is_cancel_ack {
                            info!(
                                order_id = order_id,
                                reason = ?reason,
                                "Limit order cancel command acknowledged"
                            );
                        } else {
                            self.metrics
                                .orders_executed
                                .with_label_values(&["limit_order", "cancelled"])
                                .inc();
                            self.metrics
                                .limit_orders_cancelled
                                .with_label_values(&["default"])
                                .inc();
                            info!(
                                order_id = order_id,
                                reason = ?reason,
                                "Limit order cancelled"
                            );
                        }
                    }
                    LimitOrderEvent::OrderExpired { order_id, .. } => {
                        self.metrics
                            .orders_executed
                            .with_label_values(&["limit_order", "expired"])
                            .inc();
                        self.metrics
                            .limit_orders_cancelled
                            .with_label_values(&["default"])
                            .inc();
                        info!(order_id = order_id, "Limit order expired");
                    }
                    LimitOrderEvent::OrderRejected { reason } => {
                        self.metrics
                            .orders_executed
                            .with_label_values(&["limit_order", "rejected"])
                            .inc();
                        self.metrics
                            .limit_orders_placed
                            .with_label_values(&["default", "rejected"])
                            .inc();
                        error!(reason = reason, "Limit order rejected");
                    }
                }

                // Update position manager's pending order state
                if let Err(e) = self
                    .position_handler
                    .position_manager()
                    .handle_limit_order_event(&limit_order_event)
                    .await
                {
                    error!(
                        err = ?e,
                        "Failed to update pending order state from limit order event"
                    );
                }

                // For fill events from poller/external sources, also update position
                // This is necessary for live trading where fills come from OrderStatusPoller
                // rather than from simulated fill detection via trade events
                if let Some(exec_event) = fill_event_to_execution_event(&limit_order_event) {
                    // Handle execution event in position manager to update inventory
                    match self
                        .position_handler
                        .position_manager()
                        .handle_execution(&exec_event)
                        .await
                    {
                        Ok(pos_event) => {
                            if let LimitOrderEvent::OrderPartiallyFilled {
                                mint,
                                side,
                                filled_size,
                                fill_price,
                                ..
                            } = &limit_order_event
                            {
                                info!(
                                    order_id = ?limit_order_event.order_id(),
                                    mint = %mint,
                                    side = ?side,
                                    filled_size = filled_size,
                                    fill_price = fill_price,
                                    "Fill event updated position"
                                );
                            }
                            // Enqueue to coordinator so signal generator receives inventory updates
                            if let Err(e) = self
                                .event_coordinator
                                .enqueue_event(SystemEvent::Position(pos_event))
                                .await
                            {
                                error!(
                                    err = ?e,
                                    "Failed to enqueue position event from fill"
                                );
                            }
                        }
                        Err(e) => {
                            error!(
                                err = ?e,
                                order_id = ?limit_order_event.order_id(),
                                "Failed to update position from fill event"
                            );
                        }
                    }
                }
            }
            SystemEvent::Redemption(redemption_event) => {
                // Update position manager state when redemptions complete
                match self
                    .position_handler
                    .position_manager()
                    .handle_redemption(redemption_event)
                    .await
                {
                    Ok(pos_events) => {
                        // Enqueue position events for signal generators to see
                        for pos_event in pos_events {
                            if let Err(e) = self
                                .event_coordinator
                                .enqueue_event(SystemEvent::Position(pos_event))
                                .await
                            {
                                error!(
                                    err = ?e,
                                    "Failed to enqueue position event from redemption"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        error!(
                            err = ?e,
                            "Failed to handle redemption event in position manager"
                        );
                    }
                }
            }
            _ => {}
        }

        self.generate_and_process_signals(&event).await?;
        Ok(())
    }

    /// Process orderbook events through the OrderbookTracker.
    ///
    /// This method:
    /// 1. Applies snapshots to the tracker, storing the full orderbook state
    /// 2. Applies updates to the tracker and converts them to normalized snapshots
    /// 3. Updates position prices from orderbook data
    /// 4. Evicts inactive markets on timer events
    ///
    /// Returns a potentially transformed event:
    /// - OrderbookUpdate events are converted to OrderbookSnapshot events
    /// - Other events pass through unchanged
    async fn process_orderbook_events(&mut self, event: SystemEvent) -> Result<SystemEvent> {
        match &event {
            SystemEvent::MarketData(MarketDataEvent::OrderbookSnapshot(snapshot)) => {
                // Apply snapshot to tracker
                let tracked_snapshot = self.orderbook_tracker.apply_snapshot(snapshot);

                // Let executor handle the snapshot
                self.order_executor
                    .handle_orderbook_snapshot(&tracked_snapshot)
                    .await?;

                // Calculate mid price and update positions
                self.update_position_from_snapshot(&tracked_snapshot)
                    .await?;

                Ok(event)
            }
            SystemEvent::MarketData(MarketDataEvent::OrderbookUpdate(update)) => {
                // Let executor handle the update
                self.order_executor.handle_orderbook_update(update).await?;

                // Apply update to tracker and get normalized snapshot
                if let Some(snapshot) = self.orderbook_tracker.apply_update(update) {
                    // Calculate mid price and update positions
                    self.update_position_from_snapshot(&snapshot).await?;

                    // Convert the update event to a snapshot event for signal generators
                    Ok(SystemEvent::MarketData(MarketDataEvent::OrderbookSnapshot(
                        snapshot,
                    )))
                } else {
                    // No prior snapshot - update mid price from best bid/ask anyway
                    let mid_price = (update.best_bid + update.best_ask) / 2.0;
                    let spread = update.best_ask - update.best_bid;
                    let timestamp = chrono::DateTime::from_timestamp_millis(update.timestamp)
                        .unwrap_or_else(chrono::Utc::now);

                    self.metrics
                        .orderbook_spread
                        .with_label_values(&[&update.asset_id])
                        .set(spread);
                    self.metrics
                        .orderbook_mid_price
                        .with_label_values(&[&update.asset_id])
                        .set(mid_price);

                    self.position_handler
                        .update_price(&update.asset_id, mid_price, timestamp)
                        .await?;

                    // Pass through the original event (signal generators won't receive a snapshot)
                    Ok(event)
                }
            }
            SystemEvent::MarketData(MarketDataEvent::PolymarketTrade(trade)) => {
                // Route trade events to executor for fill detection if it simulates fills
                if self.order_executor.simulates_fills() {
                    // Build inventory map from current positions
                    let inventory = self.build_inventory_map().await;
                    let available_quote = self
                        .position_handler
                        .position_manager()
                        .get_available_quote()
                        .await;

                    // Check for fills against pending orders
                    let fill_events = self
                        .order_executor
                        .check_fills_from_trade(trade, &inventory, available_quote)
                        .await;

                    // Route each simulated lifecycle event through the coordinator
                    for fill_event in fill_events {
                        let order_id = fill_event.order_id().map(str::to_string);
                        debug!(
                            order_id = ?order_id,
                            "Simulated fill detected from trade event"
                        );

                        // Route lifecycle events through the normal event loop so capture
                        // and position updates happen consistently from coordinator egress.
                        if let Err(e) = self
                            .event_coordinator
                            .enqueue_event(SystemEvent::LimitOrder(fill_event))
                            .await
                        {
                            error!(
                                err = ?e,
                                order_id = ?order_id,
                                "Failed to enqueue simulated lifecycle event"
                            );
                        }
                    }
                }

                Ok(event)
            }
            SystemEvent::Timer(_) => {
                // Evict inactive markets on timer events
                let evicted = self.orderbook_tracker.evict_inactive();
                if !evicted.is_empty() {
                    info!(
                        evicted_markets = ?evicted,
                        remaining_markets = self.orderbook_tracker.market_count(),
                        "Evicted inactive orderbooks"
                    );
                }
                Ok(event)
            }
            _ => Ok(event),
        }
    }

    /// Build an inventory map from current positions.
    ///
    /// Used for paper trading to check inventory constraints on sell orders.
    async fn build_inventory_map(&self) -> HashMap<String, f64> {
        let positions = self
            .position_handler
            .position_manager()
            .get_all_open_positions()
            .await;
        positions
            .into_iter()
            .map(|p| (p.mint.clone(), p.amount))
            .collect()
    }

    /// Update position prices and metrics from an orderbook snapshot
    async fn update_position_from_snapshot(
        &self,
        snapshot: &popeyes_trading_types::OrderbookSnapshotEvent,
    ) -> Result<()> {
        if let Some(mid_price) = calculate_mid_price(&snapshot.bids, &snapshot.asks) {
            let timestamp = chrono::DateTime::from_timestamp_millis(snapshot.timestamp)
                .unwrap_or_else(chrono::Utc::now);

            // Calculate spread and depth for metrics
            let best_bid = snapshot.bids.first().map(|b| b.price);
            let best_ask = snapshot.asks.first().map(|a| a.price);
            let spread = match (best_bid, best_ask) {
                (Some(bid), Some(ask)) => ask - bid,
                _ => 0.0,
            };
            let bid_depth: f64 = snapshot.bids.iter().map(|b| b.size).sum();
            let ask_depth: f64 = snapshot.asks.iter().map(|a| a.size).sum();

            // Update orderbook metrics
            self.metrics
                .orderbook_spread
                .with_label_values(&[&snapshot.asset_id])
                .set(spread);
            self.metrics
                .orderbook_mid_price
                .with_label_values(&[&snapshot.asset_id])
                .set(mid_price);
            self.metrics
                .orderbook_depth
                .with_label_values(&[&snapshot.asset_id, "bid"])
                .set(bid_depth);
            self.metrics
                .orderbook_depth
                .with_label_values(&[&snapshot.asset_id, "ask"])
                .set(ask_depth);

            debug!(
                asset_id = %snapshot.asset_id,
                bids_count = snapshot.bids.len(),
                asks_count = snapshot.asks.len(),
                mid_price = mid_price,
                spread = spread,
                bid_depth = bid_depth,
                ask_depth = ask_depth,
                "Updated position from orderbook"
            );

            self.position_handler
                .update_price(&snapshot.asset_id, mid_price, timestamp)
                .await?;
        } else {
            warn!(
                asset_id = %snapshot.asset_id,
                "Empty orderbook snapshot, skipping price update"
            );
        }
        Ok(())
    }

    /// Generate and process signals for an event
    async fn generate_and_process_signals(&mut self, event: &SystemEvent) -> Result<()> {
        // Collect all signals first to avoid multiple mutable borrows
        let mut all_signals: Vec<Box<dyn TradableSignal>> = Vec::new();

        // First pass: generate all signals and collect errors
        let mut errors = Vec::new();
        for (idx, signal_generator) in self.signal_generators.iter_mut().enumerate() {
            match signal_generator.generate_signal(event).await {
                Ok(signals) => {
                    debug!(
                        generator_idx = idx,
                        signals_count = signals.len(),
                        "Generated signals from generator"
                    );
                    all_signals.extend(signals);
                }
                Err(e) => errors.push(e),
            }
        }

        debug!(
            total_signals = all_signals.len(),
            "Collected signals from all generators"
        );

        // Handle any errors after the mutable borrow is done
        for error in errors {
            if let Err(e) = self.handle_signal_generation_error(error) {
                error!("Error in signal generation: {:?}", e);
                continue;
            }
        }

        // Second pass: process all signals
        for signal in all_signals {
            debug!(
                signal_type = signal.signal_type(),
                signal_id = signal.signal_id(),
                "Processing single signal"
            );
            if let Err(e) = self.process_single_signal(signal).await {
                error!("Error processing signal: {:?}", e);
                continue;
            }
        }

        Ok(())
    }

    /// Process a single generated signal
    async fn process_single_signal(&mut self, signal: Box<dyn TradableSignal>) -> Result<()> {
        let signal_type_label = signal.signal_type().to_string();

        // Increment signal processed metric
        self.metrics
            .signals_processed
            .with_label_values(&[signal_type_label.as_str()])
            .inc();

        // Always persist signal if persistence is configured
        if let Some(persistence) = &self.persistence {
            match signal.to_json() {
                Ok(signal_json) => {
                    let persistence = persistence.clone();
                    tokio::spawn(async move {
                        if let Err(e) = persistence.persist_signal(signal_json).await {
                            error!("Failed to persist signal: {:?}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to serialize signal to JSON: {:?}", e);
                }
            }
        }

        // Only notify and handle the signal if it passes the filter
        if signal.passes_filter() {
            // Notify if the signal is Notifiable
            if let Some(notifiable_signal) = signal.as_notifiable() {
                let notifier = self.signal_notifier.clone();
                tokio::spawn(async move {
                    info!(signal = signal_type_label, "Notifying signal");
                    if let Err(e) = notifier.notify(notifiable_signal.as_ref()).await {
                        error!("Failed to notify signal: {:?}", e);
                    }
                });
            }

            // Handle the signal using PositionHandler
            if let Err(e) = self.position_handler.handle_signal(signal.as_ref()).await {
                error!("Error handling signal in PositionHandler: {:?}", e);
            }
        } else {
            debug!(
                signal_type = signal.signal_type(),
                signal_id = signal.signal_id(),
                "Signal did not pass filter, skipping notification and order processing"
            );
        }

        Ok(())
    }

    /// Handle errors from signal generation
    fn handle_signal_generation_error(&self, error: anyhow::Error) -> Result<()> {
        Err(error)
    }

    /// Handle errors from the event coordinator
    async fn handle_coordinator_error(&mut self, error: anyhow::Error) -> Result<bool> {
        if let Some(EventCoordinatorError::NoMoreEvents) =
            error.downcast_ref::<EventCoordinatorError>()
        {
            info!("No more events, stopping trade server");
            Ok(true) // Signal to break the loop
        } else {
            error!("Failed to get next event: {:?}", error);
            Ok(false) // Continue the loop
        }
    }

    /// Create an API server with core handlers registered
    pub async fn create_api_server(&self) -> Result<Arc<ApiServer>> {
        let api_server = Arc::new(ApiServer::new());

        // Register core API handler
        let core_handler = Arc::new(CoreApiHandler::new(
            self.position_handler.position_manager().clone(),
        ));
        api_server.register_handler(core_handler).await?;

        Ok(api_server)
    }
}

/// Calculate the mid price from orderbook bid and ask levels.
///
/// The mid price is calculated as the average of the best bid and best ask prices.
/// Returns `None` if the orderbook is empty on both sides.
///
/// # Arguments
/// * `bids` - Bid price levels, expected to be sorted by price descending (best bid first)
/// * `asks` - Ask price levels, expected to be sorted by price ascending (best ask first)
fn calculate_mid_price(
    bids: &[popeyes_trading_types::OrderSummary],
    asks: &[popeyes_trading_types::OrderSummary],
) -> Option<f64> {
    let best_bid = bids.first().map(|b| b.price);
    let best_ask = asks.first().map(|a| a.price);

    match (best_bid, best_ask) {
        (Some(bid), Some(ask)) => Some((bid + ask) / 2.0),
        (Some(bid), None) => Some(bid),
        (None, Some(ask)) => Some(ask),
        (None, None) => None,
    }
}
