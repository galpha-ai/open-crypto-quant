# Order Status Poller

## Summary

This design introduces an `OrderStatusPoller` service within trade-server that monitors pending orders via REST API polling and emits fill events to the EventCoordinator. The service provides centralized rate limit management, reliable fill detection without WebSocket dependencies, and optional position reconciliation from the remote API as source of truth.

**Why REST polling over WebSocket?**
- **Self-healing**: API is source of truth; any local state drift is automatically corrected
- **Reliability**: No missed events from WebSocket disconnections or reconnection gaps
- **Simpler architecture**: No connection management, heartbeats, or reconnection logic
- **Central rate limiting**: Single point of control for API request budget

*Future optimization: A separate account service with account-aware tx-sub can provide streaming fill events, eliminating polling latency.*

## Goals

- Provide reliable fill detection for limit orders via REST API polling
- Centralize rate limit management for Polymarket API calls
- Emit `LimitOrderEvent::OrderPartiallyFilled` when fills are detected
- Support periodic position/balance reconciliation as a backstop
- Keep all logic within trade-server (no changes to trading-types or tx-sub)
- Decouple fill detection from order execution (clean separation of concerns)

## Non-Goals

- **Real-time fills via WebSocket**: Polling introduces latency (500ms-2s); acceptable for market making but not HFT
- **Multi-exchange support**: Initially Polymarket only; can be generalized later
- **Order submission**: Executor handles placement; poller only monitors
- **Historical fill recovery**: Only monitors orders registered after startup

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                            trade-server                                  │
│                                                                         │
│  ┌────────────────────┐                    ┌─────────────────────────┐  │
│  │  PolymarketOrder   │   monitor_order()  │   OrderStatusPoller     │  │
│  │  Executor          │ ────────────────> │                         │  │
│  │                    │                    │ - monitored_orders      │  │
│  │  execute_limit()   │   unmonitor()      │ - rate_limiter          │  │
│  │  cancel_order()    │ ────────────────> │ - poll_cycle()          │  │
│  └────────────────────┘                    │ - sync_positions()      │  │
│                                            └───────────┬─────────────┘  │
│                                                        │                │
│                                                        │ enqueue_event  │
│                                                        ▼                │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │                      EventCoordinator                             │   │
│  │                                                                   │   │
│  │  LimitOrderEvent::OrderPartiallyFilled { filled_size, ... }      │   │
│  │  LimitOrderEvent::OrderCancelled { reason: "expired", ... }      │   │
│  │  PositionSyncEvent { positions, balances }  (optional)           │   │
│  └──────────────────────────────────────────────────────────────────┘   │
│                                      │                                  │
│                                      ▼                                  │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │                    TradeServer (event loop)                       │   │
│  │                                                                   │   │
│  │  → PositionManager.handle_limit_order_event()                    │   │
│  │  → PositionEvent::PositionUpdated { source: Fill }               │   │
│  └──────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

## API

### OrderStatusPoller

```rust
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use polyfill_rs::ClobClient;

/// Service for polling order status from Polymarket API
pub struct OrderStatusPoller {
    /// Polymarket CLOB client for REST API calls
    client: Arc<ClobClient>,

    /// Event coordinator for enqueueing fill events
    event_coordinator: Arc<dyn EventCoordinator>,

    /// Orders being monitored: order_id -> MonitoredOrder
    monitored_orders: RwLock<HashMap<String, MonitoredOrder>>,

    /// Token bucket rate limiter
    rate_limiter: RateLimiter,

    /// Configuration
    config: PollerConfig,

    /// Metrics collector
    metrics: PollerMetrics,

    /// Shutdown signal
    shutdown: tokio::sync::watch::Receiver<bool>,
}

impl OrderStatusPoller {
    /// Create a new poller with the given client and config
    pub fn new(
        client: Arc<ClobClient>,
        event_coordinator: Arc<dyn EventCoordinator>,
        config: PollerConfig,
        shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> Self;

    /// Register an order for monitoring
    /// Called by executor after successful order placement
    pub async fn monitor_order(&self, order: MonitoredOrder);

    /// Stop monitoring an order
    /// Called by executor before/after cancel, or when order fully filled
    pub async fn unmonitor_order(&self, order_id: &str) -> Option<MonitoredOrder>;

    /// Get current monitored order count
    pub async fn monitored_count(&self) -> usize;

    /// Get a snapshot of monitored orders (for debugging/API)
    pub async fn get_monitored_orders(&self) -> Vec<MonitoredOrder>;

    /// Run the polling loop (spawned as background task)
    /// Polls orders and optionally syncs positions on configured intervals
    pub async fn run(&self) -> Result<()>;

    /// Execute a single poll cycle (for testing or manual trigger)
    pub async fn poll_cycle(&self) -> Result<PollCycleResult>;

    /// Sync positions/balances from API (reconciliation)
    pub async fn sync_positions(&self) -> Result<PositionSyncResult>;
}
```

