# Issue: Paper Trading Implementation for Intent-Based Orders

## Summary

Implement paper trading mode for the market making bot using intent-based orders. The bot should run continuously against live market data, produce signals, and simulate order fills using trade events—similar to backtest mode but in real-time.

## Background

The market making strategy uses intent-based orders via `OrderIntent` and the `ReconciliationEngine`. Currently:
- **Backtest mode**: Works correctly - processes historical data in batch, simulates fills from historical trades
- **Live mode**: Executes real orders on venues
- **Paper trading mode**: Not implemented

Paper trading bridges the gap—running against live data without risking capital.

## Current Infrastructure (Reusable)

| Component                           | Location                                               | Status   |
|-------------------------------------|--------------------------------------------------------|----------|
| ReconciliationEngine                | `src/position/reconciliation/engine.rs`                | Ready    |
| IntentHandler (two-phase execution) | `src/trade_server/position_handler/intent_handlers.rs` | Ready    |
| InMemoryPositionManager             | `src/position/in_mem_manager.rs`                       | Ready    |
| LimitOrderEvent lifecycle           | `src/execution/events.rs`                              | Ready    |
| BacktestOrderExecutor (template)    | `src/execution/backtest/executor.rs`                   | Template |
| OrderbookTracker                    | `src/orderbook_tracker/mod.rs`                         | Ready    |
| PendingLimitOrder tracking          | `src/position/pending_order.rs`                        | Ready    |

## Gaps to Address

### Gap 1: Paper Trading Order Executor

**Problem**: `BacktestOrderExecutor` is designed for batch processing of historical data. It processes all events upfront and isn't wired for continuous operation.

**Solution**: Create `PaperTradingOrderExecutor` that:
- Implements `OrderExecutor` trait
- Maintains pending orders in memory (like backtest)
- Exposes `check_fills_from_trade(&PolymarketTrade)` for continuous fill detection
- Emits `LimitOrderEvent` when fills occur

**Reference**: `src/execution/backtest/executor.rs:78-180` - fill simulation logic to adapt

```rust
// Proposed structure
pub struct PaperTradingOrderExecutor {
    pending_orders: Arc<RwLock<HashMap<String, PendingPaperOrder>>>,
    config: PaperTradingConfig,
    metrics: PaperTradingMetrics,
}

impl PaperTradingOrderExecutor {
    /// Called when a live trade event is received
    pub fn check_fills_from_trade(&self, trade: &PolymarketTrade) -> Vec<LimitOrderEvent>;

    /// Place a new order (from reconciliation)
    pub async fn place_order(&self, order: LimitOrder) -> Result<LimitOrderEvent>;

    /// Cancel an existing order
    pub async fn cancel_order(&self, order_id: &str) -> Result<LimitOrderEvent>;
}
```

### Gap 2: Trade Event Routing in TradeServer

**Problem**: The `TradeServer` event loop doesn't route `PolymarketTrade` events to an executor for fill checking in live/paper modes.

**Solution**: Add trade event handling path in `TradeServer`:

```rust
// In TradeServer event loop
SystemEvent::MarketData(MarketDataEvent::PolymarketTrade(trade)) => {
    if let ExecutionMode::PaperTrading = self.config.execution_mode {
        let fill_events = self.paper_executor.check_fills_from_trade(&trade);
        for event in fill_events {
            self.position_manager.handle_limit_order_event(event).await?;
        }
    }
    // Continue with signal generation...
}
```

**Reference**: `src/trade_server/trade_server.rs` - main event loop

### Gap 3: Fill Event Integration with Position Manager

**Problem**: When paper executor detects a fill, it needs to notify `PositionManager` to update positions and balances.

**Solution**: Wire `LimitOrderEvent` from paper executor back to position manager:

```rust
// Paper executor emits LimitOrderEvent::OrderFilled
// PositionManager.handle_limit_order_event() already handles this correctly
// Just need the wiring in TradeServer
```

**Reference**: `src/position/in_mem_manager.rs:400-500` - `handle_limit_order_event()`

### Gap 4: Configuration for Paper Trading Mode

**Problem**: Current `SimulationMode` is oriented toward market orders. Paper trading needs its own config.

**Solution**: Add `ExecutionMode::PaperTrading` variant:

```rust
pub enum ExecutionMode {
    Live,
    PaperTrading(PaperTradingConfig),
    Backtest,
}

pub struct PaperTradingConfig {
    /// Fill model: exact price or with slippage
    pub fill_model: FillModel,
    /// Whether to enforce inventory constraints (no naked shorts)
    pub enforce_inventory: bool,
    /// Simulated latency for order placement
    pub simulated_latency_ms: Option<u64>,
}

pub enum FillModel {
    /// Fill at exact order price (simple, unrealistic)
    ExactPrice,
    /// Fill with configurable slippage
    WithSlippage { basis_points: u32 },
}
```

