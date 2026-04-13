//! Backtest runner - main orchestrator for executing backtests.
//!
//! The `BacktestRunner` coordinates all backtest components:
//! - Loads data using `ParquetLoader`
//! - Coordinates events using `BacktestEventCoordinator`
//! - Executes orders using `BacktestOrderExecutor`
//! - Tracks positions using `InMemoryPositionManager`
//! - Streams events to JSONL output

use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use chrono::{DateTime, Utc};
use prometheus::Registry;
use tracing::{debug, info, warn};

use popeyes_trading_types::{
    OrderbookSnapshotEvent, OrderbookUpdateEvent, PolymarketTradeEvent, SpotPriceUpdate,
};

use crate::{
    event_coordinator::{
        CapturingEventCoordinator, EventCollector, EventCoordinator, EventCoordinatorError,
        JsonlFileEventCollector,
    },
    execution::backtest::BacktestOrderExecutor,
    orderbook_tracker::OrderbookTracker,
    position::{
        ConfigurableExitStrategy, ExitStrategy, InMemoryPositionManager, NoopExitStrategy,
        PositionManager,
    },
    signal::SignalGenerator,
};

use super::{
    BacktestEventCoordinator, ParquetLoader,
    completeness::{DataCompletenessChecker, IncompleteDataBehavior},
    config::{BacktestConfig, ExitStrategyMode},
    error::BacktestError,
    processor::BacktestEventProcessor,
};

/// Result of a completed backtest run.
#[derive(Debug, Clone)]
pub struct BacktestResult {
    /// Path to the JSONL output file
    pub output_path: PathBuf,

    /// Summary metrics for the backtest
    pub metrics: BacktestMetrics,
}

/// Summary metrics collected during a backtest.
#[derive(Debug, Clone, Default)]
pub struct BacktestMetrics {
    /// Total number of orderbook snapshots processed
    pub total_snapshots: u64,

    /// Total number of trade events processed
    pub total_trades: u64,

    /// Total number of signals generated
    pub total_signals: u64,

    /// Total number of limit orders placed
    pub total_orders_placed: u64,

    /// Total number of order fills (partial + full)
    pub total_fills: u64,

    /// Total number of bid (buy) order fills
    pub total_bid_fills: u64,

    /// Total number of ask (sell) order fills
    pub total_ask_fills: u64,

    /// Total number of order cancellations
    pub total_cancels: u64,

    /// Cumulative quote currency received from all sells
    pub total_quote_received: f64,

    /// Cumulative quote currency spent on all buys
    pub total_quote_spent: f64,

    /// Net cash flow (quote received - quote spent)
    pub net_cash_flow: f64,

    /// Final inventory (net position size)
    pub final_inventory: f64,

    /// Mark-to-market value of final inventory
    pub final_inventory_value: f64,

    /// Final PnL in quote currency (net_cash_flow + final_inventory_value)
    pub final_pnl: f64,

    /// Duration of the backtest run
    pub duration_ms: u64,

    /// Start time of the data
    pub data_start_time: Option<DateTime<Utc>>,

    /// End time of the data
    pub data_end_time: Option<DateTime<Utc>>,

    /// Total number of pair redemptions executed
    pub total_redemptions: u64,

    /// Total quote currency received from redemptions
    pub total_redemption_value: f64,
}

/// Initialized backtest components ready for event loop execution.
struct BacktestComponents {
    /// The capturing coordinator wrapping the backtest coordinator
    coordinator: Arc<CapturingEventCoordinator<BacktestEventCoordinator>>,
    /// The inner backtest coordinator (for fill simulation)
    inner_coordinator: Arc<BacktestEventCoordinator>,
    executor: BacktestOrderExecutor,
    position_manager: Arc<InMemoryPositionManager>,
    collector: Arc<dyn EventCollector>,
}

/// Backtest runner that orchestrates the execution of a backtest.
pub struct BacktestRunner;

