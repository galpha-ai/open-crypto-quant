# Task Tracker: Backtest Support for Market Making Strategies

## 1. Problem Statement

The trade server currently lacks the ability to backtest market making strategies using historical data. While real-time trading infrastructure exists (EventCoordinator, OrderExecutor, PositionManager), there is no mechanism to:
- Load historical orderbook snapshots and trade events from Parquet files
- Simulate limit order execution using realistic trade-based fill logic
- Collect and export all events during a backtest for post-hoc analysis

This limits strategy development and validation since traders cannot evaluate performance against historical data before deploying to production.

## 2. Plan

Implement a complete backtest infrastructure that integrates with the existing trade server architecture:

1. **Data Loading Layer**: Create `ParquetLoader` to read orderbook snapshots and trade events from Parquet files, with optional outcome filtering for multi-outcome markets.

2. **Timeline Management**: Create `BacktestTimeline` that merges heterogeneous events (snapshots, trades) into chronological order and groups them into ticks for efficient processing.

3. **Enhanced Order Execution**: Extend `BacktestOrderExecutor` to support pending limit orders with trade-based fill simulation - orders fill when historical trades cross the order price.

4. **Event Coordination**: Create a new `BacktestEventCoordinator` (replacing the legacy version) that delivers events in proper priority order: enqueued events > buffered trades > timer events > next snapshot.

5. **Event Collection**: Create `BacktestEventCollector` to record all events during the backtest and export them to JSONL format for analysis.

6. **Runner Orchestration**: Create `BacktestRunner` as the main entry point that wires all components together and executes the backtest.

7. **Documentation**: Update architecture docs and create usage guide for backtest functionality.

## 3. Implementation Phases

### Phase 1: Core Infrastructure
- **Objective**: Establish data loading and timeline management capabilities
- **Tasks**: Tasks 1-5
- **Deliverable**: Working Parquet loader and timeline that can merge events chronologically

### Phase 2: Fill Simulation
- **Objective**: Implement trade-based limit order fill simulation
- **Tasks**: Tasks 6-10
- **Deliverable**: BacktestOrderExecutor that supports limit orders with realistic fill logic

### Phase 3: Event Coordination
- **Objective**: Create backtest-specific event coordinator and collector
- **Tasks**: Tasks 11-15
- **Deliverable**: Event coordinator that delivers events in correct priority order, plus event collector for JSONL output

### Phase 4: Runner & Integration
- **Objective**: Wire all components together and enable end-to-end backtesting
- **Tasks**: Tasks 16-19
- **Deliverable**: Working BacktestRunner that can execute complete backtests

### Phase 5: Documentation
- **Objective**: Update all relevant documentation
- **Tasks**: Tasks 20-21
- **Deliverable**: Updated architecture docs and new usage guide

## 4. TODO List

### Phase 1: Core Infrastructure

1. Add Parquet and Arrow dependencies to Cargo.toml
   - Status: Completed
   - Note: Added `parquet = { version = "54.0", default-features = false, features = ["arrow"] }` and `arrow = { version = "54.0", default-features = false, features = ["json"] }` to Cargo.toml.

2. Create backtest module structure
   - Status: Completed
   - Note: Created `src/backtest/mod.rs`, `src/backtest/types.rs`, `src/backtest/error.rs`. Defined `BacktestError` enum with variants: `ParquetReadError`, `InvalidSchemaError`, `EmptyDataError`, `TimelineError`, `IoError`, `JsonError`. Exported module from `src/lib.rs`.

3. Implement ParquetLoader for orderbook snapshots
   - Status: Completed
   - Note: Implemented `ParquetLoader::load_snapshots()` in `src/backtest/loader.rs`. Parses Parquet schema with `ts`, `ticker`, `end_date`, `outcome`, `bids` (JSON), `asks` (JSON). Converts to `OrderbookSnapshotEvent` from `popeyes_trading_types`. Supports optional `outcome_filter`. Returns sorted snapshots by timestamp.
   - Success Criteria: Verified via unit tests for JSON order level parsing.

4. Implement ParquetLoader for trade events
   - Status: Completed
   - Note: Implemented `ParquetLoader::load_trades()` in `src/backtest/loader.rs`. Uses `PolymarketTradeEvent` from `popeyes_trading_types` directly (no custom struct needed). Parses `ts`, `ticker`, `outcome`, `side` (BUY/SELL), `price`, `size`. Returns sorted trades by timestamp.
   - Success Criteria: Verified via unit tests.

