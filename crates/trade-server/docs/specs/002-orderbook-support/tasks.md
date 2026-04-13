# Task Tracker: Orderbook Trading Support Implementation

## 1. Problem Statement

The trade_server crate currently only supports AMM-based trading (PumpFun, Raydium, Bonk). We need to extend it to support orderbook-based trading venues (e.g., Polymarket prediction markets) while maintaining full backward compatibility with existing AMM-based bots.

Key challenges:
- Process new `TokenEvent::OrderbookUpdate` and `TokenEvent::OrderbookSnapshot` events
- Enable signal generators to produce trading signals from orderbook data
- Support position valuation using bid/ask prices
- Provide optional limit order support for orderbook-based venues
- Implement strategy-driven exit management for CLOB markets

## 2. Plan

The implementation follows the design document at `docs/specs/002-orderbook-support/design.md`. The approach is additive and backward-compatible:

1. **Extend core data types** - Add new order types (limit orders), extend Order struct, add limit order events
2. **Extend traits with defaults** - Add optional methods to `TradableSignal`, `OrderExecutor`, and `PositionManager` with default implementations
3. **Update TradeServer event routing** - Handle new `OrderbookSnapshot` and `OrderbookUpdate` events
4. **Implement signal action system** - Add `SignalAction` enum and routing for entry/exit/modify/cancel signals
5. **Add position state extensions** - Track active exit orders and exit mode per position
6. **Add observability** - New metrics and logging for orderbook events and limit orders

## 3. Implementation Phases

### Phase 1: Core Data Types
- **Objective**: Extend order types and add limit order event structures
- **Tasks**: Tasks 1-3
- **Deliverable**: New `OrderType` variants, extended `Order` struct, `LimitOrderEvent` enum

### Phase 2: Trait Extensions
- **Objective**: Add orderbook-aware methods to core traits with backward-compatible defaults
- **Tasks**: Tasks 4-6
- **Deliverable**: Extended `TradableSignal`, `OrderExecutor`, and `PositionManager` traits

### Phase 3: Event Routing
- **Objective**: Update TradeServer to handle orderbook events
- **Tasks**: Tasks 7-8
- **Deliverable**: TradeServer processes `OrderbookSnapshot` and `OrderbookUpdate` events

### Phase 4: Signal Action System
- **Objective**: Implement strategy-driven exit management
- **Tasks**: Tasks 9-12
- **Deliverable**: `SignalAction` routing, exit signal types, position handler updates

### Phase 5: Position State Extensions
- **Objective**: Track active exit orders and exit mode
- **Tasks**: Tasks 13-14
- **Deliverable**: `ActiveExitOrder`, `ExitMode` tracking in positions

### Phase 6: Observability & Documentation
- **Objective**: Add metrics, logging, and update documentation
- **Tasks**: Tasks 15-17
- **Deliverable**: Prometheus metrics, structured logging, updated docs

## 4. TODO List

### Phase 1: Core Data Types

1. Extend `OrderType` enum with limit order variants
   - Status: Completed
   - Note: File: `src/execution/order.rs`. Added `MarketBuy`, `MarketSell`, `LimitBuy`, `LimitSell` variants. Kept existing `Buy`/`Sell` as deprecated aliases for backward compatibility. Added `TimeInForce` enum (`GoodTilCancelled`, `ImmediateOrCancel`, `FillOrKill`). Added helper methods `is_buy()`, `is_sell()`, `is_limit()`, `is_market()`, `limit_price()`, `time_in_force()`, `clear_position()`.
   - Success Criteria: Existing code compiles with deprecation warnings but no errors

2. Extend `Order` struct with orderbook market fields
   - Status: Completed
   - Note: File: `src/execution/order.rs`. Added `market: Option<String>` for market/condition ID and `venue_order_id: Option<String>` for external order tracking. Both fields are `Option` to maintain compatibility. Added convenience constructors `new_buy()`, `new_sell()`, `new_limit_buy()`, `new_limit_sell()`. Updated all existing Order struct constructions to include new fields.
   - Success Criteria: Existing order creation code works with new fields defaulted to None

