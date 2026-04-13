# Task Tracker: Unified Limit Order Lifecycle

## 1. Problem Statement
The current limit-order lifecycle behavior differs across backtest, paper trading, and live Polymarket execution, especially around cancel timing, in-flight placement, and terminal event emission. These differences create inconsistent semantics, duplicate cancellation behavior, and race-condition edge cases that are hard to reason about. We need one lifecycle authority that preserves deferred cancel intent, emits terminal events exactly once, and handles fill/cancel races consistently in every mode.

## 2. Plan
Implement a shared lifecycle engine as the canonical source of truth for order state (`SubmitPending`, `Open`, `CancelPending`, `Terminal`) and migrate each executor mode to it directly. Preserve quote-lane scheduling/replacement behavior by treating lane state as derived orchestration metadata. Validate the final behavior with cross-mode contract tests, then remove legacy mode-specific lifecycle forks.

## 3. Implementation Phases

### Phase 1: Shared Lifecycle Foundation
- **Objective**: Introduce common lifecycle types, transition guards, adapter contracts, and observability.
- **Tasks**: 1-5
- **Deliverable**: Compilable lifecycle core with tests and executor integration points.

### Phase 2: Backtest Migration
- **Objective**: Route backtest lane transitions through lifecycle authority while preserving lane policy.
- **Tasks**: 6-8
- **Deliverable**: Backtest lifecycle parity with deferred cancel and no duplicate terminal cancellation.

### Phase 3: Paper Migration
- **Objective**: Move paper executor to shared lifecycle semantics with `CancelPending` behavior.
- **Tasks**: 9-10
- **Deliverable**: Paper executor lifecycle behavior aligned with the shared contract.

### Phase 4: Live Polymarket Migration
- **Objective**: Use command-ack + venue-evidence model for terminal transitions.
- **Tasks**: 11-13
- **Deliverable**: Live executor emits terminal lifecycle events exactly once.

### Phase 5: Finalization and Documentation
- **Objective**: Validate cross-mode parity, remove legacy forks, and document the final architecture.
- **Tasks**: 14-16
- **Deliverable**: Shared lifecycle is the only lifecycle mechanism in production code and docs.

## 4. TODO List

1. Create shared lifecycle module and canonical state types.
   - Status: Not Started
   - Note: File: `src/execution/lifecycle/mod.rs`. Add `LifecycleState`, `TerminalReason`, `LifecycleOrder`, and transition APIs that enforce single-source-of-truth state authority.
   - Success Criteria: Module compiles, state/record types are reusable by all executors, and transition signatures cover place/cancel/fill/terminal paths.

2. Add lifecycle transition guard logic and idempotency rules.
   - Status: Not Started
   - Note: File: `src/execution/lifecycle/engine.rs`. Implement transition functions for submit success/failure, cancel intent, cancel confirmation, fill updates, and terminal dedup suppression.
   - Success Criteria: Invalid transitions are rejected, duplicate terminal updates are no-ops, and cancel idempotency behavior matches design.

3. Define adapter contract for mode-specific execution evidence.
   - Status: Not Started
   - Note: File: `src/execution/lifecycle/adapter.rs`. Add trait(s) for `submit_place`, `submit_cancel`, `poll_or_push_updates`, and time source abstraction used by backtest/paper/live adapters.
   - Success Criteria: Trait compiles and supports wiring existing executors without mode-specific lifecycle logic in core engine.

4. Add lifecycle configuration knobs.
   - Status: Not Started
   - Note: File: `src/config.rs`. Introduce lifecycle settings for `cancel_confirmation_timeout_ms`, `emit_cancel_requested_events`, and unknown-order cancel behavior.
   - Success Criteria: Config parses from YAML/env and controls lifecycle behavior used by all executors.

5. Add lifecycle observability primitives (logs/metrics).
   - Status: Not Started
   - Note: File: `src/execution/lifecycle/metrics.rs`. Add counters/gauges for transitions, cancel intent, terminal dedup, and in-flight cancel queue depth; emit structured transition logs with lifecycle/client/venue IDs.
   - Success Criteria: Metrics/logs are emitted from lifecycle engine paths and can distinguish mode labels.

6. Integrate lifecycle authority with backtest executor lane orchestration.
   - Status: Not Started
   - Note: Files: `src/execution/backtest/executor/quote_lifecycle.rs`, `src/execution/backtest/executor/types.rs`, `src/execution/backtest/executor.rs`. Keep lane structs for scheduling/replacement, but derive terminal outcome decisions from lifecycle state transitions.
   - Success Criteria: Backtest cancel during `SubmitPending` is preserved and executed post-placement; lane state no longer independently determines terminal lifecycle outcomes.

