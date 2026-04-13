# Signal Processing

This document describes how intent-based signals flow through the system for orderbook trading, covering both realtime and backtest execution modes.

## Overview

The trade server uses an event-driven architecture where:
1. **Events** (orderbook snapshots, trades, timers) flow through the system
2. **SignalGenerators** process events and emit **TradableSignals**
3. **PositionHandler** routes signals to appropriate handlers
4. **IntentHandler** reconciles intent signals against current order state
5. **OrderExecutor** executes the resulting orders

## Intent-Based Signal Flow

Intent signals express **desired orderbook state** rather than discrete actions. The system automatically reconciles current pending orders with the desired state, generating cancel and placement orders as needed.

### Realtime Mode

```mermaid
sequenceDiagram
    participant EC as EventCoordinator
    participant TS as TradeServer
    participant SG as SignalGenerator
    participant PH as PositionHandler
    participant IH as IntentHandler
    participant PM as PositionManager
    participant RE as ReconciliationEngine
    participant OE as OrderExecutor
    participant Venue as Exchange/Venue

    Note over EC,Venue: Event Ingestion & Signal Generation
    EC->>TS: next_event() -> OrderbookSnapshot
    TS->>SG: generate_signal(event)
    SG-->>TS: Vec<TradableSignal>

    Note over TS,Venue: Signal Processing (per signal)
    TS->>PH: handle_signal(signal)
    PH->>PH: signal.is_intent_signal()?

    alt Intent Signal
        PH->>IH: handle_intent_signal(signal)
        IH->>IH: signal.get_order_intent()

        Note over IH,RE: Reconciliation
        IH->>PM: reconcile_intent(intent)
        PM->>PM: get pending_limit_orders for mint
        PM->>RE: compute_reconciliation(intent, current_orders)
        RE->>RE: validate_intent()
        RE->>RE: reconcile_side(bids)
        RE->>RE: reconcile_side(asks)
        RE-->>PM: Vec<Order> [cancels, then placements]
        PM-->>IH: Vec<Order>

        Note over IH,Venue: Two-Phase Order Execution
        Note over IH: Phase 1: Cancel orders in parallel
        par Execute all cancels concurrently
            IH->>OE: cancel_order(order_id)
            OE->>Venue: Cancel request
            Venue-->>OE: LimitOrderEvent::OrderCancelled
            OE->>EC: enqueue_event(LimitOrder)
        end
        Note over IH: Await all cancels before placements

        Note over IH: Phase 2: Place orders in parallel
        par Execute all placements concurrently
            IH->>OE: execute_limit_order(order)
            OE->>Venue: Place limit order
            Venue-->>OE: LimitOrderEvent::OrderPlaced
            OE->>EC: enqueue_event(LimitOrder)
        end
        Note over IH: Await all placements to complete<br/>before processing next intent
    end

    Note over EC,Venue: Fill Processing (async, from venue)
    Venue-->>EC: LimitOrderEvent::OrderPartiallyFilled
    EC->>TS: next_event() -> LimitOrder(fill)
    TS->>PH: handle_limit_order_event(fill)
    PH->>PM: handle_limit_order_event(fill)
    PM->>PM: Update pending_limit_orders
    PM->>PM: handle_execution() -> PositionEvent
    PM->>EC: enqueue_event(Position)
```

### Backtest Mode

```mermaid
sequenceDiagram
    participant TL as BacktestTimeline
    participant BC as BacktestCoordinator
    participant BP as BacktestEventProcessor
    participant SG as SignalGenerator
    participant PM as PositionManager
    participant RE as ReconciliationEngine
    participant BE as BacktestOrderExecutor

    Note over TL,BE: Load Historical Data
    TL->>TL: Merge snapshots + trades chronologically

    Note over BC,BE: Event Loop
    BC->>BP: process_event(OrderbookSnapshot)
    BP->>BE: handle_orderbook_snapshot()
    BP->>SG: generate_signal(event)
    SG-->>BP: Vec<TradableSignal>

    Note over BP,BE: Intent Signal Processing
    BP->>BP: route_signal(signal)
    BP->>BP: signal.is_intent_signal()?

    alt Intent Signal
        BP->>BP: handle_intent_signal(signal)
        BP->>PM: reconcile_intent(intent)
        PM->>RE: compute_reconciliation(intent, current_orders)
        RE-->>PM: Vec<Order>
        PM-->>BP: Vec<Order>

        Note over BP,BE: Synchronous Order Execution
        loop For each order
            alt Cancel Order
                BP->>BE: cancel_order(order_id)
                BE->>BE: Remove from pending_orders
                BE-->>BP: LimitOrderEvent::OrderCancelled
                BP->>BC: enqueue_limit_order_events([event])
            else Limit Order
                BP->>BE: execute_limit_order(order)
                BE->>BE: Add to pending_orders
                BE-->>BP: LimitOrderEvent::OrderPlaced
                BP->>BC: enqueue_limit_order_events([event])
            end
        end
    end

    Note over BC,BE: Trade-Based Fill Simulation
    BC->>BP: process_event(PolymarketTrade)
    BP->>BE: handle_polymarket_trade(trade, inventory)

    BE->>BE: Check pending orders for fills
    Note right of BE: BID fills when SELL crosses at/below bid<br/>ASK fills when BUY crosses at/above ask

    BE-->>BP: Vec<LimitOrderEvent> (fills)

    loop For each fill
        BP->>PM: handle_limit_order_event(fill)
        PM->>PM: Update pending_limit_orders
        BP->>PM: handle_execution(exec_event)
        PM-->>BP: PositionEvent
        BP->>BC: enqueue_event(Position)
    end
```