3. Add `LimitOrderEvent` enum for limit order lifecycle
   - Status: Completed
   - Note: File: `src/execution/order.rs`. Added `LimitOrderEvent` enum with variants: `OrderPlaced`, `OrderPartiallyFilled`, `OrderCancelled`, `OrderExpired`, `OrderRejected`. Added `OrderSide` enum (`Buy`, `Sell`). Added helper methods `order_id()` and `is_terminal()`. All types derive Serialize/Deserialize.
   - Success Criteria: All event variants defined with proper serialization and exported from mod.rs

### Phase 2: Trait Extensions

4. Extend `TradableSignal` trait with orderbook methods
   - Status: Completed
   - Note: File: `src/signal/sig.rs`. Added optional methods with defaults: `get_market() -> Option<&str>` (default: None), `get_bid_price() -> Option<f64>` (default: get_price()), `get_ask_price() -> Option<f64>` (default: get_price()), `get_limit_price() -> Option<f64>` (default: None), `is_limit_order() -> bool` (default: false), `get_time_in_force() -> Option<TimeInForce>` (default: None). Added import for `TimeInForce` from `crate::execution::order`.
   - Success Criteria: Existing signal implementations compile without changes

5. Extend `OrderExecutor` trait with limit order methods
   - Status: Completed
   - Note: File: `src/execution/executor.rs`. Added methods with defaults: `execute_limit_order(&self, order: Order) -> Result<LimitOrderEvent>` (default: error "Limit orders not supported"), `cancel_order(&self, order_id: &str) -> Result<LimitOrderEvent>` (default: error "Order cancellation not supported"), `handle_orderbook_snapshot(&self, snapshot: &OrderbookSnapshotEvent)` (default: Ok(())), `handle_orderbook_update(&self, update: &OrderbookUpdateEvent)` (default: Ok(())), `supports_limit_orders() -> bool` (default: false). Added imports for `OrderbookSnapshotEvent`, `OrderbookUpdateEvent` from `popeyes_trading_types` and `LimitOrderEvent` from order module.
   - Success Criteria: Existing executor implementations compile without changes

6. Extend `PositionManager` trait with spread-aware methods
   - Status: Completed
   - Note: File: `src/position/manager.rs`. Added methods with defaults: `update_price_with_spread(&self, mint, bid, ask, timestamp) -> Result<PositionEvent, PositionError>` (default: uses mid price via update_price), `add_pending_limit_order(&self, order: &Order) -> Result<(), PositionError>` (default: Ok(())), `get_pending_limit_orders(&self, mint: &str) -> Vec<Order>` (default: empty vec), `remove_pending_limit_order(&self, order_id: &str) -> Result<Option<Order>, PositionError>` (default: Ok(None)).
   - Success Criteria: `InMemoryPositionManager` compiles without implementing new methods

### Phase 3: Event Routing

7. Add `SystemEvent::LimitOrder` variant
   - Status: Completed
   - Note: File: `src/domain/system.rs`. Added `LimitOrder(LimitOrderEvent)` variant to `SystemEvent` enum. Updated `event_type()` match to return "LimitOrder" for the new variant. Updated TradeServer to handle `SystemEvent::LimitOrder` with metrics tracking for all limit order lifecycle events (placed, partial_fill, cancelled, expired, rejected).
   - Success Criteria: All pattern matches on `SystemEvent` handle new variant

8. Update TradeServer to handle orderbook events
   - Status: Completed
   - Note: File: `src/trade_server/trade_server.rs`. Added metrics tracking for `TokenEvent::OrderbookUpdate` and `TokenEvent::OrderbookSnapshot` events. Added event processing for orderbook events: calls `order_executor.handle_orderbook_snapshot()` / `handle_orderbook_update()`, calculates mid price using new `calculate_mid_price()` helper function, updates positions via `position_handler.update_price()`. Also fixed timestamp handling for trade events to use `trade.timestamp()` which handles all trade event variants including Polymarket.
   - Success Criteria: Orderbook events are processed with metrics incremented and positions updated

