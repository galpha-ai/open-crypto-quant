# Cash-Flow-Based PnL Tracking - Implementation Tasks

## Overview

Implementation tasks for replacing position-close-based PnL tracking with cash-flow-based tracking. See [design.md](./design.md) for full specification.

## Tasks

### 1. Update PositionManagerState

**File**: `src/position/state.rs`

- [x] Rename `available_sol` to `available_quote`
- [x] Add `total_quote_received: f64` field (initialized to 0.0)
- [x] Add `total_quote_spent: f64` field (initialized to 0.0)
- [x] Remove `total_realized_pnl_sol` field
- [x] Update `PositionManagerState::new()` constructor

### 2. Update PositionManager Trait

**File**: `src/position/manager.rs`

- [x] Remove `get_total_realized_pnl_sol(&self) -> f64`
- [x] Remove `get_available_sol(&self) -> f64`
- [x] Add `get_available_quote(&self) -> f64`
- [x] Add `get_total_quote_received(&self) -> f64`
- [x] Add `get_total_quote_spent(&self) -> f64`
- [x] Add `get_net_cash_flow(&self) -> f64` with default impl
- [x] Add `get_total_pnl(&self) -> f64`
- [x] Add `get_total_unrealized_pnl(&self) -> f64`

### 3. Update Balance Tracker

**File**: `src/position/balance_tracker.rs`

- [x] Rename `update_sol_balance()` to `update_quote_balance()`
- [x] Update function to track `total_quote_received` on sells
- [x] Update function to track `total_quote_spent` on buys
- [x] Simplify signature: `(state, token_amount_change, quote_amount)`
- [x] Remove `calculate_realized_pnl()` function
- [x] Update `update_trade_statistics()` to remove `realized_pnl` parameter
- [x] Update tests

### 4. Update InMemPositionManager

**File**: `src/position/in_mem_manager.rs`

- [x] Implement `get_available_quote()` (rename from `get_available_sol`)
- [x] Implement `get_total_quote_received()`
- [x] Implement `get_total_quote_spent()`
- [x] Implement `get_total_pnl()` - sum cash flow + inventory MTM
- [x] Implement `get_total_unrealized_pnl()` - sum position unrealized PnL
- [x] Update all calls to renamed balance tracker functions
- [x] Remove `get_total_realized_pnl_sol()` implementation

### 5. Update Position Event Handlers

**File**: `src/trade_server/position_handler/event_handlers.rs`

- [x] Update `PositionClosedNotification` to use new methods
- [x] Update metrics recording to use `get_net_cash_flow()`
- [x] Update any references to `available_sol` or `realized_pnl_sol`

### 6. Update API Handlers

**File**: `src/api/core_handler.rs`

- [x] Update `core_getBalance` response:
  - `available_quote` (was `available_sol`)
  - `total_quote_received`
  - `total_quote_spent`
  - `net_cash_flow`
  - `total_pnl`
- [x] Update `core_getStats` response:
  - `net_cash_flow` (was `total_realized_pnl_sol`)
  - `total_unrealized_pnl`
  - `total_pnl`

### 7. Update Prometheus Metrics

**File**: `src/trade_server/metrics.rs`

- [x] Rename `realized_pnl` gauge to `net_cash_flow`
- [x] Add `total_quote_received` counter
- [x] Add `total_quote_spent` counter
- [x] Add `total_pnl` gauge
- [x] Add `total_unrealized_pnl` gauge

### 8. Update Backtest Runner

**File**: `src/backtest/runner.rs`

- [x] Update `BacktestMetrics` struct:
  - Add `total_quote_received`
  - Add `total_quote_spent`
  - Add `net_cash_flow`
  - Add `final_inventory_value`
  - Update `final_pnl` to be `net_cash_flow + final_inventory_value`
- [x] Update `finalize()` to populate new metrics
- [x] Update result logging to show new metrics

### 9. Update Domain Notifications

**File**: `src/domain/position_notification.rs`

- [x] Update `PositionClosedNotification` fields if needed
- [x] Rename any `_sol` suffixed fields

### 10. Update Tests

**Files**: Various `*_test.rs` files

- [x] Update `src/position/in_mem_manager_test.rs` for new method names
- [x] Update `src/position/balance_tracker.rs` tests
- [x] Add test: cash flow tracking across multiple buys/sells
- [x] Add test: `get_total_pnl()` includes inventory MTM
- [x] Add test: market maker scenario with non-zero PnL

### 11. Documentation Updates

- [x] Update `docs/architecture.md` if needed
- [x] Update `docs/usage-guide/` if API changes affect users

## Verification

After implementation:

1. Run all tests: `cargo test`
2. Run backtest with market maker strategy - verify non-zero `final_pnl`
3. Run backtest with directional strategy - verify correct PnL
4. Verify API responses include new fields
5. Verify Prometheus metrics are exported correctly
