# Paper Trading Guide

This guide explains how to use the trade server's paper trading mode to test trading strategies against live market data without risking capital.

## Overview

Paper trading bridges the gap between backtesting and live trading:

| Mode | Data Source | Execution | Capital Risk |
|------|-------------|-----------|--------------|
| Backtest | Historical Parquet files | Simulated (batch) | None |
| Paper Trading | Live Redis streams | Simulated (continuous) | None |
| Live | Live Redis streams | Real orders on venue | Real |

Paper trading allows you to:
- Test strategies against real-time market data
- Simulate limit order fills from live trade events
- Validate strategy behavior before going live
- Monitor performance with Prometheus metrics

## How It Works

Paper trading uses the same event loop and position management as live trading, but substitutes the real order executor with a simulated one:

```
Live Market Data (Redis) ──────► TradeServer Event Loop
                                        │
                                        ▼
                              SignalGenerator
                                        │
                                        ▼
                              OrderIntent / Signal
                                        │
                                        ▼
                              ReconciliationEngine
                                        │
                    ┌───────────────────┴───────────────────┐
                    │                                       │
            Live Mode │                               │ Paper Mode
                    ▼                                       ▼
            Real Venue API                    PaperTradingOrderExecutor
        (simulates_fills=false)              (simulates_fills=true)
                    │                                       │
                    │                       ┌───────────────┤
                    │                       │               │
                    │                       ▼               │
                    │               Live Trade Event        │
                    │               (from Redis)            │
                    │                       │               │
                    │                       ▼               │
                    │         OrderExecutor.check_fills_from_trade()
                    │                       │               │
                    └───────────────────────┴───────────────┘
                                        │
                                        ▼
                              LimitOrderEvent (Fill)
                                        │
                                        ▼
                         fill_event_to_execution_event()
                                        │
                                        ▼
                              PositionManager
                              (positions, balances, PnL)
```

When a live trade event crosses a pending paper order's price, the executor simulates a fill and emits a `LimitOrderEvent`. The position manager processes this event exactly as it would a real fill.

## Basic Usage

### Setting Up Paper Trading

```rust
use std::sync::Arc;
use trade_server::{
    execution::paper::{PaperTradingOrderExecutor, PaperTradingMetrics},
    trade_server::TradeServer,
    position::InMemoryPositionManager,
    signal::SignalGenerator,
};

async fn run_paper_trading(signal_generator: Box<dyn SignalGenerator + Send>) -> anyhow::Result<()> {
    // Create the paper trading executor
    let paper_executor = Arc::new(PaperTradingOrderExecutor::new());

    // Optionally add Prometheus metrics
    let registry = prometheus::Registry::new();
    let metrics = PaperTradingMetrics::new(&registry)?;
    let paper_executor = Arc::new(
        PaperTradingOrderExecutor::new()
            .with_metrics(metrics)
    );

    // Create position manager (same as live mode)
    let position_manager = InMemoryPositionManager::new(/* config */);

    // Build TradeServer with paper executor as the order executor
    // TradeServer automatically detects fill simulation capability via simulates_fills()
    let trade_server = TradeServer::builder()
        .with_event_coordinator(event_coordinator)
        .with_position_manager(Box::new(position_manager))
        .with_signal_generators(vec![signal_generator])
        .with_order_executor(paper_executor)
        .build()?;

    // Run the event loop (same as live mode)
    trade_server.run().await
}
```

### Configuration

Paper trading can be configured in your config file:

```yaml
execution:
  mode:
    PaperTrading:
      # Prevent selling more than available inventory (default: true)
      enforce_inventory: true
```

Or programmatically:

```rust
use trade_server::execution::paper::PaperTradingConfig;

let config = PaperTradingConfig {
    enforce_inventory_constraints: true,  // No naked shorting
    latency_config: None,                 // No latency simulation
};
```

## PaperTradingOrderExecutor

The paper trading executor maintains pending orders in memory and simulates fills based on live trade events.

### Key Methods

