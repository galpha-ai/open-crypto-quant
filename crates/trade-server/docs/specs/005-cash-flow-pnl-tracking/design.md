# Cash-Flow-Based PnL Tracking

## Summary

Replace the position-close-based PnL tracking with cash-flow-based tracking that works for all strategy types. The current implementation only calculates realized PnL when positions fully close, which never happens for market makers. By tracking cumulative quote currency inflows/outflows, we can compute accurate PnL for both directional and market-making strategies.

## Problem Statement

### Current Behavior

The `PositionManager` trait exposes `get_total_realized_pnl_sol()` which only increments when a position is **closed** (amount reaches ~0). The `update_trade_statistics()` function in `balance_tracker.rs` is only called when `position.amount.abs() < POSITION_DUST_THRESHOLD`.

### Impact on Market Makers

For market maker strategies:
- Positions accumulate through continuous buying and selling at different prices
- Positions rarely close completely - inventory builds up over time
- The "realized PnL" metric never increments

Evidence from a backtest run:
```
Final PnL: 0.0000
Final inventory: -171.6632
Orders filled: 67
```

Analysis of the 67 fills showed:
- Total quote received (from sells): 152.42
- Total quote spent (for buys): 67.23
- **Net cash flow: +85.19** (actual profit from spread capture)
- Mark-to-market PnL at 0.50: -0.64

The strategy made ~85 in quote currency from spread capture, but `final_pnl` reported 0.

### Additional Issues

1. **Currency-specific naming**: The `_sol` suffix is baked into the trait, but the system now supports orderbook venues beyond Solana
2. **Inconsistent abstraction**: The trait assumes discrete entry/exit cycles, which doesn't match continuous trading patterns

## Goals

- Track cumulative quote currency received and spent on every fill
- Compute Total PnL as: `Net Cash Flow + Inventory Mark-to-Market`
- Generalize naming to be currency-agnostic (remove `_sol` suffix)
- Update all consumers: API handlers, metrics, backtest runner

## Non-Goals

- Multi-currency portfolio tracking (single quote currency per PositionManager instance)
- Historical PnL time series or equity curves (can be built on top later)
- FIFO/LIFO cost basis tracking for tax purposes
- Position-level PnL attribution (only aggregate tracking)

## API

### PositionManager Trait

New methods to add:

```rust
/// Get cumulative quote currency received from all sells
async fn get_total_quote_received(&self) -> f64;

/// Get cumulative quote currency spent on all buys
async fn get_total_quote_spent(&self) -> f64;

/// Get net cash flow (received - spent)
/// This represents realized profit/loss from completed round-trips
async fn get_net_cash_flow(&self) -> f64 {
    self.get_total_quote_received().await - self.get_total_quote_spent().await
}

/// Get total PnL including unrealized inventory value
/// Formula: net_cash_flow + sum(position.amount * position.current_price)
async fn get_total_pnl(&self) -> f64;

/// Get total unrealized PnL from open positions
/// Formula: sum(position.amount * (current_price - entry_price))
async fn get_total_unrealized_pnl(&self) -> f64;
```

Methods to remove:
- `get_total_realized_pnl_sol()` - replaced by `get_net_cash_flow()`
- `get_available_sol()` - replaced by `get_available_quote()`

### JSON-RPC API

**`core_getBalance`** - Updated response:

```json
{
  "available_quote": 10.5,
  "total_quote_received": 152.42,
  "total_quote_spent": 67.23,
  "net_cash_flow": 85.19,
  "total_pnl": 84.55
}
```

**`core_getStats`** - Updated response:

```json
{
  "total_closed_positions": 15,
  "winning_trades": 10,
  "win_rate_pct": 66.67,
  "net_cash_flow": 85.19,
  "total_unrealized_pnl": -0.64,
  "total_pnl": 84.55,
  "open_position_count": 3
}
```

### Errors

No new error types required. Existing `PositionError` variants are sufficient.

## Behavior

### Cash Flow Tracking on Execution

Given an `ExecutionEvent::OrderFilled` or `ExecutionEvent::BuyConfirmed`/`SellConfirmed`:

1. Extract `quote_amount` from the event (or calculate as `amount * price`)
2. Determine trade direction from the event
3. If **buy**: increment `total_quote_spent` by `quote_amount`
4. If **sell**: increment `total_quote_received` by `quote_amount`
5. Update `available_quote` as before (subtract for buys, add for sells)

This happens in `balance_tracker::update_sol_balance()` (to be renamed `update_quote_balance()`).

### Total PnL Calculation

Given a request for total PnL:

1. Calculate `net_cash_flow = total_quote_received - total_quote_spent`
2. Calculate `inventory_mtm = sum(position.amount * position.current_price)` for all open positions
3. Return `net_cash_flow + inventory_mtm`