### Phase 4: Signal Action System

9. Add `SignalAction` enum
   - Status: Completed
   - Note: File: `src/signal/sig.rs`. Added `SignalAction` enum with variants: `Entry`, `Exit`, `ModifyOrder`, `CancelOrder`. Added to `TradableSignal` trait: `signal_action() -> SignalAction` (default: `Entry`), `references_position() -> Option<&str>` (default: `get_mint()`), `references_order_id() -> Option<&str>` (default: `None`). The enum derives `Default` with `Entry` as the default for backward compatibility.
   - Success Criteria: Existing signals default to `Entry` action

10. Add exit signal types
    - Status: Completed
    - Note: File: new `src/signal/exit_signals.rs`. Implemented `ExitSignal` struct with `ExitType` enum (`Market`, `Limit { price, time_in_force }`). Implemented `TradableSignal` for `ExitSignal` with `signal_action() -> Exit`. Added `ModifyOrderSignal` and `CancelOrderSignal` structs with appropriate `TradableSignal` implementations. All signal types implement full `TradableSignal` trait with proper action routing. Added comprehensive unit tests.
    - Success Criteria: Exit signals can be created and implement `TradableSignal`

11. Update PositionHandler signal routing
    - Status: Completed
    - Note: File: `src/trade_server/position_handler.rs`. Updated `handle_signal()` to route based on `signal.signal_action()`: `Entry` -> `handle_entry_signal()` (refactored original logic), `Exit` -> `handle_exit_signal()`, `ModifyOrder` -> `handle_modify_order_signal()`, `CancelOrder` -> `handle_cancel_order_signal()`. Added import for `SignalAction` and `LimitOrderEvent`.
    - Success Criteria: Signals are routed to appropriate handlers based on action

12. Implement exit signal handlers in PositionHandler
    - Status: Completed
    - Note: File: `src/trade_server/position_handler.rs`. Implemented `handle_exit_signal()`: creates sell order (market or limit based on signal), executes via `OrderExecutor`, supports both limit orders (when executor supports them) and market orders (fallback). Implemented `handle_modify_order_signal()`: cancels existing order, places new order at updated price. Implemented `handle_cancel_order_signal()`: cancels order, clears pending sell flag. All handlers include proper error handling, logging, and async execution.
    - Success Criteria: Exit, modify, and cancel signals are processed correctly

### Phase 5: Position State Extensions

13. Add `ActiveExitOrder` and `ExitMode` to Position
    - Status: Completed
    - Note: File: `src/position/position.rs`. Added `ActiveExitOrder` struct with fields: `order_id`, `price`, `size`, `placed_at`, `side: OrderSide`. Added `ExitMode` enum: `Automatic` (default), `StrategyManaged`. Added fields to `Position`: `active_exit_order: Option<ActiveExitOrder>`, `exit_mode: ExitMode`. Updated all Position creation sites to include new fields with defaults. Updated exports in `src/position/mod.rs` to include `ActiveExitOrder`, `ExitMode`, and `OrderSide`.
    - Success Criteria: Position struct extended, existing code compiles with defaults ✅

14. Add PositionManager methods for exit order tracking
    - Status: Completed
    - Note: File: `src/position/manager.rs`. Added trait methods with default implementations: `set_active_exit_order(&self, mint, order)`, `clear_active_exit_order(&self, mint)`, `get_active_exit_order(&self, mint)`, `set_exit_mode(&self, mint, mode)`. Implemented in `InMemoryPositionManager` (file: `src/position/in_mem_manager.rs`) with proper state tracking and logging. Updated `handle_timer()` to check `exit_mode`: `Automatic` uses `ExitStrategy`, `StrategyManaged` only checks safety limits via new `exceeds_safety_limits()` helper method. Safety limits include -50% absolute stop-loss and 2x max holding period.
    - Success Criteria: Exit orders can be tracked per position, timer respects exit mode ✅

