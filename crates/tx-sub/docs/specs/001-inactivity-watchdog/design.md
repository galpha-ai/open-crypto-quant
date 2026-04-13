# Inactivity Watchdog via StatsMonitor

## Summary

Add a background watchdog task owned by `StatsMonitor` that exits the process if no trade events are observed within a configurable timeout. This keeps parsing logic focused while centralizing health checks alongside existing stats logging.

## Motivation

- Recover from silent failures (e.g., stalled gRPC streams, network partitions) by letting the orchestrator restart.
- Keep watchdog/health logic out of `TransactionParser` to maintain separation of concerns and reusability.
- Leverage existing event tracking in `StatsMonitor` and its logging task.

## Goals

- Exit the process when no trade events are recorded for a configured duration.
- Minimal, isolated changes with clear observability.
- Configurable timeout, disabled by default.

## Non‑Goals

- Detecting underlying causes (gRPC vs. Redis vs. parser). This design focuses on “no trades observed.”
- Implementing graceful draining/shutdown hooks beyond a clear error log and exit.

## Configuration

Extend `StatsMonitorConfig` to include an optional inactivity timeout:

```rust
pub struct StatsMonitorConfig {
    pub enabled: bool,
    pub window_seconds: Option<u64>,
    pub log_interval_seconds: Option<u64>,
    pub inactivity_timeout_seconds: Option<u64>, // NEW
}
```

Behavior:
- `None` (default): watchdog disabled.
- `Some(n)`: exit if no trade events observed for `n` seconds.

Example YAML snippet:

```yaml
stats_monitor:
  enabled: true
  window_seconds: 10
  log_interval_seconds: 5
  inactivity_timeout_seconds: 5
```

## Design

### Data source

`StatsMonitor` already tracks events in a sliding window via `DexEventWindow` and exposes `get_stats()` which returns `last_event_age_secs` (most recent event across all DEXs). The watchdog will reuse this value.

### API additions (StatsMonitor)

1) `is_inactive(&self, threshold: Duration) -> bool` (async)
- Calls `get_stats()` and returns `true` when `last_event_age_secs >= threshold.as_secs_f64()`.
- Treats “no events yet” as inactive only after an optional warmup period (see Edge Cases) or when explicitly configured to do so.

2) `spawn_inactivity_watchdog(self: Arc<Self>, timeout: Duration)`
- Spawns a Tokio task with a 1s interval tick.
- On each tick, checks `is_inactive(timeout)`.
- If inactive, logs an error and calls `std::process::exit(1)`.

### App wiring

`src/app.rs` owns component orchestration and already calls `spawn_logging_task`. After constructing `StatsMonitor`, if `inactivity_timeout_seconds` is `Some(t)`, call:

```rust
Arc::clone(&monitor).spawn_inactivity_watchdog(Duration::from_secs(t));
```

This keeps all lifecycle/health logic in one place and avoids modifying the parser’s `recv` loop.

## Behavior Details

- Tick cadence: 1s.
- Trigger condition: `last_event_age_secs >= timeout_secs`.
- Logging: Emit a single `ERROR` with the effective timeout and `last_event_age_secs` before exit.
- Exit code: `1`.
- Optional: Increment a Prometheus counter (see Observability) just before exit.

## Observability

- Reuse existing periodic stats logs (`dex_stats_monitor`).
- Add counter (optional, recommended): `inactivity_exits_total` (Prometheus) in `metrics.rs` to track watchdog exits.

## Edge Cases & Safeguards

- Startup quiet period: To avoid false positives before first trade, either:
  - Add a fixed warmup (e.g., 2× timeout) before watchdog activates, or
  - Activate immediately but treat `last_event_age_secs == None` as “not inactive.”
  Recommended: the second approach (simple and predictable) unless a warmup is needed operationally.
- Legit low activity: Make timeout configurable per environment; 5s may be too aggressive in some deployments.
- Test mode: Consider skipping the watchdog entirely when `test` mode is enabled to avoid premature exits during controlled runs.

## Implementation Plan

1) Config
- Update `src/config.rs` to add `inactivity_timeout_seconds: Option<u64>` to `StatsMonitorConfig`.

2) StatsMonitor
- Add `pub async fn is_inactive(&self, threshold: Duration) -> bool`.
- Add `pub fn spawn_inactivity_watchdog(self: Arc<Self>, timeout: Duration)`.

3) App wiring
- In `src/app.rs`, after `spawn_logging_task`, if configured, call `spawn_inactivity_watchdog`.
- Optionally guard with test mode check.

4) Observability (optional)
- Add `inactivity_exits_total` to `metrics.rs`, increment immediately before exit.

## Testing

- Unit tests (stats monitor):
  - Simulate event recording, verify `is_inactive` returns `false` when events occur within threshold.
  - Verify `true` after advancing time beyond threshold with no events.

- Integration (Tokio):
  - Spawn monitor with short timeout; do not record events; assert process exit (in a controlled harness) or intercept exit call via abstraction (if refactoring exit behavior for tests).
  - Record events periodically; assert no exit over several intervals.

- Manual:
  - Run with `inactivity_timeout_seconds: 5` against a quiet environment; observe single `ERROR` and process exit.

## Alternatives Considered

Parser-embedded watchdog: Place `tokio::select!` with a sleep in `TransactionParser::start()` and exit if inactive.

Reasons not chosen:
- Couples lifecycle responsibility to parsing.
- Harder to reuse parser elsewhere without watchdog behavior.
- App already owns logging/lifecycle tasks, making the StatsMonitor location a natural fit.

## Risks

- False positives in low‑traffic periods if timeout is too small.
- Immediate `exit(1)` may interrupt in‑flight logs/exports. Mitigation: emit the `ERROR` first; depending on logger flush behavior, consider a brief `sleep(100–200ms)` before exit if data loss is observed in practice.

## Rollout

1) Ship with watchdog disabled (default `None`).
2) Enable in staging with a conservative timeout (e.g., 30s+).
3) Observe logs/metrics, adjust timeout.
4) Enable in production per environment norms.