**Reference**: `src/config.rs:150-180` - existing `SimulationMode`

### Gap 5: Order State Synchronization

**Problem**: In continuous operation, intents can update rapidly. Need to ensure order state stays consistent.

**Solution**:
- Paper executor tracks order state with proper locking
- Reconciliation engine already handles idempotent updates
- Two-phase execution (cancels before placements) prevents balance issues

**Reference**: `src/trade_server/position_handler/intent_handlers.rs:50-120` - two-phase execution

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                     Paper Trading Data Flow                          │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│   Redis Streams ──────────► TradeServer Event Loop                   │
│   (Live orderbook,                    │                              │
│    live trades)                       │                              │
│                                       ▼                              │
│                        ┌──────────────────────────────┐              │
│                        │      Event Router            │              │
│                        └──────────────────────────────┘              │
│                                       │                              │
│                    ┌──────────────────┼──────────────────┐           │
│                    ▼                  ▼                  ▼           │
│           OrderbookSnapshot    PolymarketTrade      Timer            │
│                    │                  │                  │           │
│                    ▼                  │                  │           │
│           SignalGenerator             │                  │           │
│                    │                  │                  │           │
│                    ▼                  │                  │           │
│              OrderIntent              │                  │           │
│                    │                  │                  │           │
│                    ▼                  │                  │           │
│           ReconciliationEngine        │                  │           │
│                    │                  │                  │           │
│                    ▼                  ▼                  │           │
│           ┌───────────────────────────────────────┐     │           │
│           │     PaperTradingOrderExecutor         │◄────┘           │
│           │  ┌─────────────────────────────────┐  │                 │
│           │  │  pending_orders: HashMap        │  │                 │
│           │  │  - place_order()                │  │                 │
│           │  │  - cancel_order()               │  │                 │
│           │  │  - check_fills_from_trade()     │  │  ◄── NEW        │
│           │  └─────────────────────────────────┘  │                 │
│           └───────────────────────────────────────┘                 │
│                           │                                          │
│                           ▼ LimitOrderEvent                          │
│           ┌───────────────────────────────────────┐                 │
│           │     InMemoryPositionManager           │                 │
│           │  - handle_limit_order_event()         │  (existing)     │
│           │  - update positions, balances         │                 │
│           └───────────────────────────────────────┘                 │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

## Implementation Plan

### Phase 1: Core Executor
1. Create `src/execution/paper/mod.rs` with `PaperTradingOrderExecutor`
2. Implement `OrderExecutor` trait for limit orders
3. Port fill simulation logic from `BacktestOrderExecutor`
4. Add `check_fills_from_trade()` method

### Phase 2: Event Integration
1. Add `ExecutionMode::PaperTrading` to config
2. Modify `TradeServer` to instantiate paper executor when configured
3. Add trade event routing to paper executor in event loop
4. Wire fill events back to position manager

### Phase 3: Testing & Validation
1. Unit tests for paper executor fill logic
2. Integration test with recorded live data
3. Compare paper trading results against backtest on same data
4. Add metrics for paper trading performance

## Files to Modify/Create

| File                               | Action | Description                          |
|------------------------------------|--------|--------------------------------------|
| `src/execution/paper/mod.rs`       | Create | Paper trading executor               |
| `src/execution/paper/executor.rs`  | Create | Core executor implementation         |
| `src/execution/mod.rs`             | Modify | Export paper module                  |
| `src/config.rs`                    | Modify | Add `ExecutionMode::PaperTrading`    |
| `src/trade_server/trade_server.rs` | Modify | Route trade events to paper executor |
| `src/trade_server/mod.rs`          | Modify | Instantiate paper executor           |

## Success Criteria

1. Bot runs continuously against live Polymarket data
2. Signals generated from live orderbook snapshots
3. Orders "placed" in paper executor (not on venue)
4. Fills simulated when live trades cross order prices
5. Position manager tracks positions and PnL correctly
6. Results comparable to backtest on same time period

## Related Files

- `src/execution/backtest/executor.rs` - Template for fill simulation
- `src/backtest/processor.rs` - Event routing pattern
- `src/trade_server/position_handler/intent_handlers.rs` - Intent handling
- `src/position/reconciliation/engine.rs` - Order reconciliation
- `docs/design/orderbook-trading.md` - Orderbook trading design
- `docs/design/backtest.md` - Backtest infrastructure design