### MonitoredOrder

```rust
/// An order being monitored for fills
#[derive(Debug, Clone)]
pub struct MonitoredOrder {
    /// Polymarket order ID
    pub order_id: String,

    /// Token/asset ID (mint)
    pub mint: String,

    /// Market identifier (condition_id)
    pub market: Option<String>,

    /// Order side
    pub side: OrderSide,

    /// Limit price
    pub price: f64,

    /// Original order size
    pub original_size: f64,

    /// Last known filled amount (for delta detection)
    pub last_known_filled: f64,

    /// Signal ID that triggered this order
    pub signal_id: Option<String>,

    /// Time order was added to monitoring
    pub added_at: Instant,

    /// Last time this order was polled
    pub last_polled: Option<Instant>,

    /// Number of times polled
    pub poll_count: u32,

    /// Consecutive poll failures
    pub consecutive_failures: u32,
}

impl MonitoredOrder {
    pub fn new(
        order_id: String,
        mint: String,
        market: Option<String>,
        side: OrderSide,
        price: f64,
        original_size: f64,
        signal_id: Option<String>,
    ) -> Self;

    /// Time since order was added
    pub fn age(&self) -> Duration;

    /// Time since last poll (None if never polled)
    pub fn time_since_poll(&self) -> Option<Duration>;

    /// Remaining unfilled size
    pub fn remaining_size(&self) -> f64;

    /// Whether order is fully filled
    pub fn is_fully_filled(&self) -> bool;
}
```

### PollerConfig

```rust
/// Configuration for the order status poller
#[derive(Debug, Clone)]
pub struct PollerConfig {
    /// Base polling interval between cycles
    /// Default: 500ms
    pub poll_interval: Duration,

    /// Maximum orders to poll per cycle
    /// Prevents starvation if many orders pending
    /// Default: 20
    pub batch_size: usize,

    /// Rate limit: requests per second
    /// Polymarket limit is ~10 req/sec for authenticated endpoints
    /// Default: 8.0 (leave headroom)
    pub rate_limit_rps: f64,

    /// Enable position sync (reconciliation)
    /// Default: true
    pub enable_position_sync: bool,

    /// Interval for position sync
    /// Default: 30s
    pub position_sync_interval: Duration,

    /// Maximum consecutive failures before giving up on an order
    /// Order will be unmonitored and error logged
    /// Default: 10
    pub max_consecutive_failures: u32,

    /// Backoff multiplier for failed polls
    /// Delay = base_interval * backoff^failures
    /// Default: 1.5
    pub failure_backoff_multiplier: f64,

    /// Maximum age for monitored orders
    /// Orders older than this are pruned (assumed expired/cancelled externally)
    /// Default: 24h
    pub max_order_age: Duration,
}

impl Default for PollerConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_millis(500),
            batch_size: 20,
            rate_limit_rps: 8.0,
            enable_position_sync: true,
            position_sync_interval: Duration::from_secs(30),
            max_consecutive_failures: 10,
            failure_backoff_multiplier: 1.5,
            max_order_age: Duration::from_secs(86400),
        }
    }
}
```