```rust
impl PaperTradingOrderExecutor {
    /// Create executor with default configuration (inventory constraints enabled, no latency)
    pub fn new() -> Self;

    /// Create executor with custom fill simulation configuration
    pub fn with_config(config: PaperTradingConfig) -> Self;

    /// Add Prometheus metrics (builder pattern)
    pub fn with_metrics(self, metrics: PaperTradingMetrics) -> Self;

    /// Get all pending orders (for inspection/debugging)
    pub async fn get_pending_orders(&self) -> Vec<PendingPaperOrder>;

    /// Get current pending order count
    pub async fn pending_order_count(&self) -> usize;

    /// Get the fill simulation configuration
    pub fn config(&self) -> &PaperTradingConfig;
}
```

### OrderExecutor Trait - Fill Simulation Methods

The paper executor implements these `OrderExecutor` trait methods for fill simulation:

```rust
/// Check for fills when a live trade event is received.
/// Called automatically by TradeServer when simulates_fills() returns true.
async fn check_fills_from_trade(
    &self,
    trade: &PolymarketTradeEvent,
    inventory: &HashMap<String, f64>,
) -> Vec<LimitOrderEvent>;

/// Returns true - indicates this executor simulates fills from trade events.
/// TradeServer uses this to determine whether to route trade events for fill detection.
fn simulates_fills(&self) -> bool;
```

### OrderExecutor Trait Implementation

The paper executor implements the `OrderExecutor` trait:

| Method | Behavior |
|--------|----------|
| `execute_limit_order` | Adds order to pending orders, returns `LimitOrderEvent::OrderPlaced` |
| `cancel_order` | Records cancel intent in lifecycle state; terminal cancellation is emitted on lifecycle confirmation (immediate with no latency, deferred when latency simulation is enabled) |
| `execute_market_order` | Returns `OrderRejected` (paper trading only supports limit orders) |
| `execute_redemption` | Simulates pair redemption, returns `RedemptionEvent::RedemptionCompleted` |
| `supports_limit_orders` | Returns `true` |
| `supports_redemption` | Returns `true` |

## Fill Simulation

Paper trading uses the same fill logic as backtesting:

### How Orders Fill

- **BID (buy) orders** fill when a SELL trade crosses at or below the bid price
- **ASK (sell) orders** fill when a BUY trade crosses at or above the ask price
- Orders fill at the **order price**, not the trade price
- Fill size is capped by `min(order.remaining_size, trade.size)`
- Partial fills are supported

### Example

```
Your bid order: price=0.48, size=100

Live trade arrives: side=SELL, price=0.47, size=50
  -> Your order fills 50 at 0.48 (your price, not trade price)
  -> 50 remaining on your order

Live trade arrives: side=SELL, price=0.48, size=60
  -> Your order fills remaining 50 at 0.48
  -> Order fully filled
```

### Inventory Constraints

By default, `enforce_inventory_constraints` is enabled:

- Sell orders can only fill if there's positive inventory
- Prevents "naked shorting" which isn't realistic for most venues
- Inventory is tracked across multiple fills within the same trade event

To disable (allow naked shorts):

```rust
let executor = PaperTradingOrderExecutor::with_config(PaperTradingConfig {
    enforce_inventory_constraints: false,
    ..Default::default()
});
```

## Unified Lifecycle Contract

Paper trading uses shared `LifecycleEngine` state as the source of truth:

- Canonical states: `SubmitPending` -> `Open` -> `CancelPending` -> terminal (`Filled`/`Cancelled`/`Rejected`/`Expired`)
- Cancel requests during `SubmitPending` are preserved and executed after placement confirmation
- Fill/cancel races are serialized through lifecycle transition guards
- Terminal evidence is deduplicated so a single order cannot emit duplicate terminal outcomes
- Unknown-order cancel behavior is policy-driven (`idempotent` or `strict`)

Live Polymarket mode follows the same terminal-evidence model. Cancel command acknowledgements are
deferred via executor capability (`defers_cancel_order_events()`), and terminal cancellation is emitted
from poller/websocket lifecycle evidence rather than command-path acknowledgement.

