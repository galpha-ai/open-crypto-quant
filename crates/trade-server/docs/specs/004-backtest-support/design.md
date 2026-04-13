# Backtest Support for Market Making Strategies

## Summary

This design enables backtesting of market making strategies using historical orderbook and trade data stored in Parquet files. The backtest simulates limit order execution using a trade-based fill model, where pending orders are filled when actual historical trades cross the order price. All events emitted during backtesting are collected and saved as JSONL for analysis, providing a complete audit trail for strategy evaluation.

## Goals

- Load historical orderbook snapshots and trade events from Parquet files
- Simulate limit order execution using realistic trade-based fill logic
- Integrate seamlessly with existing `TradeServer`, `PositionManager`, and intent-based reconciliation
- Collect all backtest events (signals, executions, position changes) and export to JSONL
- Maintain chronological event ordering across heterogeneous event types

## Non-Goals

- **Real-time simulation latency modeling**: Not modeling network latency or order acknowledgment delays
- **Partial orderbook reconstruction**: Using snapshots as-is, not reconstructing from L3 data
- **Multi-venue backtesting**: Single venue (Polymarket) per backtest run
- **Queue position modeling**: Assuming queue priority when price matches (simplified)
- **AMM backtesting**: Focus is on orderbook/CLOB venues; AMM backtest is out of scope
- **Distributed/parallel backtesting**: Single-threaded execution per market

## API

### Rust Library API

#### BacktestRunner

```rust
pub struct BacktestRunner;

impl BacktestRunner {
    /// Run a backtest with the given configuration
    pub async fn run(config: BacktestConfig) -> Result<BacktestResult>;
}

pub struct BacktestConfig {
    /// Path to Parquet file containing orderbook snapshots
    pub snapshot_path: PathBuf,
    /// Path to Parquet file containing trade events
    pub trade_path: PathBuf,
    /// Optional outcome filter (e.g., "Up", "Down")
    pub outcome_filter: Option<String>,
    /// Signal generator factory
    pub signal_generator: Box<dyn SignalGenerator>,
    /// Position manager configuration
    pub position_config: PositionConfig,
    /// Timer interval for periodic events
    pub timer_interval: Duration,
    /// Output path for JSONL event log
    pub output_path: PathBuf,
}

pub struct BacktestResult {
    /// Path to JSONL output file
    pub output_path: PathBuf,
    /// Summary metrics
    pub metrics: BacktestMetrics,
}

pub struct BacktestMetrics {
    pub total_snapshots: u64,
    pub total_trades: u64,
    pub total_signals: u64,
    pub total_orders_placed: u64,
    pub total_fills: u64,
    pub total_cancels: u64,
    pub final_pnl: f64,
    pub final_inventory: f64,
}
```

#### ParquetLoader

```rust
pub struct ParquetLoader {
    outcome_filter: Option<String>,
}

impl ParquetLoader {
    pub fn new(outcome_filter: Option<String>) -> Self;

    /// Load orderbook snapshots from Parquet file
    pub fn load_snapshots(&self, path: &Path) -> Result<Vec<OrderbookSnapshotEvent>>;

    /// Load trade events from Parquet file
    pub fn load_trades(&self, path: &Path) -> Result<Vec<PolymarketTradeEvent>>;
}
```

#### BacktestEventCollector

```rust
pub struct BacktestEventCollector;

impl BacktestEventCollector {
    pub fn new() -> Self;

    /// Record an event for later export
    pub fn record(&self, event: &SystemEvent);

    /// Write all recorded events to JSONL file
    pub fn write_to_file(&self, path: &Path) -> Result<()>;

    /// Get collected events
    pub fn events(&self) -> Vec<CollectedEvent>;
}
```

### Errors

| Error                                 | Description                                   | Handling                     |
|---------------------------------------|-----------------------------------------------|------------------------------|
| `ParquetReadError`                    | Failed to read or parse Parquet file          | Check file path and format   |
| `InvalidSchemaError`                  | Parquet schema doesn't match expected columns | Verify data export process   |
| `EmptyDataError`                      | No events found in Parquet file               | Check file contents          |
| `TimelineError`                       | Failed to merge events chronologically        | Verify timestamp consistency |
| `EventCoordinatorError::NoMoreEvents` | Backtest completed                            | Normal termination           |

## Behavior

### Backtest Initialization

Given a `BacktestConfig`:

1. Create `ParquetLoader` with optional outcome filter
2. Load orderbook snapshots from `snapshot_path`
3. Load trade events from `trade_path`
4. Create `BacktestTimeline` by merging snapshots and trades chronologically
5. Initialize `BacktestEventCoordinator` with timeline
6. Initialize `BacktestOrderExecutor` (uses trade-based fill logic)
7. Initialize `InMemoryPositionManager`
8. Initialize `BacktestEventCollector`
9. Create `TradeServer` with all components

### Event Processing Loop

For each tick in the timeline:

1. **Emit Orderbook Snapshot**
   - `BacktestEventCoordinator.next_event()` returns `SystemEvent::MarketData(OrderbookSnapshot)`
   - `OrderbookTracker` updates internal state
   - Signal generators process snapshot and may emit `OrderIntent` signals

2. **Process Intent Signals**
   - For each intent signal:
     - `PositionManager.reconcile_intent()` computes required orders (cancels + placements)
     - For cancel orders: `BacktestOrderExecutor.cancel_order()` removes from pending
     - For limit orders: `BacktestOrderExecutor.execute_limit_order()` adds to pending
     - Emit `LimitOrderEvent::OrderPlaced` or `LimitOrderEvent::OrderCancelled`

3. **Process Trade Events**
   - For each trade in the tick's trade buffer:
     - `BacktestOrderExecutor.handle_polymarket_trade()` checks for fills
     - Trade-based fill logic evaluates pending orders against trade price
     - For matching orders: emit `LimitOrderEvent::OrderFilled` or `OrderPartiallyFilled`
     - `PositionManager.handle_limit_order_event()` updates position state

4. **Process Timer Events**
   - At configured intervals, emit `TimerEvent`
   - Position manager checks safety limits (max holding period)
   - May trigger emergency market exits if limits exceeded

5. **Record Events**
   - All events are recorded by `BacktestEventCollector`

6. **Repeat** until `NoMoreEvents`

### Fill Simulation Logic (Trade-Based)

Orders are filled when historical trades cross the order price. Given a pending order and incoming trade:

**For BID orders (we're buying):**
```
IF trade.side == SELL AND trade.price <= bid.price THEN
    Fill at bid.price (our order's price)
    Fill size = min(order.remaining_size, trade.size)
```

**For ASK orders (we're selling):**
```
IF trade.side == BUY AND trade.price >= ask.price THEN
    Fill at ask.price (our order's price)
    Fill size = min(order.remaining_size, trade.size)
```

Rationale: When a SELL trade occurs, a seller is hitting bids. If our bid is at or above the trade price, we assume queue priority and get filled. Similarly for BUY trades lifting asks. Partial fills are supported based on actual trade size.

### Event Collection and Output

After backtest completes:

1. `BacktestEventCollector.write_to_file()` writes all events
2. Output format: JSONL (one JSON object per line)
3. Each line contains:
   - `timestamp`: ISO 8601 timestamp
   - `event_type`: String identifier (e.g., "Signal", "LimitOrder", "Position")
   - `data`: Full event payload as JSON

## Data Model

### Parquet Schema: Orderbook Snapshots

| Column     | Type          | Description                       |
|------------|---------------|-----------------------------------|
| `ts`       | TIMESTAMP     | Snapshot timestamp                |
| `ticker`   | STRING        | Market ticker identifier          |
| `end_date` | TIMESTAMP     | Market end/maturity date          |
| `outcome`  | STRING        | Outcome name (e.g., "Up", "Down") |
| `bids`     | STRING (JSON) | Array of {price, size} objects    |
| `asks`     | STRING (JSON) | Array of {price, size} objects    |

### Parquet Schema: Trade Events

| Column    | Type      | Description                      |
|-----------|-----------|----------------------------------|
| `ts`      | TIMESTAMP | Trade timestamp                  |
| `ticker`  | STRING    | Market ticker identifier         |
| `outcome` | STRING    | Outcome name                     |
| `side`    | STRING    | "BUY" or "SELL" (aggressor side) |
| `price`   | FLOAT64   | Trade price                      |
| `size`    | FLOAT64   | Trade size                       |

### PendingBacktestOrder

Internal representation of a pending limit order:

| Field            | Type           | Description             |
|------------------|----------------|-------------------------|
| `order_id`       | String         | Unique order identifier |
| `mint`           | String         | Asset ID                |
| `market`         | Option<String> | Market/condition ID     |
| `side`           | OrderSide      | Buy or Sell             |
| `price`          | f64            | Limit price             |
| `original_size`  | f64            | Original order size     |
| `remaining_size` | f64            | Unfilled size           |
| `placed_at`      | DateTime<Utc>  | Placement timestamp     |
| `time_in_force`  | TimeInForce    | GTC, IOC, FOK           |
| `signal_id`      | Option<String> | Originating signal ID   |

### CollectedEvent (JSONL Output)

| Field        | Type              | Description           |
|--------------|-------------------|-----------------------|
| `timestamp`  | String (ISO 8601) | Event timestamp       |
| `event_type` | String            | Event type identifier |
| `data`       | Object            | Full event payload    |

## Configuration

```rust
pub struct BacktestConfig {
    /// Path to orderbook snapshots Parquet file
    pub snapshot_path: PathBuf,

    /// Path to trade events Parquet file
    pub trade_path: PathBuf,

    /// Filter events by outcome (e.g., "Up", "Down")
    /// None means process all outcomes
    pub outcome_filter: Option<String>,

    /// Position manager configuration
    pub position: PositionConfig,

    /// Timer event interval
    pub timer_interval: Duration,

    /// Output path for JSONL event log
    pub output_path: PathBuf,
}

pub struct PositionConfig {
    /// Initial balance in quote currency
    pub initial_balance: f64,
    /// Maximum open positions
    pub max_open_positions: u32,
    /// Maximum holding period before forced exit
    pub max_holding_period: Duration,
    /// Trade amount per order
    pub trade_amount: f64,
}
```

Example configuration:

```rust
let config = BacktestConfig {
    snapshot_path: PathBuf::from("data/snapshots.parquet"),
    trade_path: PathBuf::from("data/trades.parquet"),
    outcome_filter: Some("Up".to_string()),
    position: PositionConfig {
        initial_balance: 10000.0,
        max_open_positions: 10,
        max_holding_period: Duration::from_secs(3600),
        trade_amount: 100.0,
    },
    timer_interval: Duration::from_secs(1),
    output_path: PathBuf::from("output/backtest_events.jsonl"),
};
```

## BacktestOrderExecutor

The enhanced `BacktestOrderExecutor` is the core component for simulating order execution.

### State Management

```rust
pub struct BacktestOrderExecutor {
    /// Price tracking for market orders
    last_prices: Arc<Mutex<HashMap<String, f64>>>,

    /// Slippage for market orders
    buy_slippage: f64,
    sell_slippage: f64,

    /// Pending limit orders awaiting fill
    pending_orders: Arc<Mutex<HashMap<String, PendingBacktestOrder>>>,

    /// Event coordinator for enqueueing fill events
    event_coordinator: Arc<dyn EventCoordinator>,
}
```

### Order Lifecycle

1. **Placement**: `execute_limit_order()` adds order to `pending_orders`, returns `OrderPlaced`
2. **Fill Check**: `handle_polymarket_trade()` checks all pending orders against each trade using trade-based fill logic
3. **Fill**: On price match, fill size is `min(order.remaining_size, trade.size)`. Emit `OrderPartiallyFilled` if remaining > 0, otherwise remove and emit `OrderFilled`
4. **Cancel**: `cancel_order()` removes from `pending_orders`, returns `OrderCancelled`
5. **Expiry**: Timer events can check and expire orders based on `time_in_force`

### Integration with TradeServer

The executor implements the `OrderExecutor` trait:

```rust
#[async_trait]
impl OrderExecutor for BacktestOrderExecutor {
    async fn execute_market_order(&self, order: Order) -> Result<ExecutionEvent>;
    async fn execute_limit_order(&self, order: Order) -> Result<LimitOrderEvent>;
    async fn cancel_order(&self, order_id: &str) -> Result<LimitOrderEvent>;
    async fn handle_orderbook_snapshot(&self, snapshot: &OrderbookSnapshotEvent) -> Result<()>;
    async fn handle_orderbook_update(&self, update: &OrderbookUpdateEvent) -> Result<()>;
    fn supports_limit_orders(&self) -> bool { true }
}
```

## BacktestEventCoordinator

Coordinates event delivery during backtest, ensuring chronological ordering.

### Event Priority

Events are delivered in this priority order:
1. Enqueued events (fill events, position events)
2. Buffered trade events for current tick
3. Timer events (if interval elapsed)
4. Next orderbook snapshot (advances to next tick)

### Timeline Management

```rust
pub struct BacktestEventCoordinator {
    timeline: BacktestTimeline,
    current_tick: usize,
    trade_buffer: VecDeque<PolymarketTradeEvent>,
    enqueued_events: VecDeque<SystemEvent>,
    timer_state: TimerState,
}
```

The timeline groups events by tick (snapshot + associated trades):

```rust
pub struct BacktestTick {
    pub snapshot: OrderbookSnapshotEvent,
    pub trades: Vec<PolymarketTradeEvent>,
}
```

## Idempotency & Concurrency

### Backtest Execution

- Backtests run single-threaded; no concurrency concerns during simulation
- Each backtest run is independent and reproducible given same inputs
- Order IDs are generated deterministically to ensure reproducibility

### Event Collection

- Event collector uses interior mutability (`Arc<Mutex<Vec<_>>>`)
- Events are appended atomically
- Final write to JSONL is single-threaded after backtest completes

### Fill Simulation

- Fill checks are deterministic: same pending orders + same trade = same fills
- Partial fills update `remaining_size` atomically
- Order removal on full fill is atomic

## Acceptance Criteria

### Data Loading
- [ ] Successfully load orderbook snapshots from Parquet file
- [ ] Successfully load trade events from Parquet file
- [ ] Filter events by outcome when specified
- [ ] Handle malformed Parquet data gracefully with clear error messages
- [ ] Support large files (100MB+) without memory issues

### Event Coordination
- [ ] Emit events in strict chronological order
- [ ] Correctly merge snapshots and trades by timestamp
- [ ] Deliver timer events at configured intervals
- [ ] Signal `NoMoreEvents` when timeline exhausted

### Fill Simulation
- [ ] BID orders fill when SELL trades cross at or below bid price
- [ ] ASK orders fill when BUY trades cross at or above ask price
- [ ] Fill at order price, not trade price
- [ ] Track partial fills correctly with `remaining_size`
- [ ] Emit `OrderPartiallyFilled` for partial fills, `OrderFilled` for complete fills

### Integration
- [ ] Intent signals correctly reconciled to cancel/place orders
- [ ] Position state updated correctly from fill events
- [ ] Metrics (PnL, inventory) calculated correctly
- [ ] Safety limits (max holding period) enforced

### Event Collection
- [ ] All events captured: Signal, LimitOrder, Position, Execution
- [ ] JSONL output is valid JSON per line
- [ ] Events in output are chronologically ordered
- [ ] Event data fully serializable and deserializable

### End-to-End
- [ ] Complete backtest of sample market data produces valid results
- [ ] Results reproducible: same input = same output
- [ ] Performance: process 100k+ events in < 60 seconds

## Observability

### Logs

| Event              | Level | Key Fields                            |
|--------------------|-------|---------------------------------------|
| Backtest started   | INFO  | config path, outcome filter           |
| Snapshot loaded    | DEBUG | count, date range                     |
| Trades loaded      | DEBUG | count, date range                     |
| Tick processed     | TRACE | tick index, snapshot ts, trade count  |
| Order placed       | DEBUG | order_id, side, price, size           |
| Order filled       | INFO  | order_id, fill_price, fill_size       |
| Order cancelled    | DEBUG | order_id, reason                      |
| Position updated   | DEBUG | mint, amount, pnl                     |
| Backtest completed | INFO  | duration, total events, final metrics |

### No Prometheus Metrics

Backtest mode does **not** use Prometheus metrics. Prometheus metrics are designed for real-time trading observability and are not relevant for offline backtesting. Instead:

- **Summary statistics** are returned in `BacktestMetrics` after the run completes
- **Detailed event data** is exported to JSONL for post-hoc analysis
- **Logs** provide visibility into backtest progress

### Output Analysis

The JSONL output enables post-hoc analysis:
- Reconstruct order book state at any point
- Analyze fill rates and slippage
- Compute Sharpe ratio, max drawdown
- Identify strategy behavior patterns

## Security

- **File Access**: Only reads specified Parquet files; no network access
- **Output**: Writes only to specified output path
- **No Secrets**: Configuration contains no secrets or credentials
- **Input Validation**: Parquet schema validated before processing
- **Resource Limits**: Memory-bounded loading; large files processed in chunks

## References

- [Task Tracker](./tasks.md) - Implementation task list and progress tracking
- [Architecture Documentation](../../architecture.md) - Overall trade server architecture
- [Intent Signals Guide](../../usage-guide/intent-signals.md) - Intent-based signal documentation
- [Orderbook Trading Guide](../../usage-guide/orderbook-trading.md) - Orderbook trading documentation
- [Python Backtest Reference](/home/zfeng/popeyes/polysharp/src/polysharp/backtest/) - Reference Python implementation
- [popeyes_trading_types](/home/zfeng/popeyes/trading-types/) - Shared type definitions