### Result Types

```rust
/// Result of a single poll cycle
#[derive(Debug, Default)]
pub struct PollCycleResult {
    /// Number of orders polled
    pub orders_polled: usize,

    /// Fill events detected and enqueued
    pub fills_detected: usize,

    /// Orders completed (fully filled or cancelled)
    pub orders_completed: usize,

    /// Poll errors encountered
    pub errors: usize,

    /// Duration of the poll cycle
    pub duration: Duration,
}

/// Result of position sync
#[derive(Debug)]
pub struct PositionSyncResult {
    /// Position snapshots from API
    pub positions: Vec<PositionSnapshot>,

    /// Available quote balance
    pub available_quote: f64,

    /// Timestamp of sync
    pub timestamp: DateTime<Utc>,

    /// Whether any drift was detected vs local state
    pub drift_detected: bool,
}

/// Snapshot of a position from API
#[derive(Debug, Clone)]
pub struct PositionSnapshot {
    /// Token/asset ID
    pub asset_id: String,

    /// Current position size
    pub amount: f64,

    /// Average entry price (if available)
    pub avg_price: Option<f64>,
}
```

## Data Model

### Rate Limiter

```rust
/// Token bucket rate limiter for API calls
pub struct RateLimiter {
    /// Tokens available (fractional for smooth limiting)
    tokens: AtomicU64,  // Fixed-point: tokens * 1000

    /// Maximum tokens (bucket capacity)
    max_tokens: u64,

    /// Tokens added per second
    refill_rate: f64,

    /// Last refill timestamp
    last_refill: AtomicU64,  // Unix millis
}

impl RateLimiter {
    pub fn new(requests_per_second: f64) -> Self;

    /// Acquire a token, waiting if necessary
    pub async fn acquire(&self);

    /// Try to acquire without waiting
    pub fn try_acquire(&self) -> bool;

    /// Current available tokens
    pub fn available(&self) -> f64;
}
```

### New Event Type (Optional)

If position reconciliation detects drift, emit a sync event:

```rust
/// Event for position reconciliation from external source
pub enum PositionSyncEvent {
    /// Positions synced from remote API
    PositionsSynced {
        /// Source identifier
        source: String,  // "polymarket"

        /// Position snapshots
        positions: Vec<PositionSnapshot>,

        /// Available quote currency
        available_quote: f64,

        /// Sync timestamp
        timestamp: DateTime<Utc>,
    },
}
```

**Alternative**: Use existing `PositionEvent::PositionUpdated` with a new `PositionUpdateSource::Reconciliation` variant.

## Behavior

### Poll Cycle

Given a set of monitored orders:

1. **Select orders to poll**
   - Filter orders that need polling (time since last poll > interval)
   - Sort by priority: newer orders first, then by time since last poll
   - Take up to `batch_size` orders

2. **For each order in batch**:
   ```
   a. Acquire rate limit token (blocks if exhausted)
   b. Call client.get_order(order_id)
   c. On success:
      - Compare size_matched vs last_known_filled
      - If increased: emit LimitOrderEvent::OrderPartiallyFilled
      - Update last_known_filled, last_polled, poll_count
      - If fully filled: unmonitor and emit completion
      - Reset consecutive_failures to 0
   d. On 404/NOT_FOUND:
      - Order no longer exists (cancelled/expired externally)
      - Unmonitor and emit LimitOrderEvent::OrderCancelled
   e. On error:
      - Increment consecutive_failures
      - If >= max_consecutive_failures: unmonitor with error log
      - Apply backoff to next poll time
   ```

3. **Prune stale orders**
   - Remove orders older than `max_order_age`
   - Log warning for each pruned order

4. **Record metrics**
   - Update poll cycle duration, orders polled, fills detected

### Fill Detection Logic