5. Implement BacktestTimeline for event merging
   - Status: Completed
   - Note: Implemented `BacktestTimeline` in `src/backtest/timeline.rs`. Merges snapshots and trades chronologically into `BacktestTick` structs. Provides `Iterator` interface. Handles edge cases: trades before first snapshot (discarded), trades after last snapshot (included in last tick). Comprehensive unit tests covering all edge cases pass.
   - Success Criteria: All 10 timeline tests pass.

### Phase 2: Fill Simulation

6. Add pending order tracking to BacktestOrderExecutor
   - Status: Completed
   - Note: Added `pending_orders: Arc<Mutex<HashMap<String, PendingBacktestOrder>>>` and `order_id_counter: Arc<AtomicU64>` fields to `BacktestOrderExecutor`. Defined `PendingBacktestOrder` struct with fields: `order_id`, `mint`, `market`, `side` (OrderSide), `price`, `original_size`, `remaining_size`, `placed_at`, `time_in_force`, `signal_id`. Exported `PendingBacktestOrder` from module. Note: `event_coordinator` field not needed as fill events are returned directly from `handle_polymarket_trade()`.
   - Success Criteria: BacktestOrderExecutor compiles with new fields

7. Implement execute_limit_order for BacktestOrderExecutor
   - Status: Completed
   - Note: Implemented `execute_limit_order()` that creates `PendingBacktestOrder` from `Order`, generates unique order ID using atomic counter (format: `bt-order-{n}`), adds to `pending_orders` map, returns `LimitOrderEvent::OrderPlaced`. Validates that order type is limit (buy/sell). Does not implement immediate fill for IOC orders - relies on subsequent trade events.
   - Success Criteria: Limit orders are tracked in pending_orders and OrderPlaced event is returned

8. Implement trade-based fill simulation
   - Status: Completed
   - Note: Implemented `handle_polymarket_trade(&self, trade: &PolymarketTradeEvent) -> Result<Vec<LimitOrderEvent>>`. Trade-based fill logic: BID orders fill when SELL trades cross at/below bid price; ASK orders fill when BUY trades cross at/above ask price. Fill size capped by `min(order.remaining_size, trade.size)`. Fills at order price (not trade price). Emits `OrderPartiallyFilled` for both partial and full fills (with `remaining_size=0.0` for full fills). Removes fully filled orders from pending_orders.
   - Success Criteria: Orders fill correctly when trades cross their price, partial fills work

9. Implement cancel_order for BacktestOrderExecutor
   - Status: Completed
   - Note: Implemented `cancel_order()` that removes order from `pending_orders` by ID and returns `LimitOrderEvent::OrderCancelled`. Returns error if order not found. Also implemented `supports_limit_orders()` returning `true`.
   - Success Criteria: Can cancel pending orders, get error for non-existent orders

10. Add unit tests for BacktestOrderExecutor fill simulation
    - Status: Completed
    - Note: Added 17 new tests covering: limit order placement (buy/sell), bid fills on sell trades (at/below price), ask fills on buy trades (at/above price), partial fills with remaining size tracking, trades that don't cross price (no fill), buy/sell trade direction filtering, cancel removes order, cancel nonexistent error, multiple orders fill from same trade, different asset no fill, `supports_limit_orders()`, non-limit order type error, unique order ID generation. All 20 tests (3 existing + 17 new) pass.
    - Success Criteria: All fill simulation edge cases are covered with passing tests

### Phase 3: Event Coordination

11. Remove legacy BacktestEventCoordinator
    - Status: Completed
    - Note: Removed `src/event_coordinator/backtest_event_coordinator.rs` and `src/event_coordinator/backtest_event_coordinator_test.rs`. Updated `src/event_coordinator/mod.rs` to remove conditional compilation and exports. The `legacy_code` feature flag is no longer referenced in code.
    - Success Criteria: Legacy code completely removed, no references remain, cargo check passes

12. Create new BacktestEventCoordinator
    - Status: Completed
    - Note: Implemented `BacktestEventCoordinator` in `src/backtest/coordinator.rs`. Uses `CoordinatorState` with: `timeline: BacktestTimeline`, `current_tick: Option<BacktestTick>`, `snapshot_delivered: bool`, `trade_index: usize`, `enqueued_events: VecDeque<SystemEvent>`, `timer_state: TimerState`, `current_time: DateTime<Utc>`, `exhausted: bool`. Implements `EventCoordinator` trait with `next_event()` priority: (1) enqueued events, (2) snapshot for current tick, (3) buffered trades, (4) timer events, (5) advance to next tick. Returns `NoMoreEvents` when timeline exhausted. Includes `enqueue_limit_order_events()` helper method.
    - Success Criteria: Events delivered in correct priority order, timeline advances correctly