### Unrealized PnL Calculation

For strategies that want traditional unrealized PnL:

1. For each open position with `entry_price` and `current_price`:
   - `position_unrealized = amount * (current_price - entry_price)`
2. Sum across all positions
3. Return total

Note: For market makers with no clear entry price, this may be less meaningful than `total_pnl`.

## Data Model

### PositionManagerState

Updated structure:

```rust
pub struct PositionManagerState {
    pub available_quote: f64,

    // Cash flow tracking
    pub total_quote_received: f64,      // cumulative from sells
    pub total_quote_spent: f64,         // cumulative from buys

    // Trade statistics
    pub total_closed_positions: u32,
    pub winning_trades: u32,

    // Position tracking
    pub positions: HashMap<String, Position>,
    pub pending_buys: HashSet<String>,
    pub pending_sells: HashSet<String>,
}
```

### Migration

The field `total_realized_pnl_sol` is removed. For in-memory state, no migration needed. If this were persisted (it's not currently), migration would sum historical fills.

## Configuration

No new configuration required. The quote currency is implicit in the venue/market being traded.

## Balance Tracker Updates

### Renamed Functions

| Old Name                   | New Name                  |
|----------------------------|---------------------------|
| `update_sol_balance()`     | `update_quote_balance()`  |
| `calculate_realized_pnl()` | Remove (no longer needed) |

### New Implementation

```rust
/// Updates quote balance and cash flow tracking based on a trade execution.
pub fn update_quote_balance(
    state: &mut PositionManagerState,
    token_amount_change: f64,
    quote_amount: f64,
) {
    if token_amount_change > 0.0 {
        // Buy - quote flows out
        state.available_quote -= quote_amount;
        state.total_quote_spent += quote_amount;
    } else {
        // Sell - quote flows in
        state.available_quote += quote_amount;
        state.total_quote_received += quote_amount;
    }
}
```

### Trade Statistics

`update_trade_statistics()` continues to track `total_closed_positions` and `winning_trades` for strategies that have discrete position lifecycles. The `realized_pnl` parameter is removed as it's now derived from cash flow.

## Metrics Updates

### Prometheus Metrics

| Old Metric     | New Metric             | Type    |
|----------------|------------------------|---------|
| `realized_pnl` | `net_cash_flow`        | Gauge   |
| -              | `total_quote_received` | Counter |
| -              | `total_quote_spent`    | Counter |
| -              | `total_pnl`            | Gauge   |
| -              | `total_unrealized_pnl` | Gauge   |

### Backtest Metrics

```rust
pub struct BacktestMetrics {
    pub total_signals: u64,
    pub orders_submitted: u64,
    pub orders_filled: u64,

    // PnL metrics
    pub total_quote_received: f64,
    pub total_quote_spent: f64,
    pub net_cash_flow: f64,
    pub final_inventory: f64,
    pub final_inventory_value: f64,
    pub final_pnl: f64,  // net_cash_flow + final_inventory_value
}
```

## Idempotency & Concurrency

Cash flow updates happen within the existing `PositionManagerState` mutex lock, maintaining the same concurrency guarantees as the current implementation. Each execution event is processed exactly once.

## Acceptance Criteria

- [ ] `get_net_cash_flow()` returns correct value after a sequence of buys and sells
- [ ] `get_total_pnl()` correctly includes inventory mark-to-market
- [ ] Backtest with market maker strategy shows non-zero `final_pnl` reflecting spread capture
- [ ] Backtest with directional strategy shows correct PnL
- [ ] API endpoints return new fields (`net_cash_flow`, `total_pnl`, etc.)
- [ ] Prometheus metrics include new gauges/counters
- [ ] All tests pass
- [ ] No `_sol` suffix remains in public API

## Observability

- **Logs**: Log cash flow updates at DEBUG level with `quote_received`/`quote_spent` fields
- **Metrics**:
  - `trade_server_net_cash_flow` (gauge): Current net cash flow
  - `trade_server_total_pnl` (gauge): Current total PnL including inventory
  - `trade_server_quote_received_total` (counter): Cumulative quote received
  - `trade_server_quote_spent_total` (counter): Cumulative quote spent
- **Alerts**: No new alerts required (existing position/execution alerts sufficient)

## Security

No security implications. This is internal accounting logic with no new external inputs or privilege changes.

## References

- [Implementation Tasks](./tasks.md) - Detailed implementation checklist
- [Architecture Overview](../architecture.md) - Trade server architecture
- [Position Manager Trait](../../src/position/manager.rs) - Current trait definition
- [Balance Tracker](../../src/position/balance_tracker.rs) - Current balance tracking implementation
