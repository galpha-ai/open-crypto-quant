# Position Manager Refactor Plan

## Summary

The `InMemoryPositionManager` in `src/position/in_mem_manager.rs` has grown to ~1700 lines with multiple distinct responsibilities. This document outlines a phased approach to decompose it into focused, testable modules while maintaining backward compatibility.

## Problem

The current implementation handles:
1. Core position state management (CRUD)
2. SOL balance and trade statistics tracking
3. Signal handling (signal-to-order conversion)
4. Execution event handling (fills, position updates)
5. Timer-based exit logic (ExitStrategy evaluation)
6. Pending sell management
7. Active exit order management
8. Intent-based reconciliation (newest, most complex)
9. Embedded tests (~450 lines)

This violates single responsibility principle and makes the code harder to:
- Test individual components in isolation
- Reason about behavior
- Extend with new features
- Onboard new developers

## Goals

- Break `in_mem_manager.rs` into focused modules under 300 lines each
- Improve testability by isolating concerns
- Maintain 100% backward compatibility with existing API
- Enable independent development of reconciliation engine
- Move tests to dedicated test directory

## Non-Goals

- Changing the `PositionManager` trait interface
- Modifying AMM signal processing behavior
- Performance optimization (refactor only)
- Adding new features during refactor

## Target Structure

```
src/position/
├── mod.rs                  # Module exports
├── manager.rs              # PositionManager trait (unchanged)
├── in_mem_manager.rs       # Thin coordinator (~200 lines)
├── state.rs                # State struct and basic accessors
├── balance_tracker.rs      # SOL balance, PnL, trade statistics
├── signal_handler.rs       # Signal-to-order conversion logic
├── execution_handler.rs    # ExecutionEvent processing
├── exit_manager.rs         # Timer handling, exit order management
├── reconciliation/
│   ├── mod.rs
│   ├── engine.rs           # Core reconciliation algorithm
│   ├── pending_orders.rs   # PendingLimitOrder state management
│   └── validation.rs       # Intent validation
├── pending_order.rs        # PendingLimitOrder type (existing)
└── tests/
    ├── mod.rs
    ├── reconciliation_tests.rs
    ├── signal_handler_tests.rs
    ├── execution_handler_tests.rs
    └── exit_manager_tests.rs
```

## Module Responsibilities

### `state.rs` (~150 lines)
- `InMemoryPositionManagerState` struct definition
- Basic state accessors (get/set position, pending orders)
- State initialization

### `balance_tracker.rs` (~100 lines)
- `get_available_sol()`, `update_sol_balance()`
- `get_total_realized_pnl_sol()`
- Trade statistics (winning trades, closed positions)
- SOL reservation for pending buys

### `signal_handler.rs` (~150 lines)
- `handle_signal()` implementation
- Max position limit checks
- SOL balance validation
- Buy order creation from signals

### `execution_handler.rs` (~200 lines)
- `handle_execution()` implementation
- `handle_order_filled()` for position creation/updates
- Position closure on sells
- PnL calculation

### `exit_manager.rs` (~150 lines)
- `handle_timer()` implementation
- ExitStrategy evaluation
- Safety limit checks for StrategyManaged mode
- Active exit order tracking

### `reconciliation/engine.rs` (~200 lines)
- `reconcile_intent()` implementation
- `reconcile_side()` for bid/ask processing
- Level matching logic
- Order generation (cancel/limit)

### `reconciliation/pending_orders.rs` (~100 lines)
- `handle_limit_order_event()` implementation
- Pending order CRUD operations
- Partial fill handling

### `in_mem_manager.rs` (~200 lines)
- `InMemoryPositionManager` struct with component references
- `PositionManager` trait implementation (delegates to components)
- Constructor and initialization

## Implementation Phases

### Phase 1: Extract Tests (Low Risk)

**Scope:**
1. Create `src/position/tests/` directory
2. Move `reconciliation_tests` module to `tests/reconciliation_tests.rs`
3. Update `mod.rs` to include test module conditionally

**Verification:**
- All tests pass
- No behavior change

**Estimated effort:** Small

### Phase 2: Extract Reconciliation Module

**Scope:**
1. Create `src/position/reconciliation/` directory
2. Extract `reconcile_intent()`, `reconcile_side()`, helper methods to `engine.rs`
3. Extract pending order management to `pending_orders.rs`
4. Extract validation to `validation.rs`
5. Update `in_mem_manager.rs` to use extracted modules

**Verification:**
- All reconciliation tests pass
- Intent-based signals work correctly

**Estimated effort:** Medium

### Phase 3: Extract State and Balance Tracking

**Scope:**
1. Create `state.rs` with state struct and accessors
2. Create `balance_tracker.rs` with SOL/PnL logic
3. Update `in_mem_manager.rs` to compose these modules

**Verification:**
- Position state operations work correctly
- SOL balance tracking accurate

**Estimated effort:** Medium

### Phase 4: Extract Signal and Execution Handlers

**Scope:**
1. Create `signal_handler.rs` with signal processing logic
2. Create `execution_handler.rs` with execution event handling
3. Update `in_mem_manager.rs` to delegate

**Verification:**
- AMM signal flow works correctly
- Execution events processed correctly

**Estimated effort:** Medium

### Phase 5: Extract Exit Manager

**Scope:**
1. Create `exit_manager.rs` with timer and exit logic
2. Move active exit order management
3. Move safety limit checks

**Verification:**
- Timer-based exits work correctly
- StrategyManaged mode works correctly

**Estimated effort:** Small

### Phase 6: Finalize Coordinator

**Scope:**
1. Slim down `in_mem_manager.rs` to coordinator role
2. Clean up imports and module structure
3. Update documentation

**Verification:**
- All existing tests pass
- Integration tests pass
- Documentation accurate

**Estimated effort:** Small

## Design Decisions

### Composition vs Inheritance

Use composition: `InMemoryPositionManager` holds references to handler structs rather than using trait inheritance. This allows:
- Independent testing of components
- Clear ownership of state
- Simpler mental model

### State Ownership

Single `Mutex<State>` owned by coordinator, passed to handlers by reference. This:
- Maintains current concurrency model
- Avoids distributed locking complexity
- Keeps state consistent

### Module Visibility

- Public: `PositionManager` trait, `InMemoryPositionManager`, types
- Crate-internal: Handler modules, state internals
- Test-only: Test utilities and mocks

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Breaking existing behavior | Low | High | Comprehensive test coverage before refactor |
| Introducing race conditions | Low | High | Keep single Mutex pattern, code review |
| Performance regression | Low | Low | No algorithm changes, benchmark if needed |
| Incomplete extraction | Medium | Low | Phased approach allows partial completion |

## Acceptance Criteria

- [ ] All existing tests pass without modification
- [ ] No files exceed 300 lines (excluding tests)
- [ ] Each module has single clear responsibility
- [ ] `in_mem_manager.rs` reduced to ~200 lines
- [ ] Tests moved to dedicated directory
- [ ] Module structure matches target structure
- [ ] No public API changes to `PositionManager` trait

## References

- [Architecture Documentation](../../architecture.md)
- [Intent-Based Position Management Design](./design.md)
- [Current Implementation](../../../src/position/in_mem_manager.rs)