13. Implement BacktestEventCollector
    - Status: Completed
    - Note: Implemented `BacktestEventCollector` in `src/backtest/collector.rs` with `events: Mutex<Vec<CollectedEvent>>`. Methods: `record(&self, event: &SystemEvent)`, `write_to_file(&self, path: &Path) -> Result<()>`, `events(&self) -> Vec<CollectedEvent>`, `len()`, `is_empty()`. Includes comprehensive conversion logic for all `SystemEvent` variants (Token, MarketData, Timer, Signal, Position, Execution, LimitOrder). `CollectedEvent` struct with `timestamp` (ISO 8601), `event_type` (String with subtype e.g., "LimitOrder.OrderPlaced"), `data` (serde_json::Value). Unit tests verify JSONL output format.
    - Success Criteria: Can record events and write valid JSONL file with one JSON object per line

14. Integrate BacktestEventCoordinator with BacktestOrderExecutor
    - Status: Completed
    - Note: Integration uses a callback pattern: the event loop processes `PolymarketTrade` events through `executor.handle_polymarket_trade()`, which returns fill events. The caller then enqueues these via `coordinator.enqueue_limit_order_events(fills)`. This avoids circular dependency since the coordinator doesn't hold a reference to the executor. The `BacktestEventCoordinator.enqueue_limit_order_events()` method wraps each `LimitOrderEvent` in `SystemEvent::LimitOrder` and adds to the priority queue.
    - Success Criteria: Fill events from executor are properly enqueued and delivered by coordinator

15. Add integration tests for event coordination
    - Status: Completed
    - Note: Created integration tests in `src/backtest/integration_test.rs`. Tests cover: (1) `test_chronological_order_preserved` - events in timestamp order, (2) `test_fill_events_enqueued_with_priority` - enqueued events take priority, (3) `test_timer_events_fire_between_ticks` - timer events at correct intervals, (4) `test_no_more_events_when_exhausted` - NoMoreEvents returned when timeline exhausted, (5) `test_event_coordination_flow` - full event flow with executor and collector, (6) `test_collector_records_all_event_types` - collector captures all event types. All 7 integration tests pass.
    - Success Criteria: Integration tests pass demonstrating correct event flow

### Phase 4: Runner & Integration

16. Implement BacktestConfig and PositionConfig
    - Status: Completed
    - Note: Implemented `BacktestConfig` and `PositionConfig` in `src/backtest/config.rs`. `BacktestConfig` includes: `snapshot_path`, `trade_path`, `outcome_filter`, `position` (PositionConfig), `timer_interval`, `output_path`, `buy_slippage`, `sell_slippage`. `PositionConfig` includes: `initial_balance`, `max_open_positions`, `max_holding_period`, `trade_amount`, `take_profit_threshold`, `stop_loss_threshold`, `max_sell_failures`. Builder pattern supported via `with_*` methods. Validation via `validate()` method. Serde support with humantime-serde for Duration fields. Unit tests for defaults, builder, validation, and chrono conversion.
    - Success Criteria: Config structs defined with serde support for YAML/JSON loading

17. Implement BacktestRunner
    - Status: Completed
    - Note: Implemented `BacktestRunner::run()` in `src/backtest/runner.rs`. Orchestrates: (1) ParquetLoader to load snapshots and trades, (2) BacktestTimeline from loaded data, (3) BacktestEventCoordinator with timeline and timer interval, (4) BacktestOrderExecutor with slippage settings, (5) InMemoryPositionManager with ConfigurableExitStrategy, (6) BacktestEventCollector for JSONL output. Event loop processes OrderbookSnapshot, PolymarketTrade, Timer, LimitOrder, Execution, and Position events. Handles NoMoreEvents for clean termination. Returns BacktestResult with output path and metrics.
    - Success Criteria: Can execute complete backtest with all components wired together

18. Implement BacktestMetrics calculation
    - Status: Completed
    - Note: Implemented `BacktestResult` and `BacktestMetrics` in `src/backtest/runner.rs`. `BacktestMetrics` includes: `total_snapshots`, `total_trades`, `total_signals`, `total_orders_placed`, `total_fills`, `total_cancels`, `final_pnl`, `final_inventory`, `duration_ms`, `data_start_time`, `data_end_time`. Metrics collected during event loop processing. Final PnL and inventory calculated from position manager state after loop completion.
    - Success Criteria: Metrics accurately reflect backtest activity

