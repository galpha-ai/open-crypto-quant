# Unified Limit Order Lifecycle

## Summary

This design standardizes limit-order lifecycle behavior across backtest, paper trading, and live Polymarket execution. The current implementations differ in when cancel is considered final, how in-flight placement is represented, and when lifecycle events are emitted. The proposal introduces a shared lifecycle contract and state machine so all modes follow the same semantics while still allowing mode-specific adapters for latency and venue integration.

## Goals

- Define one lifecycle contract for place/cancel/fill/terminal transitions across all modes.
- Preserve in-flight cancel intent instead of dropping it when venue order ID is not yet known.
- Eliminate duplicate or contradictory terminal cancellation behavior.
- Keep existing strategy/reconciliation interfaces mostly unchanged.
- Support realistic race behavior: fills may occur before cancel is finalized.

## Non-Goals

- Rewriting intent reconciliation logic or quote computation algorithms.
- Making fill source identical across modes (trade replay vs poller vs websocket).
- Redesigning market-order execution behavior.
- Introducing distributed workflow orchestration outside trade-server.

## Behavior

Given a limit-order placement request:
1. Create a local lifecycle record in `SubmitPending` with a stable `client_order_id`.
2. If venue placement succeeds, store `venue_order_id` and transition to `Open` unless cancel was previously requested.
3. If cancel was requested while `SubmitPending`, immediately submit cancel once `venue_order_id` is known and transition to `CancelPending`.
4. If placement fails/rejects, transition directly to terminal `Rejected`.

Given a cancel request:
1. If order is `SubmitPending`, record `cancel_requested=true` and emit optional non-terminal ack (`CancelRequested`).
2. If order is `Open`, submit venue cancel and transition to `CancelPending`.
3. If order is already terminal, return idempotent success (no state change).
4. Terminal cancellation is only emitted when cancellation is confirmed by adapter evidence (ack/status/timeout policy), not at intent receipt.

Given fill and cancel races:
1. Fills before cancel finalization are valid and must be applied.
2. Once terminal state is reached (`Filled`, `Cancelled`, `Rejected`, `Expired`), no further lifecycle transitions are allowed.
3. Duplicate terminal signals from different channels (executor return path, poller, websocket) are deduplicated by lifecycle state.

## API

### Lifecycle Interface (internal module)

```rust
/// Canonical state for a single logical order intent.
enum LifecycleState {
    SubmitPending,
    Open,
    CancelPending,
    Terminal(TerminalReason),
}

enum TerminalReason {
    Filled,
    Cancelled,
    Rejected,
    Expired,
}

struct LifecycleOrder {
    lifecycle_id: String,            // internal stable ID
    client_order_id: String,         // caller-facing correlation ID
    venue_order_id: Option<String>,  // known after placement confirmation
    mint: String,
    market: Option<String>,
    side: OrderSide,
    price: f64,
    original_size: f64,
    remaining_size: f64,
    time_in_force: TimeInForce,
    state: LifecycleState,
    cancel_requested: bool,
    signal_id: Option<String>,
}
```

### Adapter Contract

Each mode (backtest/paper/live) implements adapter hooks:

- `submit_place(order) -> PlacementResult`
- `submit_cancel(venue_order_id) -> CancelResult`
- `poll_or_push_updates() -> Vec<VenueLifecycleUpdate>`
- `now() -> DateTime<Utc>` (simulation clock for backtest, wall clock for live/paper)

The lifecycle engine owns state transitions and deduplication; adapters only provide evidence/events and timing behavior.

### Event Semantics

Existing `LimitOrderEvent` remains externally compatible:

- `OrderPlaced`: emitted when state enters `Open`.
- `OrderPartiallyFilled`: emitted on fill deltas.
- `OrderCancelled`: emitted once on transition to terminal `Cancelled`.
- `OrderRejected` / `OrderExpired`: emitted once on corresponding terminal transitions.

Optional internal-only event:

- `CancelRequested`: non-terminal acknowledgement used for observability/debugging. Not required to be exposed through public event streams in phase 1.

## Data Model

### New Shared Types

- `LifecycleOrder` and `LifecycleState` in a shared execution lifecycle module.
- `lifecycle_id` as authoritative internal key.
- `client_order_id` retained for upstream correlation before `venue_order_id` exists.

### State Authority

- `LifecycleOrder.state` is the single source of truth for order lifecycle transitions in all modes.
- Quote-lane tracking (for `(market, mint, side)` scheduling/replacement) is derived orchestration metadata and must not independently define terminal lifecycle outcomes.
- Existing lane structures may remain as internal scheduling/replacement details, but they must project from lifecycle records rather than act as a second authoritative state machine.