```rust
fn detect_fill(
    &self,
    local: &MonitoredOrder,
    api_order: &OpenOrder,
) -> Option<LimitOrderEvent> {
    let size_matched = api_order.size_matched.to_f64().unwrap_or(0.0);

    if size_matched > local.last_known_filled {
        let new_fill = size_matched - local.last_known_filled;
        let remaining = local.original_size - size_matched;

        Some(LimitOrderEvent::OrderPartiallyFilled {
            order_id: local.order_id.clone(),
            mint: local.mint.clone(),
            side: local.side,
            filled_size: new_fill,
            remaining_size: remaining,
            fill_price: local.price,  // Use order price; actual fill price not in API
            timestamp: Utc::now(),
            signal_id: local.signal_id.clone(),
            exit_mode: None,
        })
    } else {
        None
    }
}
```

### Position Sync (Reconciliation)

Every `position_sync_interval`:

1. Call `client.get_balance_allowance()` to get current balances
2. Convert to `PositionSnapshot` for each conditional token with balance > 0
3. Compare with local PositionManager state
4. If drift detected:
   - Log warning with details
   - Optionally emit `PositionSyncEvent` for correction
5. Record sync metrics

### Polling Priority

Orders are polled with priority based on:

1. **Recency**: Newer orders more likely to fill soon
2. **Time since last poll**: Ensure all orders get polled eventually
3. **Failure state**: Back off on failing orders to avoid hammering

```rust
fn poll_priority(order: &MonitoredOrder) -> impl Ord {
    (
        order.added_at,                          // Newer first
        order.last_polled.unwrap_or(Instant::now()), // Never-polled first
        Reverse(order.consecutive_failures),     // Healthy orders first
    )
}
```

## Integration

### With PolymarketOrderExecutor

```rust
impl PolymarketOrderExecutor {
    // Poller is owned by executor or passed as dependency
    poller: Arc<OrderStatusPoller>,

    async fn execute_limit_order(&self, order: Order) -> Result<LimitOrderEvent> {
        // ... submit order to API ...

        // Register with poller (replaces local pending_orders tracking)
        self.poller.monitor_order(MonitoredOrder {
            order_id: order_id.clone(),
            mint: order.mint.clone(),
            market: order.market.clone(),
            side,
            price,
            original_size: size,
            last_known_filled: 0.0,
            signal_id: order.signal_id.clone(),
            added_at: Instant::now(),
            last_polled: None,
            poll_count: 0,
            consecutive_failures: 0,
        }).await;

        Ok(LimitOrderEvent::OrderPlaced { ... })
    }

    async fn cancel_order(&self, order_id: &str) -> Result<LimitOrderEvent> {
        // Unmonitor BEFORE cancel to prevent race with poller
        self.poller.unmonitor_order(order_id).await;

        // ... cancel via API ...

        Ok(LimitOrderEvent::OrderCancelled { ... })
    }
}
```

### With TradeServer

```rust
impl TradeServer {
    async fn run(&mut self) -> Result<()> {
        // Spawn poller as background task
        let poller = self.executor.poller().clone();
        let poller_handle = tokio::spawn(async move {
            if let Err(e) = poller.run().await {
                error!(error = %e, "Order status poller failed");
            }
        });

        // Main event loop
        loop {
            match self.event_coordinator.next_event().await {
                // ... handle events including LimitOrderEvent from poller ...
            }
        }

        // Shutdown
        poller_handle.abort();
    }
}
```

### Migration from Current Implementation

The current `poll_pending_orders()` in executor can be deprecated:

1. **Phase 1**: Introduce `OrderStatusPoller` alongside existing polling
2. **Phase 2**: Route new limit orders to poller via `monitor_order()`
3. **Phase 3**: Remove `pending_orders` from executor, delete `poll_pending_orders()`

## Observability

### Metrics