19. Add end-to-end backtest test
    - Status: Completed
    - Note: Implemented 4 e2e tests in `src/backtest/e2e_test.rs`: (1) `test_backtest_e2e_basic` - verifies snapshot/trade processing and JSONL output validity, (2) `test_backtest_reproducibility` - same input produces same metrics, (3) `test_backtest_empty_data` - handles empty data gracefully with error, (4) `test_backtest_data_time_range` - verifies data time range captured in metrics. Includes helper functions `create_snapshot_parquet()` and `create_trade_parquet()` for generating test Parquet files. Also includes example `SimpleMarketMaker` and `MarketMakerSignal` implementations for documentation purposes. All 4 tests pass.
    - Success Criteria: End-to-end test passes with realistic signal generator

### Phase 5: Documentation

20. Update architecture.md with backtest components
    - Status: Completed
    - Note: Updated `docs/architecture.md` with: (1) New `src/backtest/` module structure in directory tree, (2) New "Backtest Support" section covering all components (ParquetLoader, BacktestTimeline, BacktestEventCoordinator, BacktestOrderExecutor, BacktestEventCollector, BacktestRunner), (3) ASCII data flow diagram showing component interactions, (4) Configuration structs documented, (5) Parquet schema tables, (6) BacktestMetrics struct, (7) Key design decisions for backtest mode.
    - Success Criteria: Architecture doc accurately reflects new backtest infrastructure

21. Create backtest usage guide
    - Status: Completed
    - Note: Created `docs/usage-guide/backtesting.md` with: (1) Prerequisites section with Parquet schema tables, (2) Basic usage with full code example, (3) Configuration options (BacktestConfig, PositionConfig) with builder pattern examples, (4) Complete signal generator implementation example (SimpleMarketMaker), (5) JSONL output format documentation with event type table, (6) Fill simulation logic explanation, (7) Performance considerations section, (8) Common patterns (filtering, analysis, comparing strategies), (9) Troubleshooting guide. Also updated `docs/usage-guide/README.md` to include backtesting in the guide contents table.
    - Success Criteria: New users can follow guide to run their first backtest

## 5. Usage Guide

### For the AI Agent

This tracker documents the implementation of backtest support for the trade-server. Use it as follows:

1. **Task Execution**:
   - Work through tasks in phase order (Phase 1 before Phase 2, etc.)
   - Within a phase, tasks can often be done in parallel, but note dependencies (e.g., Task 3 depends on Task 1)
   - Mark task status as `In Progress` when starting, `Completed` when done
   - If blocked, document the blocker in the Note field

2. **Status Updates**:
   - Update the Status field immediately when a task changes state
   - Add implementation details to the Note field as work progresses
   - If the approach changes, document the rationale

3. **Success Criteria**:
   - Verify each task's success criteria before marking as Completed
   - Run relevant tests to confirm functionality
   - Check that code compiles without warnings

4. **Phase Completion**:
   - Complete all tasks in a phase before moving to the next
   - Run `cargo check` and `cargo test` at the end of each phase
   - The user can instruct "complete Phase N" to execute all tasks in that phase

5. **Task References**:
   - Tasks can be referenced by number (e.g., "complete task 3")
   - Dependencies between tasks are implicit in the ordering

6. **Note Field Evolution**:
   - Initial notes provide scope and file locations
   - Expand notes with implementation details during work
   - Document any deviations from the original plan

7. **Before Creating PR**:
   - Ensure all tests pass
   - Verify documentation is updated (Tasks 19-20)
   - Remove this tracker file from the PR (it documents the process, not the result)
   - Review all files changed against the design doc for consistency

### Key File Locations

- Design specification: `docs/specs/004-backtest-support/design.md`
- BacktestOrderExecutor: `src/execution/backtest/executor.rs`
- BacktestEventCoordinator: `src/backtest/coordinator.rs`
- BacktestEventCollector: `src/backtest/collector.rs`
- BacktestTimeline: `src/backtest/timeline.rs`
- ParquetLoader: `src/backtest/loader.rs`
- Integration tests: `src/backtest/integration_test.rs`
- Position Manager trait: `src/position/manager.rs`
- InMemoryPositionManager: `src/position/in_mem_manager.rs`
- Cargo.toml: `Cargo.toml`
- Architecture doc: `docs/architecture.md`
- Usage guides: `docs/usage-guide/`