## Latency Simulation

By default, paper trading simulates fills immediately when trades cross order prices. However, in production there are significant order-management latencies:

1. **Order placement latency (150-500ms)**: Time from deciding to place an order to it landing on the exchange orderbook
2. **Order cancellation latency (often similar)**: Time from deciding to cancel to the order actually being removed

Without latency simulation, paper trading is overly optimistic - orders fill on trades that would have occurred before the order could realistically land on the book.

### Enabling Latency Simulation

Configure latency simulation in your config file:

```yaml
execution:
  execution_mode:
    paper_trading:
      enforce_inventory: true
      latency:
        # Minimum order placement latency (ms)
        min_place_latency_ms: 150
        # Maximum order placement latency (ms)
        max_place_latency_ms: 500
        # Optional cancellation latency (ms)
        min_cancel_latency_ms: 150
        max_cancel_latency_ms: 500
```

Or programmatically:

```rust
use trade_server::config::LatencySimulationConfig;
use trade_server::execution::paper::PaperTradingConfig;

let latency_config = LatencySimulationConfig {
    min_place_latency_ms: 150,
    max_place_latency_ms: 500,
    min_cancel_latency_ms: Some(150),
    max_cancel_latency_ms: Some(500),
};

// With latency, keeping default inventory constraints
let executor = PaperTradingOrderExecutor::with_config(PaperTradingConfig {
    latency_config: Some(latency_config),
    ..Default::default()
});

// Custom config with both options
let executor = PaperTradingOrderExecutor::with_config(PaperTradingConfig {
    enforce_inventory_constraints: false,
    latency_config: Some(latency_config),
});
```

### How Latency Simulation Works

When latency simulation is enabled:

1. **Order placement**: When an order is placed at time `T`, an `eligible_for_fills_at` timestamp is computed as:
   ```
   eligible_for_fills_at = T + random(min_place_latency, max_place_latency)
   ```

2. **Fill checking**: When a trade event arrives, orders are only considered for fills if:
   ```
   trade.timestamp >= order.eligible_for_fills_at
   ```

### Example Timeline

```
Without Latency Simulation (Overly Optimistic):
t=0ms    Trade T1 occurs on exchange
t=0ms    We receive T1, generate signal, place order O1 (instant)
t=100ms  Trade T2 occurs
t=100ms  O1 can be filled by T2 if prices cross
```

```
With Latency Simulation (Realistic):
t=0ms      Trade T1 occurs on exchange
t=50ms     We receive T1 (50ms data latency)
t=50ms     Generate signal, start order placement
t=100ms    Trade T2 occurs (we miss this - order not on book yet)
t=500ms    Trade T3 occurs (we miss this too)
t=900ms    Order O1 lands on exchange (850ms total latency)
t=1000ms   Trade T4 can now fill O1
```

### Benefits

- **More accurate fill rates**: Prevents overstated fill rates in paper trading
- **Realistic profit expectations**: Strategies that appear profitable without latency may underperform with it
- **Production-ready validation**: Identify strategies that are sensitive to latency before going live

## Prometheus Metrics

When metrics are configured, the paper executor exports:

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `paper_trading_orders_placed_total` | Counter | `side` | Total limit orders placed |
| `paper_trading_orders_cancelled_total` | Counter | - | Total orders cancelled |
| `paper_trading_orders_filled_total` | Counter | `side` | Total fills (partial or full) |
| `paper_trading_fill_volume_total` | Counter | `side` | Total fill volume in base units |
| `paper_trading_fill_latency_ms` | Histogram | - | Time from placement to fill |
| `paper_trading_pending_orders` | Gauge | - | Current pending order count |

### Setting Up Metrics

```rust
use prometheus::Registry;
use trade_server::execution::paper::PaperTradingMetrics;

let registry = Registry::new();
let metrics = PaperTradingMetrics::new(&registry)?;

let executor = PaperTradingOrderExecutor::new()
    .with_metrics(metrics);
```

## Integration with TradeServer

The `TradeServer` automatically routes trade events to any executor that simulates fills:

1. Live trade event arrives via `EventCoordinator`
2. TradeServer checks `order_executor.simulates_fills()`
3. If true, builds inventory map from current positions
4. Calls `order_executor.check_fills_from_trade(trade, inventory)`
5. Converts fill events to execution events via `fill_event_to_execution_event()`
6. Processes execution events through position manager
7. Continues with signal generation

This unified approach means paper trading, backtesting, and any future simulation executor share the same fill detection flow. The `OrderExecutor` trait methods (`simulates_fills()` and `check_fills_from_trade()`) provide a clean abstraction for fill simulation.

## Comparison with Backtesting

| Aspect | Backtesting | Paper Trading |
|--------|-------------|---------------|
| Data source | Historical Parquet files | Live Redis streams |
| Processing | Batch (all events upfront) | Continuous (one event at a time) |
| Timing | As fast as possible | Real-time |
| Use case | Strategy development | Pre-deployment validation |
| Reproducibility | Deterministic | Non-deterministic |
| Fill model | Same trade-based fill logic | Same trade-based fill logic |

### When to Use Each

**Use Backtesting when:**
- Developing and iterating on strategy logic
- Need reproducible results for comparison
- Testing against specific historical scenarios
- Running parameter sweeps

**Use Paper Trading when:**
- Validating strategy on live market conditions
- Testing integration with live data feeds
- Building confidence before going live
- Monitoring strategy behavior in real-time

## Event Persistence

Paper trading can optionally persist trading events to Redis for downstream analysis. This enables real-time monitoring and historical review of paper trading sessions.

### Configuration

Enable event persistence in your config file:

```yaml
execution:
  execution_mode:
    paper_trading:
      enforce_inventory: true
      event_persistence:
        # Redis URL for event publishing
        redis_url: "redis://localhost:6379"
        # Publishing mode: list, stream, or pubsub
        mode: list
        # Fixed queue name (deterministic for downstream consumers)
        key: "paper_trading_events"
        # Optional: explicit session_id for event correlation
        # If not set, a unique session ID is auto-generated
        # session_id: "my-explicit-session"
        # Max entries before trimming (for list/stream modes)
        max_length: 100000
        # Event filtering
        filter:
          exclude_orderbook_snapshots: true
          exclude_orderbook_updates: true
          exclude_polymarket_trades: true
          exclude_timer_events: true
          exclude_token_events: true
          exclude_spot_price_events: true
        # Async channel buffer (events dropped if full)
        channel_buffer_size: 10000
```

### Programmatic Setup

```rust
use std::sync::Arc;
use trade_server::{
    config::PaperTradingConfig,
    event_coordinator::{
        create_event_coordinator_from_config,
        wrap_coordinator_with_event_persistence,
    },
};

async fn setup_paper_trading_with_events() -> anyhow::Result<()> {
    // Create base event coordinator
    let base_coordinator = Arc::new(
        create_event_coordinator_from_config(&config.redis, timer_frequency).await?
    );

    // Extract paper trading config
    let paper_config = match &config.execution.execution_mode {
        ExecutionModeConfig::PaperTrading(pc) => pc,
        _ => &PaperTradingConfig::default(),
    };

    // Wrap with event persistence (if configured)
    let coordinator = wrap_coordinator_with_event_persistence(
        base_coordinator,
        paper_config,
    ).await?;

    // Use coordinator with TradeServer
    let trade_server = TradeServer::new(
        coordinator,
        signal_generators,
        notifier,
        position_manager,
        paper_executor,
        exit_strategy,
        max_sell_failures,
        registry,
    );

    trade_server.run().await
}
```

### Events Persisted

By default, only trading-relevant events are persisted:

| Event Type | Description |
|------------|-------------|
| `Signal.*` | Generated trading signals (Entry, Exit, Modify, Cancel) |
| `LimitOrder.*` | Order lifecycle (OrderPlaced, OrderFilled, OrderCancelled, etc.) |
| `Position.*` | Position lifecycle (PositionCreated, PositionUpdated, PositionClosed) |
| `Execution.*` | Order fills and rejections |
| `Redemption.*` | Pair redemption events |