```rust
pub struct PollerMetrics {
    /// Orders currently being monitored
    pub monitored_orders: Gauge,

    /// Poll cycles completed
    pub poll_cycles_total: Counter,

    /// Poll cycle duration histogram
    pub poll_cycle_duration_seconds: Histogram,

    /// Orders polled per cycle
    pub orders_polled_per_cycle: Histogram,

    /// Fills detected
    pub fills_detected_total: Counter,

    /// API errors by type
    pub api_errors_total: CounterVec,  // labels: error_type

    /// Rate limiter wait time
    pub rate_limit_wait_seconds: Histogram,

    /// Position syncs completed
    pub position_syncs_total: Counter,

    /// Position drift events
    pub position_drift_detected_total: Counter,
}
```

### Logs

| Event | Level | Fields |
|-------|-------|--------|
| Order monitored | DEBUG | order_id, mint, side, price, size |
| Order unmonitored | DEBUG | order_id, reason |
| Fill detected | INFO | order_id, mint, filled_size, remaining_size |
| Order completed | INFO | order_id, mint, total_filled |
| Poll cycle complete | DEBUG | orders_polled, fills_detected, duration_ms |
| API error | WARN | order_id, error, consecutive_failures |
| Order pruned (stale) | WARN | order_id, age_seconds |
| Max failures reached | ERROR | order_id, total_failures |
| Position drift | WARN | asset_id, local_amount, remote_amount |

## Security

- **API credentials**: Poller uses the same `ClobClient` as executor; credentials managed at client level
- **Rate limiting**: Prevents accidental API abuse; configurable to stay within Polymarket limits
- **No new attack surface**: Polling is read-only; no order modification capability

## Acceptance Criteria

### Functional

- [ ] Orders registered via `monitor_order()` are polled at configured interval
- [ ] `LimitOrderEvent::OrderPartiallyFilled` emitted when `size_matched` increases
- [ ] Orders automatically unmonitored when fully filled
- [ ] Orders unmonitored when API returns 404 (cancelled/expired)
- [ ] Rate limiter prevents exceeding configured requests/second
- [ ] Position sync detects and logs drift when enabled

### Edge Cases

- [ ] Handles API timeout gracefully (retry with backoff)
- [ ] Handles rate limit exceeded (429) response
- [ ] Handles order not found (404) - emit cancellation event
- [ ] Handles concurrent `monitor_order` and `unmonitor_order` calls
- [ ] Handles shutdown signal cleanly (drains in-flight requests)
- [ ] Handles poller restart (no duplicate fill events due to `last_known_filled`)

### Performance

- [ ] Poll cycle completes within 2x poll_interval under normal load
- [ ] Rate limiter adds < 10ms overhead when tokens available
- [ ] Memory usage scales linearly with monitored order count
- [ ] No CPU spin when no orders to poll

## Rollout Plan

### Phase 1: Implementation

1. Implement `RateLimiter` with token bucket algorithm
2. Implement `OrderStatusPoller` with `monitor_order`, `unmonitor_order`, `poll_cycle`
3. Add metrics and logging
4. Unit tests for fill detection logic, rate limiting

### Phase 2: Integration

1. Add poller to `PolymarketOrderExecutor` as optional component
2. Route new limit orders to poller in parallel with existing `pending_orders`
3. Verify fills detected match between old and new systems
4. Integration tests with mock API

### Phase 3: Migration

1. Remove `pending_orders` from executor
2. Delete `poll_pending_orders()` method
3. Make poller the sole fill detection mechanism
4. Update documentation

### Phase 4: Position Sync

1. Implement `sync_positions()` with balance API
2. Add `PositionSyncEvent` or use existing reconciliation path
3. Test drift detection and correction

## Future Considerations

- **Generalize for other exchanges**: Extract interface for multi-exchange poller
- **WebSocket hybrid**: Use WS for low-latency fills, polling as fallback
- **Batch order queries**: If Polymarket adds batch `get_orders` by ID, use it
- **Adaptive polling**: Increase frequency during active trading, decrease when idle

## References

- [Polymarket CLOB API](https://docs.polymarket.com/)
- [polyfill-rs library](https://github.com/floor-licker/polyfill-rs)
- [005-polymarket-executor design](../005-polymarket-executor/design.md)
- [003-intent-based-position-management](../003-intent-based-position-management/design.md)