### Position Manager Interaction

Current in-flight tracking (`in_flight_orders`) remains but should key correlation by `client_order_id` (or carry both ID forms) so order placement confirmation and deferred cancel intent map cleanly to the same lifecycle record.

### Compatibility

- `OrderType::Cancel { order_id }` remains supported as the request shape.
- `order_id` resolution is unified through lifecycle record lookups (`client_order_id` and `venue_order_id` mappings) rather than mode-specific old/new paths.

## Configuration

Existing latency knobs stay in place and map to adapter timing:

- `LatencySimulationConfig.min_place_latency_ms`
- `LatencySimulationConfig.max_place_latency_ms`
- `LatencySimulationConfig.min_cancel_latency_ms`
- `LatencySimulationConfig.max_cancel_latency_ms`

New optional settings:

```yaml
execution:
  lifecycle:
    cancel_confirmation_timeout_ms: 5000
    emit_cancel_requested_events: false
```

## Idempotency & Concurrency

- Cancel requests are idempotent by lifecycle state.
- Terminal events are emitted exactly once per order lifecycle record.
- Concurrent fill and cancel updates are serialized through lifecycle transition guards.
- Unknown-order cancel requests follow explicit configured policy (strict or idempotent) and always emit a diagnostic metric.

## Observability

### Logs

- Lifecycle transition logs: `from_state`, `to_state`, `lifecycle_id`, `client_order_id`, `venue_order_id`.
- Cancel intent logs: indicate whether request was queued (`SubmitPending`) or sent to venue (`Open`).
- Dedup logs: when terminal event suppressed due to already-terminal state.

### Metrics

- `order_lifecycle_transitions_total{from,to,mode}`
- `order_cancel_requested_total{mode,state}`
- `order_cancel_terminal_total{mode,reason}`
- `order_lifecycle_terminal_dedup_total{mode,event_type}`
- `order_inflight_cancel_queue_depth{mode}`

## Security

- No new external API surface in phase 1.
- No credential changes; live adapter continues using existing venue clients.
- Lifecycle dedup guards reduce risk of repeated side effects from duplicate downstream events.

## Rollout Plan

1. **Phase 1: Shared Engine Skeleton**
   - Add lifecycle module, states, transition guards, and unit tests.
   - Add adapter trait without changing existing executors.

2. **Phase 2: Backtest Adapter Migration**
   - Route backtest lane transitions through shared lifecycle.
   - Preserve latency simulation and existing quote-lane policy.
   - Ensure simulation clock progression is not trade-only.

3. **Phase 3: Paper Adapter Migration**
   - Introduce cancel-pending behavior and optional cancel latency.
   - Stop treating cancel as immediate terminal by default in lifecycle mode.

4. **Phase 4: Live Adapter Migration**
   - Treat executor cancel response as command ack, not final terminal unless venue contract guarantees finality.
   - Use poller/websocket updates as terminal evidence source.
   - Remove duplicate cancellation emission path.

5. **Phase 5: Cutover**
   - Remove legacy mode-specific lifecycle forks once parity tests pass.
   - Run all executors on the shared lifecycle path as the only lifecycle mechanism.

## Acceptance Criteria

- Cancel during `SubmitPending` is preserved and executed after placement confirmation in all modes.
- No duplicate terminal cancellation event for a single order in any mode.
- Fill-before-cancel-finalization race is handled consistently.
- Reconciliation does not generate duplicate placements during in-flight periods.
- Contract test suite passes for all adapters:
  - place then immediate cancel (before placement confirm)
  - place then cancel with partial fill race
  - cancel after terminal fill
  - duplicate cancel requests
  - unknown-order cancel behavior

## Open Questions

- Should `CancelRequested` be externally visible or internal-only initially?
- For live mode, what constitutes sufficient cancel confirmation: REST ack, order status `CANCELED`, or either with timeout fallback?
- Should unknown-order cancel continue returning success unconditionally, or become mode-configurable with stricter default?

## References

- `docs/architecture.md`
- `docs/design/orderbook-trading.md`
- `docs/design/backtest.md`
- `src/execution/executor.rs`
- `src/execution/backtest/executor/quote_lifecycle.rs`
- `src/execution/backtest/executor/fill_engine.rs`
- `src/execution/paper/executor.rs`
- `src/execution/polymarket/executor.rs`
- `src/execution/polymarket/poller/poller.rs`
- `src/position/orderbook/intent.rs`
- `src/trade_server/position_handler/order_executor.rs`
- `src/config.rs`