7. Update backtest fill/cancel race handling to flow through lifecycle engine.
   - Status: Not Started
   - Note: File: `src/execution/backtest/executor/fill_engine.rs`. Route fill-before-cancel-finalization and cancellation completion through lifecycle transition guards.
   - Success Criteria: Fill/cancel race scenarios produce consistent transitions with exactly one terminal event.

8. Add/refresh backtest lifecycle tests for contract scenarios.
   - Status: Not Started
   - Note: File: `src/execution/backtest/executor_test.rs`. Add tests for immediate cancel before placement confirmation, partial fill + cancel race, duplicate cancel requests, and cancel after terminal fill.
   - Success Criteria: New tests pass and assert no duplicate terminal cancellation events.

9. Integrate shared lifecycle into paper executor behavior.
   - Status: Not Started
   - Note: File: `src/execution/paper/executor.rs`. Replace immediate terminal cancel assumptions with `CancelPending` + confirmation behavior while preserving fill simulation integration.
   - Success Criteria: Paper lifecycle transitions align with shared engine semantics.

10. Add paper lifecycle unit/integration tests.
   - Status: Not Started
   - Note: File: `src/execution/paper/executor_test.rs`. Cover submit-pending cancel intent, confirmation-driven cancellation, and dedup of terminal events.
   - Success Criteria: Paper tests demonstrate parity with lifecycle contract and no regressions in fill simulation behavior.

11. Integrate shared lifecycle into live Polymarket executor command path.
   - Status: Not Started
   - Note: File: `src/execution/polymarket/executor.rs`. Treat cancel API response as command acknowledgment unless venue guarantees terminal finality; hand off terminal resolution to lifecycle evidence processing.
   - Success Criteria: Live cancel command does not emit premature terminal cancellation.

12. Wire poller/websocket status updates into lifecycle terminal evidence.
   - Status: Not Started
   - Note: Files: `src/execution/polymarket/poller/poller.rs`, `src/trade_server/position_handler/order_executor.rs`. Feed venue updates into lifecycle engine and deduplicate terminal states across channels.
   - Success Criteria: Terminal cancellation/fill/reject/expire is emitted once even with duplicate updates from multiple sources.

13. Implement unknown-order cancel policy + diagnostics.
   - Status: Not Started
   - Note: Files: `src/execution/lifecycle/engine.rs`, `src/config.rs`. Add explicit unknown-order cancel handling and diagnostic metric/log behavior.
   - Success Criteria: Unknown cancel behavior is deterministic, configurable, and observable.

14. Build shared lifecycle contract test suite across adapters.
   - Status: Not Started
   - Note: File: `tests/execution/lifecycle_contract_test.rs`. Add adapter-agnostic contract tests for all acceptance scenarios listed in the design doc.
   - Success Criteria: Backtest, paper, and live adapter test harnesses pass the same lifecycle contract cases.

15. Remove legacy mode-specific lifecycle forks and dead code paths.
   - Status: Not Started
   - Note: Files: `src/execution/backtest/executor.rs`, `src/execution/paper/executor.rs`, `src/execution/polymarket/executor.rs`. Delete redundant per-mode lifecycle logic once shared lifecycle parity is proven.
   - Success Criteria: Shared lifecycle is the only lifecycle mechanism used for place/cancel/fill/terminal transitions.

16. Update architecture and usage docs for final unified lifecycle behavior.
   - Status: Not Started
   - Note: Files: `docs/architecture.md`, `docs/design/backtest.md`, `docs/usage-guide/backtesting.md`, `docs/usage-guide/paper-trading.md`. Document lifecycle authority model, adapter responsibilities, and behavioral guarantees.
   - Success Criteria: Documentation matches implementation and explicitly states lifecycle state as source of truth with lane state as derived metadata.

## 5. Usage Guide

**For AI Agent Execution:**
- Update Status to "In Progress" when starting a task.
- Update Status to "Completed" when done.
- Do not include commit hashes in status updates (they change after rebase).
- Verify Success Criteria before marking complete.
- Reference tasks by number (for example, "complete task 6").
- Complete phases sequentially unless an explicit dependency exception is documented.
- Expand task Notes during execution with concrete implementation/test details discovered during work.