### Phase 6: Observability & Documentation

15. Add Prometheus metrics for orderbook events
    - Status: Completed
    - Note: File: `src/trade_server/metrics.rs`. Added gauges: `orderbook_spread{asset_id}`, `orderbook_mid_price{asset_id}`, `orderbook_depth{asset_id,side}`. Added counters: `limit_orders_placed{venue,status}`, `limit_orders_filled{venue}`, `limit_orders_cancelled{venue}`. Added histogram: `limit_order_fill_latency_ms`. Updated `trade_server.rs` to record orderbook metrics on snapshot/update events and limit order metrics on LimitOrderEvent processing.
    - Success Criteria: Metrics are registered and updated during event processing

16. Add structured logging for orderbook operations
    - Status: Completed
    - Note: Files: `src/trade_server/trade_server.rs`, `src/position/in_mem_manager.rs`. Added INFO logs for orderbook snapshot processing with spread/depth data. Added DEBUG logs for orderbook updates with mid price calculation. Changed empty orderbook from DEBUG to WARN level. Added detailed INFO/ERROR logs for all LimitOrderEvent variants. Exit mode logging already present in in_mem_manager.rs for Automatic vs StrategyManaged modes.
    - Success Criteria: Orderbook operations are logged at appropriate levels

17. Update documentation
    - Status: Completed
    - Note: Updated `docs/architecture.md` with: new SystemEvent::LimitOrder variant, SignalIntent enum, ExitMode enum, extended OrderType variants, extended Order struct, new OrderExecutor methods, and new "Orderbook Trading Support" section covering event types, order types, signal intent system, exit modes, metrics, and backward compatibility. Updated `docs/usage-guide.md` with: expanded SystemEvent and TokenEvent documentation, new "Orderbook Trading Support" section with examples for processing orderbook events, creating exit signals, modifying/cancelling orders, exit modes explanation, and complete CLOB strategy example.
    - Success Criteria: Documentation reflects new capabilities

## 5. Usage Guide

### For AI Agents Executing This Task

1. **Status Updates**: Update the `Status` field for each task as you work:
   - `Not Started` -> `In Progress` when beginning work
   - `In Progress` -> `Completed` when done and verified

2. **Task Notes**: The Note field provides:
   - File locations where changes should be made
   - What will be implemented (key types, methods, changes)
   - Key technical details and patterns to follow
   - Notes can be expanded with implementation details as you work

3. **Success Criteria**: Before marking a task `Completed`, verify:
   - The success criteria listed for that task is met
   - Existing tests pass (run `cargo test`)
   - Code compiles without warnings

4. **Phase Execution**: You can be instructed to complete specific phases:
   - "Complete Phase 1" -> Work on tasks 1-3
   - "Complete Phase 2" -> Work on tasks 4-6
   - Tasks within a phase can be done in parallel if independent

5. **Task References**: Tasks can be referenced by number:
   - "Complete task 5" -> Work on extending `OrderExecutor` trait
   - "What's the status of task 8?" -> Report on TradeServer event routing

6. **Dependencies**: Tasks are ordered so dependencies come first:
   - Phase 1 (data types) must complete before Phase 2 (traits)
   - Phase 2 (traits) must complete before Phase 3 (event routing)
   - Phases 4 and 5 can proceed in parallel after Phase 3

7. **Design Reference**: For detailed specifications, refer to:
   - `docs/specs/002-orderbook-support/design.md` - Full design document
   - `docs/architecture.md` - Current architecture
   - `docs/usage-guide.md` - Current usage patterns

8. **Git Workflow**: This tracker document can be committed during development to track progress. **Before creating a PR**, remove this file - only the final code changes should be in the PR.

9. **External Types**: The `OrderbookSnapshotEvent` and `OrderbookUpdateEvent` types come from `popeyes_trading_types` crate (checked out at `/home/zfeng/popeyes/trading-types/`). Reference those types when implementing event handlers.