impl BacktestRunner {
    /// Run a backtest with the given configuration and signal generator.
    ///
    /// # Arguments
    /// * `config` - Backtest configuration
    /// * `signal_generator` - The signal generator to use for generating trading signals
    ///
    /// # Returns
    /// A `BacktestResult` containing the output path and summary metrics.
    pub async fn run(
        config: BacktestConfig,
        mut signal_generator: Box<dyn SignalGenerator + Send>,
    ) -> Result<BacktestResult> {
        let start_time = std::time::Instant::now();
        let mut metrics = BacktestMetrics::default();

        info!(
            snapshot_path = %config.snapshot_path.display(),
            update_path = %config.update_path.display(),
            trade_path = %config.trade_path.display(),
            outcome_filter = ?config.outcome_filter,
            ticker_patterns = ?config.ticker_patterns,
            output_path = %config.output_path.display(),
            "Starting backtest"
        );

        // Phase 1: Load data
        let (snapshots, updates, trades, spot_events) = Self::load_data(&config)?;
        if snapshots.is_empty() {
            return Self::handle_empty_data(&config);
        }

        // Phase 1.5: Check data completeness
        let (snapshots, updates, trades) =
            Self::check_data_completeness(&config, snapshots, updates, trades)?;
        if snapshots.is_empty() {
            warn!("No complete markets found after completeness filtering");
            return Self::handle_empty_data(&config);
        }

        // Phase 2: Capture time range
        Self::capture_time_range(&snapshots, &mut metrics);

        // Phase 3: Initialize components
        let components =
            Self::initialize_components(&config, snapshots, updates, trades, spot_events)?;

        // Initialize optional signal persistence
        let persistence = if let Some(signal_path) = &config.signal_output_path {
            tracing::info!(signal_path = %signal_path.display(), "Initializing signal persistence");
            let position_path = signal_path.with_extension("positions.jsonl");
            match crate::persistence::FileTradingEventPersistence::new(
                signal_path.clone(),
                position_path,
            ) {
                Ok(p) => {
                    tracing::info!("Signal persistence initialized successfully");
                    Some(Arc::new(p) as Arc<dyn crate::persistence::TradingEventPersistence>)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to initialize signal persistence");
                    None
                }
            }
        } else {
            tracing::info!("No signal_output_path configured, skipping signal persistence");
            None
        };

        // Phase 4: Run event loop
        {
            let mut processor = BacktestEventProcessor::new(
                &mut signal_generator,
                Arc::clone(&components.position_manager),
                &components.executor,
                &components.inner_coordinator,
                persistence,
            );
            Self::run_event_loop(&components.coordinator, &mut processor, &mut metrics).await?;
        }

        // Phase 5: Finalize
        Self::finalize(
            &config.output_path,
            start_time,
            &components.position_manager,
            components.collector.as_ref(),
            &mut metrics,
        )
        .await
    }

    /// Load snapshots, trades, and optional spot events from files.
    fn load_data(
        config: &BacktestConfig,
    ) -> Result<(
        Vec<OrderbookSnapshotEvent>,
        Vec<OrderbookUpdateEvent>,
        Vec<PolymarketTradeEvent>,
        Option<Vec<SpotPriceUpdate>>,
    )> {
        let loader = ParquetLoader::with_filters_and_time_range(
            config.outcome_filter.clone(),
            config.ticker_patterns.clone(),
            config.ticker_time_range_filter,
            config.ticker_time_range_buffer,
        );

        let snapshots = loader.load_snapshots(&config.snapshot_path)?;
        let updates = loader.load_updates(&config.update_path)?;
        let trades = loader.load_trades(&config.trade_path)?;

        // Load spot events if configured
        let spot_events = if let Some(ref spot_path) = config.spot_event_path {
            info!(spot_path = %spot_path.display(), "Attempting to load spot events");
            match loader.load_spot_events(spot_path) {
                Ok(spot_events) => {
                    info!(
                        snapshot_count = snapshots.len(),
                        trade_count = trades.len(),
                        spot_event_count = spot_events.len(),
                        "Successfully loaded Parquet data and spot events"
                    );
                    Some(spot_events)
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        spot_path = %spot_path.display(),
                        "Failed to load spot events, continuing without them"
                    );
                    None
                }
            }
        } else {
            info!(
                snapshot_count = snapshots.len(),
                trade_count = trades.len(),
                "Loaded Parquet data (no spot events configured)"
            );
            None
        };