High-volume market data events are excluded by default to reduce noise:
- Orderbook snapshots (~30KB each)
- Orderbook updates
- Polymarket trade events
- Timer events
- Token events
- Spot price events

### Publishing Modes

| Mode | Redis Command | Use Case |
|------|--------------|----------|
| `list` | LPUSH + LTRIM | Queue consumption with FIFO ordering |
| `stream` | XADD + MAXLEN | Consumer groups, replay capability |
| `pubsub` | PUBLISH | Real-time broadcast to multiple subscribers |

### Session Identification

Each paper trading session uses a unique session ID included in the `session_id` field of each event. The default format is:
`{hostname}-{timestamp}-{uuid_short}`

Example event payload:
```json
{
  "session_id": "myhost-20250123-abc12345",
  "timestamp": "2025-01-23T12:00:00Z",
  "event_type": "LimitOrder.OrderPlaced",
  "data": { ... }
}
```

**Deterministic Queue Discovery**: The default queue name is fixed (`paper_trading_events`), allowing downstream consumers to connect to a single, known queue. Events from different trading sessions are differentiated by the `session_id` field in each event, eliminating the need for wildcard-based queue discovery.

You can optionally set an explicit `session_id` in the config for reproducibility:
```yaml
event_persistence:
  key: "paper_trading_events"
  session_id: "my-explicit-session"
```

### Monitoring Dropped Events

The `RedisEventCollector` tracks dropped events due to channel backpressure:

```rust
// Get collector via wrap_coordinator_with_event_persistence_ext
let wrapped = wrap_coordinator_with_event_persistence_ext(
    base_coordinator,
    paper_config,
).await?;

if let Some(collector) = &wrapped.collector {
    // Check dropped event count
    let dropped = collector.dropped_count();
    if dropped > 0 {
        warn!("Dropped {} events due to backpressure", dropped);
    }
}
```

## Common Patterns

### Running Paper Trading Alongside Live

You can run paper trading in parallel with live trading to compare performance:

```rust
// Paper trading instance
let paper_executor = Arc::new(PaperTradingOrderExecutor::new());
let paper_server = TradeServer::builder()
    .with_paper_executor(paper_executor)
    // ... same signal generator, different metrics namespace
    .build()?;

// Live trading instance
let live_server = TradeServer::builder()
    .with_order_executor(live_executor)
    // ... same signal generator
    .build()?;

// Run both
tokio::try_join!(paper_server.run(), live_server.run())?;
```

### Transitioning from Paper to Live

1. Run paper trading until confident in strategy behavior
2. Compare paper trading metrics against expectations
3. Switch configuration from `PaperTrading` to `Live`
4. Monitor closely during initial live trading

```yaml
# Paper trading config
execution:
  mode:
    PaperTrading:
      enforce_inventory: true

# Live config (switch when ready)
execution:
  mode: Live
```

## Troubleshooting

### Orders Not Filling

If paper orders aren't filling:
- Verify trade events are being received (check logs for `PolymarketTrade` events)
- Ensure order prices are realistic (trades must cross them)
- Check that the asset_id matches between orders and trades
- Review fill logic: SELL trades fill bids, BUY trades fill asks

### Inventory Constraint Issues

If sell orders aren't filling despite matching trades:
- Check that `enforce_inventory_constraints` is the desired setting
- Verify position manager has the expected inventory
- Review logs for "Skipping sell fill - no inventory available"

### Metrics Not Appearing

If Prometheus metrics aren't showing:
- Verify `with_metrics()` was called on the executor
- Check the registry is being scraped by Prometheus
- Look for metrics with the `paper_trading_` prefix

## References

- [Backtesting Guide](backtesting.md) - Historical strategy testing
- [Intent Signals Guide](intent-signals.md) - Market making with intents
- [Orderbook Trading Guide](orderbook-trading.md) - Orderbook trading concepts
- [Architecture Documentation](../architecture.md) - Overall system architecture