## Key Components

### Signal Types

| Type | Method | Description |
|------|--------|-------------|
| Intent Signal | `is_intent_signal() = true` | Expresses desired orderbook state via `OrderIntent` |
| Entry Signal | `signal_action() = Entry` | Open a new position |
| Exit Signal | `signal_action() = Exit` | Close an existing position |
| ModifyOrder | `signal_action() = ModifyOrder` | Cancel and replace existing order |
| CancelOrder | `signal_action() = CancelOrder` | Cancel an active order |

### OrderIntent Structure

```rust
OrderIntent {
    signal_id: String,
    mint: String,                      // Asset identifier
    market: Option<String>,            // Market ID for CLOB venues
    bids: Option<Vec<QuoteLevel>>,     // Desired bid levels
    asks: Option<Vec<QuoteLevel>>,     // Desired ask levels
    timestamp: DateTime<Utc>,
}

QuoteLevel {
    price: f64,
    size: f64,
    time_in_force: TimeInForce,
}
```

**Option Semantics:**
- `None` - Preserve existing orders on this side
- `Some([])` - Cancel all orders on this side
- `Some([levels...])` - Reconcile to match these exact levels

### Reconciliation Logic

The `ReconciliationEngine` computes the diff between current and desired state:

```
Current Orders          Desired Levels         Result
─────────────────────────────────────────────────────────
Bid @ 0.45, size 100    Bid @ 0.45, size 100   No change (match)
Bid @ 0.44, size 50     (none)                 Cancel bid @ 0.44
(none)                  Bid @ 0.46, size 75    Place bid @ 0.46
```

Orders are sorted with **cancels before placements** to avoid exceeding position limits.

## Execution Path Comparison

| Aspect | Realtime | Backtest |
|--------|----------|----------|
| Event Source | Redis streams | Parquet files via Timeline |
| Order Execution | Two-phase: cancels then placements (parallel within each phase) | Synchronous in-place |
| Fill Detection | Venue callbacks/WebSocket | Trade-based simulation |
| Limit Orders | Venue API calls | Internal `pending_orders` map |
| Event Coordination | `EventCoordinator` (Redis) | `BacktestEventCoordinator` |

### Backtest Fill Simulation

The `BacktestOrderExecutor` uses trade-based fill logic:
- **BID orders** fill when historical SELL trades cross at or below bid price
- **ASK orders** fill when historical BUY trades cross at or above ask price
- Fills occur at the **order price**, not the trade price
- Partial fills are supported based on trade size
- Optional inventory constraints prevent "naked shorting"

## Position Updates from Fills

When a limit order fills:

1. `LimitOrderEvent::OrderPartiallyFilled` is generated
2. `PositionManager.handle_limit_order_event()` updates `pending_limit_orders`
3. An `ExecutionEvent::OrderFilled` is created with:
   - `token_amount_change`: positive for buys, negative for sells
   - `quote_amount_change`: cash flow from the trade
   - `exit_mode`: propagated from the order (e.g., `StrategyManaged`)
4. `PositionManager.handle_execution()` updates position state
5. `PositionEvent` is emitted for the signal generator to update inventory

## ExitMode Propagation

Orders from intent-based signals use `ExitMode::StrategyManaged`:

```
OrderIntent
    └─> ReconciliationEngine.create_limit_order()
            └─> Order.with_exit_mode(StrategyManaged)
                    └─> LimitOrderEvent.exit_mode
                            └─> ExecutionEvent.exit_mode
                                    └─> Position.exit_mode
```

This ensures positions created from intent-based fills are managed by the strategy, not auto-exited by `ExitStrategy`.

## File Reference

| File | Purpose |
|------|---------|
| `src/signal/sig.rs` | `TradableSignal` trait, `SignalAction` enum |
| `src/signal/intent.rs` | `OrderIntent`, `QuoteLevel` structs |
| `src/trade_server/trade_server.rs` | Main event loop |
| `src/trade_server/position_handler/mod.rs` | Signal routing |
| `src/trade_server/position_handler/intent_handlers.rs` | `IntentHandler` |
| `src/position/reconciliation/engine.rs` | `ReconciliationEngine` |
| `src/position/manager.rs` | `PositionManager` trait |
| `src/execution/executor.rs` | `OrderExecutor` trait |
| `src/execution/backtest/executor.rs` | `BacktestOrderExecutor` |
| `src/backtest/processor.rs` | `BacktestEventProcessor` |

## See Also

- [Intent-Based Position Management](usage-guide/intent-signals.md) - Usage guide for implementing intent signals
- [Orderbook Trading](design/orderbook-trading.md) - Design doc for CLOB support
- [Backtest Infrastructure](design/backtest.md) - Backtest system design
