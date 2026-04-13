# Task Tracker: Paper Trading Implementation

## 1. Problem Statement

Implement paper trading mode for the market making bot using intent-based orders. The bot should run continuously against live market data, produce signals, and simulate order fills using trade events—similar to backtest mode but in real-time. This bridges the gap between backtest (historical data) and live mode (real execution), allowing strategy validation without risking capital.

## 2. Plan

1. **Create Paper Trading Executor** - New executor that maintains pending orders in memory and simulates fills from live trade events
2. **Add Configuration** - New `ExecutionMode::PaperTrading` variant with fill model settings
3. **Wire Event Routing** - Route `PolymarketTrade` events to paper executor for fill checking in TradeServer
4. **Integrate with Position Manager** - Connect fill events back to position manager for position/balance updates
5. **Test & Validate** - Unit tests, integration tests, metrics

## 3. Implementation Phases

### Phase 1: Core Executor
- **Objective**: Create the paper trading executor with fill simulation logic
- **Tasks**: 1-4
- **Deliverable**: `PaperTradingOrderExecutor` that can place/cancel orders and detect fills from trades

### Phase 2: Configuration & Integration
- **Objective**: Wire paper executor into TradeServer with proper config
- **Tasks**: 5-8
- **Deliverable**: TradeServer can be configured for paper trading mode and routes events correctly

### Phase 3: Testing & Validation
- **Objective**: Ensure correctness and add observability
- **Tasks**: 9-11
- **Deliverable**: Tested, metrics-enabled paper trading mode

## 4. TODO List

### Phase 1: Core Executor

1. Create paper trading module structure
   - Status: Completed
   - Note: Files: `src/execution/paper/mod.rs`, `src/execution/paper/executor.rs`. Created module with `PaperTradingOrderExecutor` struct containing `pending_orders: Arc<RwLock<HashMap<String, PendingPaperOrder>>>`. Exported from `src/execution/mod.rs`.
   - Success Criteria: Module compiles and is exported from `src/execution/mod.rs`

2. Define `PendingPaperOrder` struct
   - Status: Completed
   - Note: File: `src/execution/paper/executor.rs`. Defined struct with fields: `order_id`, `mint`, `market`, `side`, `price`, `original_size`, `remaining_size`, `placed_at`, `time_in_force`, `signal_id`, `exit_mode`, `context`. Follows patterns from `src/execution/backtest/executor.rs`.
   - Success Criteria: Struct defined with all necessary fields for fill simulation

3. Implement `OrderExecutor` trait for `PaperTradingOrderExecutor`
   - Status: Completed
   - Note: File: `src/execution/paper/executor.rs`. Implemented `execute_limit_order()` (adds to pending_orders, emits `LimitOrderEvent::OrderPlaced`), `cancel_order()` (removes from pending_orders, emits `LimitOrderEvent::OrderCancelled`), `execute_redemption()`, `supports_limit_orders()`, `supports_redemption()`. Market orders return `OrderRejected` since paper trading is limit-order focused.
   - Success Criteria: `place_order` and `cancel_order` work correctly, emit proper events

4. Implement `check_fills_from_trade()` method
   - Status: Completed
   - Note: File: `src/execution/paper/executor.rs`. Ported fill simulation logic from backtest executor. Method signature: `pub async fn check_fills_from_trade(&self, trade: &PolymarketTradeEvent, inventory: &HashMap<String, f64>) -> Vec<LimitOrderEvent>`. Checks if trade price crosses pending order prices, generates `LimitOrderEvent::OrderPartiallyFilled` for matches, handles partial fills, respects inventory constraints for sell orders.
   - Success Criteria: Correctly detects fills when trade prices cross order prices, handles partial fills

### Phase 2: Configuration & Integration

5. Add `PaperTradingConfig` struct
   - Status: Completed
   - Note: File: `src/config.rs`. Added `PaperTradingConfig` with fields: `fill_model: FillModelConfig`, `enforce_inventory: bool`, `simulated_latency_ms: Option<u64>`. Added `FillModelConfig` enum with `ExactPrice` and `WithSlippage { basis_points: u32 }` variants. Added default implementations for all fields.
   - Success Criteria: Config struct compiles and is usable in executor construction

6. Add `ExecutionMode::PaperTrading` variant
   - Status: Completed
   - Note: File: `src/config.rs`. Added `ExecutionModeConfig` enum with `Live` (default), `PaperTrading(PaperTradingConfig)`, and `Backtest` variants. Added `execution_mode` field to `ExecutionConfig`. Backward compatible - existing configs use default `Live` mode.
   - Success Criteria: Config can express paper trading mode, existing configs still work

7. Instantiate paper executor in TradeServer
   - Status: Completed
   - Note: File: `src/trade_server/trade_server.rs`. Added `paper_executor: Option<Arc<PaperTradingOrderExecutor>>` field to TradeServer struct. Added `with_paper_executor(executor)` builder method to configure paper trading mode. Executor instantiation left to the caller (library user) for flexibility.
   - Success Criteria: TradeServer constructs paper executor when configured for paper trading

8. Route trade events to paper executor
   - Status: Completed
   - Note: File: `src/trade_server/trade_server.rs`. In `process_orderbook_events()`, added handling for `PolymarketTrade` events that routes to paper executor when configured. Builds inventory map from positions via `build_inventory_map()`, calls `check_fills_from_trade()`, and processes fills through `position_manager.handle_limit_order_event()`.
   - Success Criteria: Trade events trigger fill checking, fill events update position manager

### Phase 3: Testing & Validation

9. Add unit tests for paper executor
   - Status: Completed
   - Note: File: `src/execution/paper/executor_test.rs`. Implemented 32 test cases covering: basic functionality, limit order placement, order cancellation, fill detection (bid/ask orders with various trade scenarios), inventory constraints, partial fills, multiple order handling, redemption support, and utility methods. All tests pass.
   - Success Criteria: All unit tests pass

10. Add metrics for paper trading
    - Status: Completed
    - Note: File: `src/execution/paper/metrics.rs`. Created `PaperTradingMetrics` struct with Prometheus metrics: `paper_trading_orders_placed_total` (counter by side), `paper_trading_orders_cancelled_total` (counter), `paper_trading_orders_filled_total` (counter by side), `paper_trading_fill_volume_total` (counter by side), `paper_trading_fill_latency_ms` (histogram), `paper_trading_pending_orders` (gauge). Integrated with executor via `with_metrics()` builder method. Added 2 unit tests for metrics. Exported from `src/execution/mod.rs`.
    - Success Criteria: Metrics exported and visible in Prometheus

11. Validate against backtest results
    - Status: Not Started
    - Note: Run both backtest and paper trading on same recorded data period. Compare: number of signals, number of fills, final PnL. Results should be comparable (not identical due to timing differences). Document any significant discrepancies.
    - Success Criteria: Paper trading results within reasonable tolerance of backtest results on same data

## 5. Usage Guide

**For AI Agent Execution:**
- Update Status to "In Progress" when starting a task
- Update Status to "Completed" when done
- Verify Success Criteria before marking complete
- Reference tasks by number (e.g., "complete task 3")
- Complete phases sequentially (Phase 1 before Phase 2, etc.)
- Expand Notes during planning with implementation steps

**Key References:**
- `src/execution/backtest/executor.rs` - Template for fill simulation logic
- `src/trade_server/trade_server.rs` - Main event loop
- `src/position/in_mem_manager.rs:400-500` - `handle_limit_order_event()`
- `src/trade_server/position_handler/intent_handlers.rs:50-120` - Two-phase execution
- `src/config.rs:150-180` - Existing `SimulationMode`