        Ok((snapshots, updates, trades, spot_events))
    }

    #[allow(dead_code)]
    fn apply_updates_to_snapshots(
        mut snapshots: Vec<OrderbookSnapshotEvent>,
        mut updates: Vec<OrderbookUpdateEvent>,
    ) -> Vec<OrderbookSnapshotEvent> {
        if updates.is_empty() {
            return snapshots;
        }

        snapshots.sort_by_key(|s| s.timestamp);
        updates.sort_by_key(|u| u.timestamp);

        let mut tracker = OrderbookTracker::new();
        let mut merged = Vec::with_capacity(snapshots.len() + updates.len());
        let mut snapshot_iter = snapshots.into_iter().peekable();
        let mut update_iter = updates.into_iter().peekable();
        let mut applied_updates = 0_u64;
        let mut ignored_updates = 0_u64;

        loop {
            let next_snapshot_ts = snapshot_iter.peek().map(|s| s.timestamp);
            let next_update_ts = update_iter.peek().map(|u| u.timestamp);

            match (next_snapshot_ts, next_update_ts) {
                (Some(snapshot_ts), Some(update_ts)) if snapshot_ts <= update_ts => {
                    while snapshot_iter
                        .peek()
                        .map(|s| s.timestamp == snapshot_ts)
                        .unwrap_or(false)
                    {
                        if let Some(snapshot) = snapshot_iter.next() {
                            let tracked_snapshot = tracker.apply_snapshot(&snapshot);
                            merged.push(tracked_snapshot);
                        }
                    }
                }
                (Some(_), Some(update_ts)) | (None, Some(update_ts)) => {
                    while update_iter
                        .peek()
                        .map(|u| u.timestamp == update_ts)
                        .unwrap_or(false)
                    {
                        if let Some(update) = update_iter.next() {
                            if let Some(snapshot) = tracker.apply_update(&update) {
                                merged.push(snapshot);
                                applied_updates += 1;
                            } else {
                                ignored_updates += 1;
                            }
                        }
                    }
                }
                (Some(_), None) => {
                    while let Some(snapshot) = snapshot_iter.next() {
                        let tracked_snapshot = tracker.apply_snapshot(&snapshot);
                        merged.push(tracked_snapshot);
                    }
                }
                (None, None) => break,
            }
        }

        info!(
            snapshot_count = merged.len(),
            updates_applied = applied_updates,
            updates_ignored = ignored_updates,
            "Built snapshot stream with synthetic updates"
        );

        merged
    }

    /// Check data completeness and handle according to configuration.
    ///
    /// Returns filtered data if `on_incomplete` is `Filter`, otherwise returns
    /// the original data (possibly after logging warnings or returning an error).
    fn check_data_completeness(
        config: &BacktestConfig,
        snapshots: Vec<OrderbookSnapshotEvent>,
        updates: Vec<OrderbookUpdateEvent>,
        trades: Vec<PolymarketTradeEvent>,
    ) -> Result<(
        Vec<OrderbookSnapshotEvent>,
        Vec<OrderbookUpdateEvent>,
        Vec<PolymarketTradeEvent>,
    )> {
        if !config.completeness.enabled {
            debug!("Data completeness checking disabled");
            return Ok((snapshots, updates, trades));
        }

        let checker = DataCompletenessChecker::new(config.completeness.clone());
        let report = checker.check(&snapshots, &trades);

        // Always log the report
        checker.log_report(&report);

        // Handle based on configuration
        match config.completeness.on_incomplete {
            IncompleteDataBehavior::Warn => {
                // Just continue with all data
                Ok((snapshots, updates, trades))
            }
            IncompleteDataBehavior::Filter => {
                // Filter out incomplete markets
                let (filtered_snapshots, filtered_trades) =
                    checker.filter_complete(&snapshots, &trades, &report);

                let complete_tickers = report.complete_tickers();
                let original_updates = updates.len();
                let filtered_updates: Vec<_> = updates
                    .into_iter()
                    .filter(|u| {
                        u.market_metadata
                            .as_ref()
                            .map(|m| complete_tickers.contains(m.ticker.as_str()))
                            .unwrap_or(false)
                    })
                    .collect();

                info!(
                    original_snapshots = snapshots.len(),
                    filtered_snapshots = filtered_snapshots.len(),
                    original_trades = trades.len(),
                    filtered_trades = filtered_trades.len(),
                    original_updates,
                    filtered_updates = filtered_updates.len(),
                    "Filtered incomplete markets"
                );

                Ok((filtered_snapshots, filtered_updates, filtered_trades))
            }
            IncompleteDataBehavior::Fail => {
                if report.incomplete_count > 0 {
                    let details: Vec<String> = report
                        .markets
                        .iter()
                        .filter(|m| !m.is_complete)
                        .map(|m| {
                            let reasons: Vec<String> =
                                m.failure_reasons.iter().map(|r| r.to_string()).collect();
                            format!("{}: {}", m.ticker, reasons.join(", "))
                        })
                        .collect();

                    Err(BacktestError::DataCompletenessError {
                        incomplete_count: report.incomplete_count,
                        details: details.join("; "),
                    }
                    .into())
                } else {
                    Ok((snapshots, updates, trades))
                }
            }
        }
    }

    /// Handle the case when no snapshots are found.
    fn handle_empty_data(config: &BacktestConfig) -> Result<BacktestResult> {
        warn!("No snapshots found in Parquet file");
        let collector = JsonlFileEventCollector::new(config.output_path.clone())?;
        collector.export(&config.output_path)?;

        Ok(BacktestResult {
            output_path: config.output_path.clone(),
            metrics: BacktestMetrics::default(),
        })
    }

    /// Capture the time range from snapshot data.
    fn capture_time_range(snapshots: &[OrderbookSnapshotEvent], metrics: &mut BacktestMetrics) {
        if let Some(first_snapshot) = snapshots.first() {
            metrics.data_start_time = DateTime::from_timestamp_millis(first_snapshot.timestamp);
        }
        if let Some(last_snapshot) = snapshots.last() {
            metrics.data_end_time = DateTime::from_timestamp_millis(last_snapshot.timestamp);
        }
    }

    /// Initialize all backtest components.
    fn initialize_components(
        config: &BacktestConfig,
        snapshots: Vec<OrderbookSnapshotEvent>,
        updates: Vec<OrderbookUpdateEvent>,
        trades: Vec<PolymarketTradeEvent>,
        spot_events: Option<Vec<SpotPriceUpdate>>,
    ) -> Result<BacktestComponents> {
        // Create inner backtest coordinator
        let inner_coordinator = Arc::new(BacktestEventCoordinator::new_from_data(
            snapshots,
            updates,
            trades,
            spot_events,
            config.timer_interval,
        ));

        // Create collector
        let collector: Arc<dyn EventCollector> =
            Arc::new(JsonlFileEventCollector::new(config.output_path.clone())?);

        // Wrap with capturing coordinator, passing logical time function for backtest
        let capture_cfg = config.capture.clone();
        let coordinator = Arc::new(CapturingEventCoordinator::with_logical_time_fn_and_filter(
            Arc::clone(&inner_coordinator),
            collector.clone(),
            {
                let coord = Arc::clone(&inner_coordinator);
                Arc::new(move || coord.current_time())
            },
            Arc::new(move |event| capture_cfg.should_capture(event)),
        ));

        // Create executor with latency simulation if configured
        let executor = BacktestOrderExecutor::builder()
            .buy_slippage(config.buy_slippage)
            .sell_slippage(config.sell_slippage)
            .enforce_inventory_constraints(true)
            .latency_config(config.latency.clone())
            .build();

        // Create exit strategy from config
        let exit_strategy: Arc<dyn ExitStrategy> = match config.position.exit_strategy_mode {
            ExitStrategyMode::Noop => Arc::new(NoopExitStrategy::new()),
            ExitStrategyMode::Configurable => Arc::new(ConfigurableExitStrategy::new(
                config.position.take_profit_threshold,
                config.position.stop_loss_threshold,
                config.position.max_holding_period_chrono(),
                config.position.max_sell_failures,
            )),
        };

        // Create a no-op registry for backtest (we don't use Prometheus metrics)
        let registry = Registry::new();

        // Create position manager with optional quote-lifetime debounce.
        let min_quote_lifetime_ms = config
            .latency
            .as_ref()
            .and_then(|latency| latency.min_quote_lifetime_ms);
        let position_manager = Arc::new(InMemoryPositionManager::new_with_quote_lifetime(
            config.position.trade_amount,
            config.position.initial_balance,
            config.position.max_holding_period_chrono(),
            config.position.max_open_positions,
            min_quote_lifetime_ms,
            &registry,
            exit_strategy,
        ));

        Ok(BacktestComponents {
            coordinator,
            inner_coordinator,
            executor,
            position_manager,
            collector,
        })
    }

    /// Run the main event loop.
    async fn run_event_loop(
        coordinator: &Arc<CapturingEventCoordinator<BacktestEventCoordinator>>,
        processor: &mut BacktestEventProcessor<'_>,
        metrics: &mut BacktestMetrics,
    ) -> Result<()> {
        loop {
            // Get next event from capturing coordinator (events are recorded automatically)
            let event = match coordinator.next_event().await {
                Ok(event) => event,
                Err(e) => {
                    // Check if this is NoMoreEvents (normal termination)
                    if e.downcast_ref::<EventCoordinatorError>()
                        .map(|e| matches!(e, EventCoordinatorError::NoMoreEvents))
                        .unwrap_or(false)
                    {
                        debug!("Backtest completed - no more events");
                        break;
                    }
                    // Unexpected error
                    return Err(e);
                }
            };

            // Process the event (no need to record - already captured by coordinator)
            processor.process_event(&event, metrics).await?;
        }

        Ok(())
    }

    /// Finalize the backtest and write output.
    async fn finalize(
        output_path: &PathBuf,
        start_time: std::time::Instant,
        position_manager: &Arc<InMemoryPositionManager>,
        collector: &dyn EventCollector,
        metrics: &mut BacktestMetrics,
    ) -> Result<BacktestResult> {
        // Get cash flow metrics
        metrics.total_quote_received = position_manager.get_total_quote_received().await;
        metrics.total_quote_spent = position_manager.get_total_quote_spent().await;
        metrics.net_cash_flow = position_manager.get_net_cash_flow().await;

        // Calculate final inventory from open positions
        let open_positions = position_manager.get_all_open_positions().await;
        metrics.final_inventory = open_positions.iter().map(|p| p.amount).sum();

        // Calculate inventory mark-to-market value
        metrics.final_inventory_value = open_positions
            .iter()
            .filter_map(|p| p.current_price.map(|price| p.amount * price))
            .sum();

        // Final PnL = net cash flow + inventory value
        metrics.final_pnl = metrics.net_cash_flow + metrics.final_inventory_value;

        // Record duration
        metrics.duration_ms = start_time.elapsed().as_millis() as u64;

        // Write output
        collector.export(output_path)?;

        info!(
            event_count = collector.len(),
            total_snapshots = metrics.total_snapshots,
            total_trades = metrics.total_trades,
            total_signals = metrics.total_signals,
            total_orders_placed = metrics.total_orders_placed,
            total_fills = metrics.total_fills,
            total_bid_fills = metrics.total_bid_fills,
            total_ask_fills = metrics.total_ask_fills,
            total_cancels = metrics.total_cancels,
            total_quote_received = metrics.total_quote_received,
            total_quote_spent = metrics.total_quote_spent,
            net_cash_flow = metrics.net_cash_flow,
            final_inventory = metrics.final_inventory,
            final_inventory_value = metrics.final_inventory_value,
            final_pnl = metrics.final_pnl,
            duration_ms = metrics.duration_ms,
            "Backtest completed"
        );

        Ok(BacktestResult {
            output_path: output_path.clone(),
            metrics: metrics.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use popeyes_trading_types::{OrderSummary, OrderbookSource, OrderbookUpdateEvent, TradeSide};
    use tempfile::tempdir;

    use crate::config::LatencySimulationConfig;
    use crate::execution::OrderSide;
    use crate::position::PositionManager;
    use crate::position::pending_order::PendingLimitOrder;
    use crate::signal::{OrderIntent, QuoteLevel};

    #[test]
    fn test_metrics_default() {
        let metrics = BacktestMetrics::default();
        assert_eq!(metrics.total_snapshots, 0);
        assert_eq!(metrics.total_trades, 0);
        assert_eq!(metrics.total_signals, 0);
        assert_eq!(metrics.total_orders_placed, 0);
        assert_eq!(metrics.total_fills, 0);
        assert_eq!(metrics.total_cancels, 0);
        assert_eq!(metrics.total_quote_received, 0.0);
        assert_eq!(metrics.total_quote_spent, 0.0);
        assert_eq!(metrics.net_cash_flow, 0.0);
        assert_eq!(metrics.final_inventory, 0.0);
        assert_eq!(metrics.final_inventory_value, 0.0);
        assert_eq!(metrics.final_pnl, 0.0);
        assert_eq!(metrics.total_redemptions, 0);
        assert_eq!(metrics.total_redemption_value, 0.0);
    }

    #[test]
    fn test_backtest_result() {
        let result = BacktestResult {
            output_path: PathBuf::from("/tmp/test.jsonl"),
            metrics: BacktestMetrics {
                total_snapshots: 100,
                total_trades: 500,
                total_signals: 50,
                total_orders_placed: 25,
                total_fills: 20,
                total_bid_fills: 12,
                total_ask_fills: 8,
                total_cancels: 5,
                total_quote_received: 200.0,
                total_quote_spent: 60.0,
                net_cash_flow: 140.0,
                final_inventory: 10.0,
                final_inventory_value: 10.0,
                final_pnl: 150.0,
                duration_ms: 1000,
                data_start_time: None,
                data_end_time: None,
                total_redemptions: 3,
                total_redemption_value: 30.0,
            },
        };

        assert_eq!(result.output_path, PathBuf::from("/tmp/test.jsonl"));
        assert_eq!(result.metrics.total_snapshots, 100);
        assert_eq!(result.metrics.net_cash_flow, 140.0);
        assert_eq!(result.metrics.final_pnl, 150.0);
        assert_eq!(result.metrics.total_redemptions, 3);
        assert_eq!(result.metrics.total_redemption_value, 30.0);
    }

    #[test]
    fn test_apply_updates_to_snapshots_builds_synthetic_snapshots() {
        let snapshot = OrderbookSnapshotEvent {
            asset_id: "asset-1".to_string(),
            market: "MARKET".to_string(),
            bids: vec![OrderSummary {
                price: 0.49,
                size: 1.0,
            }],
            asks: vec![OrderSummary {
                price: 0.51,
                size: 1.0,
            }],
            hash: "snapshot-hash".to_string(),
            timestamp: 1000,
            observed_at: Utc.timestamp_millis_opt(1000).unwrap(),
            source: OrderbookSource::Polymarket,
            market_metadata: None,
        };

        let early_update = OrderbookUpdateEvent {
            asset_id: "asset-1".to_string(),
            market: "MARKET".to_string(),
            price: 0.49,
            size: 2.0,
            side: TradeSide::Buy,
            hash: "update-early".to_string(),
            best_bid: 0.49,
            best_ask: 0.51,
            timestamp: 900,
            observed_at: Utc.timestamp_millis_opt(900).unwrap(),
            source: OrderbookSource::Polymarket,
            market_metadata: None,
        };

        let late_update = OrderbookUpdateEvent {
            asset_id: "asset-1".to_string(),
            market: "MARKET".to_string(),
            price: 0.49,
            size: 2.5,
            side: TradeSide::Buy,
            hash: "update-late".to_string(),
            best_bid: 0.49,
            best_ask: 0.51,
            timestamp: 1500,
            observed_at: Utc.timestamp_millis_opt(1500).unwrap(),
            source: OrderbookSource::Polymarket,
            market_metadata: None,
        };

        let merged = BacktestRunner::apply_updates_to_snapshots(
            vec![snapshot],
            vec![early_update, late_update],
        );

        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].timestamp, 1000);
        assert_eq!(merged[1].timestamp, 1500);
        assert_eq!(merged[1].bids.len(), 1);
        assert_eq!(merged[1].bids[0].size, 2.5);
        assert_eq!(merged[1].hash, "update-late");
    }

    #[tokio::test]
    async fn test_build_components_wires_quote_update_debounce_from_latency_config() {
        let tmp = tempdir().unwrap();
        let mut config = BacktestConfig::new(
            tmp.path().join("snapshots.parquet"),
            tmp.path().join("updates.parquet"),
            tmp.path().join("trades.parquet"),
            tmp.path().join("events.jsonl"),
        );
        config.latency = Some(LatencySimulationConfig {
            min_place_latency_ms: 0,
            max_place_latency_ms: 0,
            seed: Some(7),
            min_cancel_latency_ms: Some(0),
            max_cancel_latency_ms: Some(0),
            min_quote_lifetime_ms: Some(500),
        });

        let components =
            BacktestRunner::initialize_components(&config, vec![], vec![], vec![], None).unwrap();

        let base_time = Utc::now();
        components.position_manager.add_pending_order_for_testing(
            "token123",
            "bid1",
            PendingLimitOrder::new(
                "bid1".to_string(),
                "token123".to_string(),
                Some("market1".to_string()),
                OrderSide::Buy,
                0.42,
                7.5,
                base_time,
                None,
            ),
        );

        let transient_reprice = OrderIntent::new(
            "token123".to_string(),
            Some("market1".to_string()),
            Some(vec![QuoteLevel::gtc(0.41, 7.5)]),
            None,
            "sig-reprice".to_string(),
            base_time + chrono::Duration::milliseconds(100),
            None,
        );
        let orders = components
            .position_manager
            .reconcile_intent(&transient_reprice)
            .await
            .unwrap();

        assert!(
            orders.is_empty(),
            "Expected backtest runner to honor min_quote_lifetime_ms and debounce transient quote updates"
        );
    }
}
