# Intent-Based Position Management for Orderbook Venues

## Summary

This design introduces an intent-based position management model for orderbook trading venues (e.g., Polymarket, prediction markets). Instead of strategies emitting discrete action signals (place order, cancel order), they express desired state ("I want quotes at these prices"). The PositionManager reconciles current order state with desired state and generates the necessary actions. This approach simplifies strategy logic, provides natural idempotency, and unifies the mental model across AMM and orderbook venues while keeping the existing AMM code path unchanged.

## Goals

- Enable market making strategies to express desired order book state rather than discrete actions
- Provide automatic reconciliation between current and desired order state
- Track pending limit orders to enable accurate state diffing
- Support order cancellation as a first-class operation
- Maintain full backward compatibility with existing AMM signal flow
- Reduce strategy complexity by moving order lifecycle management to PositionManager

## Non-Goals

- Modifying the existing AMM signal processing path (it works, it's tested)
- Supporting order amendments/modifications in-place (cancel-and-replace is sufficient for v1)
- Portfolio-level position aggregation across multiple mints (future work)
- Sub-millisecond latency optimization (reconciliation adds negligible compute overhead)
- Disabling safety limits for hedged strategies (separate concern, tracked separately)

## API

### TradableSignal Trait Extensions

New methods added to the `TradableSignal` trait with backward-compatible defaults:

```rust
pub trait TradableSignal: Send + Sync + Debug {
    // ... existing methods unchanged ...

    /// Returns true if this is an intent-based signal requiring reconciliation.
    /// Default: false (existing action-based signals)
    fn is_intent_signal(&self) -> bool { false }

    /// Returns the desired order state for intent-based signals.
    /// Only called when is_intent_signal() returns true.
    fn get_order_intent(&self) -> Option<OrderIntent> { None }
}
```

### PositionManager Trait Extensions

New method added with default no-op implementation:

```rust
#[async_trait]
pub trait PositionManager: Send + Sync {
    // ... existing methods unchanged ...

    /// Reconcile current order state with desired intent.
    /// Returns the orders needed to reach the desired state.
    async fn reconcile_intent(
        &self,
        intent: &OrderIntent
    ) -> Result<Vec<Order>, PositionError> {
        Ok(vec![])  // Default: no-op for AMM-only managers
    }

    /// Handle limit order lifecycle events from the venue.
    /// Updates internal pending order tracking.
    async fn handle_limit_order_event(
        &self,
        event: &LimitOrderEvent
    ) -> Result<(), PositionError> {
        Ok(())  // Default: no-op
    }
}
```

### Errors

| Error | Description |
|-------|-------------|
| `PositionError::ReconciliationFailed(String)` | Failed to compute required actions |
| `PositionError::OrderNotFound(String)` | Referenced order_id not in pending orders |
| `PositionError::InvalidIntent(String)` | Intent validation failed (e.g., negative size) |

## Behavior

### Signal Dispatch (TradeServer)

Given a signal from a SignalGenerator:

1. Check `signal.is_intent_signal()`
2. **If false (action-based, AMM path):**
   - Call `position_manager.handle_signal(signal)` (existing logic, unchanged)
   - If order returned, execute via `executor.execute_order(order)`
3. **If true (intent-based, orderbook path):**
   - Extract intent via `signal.get_order_intent()`
   - Call `position_manager.reconcile_intent(&intent)`
   - For each order in result, execute via `executor.execute_order(order)`

### Reconciliation Algorithm

Given an `OrderIntent` with desired bids and asks:

1. Retrieve current pending orders for the mint from internal state
2. Partition current orders by side (buy/sell)
3. For each side where intent specifies desired levels (`Some(levels)`):
   a. **Find orders to cancel**: Current orders not matching any desired level
   b. **Find orders to place**: Desired levels not matching any current order
   c. Generate `Order` with `OrderType::Cancel` for each cancellation
   d. Generate `Order` with `OrderType::LimitBuy/LimitSell` for each placement
4. Return collected orders in cancel-first order (cancels before new placements)

**Level Matching Logic:**
Two levels match if both price and size are equal within tolerance (1e-9).
Future enhancement: match by price only and generate modify orders for size changes.

### Pending Order State Updates

When `LimitOrderEvent` is received:

| Event | Action |
|-------|--------|
| `OrderPlaced` | Add to pending orders map |
| `OrderPartiallyFilled` | Update `remaining_size` |
| `OrderFilled` | Remove from pending orders |
| `OrderCancelled` | Remove from pending orders |
| `OrderExpired` | Remove from pending orders |
| `OrderRejected` | Do not add (was never pending) |

### Intent Semantics

The `bids` and `asks` fields in `OrderIntent` use Option semantics:

| Value | Meaning |
|-------|---------|
| `None` | No change to this side (preserve existing orders) |
| `Some([])` | Cancel all orders on this side |
| `Some([levels...])` | Desired state for this side (reconcile to match) |

## Data Model

### OrderIntent

```rust
/// Desired order book state from strategy
#[derive(Debug, Clone)]
pub struct OrderIntent {
    /// Token mint address
    pub mint: String,
    /// Market/condition ID for orderbook venues
    pub market: Option<String>,
    /// Desired bid orders. None = unchanged, Some([]) = cancel all
    pub bids: Option<Vec<QuoteLevel>>,
    /// Desired ask orders. None = unchanged, Some([]) = cancel all
    pub asks: Option<Vec<QuoteLevel>>,
    /// Signal ID for tracing and debugging
    pub signal_id: String,
    /// Timestamp of intent generation
    pub timestamp: DateTime<Utc>,
}
```

**Location:** `src/signal/intent.rs` (new file) or `src/domain/intent.rs`

### QuoteLevel

```rust
/// A single price level in desired order state
#[derive(Debug, Clone)]
pub struct QuoteLevel {
    /// Limit price
    pub price: f64,
    /// Order size in base units
    pub size: f64,
    /// Time-in-force for the order
    pub time_in_force: TimeInForce,
}
```

### PendingLimitOrder

```rust
/// Internal tracking of a pending limit order
#[derive(Debug, Clone)]
pub struct PendingLimitOrder {
    /// Venue-assigned order ID
    pub order_id: String,
    /// Order side
    pub side: OrderSide,
    /// Limit price
    pub price: f64,
    /// Original order size
    pub original_size: f64,
    /// Remaining size after partial fills
    pub remaining_size: f64,
    /// When the order was placed
    pub placed_at: DateTime<Utc>,
}
```

**Location:** `src/position/pending_order.rs` (new file)

### OrderType Extension

Add `Cancel` variant to existing enum:

```rust
pub enum OrderType {
    // ... existing variants ...

    /// Cancel an existing order by venue order ID
    Cancel {
        /// The venue-assigned order ID to cancel
        order_id: String
    },
}
```

**Location:** `src/execution/order.rs`

### State Extension

```rust
// In InMemoryPositionManagerState
pub struct InMemoryPositionManagerState {
    // ... existing fields ...

    /// Pending limit orders by mint -> order_id -> order
    pub pending_limit_orders: HashMap<String, HashMap<String, PendingLimitOrder>>,
}
```

## Configuration

No new configuration required. The intent-based path is activated automatically when a strategy returns intent signals (`is_intent_signal() = true`).

Future configuration options (out of scope for v1):
- Level matching tolerance
- Maximum orders per side
- Reconciliation debounce interval

## Reconciliation Engine

### Core Algorithm

```rust
impl InMemoryPositionManager {
    async fn reconcile_intent(
        &self,
        intent: &OrderIntent
    ) -> Result<Vec<Order>, PositionError> {
        let current = self.get_pending_orders_for_mint(&intent.mint).await;
        let mut orders = vec![];

        // Process bids
        if let Some(desired_bids) = &intent.bids {
            let current_bids = current.values()
                .filter(|o| o.side == OrderSide::Buy)
                .collect();
            orders.extend(self.reconcile_side(
                intent, current_bids, desired_bids, OrderSide::Buy
            )?);
        }

        // Process asks
        if let Some(desired_asks) = &intent.asks {
            let current_asks = current.values()
                .filter(|o| o.side == OrderSide::Sell)
                .collect();
            orders.extend(self.reconcile_side(
                intent, current_asks, desired_asks, OrderSide::Sell
            )?);
        }

        // Sort: cancels first, then placements
        orders.sort_by_key(|o| match &o.order_type {
            OrderType::Cancel { .. } => 0,
            _ => 1,
        });

        Ok(orders)
    }

    fn reconcile_side(
        &self,
        intent: &OrderIntent,
        current: Vec<&PendingLimitOrder>,
        desired: &[QuoteLevel],
        side: OrderSide,
    ) -> Result<Vec<Order>, PositionError> {
        let mut actions = vec![];

        // Cancels: current orders not in desired
        for order in &current {
            if !self.has_matching_level(order, desired) {
                actions.push(self.create_cancel_order(intent, &order.order_id));
            }
        }

        // Placements: desired levels not in current
        for level in desired {
            if !self.has_matching_order(level, &current) {
                actions.push(self.create_limit_order(intent, level, side)?);
            }
        }

        Ok(actions)
    }

    fn has_matching_level(&self, order: &PendingLimitOrder, desired: &[QuoteLevel]) -> bool {
        desired.iter().any(|d|
            (d.price - order.price).abs() < 1e-9 &&
            (d.size - order.remaining_size).abs() < 1e-9
        )
    }

    fn has_matching_order(&self, level: &QuoteLevel, current: &[&PendingLimitOrder]) -> bool {
        current.iter().any(|c|
            (level.price - c.price).abs() < 1e-9 &&
            (level.size - c.remaining_size).abs() < 1e-9
        )
    }
}
```

### Order Generation

```rust
fn create_cancel_order(&self, intent: &OrderIntent, order_id: &str) -> Order {
    Order {
        mint: intent.mint.clone(),
        market: intent.market.clone(),
        order_type: OrderType::Cancel { order_id: order_id.to_string() },
        price: None,
        status: OrderStatus::Pending,
        timestamp: intent.timestamp,
        signal_slot: None,
        signal_id: Some(intent.signal_id.clone()),
        dex_type: None,
        venue_order_id: Some(order_id.to_string()),
    }
}

fn create_limit_order(
    &self,
    intent: &OrderIntent,
    level: &QuoteLevel,
    side: OrderSide,
) -> Result<Order, PositionError> {
    let order_type = match side {
        OrderSide::Buy => OrderType::LimitBuy {
            quote_amount: level.price * level.size,
            limit_price: level.price,
            time_in_force: level.time_in_force,
        },
        OrderSide::Sell => OrderType::LimitSell {
            token_amount: level.size,
            limit_price: level.price,
            clear_position: false,
            time_in_force: level.time_in_force,
        },
    };

    Ok(Order {
        mint: intent.mint.clone(),
        market: intent.market.clone(),
        order_type,
        price: Some(level.price),
        status: OrderStatus::Pending,
        timestamp: intent.timestamp,
        signal_slot: None,
        signal_id: Some(intent.signal_id.clone()),
        dex_type: None,
        venue_order_id: None,  // Assigned by venue after execution
    })
}
```

## Idempotency & Concurrency

### Natural Idempotency

Intent-based design provides idempotency by construction:
- Same intent + same current state = same actions (or no actions if already reconciled)
- Duplicate intent signals result in no-op if state hasn't changed

### Concurrency Model

- Reconciliation holds the state lock only during the diff computation
- Orders are executed after releasing the lock
- Fill events update state asynchronously
- **Race condition mitigation:** If a fill arrives between reconciliation and order execution, the next reconciliation cycle will correct the state

### State Consistency

The pending order state is eventually consistent with the venue:
1. Order placed -> immediate add to pending (optimistic)
2. If placement fails -> removed on next reconciliation
3. Fill/cancel events -> update state reactively

## OrderExecutor Integration

### Cancel Order Execution

The `OrderExecutor` trait needs to handle the new `Cancel` order type:

```rust
#[async_trait]
pub trait OrderExecutor: Send + Sync + 'static {
    // ... existing methods ...

    async fn execute_order(&self, order: Order) -> Result<ExecutionEvent> {
        match &order.order_type {
            OrderType::Cancel { order_id } => {
                self.cancel_order(order_id).await
            }
            OrderType::LimitBuy { .. } | OrderType::LimitSell { .. } => {
                self.execute_limit_order(order).await
            }
            // ... existing market order handling ...
        }
    }
}
```

### Event Flow

```
1. Strategy generates IntentSignal
   |
2. TradeServer calls reconcile_intent()
   |
3. PositionManager diffs current vs desired
   |
4. Orders returned (cancels + placements)
   |
5. TradeServer executes each order
   |
6. Venue returns LimitOrderEvent
   |
7. PositionManager.handle_limit_order_event() updates state
   |
8. Next intent signal triggers new reconciliation
```

## Acceptance Criteria

### Basic Reconciliation

- [ ] Intent with empty current state generates placement orders for all desired levels
- [ ] Intent matching current state generates no orders
- [ ] Intent with `bids: Some([])` cancels all current bids
- [ ] Intent with `bids: None` preserves all current bids unchanged
- [ ] Price change in desired level generates cancel + placement

### Order Lifecycle

- [ ] `OrderPlaced` event adds order to pending state
- [ ] `OrderPartiallyFilled` event updates remaining_size
- [ ] `OrderFilled` event removes order from pending state
- [ ] `OrderCancelled` event removes order from pending state

### Backward Compatibility

- [ ] Existing AMM signals (`is_intent_signal() = false`) route through unchanged `handle_signal()` path
- [ ] Existing tests pass without modification
- [ ] Default trait implementations maintain no-op behavior

### Integration

- [ ] Market making strategy can express quotes as intent
- [ ] Cancel orders are executed before placement orders
- [ ] Signal ID propagates to generated orders for tracing

## Observability

### Logs

| Event | Level | Fields |
|-------|-------|--------|
| Intent received | DEBUG | `signal_id`, `mint`, `bid_count`, `ask_count` |
| Reconciliation complete | INFO | `signal_id`, `cancels`, `placements` |
| Order placed | INFO | `order_id`, `side`, `price`, `size` |
| Order cancelled | INFO | `order_id`, `reason` |
| State mismatch detected | WARN | `mint`, `expected`, `actual` |

### Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `position_manager_reconciliations_total` | Counter | `mint` | Total reconciliation calls |
| `position_manager_reconciliation_actions` | Histogram | `action_type` | Actions per reconciliation |
| `position_manager_pending_orders` | Gauge | `mint`, `side` | Current pending order count |
| `position_manager_reconciliation_duration_ms` | Histogram | - | Reconciliation compute time |

### Alerts

- `PendingOrdersStale`: Orders pending > 5 minutes without fill/cancel
- `ReconciliationBacklog`: Reconciliation taking > 100ms consistently

## Security

- **Input validation:** Intent prices and sizes must be positive
- **Order ID validation:** Cancel order IDs must exist in pending state
- **Rate limiting:** Venue-specific rate limits enforced at executor level (not PM)
- **No new attack surface:** Intent signals processed identically to action signals

## Rollout Plan

### Phase 1: Infrastructure (No Behavior Change) ✅

1. Add `OrderIntent`, `QuoteLevel`, `PendingLimitOrder` types
2. Add `Cancel` variant to `OrderType`
3. Add trait methods with default no-op implementations
4. Add pending order state to `InMemoryPositionManager`
5. **Verification:** All existing tests pass, no behavior change

**Completed in commit `5f96409`:**
- Added `OrderIntent` and `QuoteLevel` types in `src/signal/intent.rs` with builder pattern and Option semantics for bids/asks
- Added `PendingLimitOrder` in `src/position/pending_order.rs` with level matching logic and partial fill tracking
- Extended `OrderType` with `Cancel { order_id }` variant and helper methods (`is_cancel()`, `is_limit()`, `cancel_order_id()`)
- Added `reconcile_intent()` and `handle_limit_order_event()` to `PositionManager` trait with default no-op implementations
- Extended `TradableSignal` trait with `is_intent_signal()` and `get_order_intent()` methods
- Added `pending_limit_orders: HashMap<String, HashMap<String, PendingLimitOrder>>` to manager state
- Added `InvalidIntent` error variant to `PositionError`
- Updated all executors (backtest, solana) to handle cancel orders

### Phase 2: Reconciliation Logic ✅

1. Implement `reconcile_intent()` in `InMemoryPositionManager`
2. Implement `handle_limit_order_event()` for state updates
3. Add unit tests for reconciliation algorithm
4. **Verification:** Unit tests cover all reconciliation scenarios

**Completed in commit `2def05e`:**
- Implemented `reconcile_intent()` with full reconciliation algorithm:
  - Validates intent (positive prices/sizes)
  - Partitions current orders by side
  - Computes diff: orders to cancel (current not in desired) and orders to place (desired not in current)
  - Returns orders sorted with cancels before placements
- Implemented `handle_limit_order_event()` for all event types:
  - `OrderPlaced`: adds to pending orders
  - `OrderPartiallyFilled`: updates remaining_size
  - `OrderCancelled`/`OrderExpired`: removes from pending
  - `OrderRejected`: no-op (never added)
- Added helper methods: `validate_intent()`, `reconcile_side()`, `has_matching_level()`, `has_matching_order()`, `create_cancel_order()`, `create_limit_order()`, `remove_pending_order_by_id()`, `get_pending_orders_for_mint()`
- Comprehensive test suite (`reconciliation_tests` module) covering:
  - Basic reconciliation scenarios (empty state, matching state, price changes)
  - Order lifecycle (placed, partial fill, cancelled, expired, rejected)
  - Validation (negative price, zero size)
  - Integration (signal_id propagation, cancel ordering, multi-level reconciliation, idempotency)

### Phase 3: Position Manager Refactor ✅

The `InMemoryPositionManager` has grown to ~1700 lines with the addition of intent-based reconciliation. This phase decomposes it into focused, testable modules.

See [Position Manager Refactor Plan](./position-manager-refactor.md) for detailed design.

**Summary:**
1. Extract tests to `src/position/tests/` directory
2. Extract reconciliation engine to `src/position/reconciliation/` module
3. Extract state, balance tracking, signal/execution handlers to separate files
4. Reduce `in_mem_manager.rs` to thin coordinator (~200 lines)

**Verification:** All existing tests pass, no public API changes

**Completed in commit `0475cea`:**
- Extracted `BalanceTracker` to `src/position/balance_tracker.rs` (257 lines): SOL balance and PnL tracking with atomic operations
- Extracted `ExecutionHandler` to `src/position/execution_handler.rs` (231 lines): Position creation and update logic from execution events
- Extracted `ExitManager` to `src/position/exit_manager.rs` (148 lines): Safety limit enforcement for position exits
- Extracted `InMemoryPositionManagerState` to `src/position/state.rs` (230 lines): Centralized state container with accessor methods
- Extracted reconciliation engine to `src/position/reconciliation/` module:
  - `engine.rs` (336 lines): Core reconciliation algorithm
  - `validation.rs` (153 lines): Intent validation logic
  - `mod.rs` (37 lines): Module exports
- Extracted tests to `src/position/tests/` directory:
  - `reconciliation_tests.rs` (467 lines): Comprehensive reconciliation test suite
- Reduced `in_mem_manager.rs` from ~1700 to ~840 lines as a thin coordinator delegating to subsystems
- All existing tests pass with the new modular structure

### Phase 4: Signal Dispatch ✅

1. Update TradeServer to detect intent signals
2. Route intent signals to `reconcile_intent()`
3. Execute returned orders through existing executor
4. **Verification:** Integration test with mock executor

**Completed:**
- Added `handle_intent_signal()` method to `PositionHandler` (`src/trade_server/position_handler.rs:1073-1135`)
  - Extracts `OrderIntent` from signal via `get_order_intent()`
  - Calls `position_manager.reconcile_intent(&intent)` to compute orders
  - Logs reconciliation metrics (cancels, placements)
- Added `execute_reconciled_order()` helper method (`src/trade_server/position_handler.rs:1137-1255`)
  - Handles cancel orders via `executor.cancel_order()`
  - Handles limit orders via `executor.execute_limit_order()`
  - Falls back to market orders for edge cases
  - Enqueues resulting events to EventCoordinator
- Updated `handle_signal()` to check `is_intent_signal()` first (`src/trade_server/position_handler.rs:565-568`)
  - Intent signals routed to `handle_intent_signal()`
  - Action signals (Entry/Exit/ModifyOrder/CancelOrder) use existing handlers
- Updated `TradeServer::process_event()` to route `LimitOrderEvent` to position manager (`src/trade_server/trade_server.rs:311-321`)
  - Calls `handle_limit_order_event()` to update pending order tracking
  - Keeps position manager's view in sync with venue state
- Added unit tests for signal dispatch (`src/trade_server/position_handler.rs:1263-1465`)
  - `TestIntentSignal`: test signal with `is_intent_signal() = true`
  - `TestActionSignal`: test signal with `is_intent_signal() = false`
  - Tests verify correct routing behavior

### Phase 5: Documentation & Integration Tests

1. Update `docs/usage-guide.md` with high-level intent-based trading guidance
2. Add integration tests to verify the intent-based data flow (no external dependencies)
3. **Verification:** Tests pass and serve as documentation for strategy developers

**Completed:**
- Updated `docs/usage-guide.md` with comprehensive intent-based trading section:
  - Why Intent-Based? (motivation and benefits)
  - OrderIntent Structure and Option Semantics
  - Implementing an Intent Signal (complete TradableSignal example)
  - How Reconciliation Works (step-by-step flow)
  - Signal Generator Example (market making strategy)
  - Pending Order Tracking (LimitOrderEvent handling)
  - Combining with Exit Modes (Strategy-Managed mode)
- Added integration test suite in `src/trade_server/tests/intent_signal_integration.rs`:
  - `test_intent_signal_generates_limit_orders`: Intent signals route to handle_intent_signal and generate orders
  - `test_action_signal_does_not_use_intent_handler`: Action signals use existing path
  - `test_intent_reconciliation_idempotent`: Same intent + same state = no orders
  - `test_intent_price_change_triggers_cancel_and_place`: Price change triggers cancel + placement
  - `test_intent_cancel_all`: Empty intent (Some([])) cancels all orders
  - `test_intent_none_preserves_orders`: None preserves existing orders
  - `test_intent_partial_fill_affects_reconciliation`: Partial fills affect matching
  - `test_signal_id_propagation`: Signal ID flows through the system
  - `test_multiple_mints_independent`: Multiple mints handled independently
  - `test_invalid_intent_rejected`: Invalid intents (negative price) are rejected
- All 157 library tests pass (10 new integration tests + 147 existing)

## References

- [Architecture Documentation](../architecture.md) - System overview and existing design
- [Orderbook Trading Support](../architecture.md#orderbook-trading-support) - Current orderbook types
- [TradableSignal Trait](../../src/signal/sig.rs) - Signal abstraction
- [PositionManager Trait](../../src/position/manager.rs) - Position management interface
- [InMemoryPositionManager](../../src/position/in_mem_manager.rs) - Concrete implementation
