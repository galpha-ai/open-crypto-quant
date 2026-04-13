# Backtesting Guide

This guide explains how to use the trade server's backtesting infrastructure to evaluate trading strategies against historical data.

## Overview

The backtest system allows you to:
- Replay historical orderbook snapshots and trade events
- Test signal generators against historical data
- Simulate limit order fills using a trade-based model
- Collect all events for post-hoc analysis

## Prerequisites

### Parquet Data Files

You need Parquet files containing historical data:

**1. Orderbook Snapshots (`snapshots.parquet`)**

| Column   | Type          | Description                       |
|----------|---------------|-----------------------------------|
| ts       | INT64         | Timestamp in milliseconds         |
| ticker   | STRING        | Market ticker identifier          |
| outcome  | STRING        | Outcome name (e.g., "Up", "Down") |
| bids     | STRING (JSON) | Array of {price, size} objects    |
| asks     | STRING (JSON) | Array of {price, size} objects    |
| end_date | STRING        | Market end/maturity date          |

Example bids/asks JSON format:
```json
[{"price": 0.50, "size": 100.0}, {"price": 0.49, "size": 200.0}]
```

**2. Orderbook Updates (`updates.parquet`)**

| Column   | Type    | Description                      |
|----------|---------|----------------------------------|
| ts       | INT64   | Timestamp in milliseconds        |
| asset_id | STRING  | Asset identifier                 |
| market   | STRING  | Market identifier                |
| price    | FLOAT64 | Price level                      |
| size     | FLOAT64 | Size at this level               |
| side     | STRING  | "BUY" or "SELL"                  |
| hash     | STRING  | Orderbook hash                   |
| best_bid | FLOAT64 | Current best bid                 |
| best_ask | FLOAT64 | Current best ask                 |
| source   | STRING  | Source identifier                |
| event_id | STRING  | Event identifier                 |
| ticker   | STRING  | Market ticker                    |
| title    | STRING  | Market title                     |
| end_date | STRING  | Market end/maturity date         |
| outcome  | STRING  | Outcome name                     |

**3. Trade Events (`trades.parquet`)**

| Column  | Type    | Description                      |
|---------|---------|----------------------------------|
| ts      | INT64   | Timestamp in milliseconds        |
| ticker  | STRING  | Market ticker identifier         |
| outcome | STRING  | Outcome name                     |
| side    | STRING  | "BUY" or "SELL" (aggressor side) |
| price   | FLOAT64 | Trade price                      |
| size    | FLOAT64 | Trade size                       |

## Basic Usage

### Running a Backtest

```rust
use std::{path::PathBuf, time::Duration};
use trade_server::backtest::{BacktestConfig, BacktestRunner, PositionConfig};
use trade_server::signal::SignalGenerator;

async fn run_backtest(signal_generator: Box<dyn SignalGenerator + Send>) -> anyhow::Result<()> {
    // Configure the backtest
    let config = BacktestConfig::new(
        PathBuf::from("data/snapshots.parquet"),
        PathBuf::from("data/updates.parquet"),
        PathBuf::from("data/trades.parquet"),
        PathBuf::from("output/backtest_events.jsonl"),
    )
    .with_outcome_filter("Up")  // Filter for specific outcome
    .with_ticker_patterns(vec!["btc-updown-15m-1768780800".to_string()])
    .with_ticker_time_range_filter(true) // Disable if tickers are not Polymarket binary format
    .with_ticker_time_range_buffer(Duration::from_secs(300))
    .with_timer_interval(Duration::from_secs(1))
    .with_position_config(PositionConfig::new(
        10000.0,                    // initial_balance
        10,                         // max_open_positions
        Duration::from_secs(3600),  // max_holding_period
        100.0,                      // trade_amount
    ));

    // Run the backtest
    let result = BacktestRunner::run(config, signal_generator).await?;

    // Print results
    println!("Backtest completed:");
    println!("  Snapshots processed: {}", result.metrics.total_snapshots);
    println!("  Trades processed: {}", result.metrics.total_trades);
    println!("  Signals generated: {}", result.metrics.total_signals);
    println!("  Orders placed: {}", result.metrics.total_orders_placed);
    println!("  Orders filled: {}", result.metrics.total_fills);
    println!("  Net cash flow: {:.2}", result.metrics.net_cash_flow);
    println!("  Final inventory: {:.4}", result.metrics.final_inventory);
    println!("  Final inventory value: {:.2}", result.metrics.final_inventory_value);
    println!("  Final PnL: {:.2}", result.metrics.final_pnl);
    println!("  Duration: {}ms", result.metrics.duration_ms);
    println!("  Output: {}", result.output_path.display());

    Ok(())
}
```

## Configuration Options

### BacktestConfig

```rust
pub struct BacktestConfig {
    /// Path to Parquet file containing orderbook snapshots
    pub snapshot_path: PathBuf,

    /// Path to Parquet file containing orderbook updates
    pub update_path: PathBuf,

    /// Path to Parquet file containing trade events
    pub trade_path: PathBuf,

    /// Filter events by outcome (e.g., "Up", "Down")
    /// None means process all outcomes
    pub outcome_filter: Option<String>,

    /// Filter events by ticker patterns (glob patterns supported)
    pub ticker_patterns: Option<Vec<String>>,

    /// Enable time-range filtering derived from exact Polymarket tickers
    pub ticker_time_range_filter: bool,

    /// Buffer applied to derived time-range filter
    pub ticker_time_range_buffer: Duration,

    /// Position manager configuration
    pub position: PositionConfig,

    /// Timer event interval (for checking exit conditions)
    pub timer_interval: Duration,

    /// Output path for JSONL event log
    pub output_path: PathBuf,

    /// Buy slippage for market orders (0.0 to 1.0)
    pub buy_slippage: f64,

    /// Sell slippage for market orders (0.0 to 1.0)
    pub sell_slippage: f64,
}
```

### PositionConfig

```rust
pub struct PositionConfig {
    /// Initial balance in quote currency (e.g., USDC)
    pub initial_balance: f64,

    /// Maximum number of open positions allowed
    pub max_open_positions: u32,

    /// Maximum holding period before forced exit
    pub max_holding_period: Duration,

    /// Trade amount per order (in quote currency)
    pub trade_amount: f64,

    /// Take profit threshold (e.g., 0.2 for 20%)
    pub take_profit_threshold: f64,

    /// Stop loss threshold (e.g., 0.1 for 10%)
    pub stop_loss_threshold: f64,

    /// Maximum sell failures before giving up
    pub max_sell_failures: u32,
}
```

### Builder Pattern

Configuration supports a builder pattern for cleaner setup:

```rust
let config = BacktestConfig::new(snapshot_path, update_path, trade_path, output_path)
    .with_outcome_filter("Up")
    .with_ticker_time_range_filter(false)
    .with_timer_interval(Duration::from_secs(5))
    .with_slippage(0.001, 0.001)  // 0.1% slippage
    .with_position_config(
        PositionConfig::new(10000.0, 10, Duration::from_secs(3600), 100.0)
            .with_take_profit(0.15)  // 15% take profit
            .with_stop_loss(0.05)    // 5% stop loss
    );
```

## Implementing a Signal Generator

To backtest your strategy, implement the `SignalGenerator` trait:

```rust
use async_trait::async_trait;
use anyhow::Result;
use chrono::Utc;
use popeyes_trading_types::MarketDataEvent;
use trade_server::{
    domain::SystemEvent,
    execution::TimeInForce,
    signal::{SignalGenerator, TradableSignal},
};

/// A simple market making signal generator
pub struct SimpleMarketMaker {
    spread: f64,  // Distance from mid to place orders
    size: f64,    // Order size
}

impl SimpleMarketMaker {
    pub fn new(spread: f64, size: f64) -> Self {
        Self { spread, size }
    }
}

#[async_trait]
impl SignalGenerator for SimpleMarketMaker {
    async fn generate_signal(
        &mut self,
        event: &SystemEvent,
    ) -> Result<Vec<Box<dyn TradableSignal>>> {
        match event {
            SystemEvent::MarketData(MarketDataEvent::OrderbookSnapshot(snapshot)) => {
                // Calculate mid price from orderbook
                let best_bid = snapshot.bids.first().map(|o| o.price).unwrap_or(0.0);
                let best_ask = snapshot.asks.first().map(|o| o.price).unwrap_or(1.0);
                let mid = (best_bid + best_ask) / 2.0;

                // Generate bid and ask signals
                let bid_signal = MarketMakerSignal {
                    signal_id: format!("bid-{}", snapshot.timestamp),
                    mint: snapshot.asset_id.clone(),
                    market: Some(snapshot.market.clone()),
                    price: mid - self.spread,
                    size: self.size,
                    is_buy: true,
                    timestamp: chrono::DateTime::from_timestamp_millis(snapshot.timestamp)
                        .unwrap_or_else(Utc::now),
                };

                let ask_signal = MarketMakerSignal {
                    signal_id: format!("ask-{}", snapshot.timestamp),
                    mint: snapshot.asset_id.clone(),
                    market: Some(snapshot.market.clone()),
                    price: mid + self.spread,
                    size: self.size,
                    is_buy: false,
                    timestamp: chrono::DateTime::from_timestamp_millis(snapshot.timestamp)
                        .unwrap_or_else(Utc::now),
                };

                Ok(vec![Box::new(bid_signal), Box::new(ask_signal)])
            }
            _ => Ok(vec![]),
        }
    }
}

/// A limit order signal for market making
#[derive(Debug, Clone)]
struct MarketMakerSignal {
    signal_id: String,
    mint: String,
    market: Option<String>,
    price: f64,
    size: f64,
    is_buy: bool,
    timestamp: chrono::DateTime<Utc>,
}

impl TradableSignal for MarketMakerSignal {
    fn signal_id(&self) -> &str { &self.signal_id }
    fn signal_type(&self) -> &str { "MarketMaker" }
    fn get_mint(&self) -> Option<&str> { Some(&self.mint) }
    fn get_price(&self) -> Option<f64> { Some(self.price) }
    fn get_timestamp(&self) -> Option<chrono::DateTime<Utc>> { Some(self.timestamp) }
    fn get_slot(&self) -> Option<u64> { None }
    fn get_creator(&self) -> Option<solana_sdk::pubkey::Pubkey> { None }
    fn passes_filter(&self) -> bool { true }
    fn as_notifiable(&self) -> Option<Box<dyn trade_server::notifier::Notifiable + Send + Sync>> { None }
    fn as_any(&self) -> &dyn std::any::Any { self }

    fn to_json(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::json!({
            "signal_id": self.signal_id,
            "mint": self.mint,
            "market": self.market,
            "price": self.price,
            "size": self.size,
            "is_buy": self.is_buy,
        }))
    }

    // Limit order specific methods
    fn get_market(&self) -> Option<&str> { self.market.as_deref() }
    fn get_limit_price(&self) -> Option<f64> { Some(self.price) }
    fn is_limit_order(&self) -> bool { true }
    fn get_time_in_force(&self) -> Option<TimeInForce> {
        Some(TimeInForce::GoodTilCancelled)
    }
}
```

## Understanding the Output

### JSONL Event Log

The backtest outputs a JSONL (JSON Lines) file where each line is a complete event:

```json
{"timestamp":"2024-01-15T10:00:00Z","event_type":"MarketData.OrderbookSnapshot","data":{"asset_id":"TEST-Up","market":"TEST","bids":[{"price":0.49,"size":1000}],"asks":[{"price":0.51,"size":1000}]}}
{"timestamp":"2024-01-15T10:00:01Z","event_type":"LimitOrder.OrderPlaced","data":{"order_id":"bt-order-1","mint":"TEST-Up","price":0.48,"size":100,"side":"Buy"}}
{"timestamp":"2024-01-15T10:00:02Z","event_type":"MarketData.PolymarketTrade","data":{"asset_id":"TEST-Up","price":0.48,"size":50,"side":"Sell"}}
{"timestamp":"2024-01-15T10:00:02Z","event_type":"LimitOrder.OrderPartiallyFilled","data":{"order_id":"bt-order-1","filled_size":50,"remaining_size":50,"fill_price":0.48}}
```

### Event Types

| Event Type | Description |
|------------|-------------|
| `MarketData.OrderbookSnapshot` | Orderbook state at a point in time |
| `MarketData.PolymarketTrade` | Historical trade event |
| `Timer` | Periodic timer tick |
| `Signal.*` | Trading signals generated by your strategy |
| `LimitOrder.OrderPlaced` | Limit order placed |
| `LimitOrder.OrderPartiallyFilled` | Limit order (partially) filled |
| `LimitOrder.OrderCancelled` | Limit order cancelled |
| `Execution` | Market order filled |
| `Position` | Position created/updated/closed |
| `Redemption.RedemptionCompleted` | Pair redemption completed successfully |
| `Redemption.RedemptionFailed` | Pair redemption failed |

`LimitOrder.OrderCancelled` is terminal lifecycle evidence. With latency simulation enabled,
cancel commands are deferred internally and this event is emitted only when cancellation is
confirmed by lifecycle processing.

### BacktestMetrics

```rust
pub struct BacktestMetrics {
    pub total_snapshots: u64,        // Number of orderbook snapshots processed
    pub total_trades: u64,           // Number of trade events processed
    pub total_signals: u64,          // Number of signals generated
    pub total_orders_placed: u64,    // Number of orders placed
    pub total_fills: u64,            // Number of fills (partial + full)
    pub total_cancels: u64,          // Number of cancellations
    pub total_quote_received: f64,   // Cumulative quote from sells
    pub total_quote_spent: f64,      // Cumulative quote spent on buys
    pub net_cash_flow: f64,          // Quote received - quote spent
    pub final_inventory: f64,        // Net position size at end
    pub final_inventory_value: f64,  // Mark-to-market value of inventory
    pub final_pnl: f64,              // Total PnL (net_cash_flow + inventory_value)
    pub duration_ms: u64,            // Backtest execution time
    pub data_start_time: Option<DateTime<Utc>>,  // First event timestamp
    pub data_end_time: Option<DateTime<Utc>>,    // Last event timestamp
    pub total_redemptions: u64,      // Number of pair redemptions executed
    pub total_redemption_value: f64, // Quote currency received from redemptions
}
```

The `final_pnl` metric uses cash-flow-based PnL calculation, which correctly accounts for spread capture in market-making strategies. Unlike position-close-based PnL (which only updates when positions fully close), cash-flow PnL tracks cumulative quote currency flows on every fill.

For binary market strategies using pair redemption, `total_redemptions` and `total_redemption_value` track redemption activity. See [Intent Signals - Pair Redemption](intent-signals.md#pair-redemption-for-binary-markets) for more details.

## Fill Simulation

The backtest uses a **trade-based fill model** for realistic limit order simulation. This fill simulation logic is implemented via the `OrderExecutor` trait methods, shared with paper trading:

- `check_fills_from_trade(trade, inventory)` - Checks pending orders against a trade event
- `simulates_fills()` - Returns `true` for backtest/paper executors
- `fill_event_to_execution_event()` - Converts fill events to execution events

This unified approach ensures consistent fill behavior between backtesting and paper trading.

## Unified Lifecycle Contract

Backtest limit orders follow the same lifecycle authority used by paper/live paths:

- Canonical states: `SubmitPending` -> `Open` -> `CancelPending` -> terminal (`Filled`/`Cancelled`/`Rejected`/`Expired`)
- Cancel during `SubmitPending` is queued and preserved until placement confirmation
- Fill-before-cancel-finalization races are valid and serialized through lifecycle guards
- Duplicate cancel/terminal evidence is deduplicated by lifecycle state
- Unknown-order cancel follows configured policy (`idempotent` or `strict`)

### How Orders Fill

- **BID (buy) orders** fill when a SELL trade crosses at or below the bid price
- **ASK (sell) orders** fill when a BUY trade crosses at or above the ask price
- Orders fill at the **order price**, not the trade price
- Fill size is capped by `min(order.remaining_size, trade.size)`
- Partial fills are supported when trade size is smaller than order size

### Example

```
Your bid order: price=0.48, size=100

Historical trade: side=SELL, price=0.47, size=50
  -> Your order fills 50 at 0.48 (your price, not trade price)
  -> 50 remaining on your order

Historical trade: side=SELL, price=0.48, size=60
  -> Your order fills remaining 50 at 0.48
  -> Order fully filled
```

## Latency Simulation

By default, backtests simulate fills immediately when trades cross order prices. However, in production there are significant order-management latencies that can affect fill rates:

1. **Order placement latency (150-500ms)**: Time from deciding to place an order to it landing on the exchange orderbook
2. **Order cancellation latency (often similar)**: Time from deciding to cancel to the order actually being removed

Without latency simulation, backtests are overly optimistic - orders fill on trades that would have occurred before the order could realistically land on the book.

### Enabling Latency Simulation

Configure latency simulation in your backtest config:

```rust
use trade_server::config::LatencySimulationConfig;
use trade_server::backtest::BacktestConfig;

let latency_config = LatencySimulationConfig {
    min_place_latency_ms: 150,
    max_place_latency_ms: 500,
    min_cancel_latency_ms: Some(150),
    max_cancel_latency_ms: Some(500),
};

let config = BacktestConfig::new(snapshot_path, trade_path, output_path)
    .with_latency(latency_config);
```

Or in YAML configuration:

```yaml
backtest:
  snapshot_path: "data/snapshots.parquet"
  trade_path: "data/trades.parquet"
  output_path: "output/backtest_events.jsonl"
  latency:
    min_place_latency_ms: 150
    max_place_latency_ms: 500
    min_cancel_latency_ms: 150
    max_cancel_latency_ms: 500
```

### How Latency Simulation Works

When latency simulation is enabled:

1. **Order placement**: When an order is placed at time `T`, an `eligible_for_fills_at` timestamp is computed:
   ```
   eligible_for_fills_at = T + random(min_place_latency, max_place_latency)
   ```

2. **Fill checking**: When a trade event is processed, orders are only considered for fills if:
   ```
   trade.timestamp >= order.eligible_for_fills_at
   ```

3. **Cancellation**: When an order is cancelled at time `T`, it remains fillable until:
   ```
   active_until = T + random(min_cancel_latency, max_cancel_latency)
   ```

### Example Timeline

```
Without Latency Simulation (Overly Optimistic):
t=0ms    Trade T1 processed, generates signal, order O1 placed
t=100ms  Trade T2 processed
t=100ms  O1 can be filled by T2 if prices cross
```

```
With Latency Simulation (Realistic):
t=0ms      Trade T1 processed (50ms data latency from actual t=-50ms)
t=0ms      Generate signal, start order placement
t=100ms    Trade T2 processed (we miss this - order not on book yet)
t=500ms    Trade T3 processed (we miss this too)
t=900ms    Order O1 eligible for fills (850ms total latency)
t=1000ms   Trade T4 can now fill O1
```

### Benefits

- **More accurate fill rates**: Prevents overstated fill rates in backtests
- **Realistic profit expectations**: Strategies that appear profitable without latency may underperform with it
- **Strategy validation**: Identify strategies that are sensitive to latency before paper trading or going live

## Performance Considerations

### Large Datasets

For large datasets (100MB+ Parquet files):
- Events are loaded into memory during initialization
- Processing is single-threaded for deterministic results
- Target: 100k+ events in < 60 seconds

### Memory Usage

- All snapshots and trades are loaded into memory
- Events are collected during the run for JSONL output
- For very large backtests, consider splitting data into chunks

### Reproducibility

Backtests are fully reproducible:
- Same input data produces identical results
- Order IDs are generated deterministically
- Single-threaded execution eliminates race conditions

## Common Patterns

### Filtering by Outcome

For multi-outcome markets (e.g., prediction markets with "Up"/"Down" outcomes):

```rust
let config = BacktestConfig::new(snapshot_path, trade_path, output_path)
    .with_outcome_filter("Up");  // Only process "Up" outcome events
```

### Analyzing Results

Process the JSONL output with external tools:

```python
import json
import pandas as pd

# Load events
events = []
with open("output/backtest_events.jsonl") as f:
    for line in f:
        events.append(json.loads(line))

# Convert to DataFrame
df = pd.DataFrame(events)

# Filter by event type
fills = df[df['event_type'].str.contains('Filled')]
print(f"Total fills: {len(fills)}")

# Calculate metrics
pnl_events = df[df['event_type'] == 'Position']
# ... analyze position events
```

### Testing Multiple Strategies

Run the same data through different signal generators:

```rust
async fn compare_strategies() -> anyhow::Result<()> {
    let snapshot_path = PathBuf::from("data/snapshots.parquet");
    let trade_path = PathBuf::from("data/trades.parquet");

    // Strategy A: Tight spread
    let config_a = BacktestConfig::new(
        snapshot_path.clone(),
        trade_path.clone(),
        PathBuf::from("output/strategy_a.jsonl"),
    );
    let result_a = BacktestRunner::run(
        config_a,
        Box::new(SimpleMarketMaker::new(0.01, 100.0)),
    ).await?;

    // Strategy B: Wide spread
    let config_b = BacktestConfig::new(
        snapshot_path,
        trade_path,
        PathBuf::from("output/strategy_b.jsonl"),
    );
    let result_b = BacktestRunner::run(
        config_b,
        Box::new(SimpleMarketMaker::new(0.02, 100.0)),
    ).await?;

    // Compare results
    println!("Strategy A PnL: {:.2}", result_a.metrics.final_pnl);
    println!("Strategy B PnL: {:.2}", result_b.metrics.final_pnl);

    Ok(())
}
```

## Signal Action Routing

The backtest runner supports the full signal action system, mirroring the behavior of the live `PositionHandler`:

### Signal Actions

Signals can indicate different actions via `signal_action()`:

| Action | Description |
|--------|-------------|
| `Entry` | Open a new position (default, backward compatible) |
| `Exit` | Close an existing position |
| `ModifyOrder` | Cancel and replace an existing limit order |
| `CancelOrder` | Cancel an existing limit order |

### Intent-Based Signals

For orderbook market making, you can use intent-based signals that express desired order book state rather than discrete actions:

```rust
impl TradableSignal for MarketMakerSignal {
    // Indicate this is an intent signal
    fn is_intent_signal(&self) -> bool { true }

    // Return the desired order book state
    fn get_order_intent(&self) -> Option<OrderIntent> {
        Some(OrderIntent {
            signal_id: self.signal_id.clone(),
            mint: self.mint.clone(),
            market: self.market.clone(),
            bids: self.desired_bids.clone(),
            asks: self.desired_asks.clone(),
            timestamp: self.timestamp,
        })
    }
}
```

The backtest runner will automatically call `reconcile_intent()` on the position manager, which diffs current pending orders against desired state and generates the necessary cancels and placements.

### Exit Signals

For strategy-managed exits (instead of automatic stop-loss/take-profit):

```rust
impl TradableSignal for ExitSignal {
    fn signal_action(&self) -> SignalAction {
        SignalAction::Exit
    }

    // Reference the position to exit
    fn references_position(&self) -> Option<&str> {
        Some(&self.mint)
    }

    // Use limit order for exit (optional)
    fn is_limit_order(&self) -> bool { true }
    fn get_limit_price(&self) -> Option<f64> { Some(self.exit_price) }
}
```

## Signal Timestamps and Logical Time

When running backtests with latency simulation, it's critical that signals use **logical time** (the timestamp from the event being processed) rather than wall-clock time (`Utc::now()`). If signals use wall-clock time, the latency simulation will compute eligibility timestamps far in the future relative to historical trade data, resulting in zero fills.

### The Problem

```rust
// WRONG: Using wall-clock time in backtest
impl SignalMetadata {
    pub fn new() -> Self {
        Self {
            timestamp: Utc::now(),  // Current time, not logical time!
        }
    }
}
```

When backtesting historical data from November 2025 in December 2025:
1. Signal created with timestamp = December 2025 (wall-clock)
2. Latency simulation adds ~1s: `eligible_for_fills_at` = December 2025 + 1s
3. Historical trades are from November 2025
4. Fill check: `November < December` → all fills skipped!

### The Solution

Use `SignalMetadata::with_timestamp()` to create signals with logical time derived from the event:

```rust
use trade_server::signal::SignalMetadata;

pub struct MySignalGenerator {
    /// Current logical time derived from events.
    current_time: DateTime<Utc>,
}

impl MySignalGenerator {
    /// Update logical time from event timestamp.
    fn update_time_from_event(&mut self, event: &SystemEvent) {
        if let Some(ts) = event.timestamp() {
            self.current_time = ts;
        }
    }
}

#[async_trait]
impl SignalGenerator for MySignalGenerator {
    async fn generate_signal(
        &mut self,
        event: &SystemEvent,
    ) -> Result<Vec<Box<dyn TradableSignal>>> {
        // Update logical time from event
        self.update_time_from_event(event);

        // Create signal with logical time
        let meta = SignalMetadata::with_timestamp(self.current_time);

        // ... generate signals using meta ...
    }
}
```

### API Reference

#### `SignalMetadata::with_timestamp(timestamp: DateTime<Utc>)`

Creates signal metadata with a specific timestamp instead of wall-clock time.

```rust
// Create with logical time (for backtests)
let meta = SignalMetadata::with_timestamp(event_timestamp);

// Create with wall-clock time (for live trading, default behavior)
let meta = SignalMetadata::new();
```

#### `SystemEvent::timestamp() -> Option<DateTime<Utc>>`

Extracts the logical timestamp from an event. Returns `Some` for events that have timestamps (market data, token events, limit order events) and `None` for events without meaningful source timestamps (timer, signal, position, execution, redemption).

```rust
if let Some(ts) = event.timestamp() {
    self.current_time = ts;
}
```

### Best Practices

1. **Track logical time in your generator**: Store the current logical time as a field in your signal generator struct.

2. **Update time on every event**: Call `event.timestamp()` at the start of `generate_signal()` and update your internal time tracker.

3. **Use `with_timestamp()` for all signals**: When creating `SignalMetadata`, always use `with_timestamp(self.current_time)` to ensure signals carry the correct logical timestamp.

4. **Works for live trading too**: In live mode, events carry real timestamps, so the behavior is consistent. Your generator will naturally track real time.

## Troubleshooting

### Empty Data Error

If you see `EmptyDataError`:
- Check that your Parquet files contain data
- Verify the outcome filter matches outcomes in your data
- Ensure timestamps are in milliseconds

### Schema Validation Errors

If you see `InvalidSchemaError`:
- Verify column names match expected schema
- Check data types (ts must be INT64, price/size must be FLOAT64)
- Ensure bids/asks columns contain valid JSON arrays

### No Fills

If orders aren't filling:
- Verify your bid prices are realistic (trades must cross them)
- Check that trade sides match fill logic (SELL trades fill bids)
- Review the JSONL output to see trade prices vs order prices

### Zero Fills with Latency Simulation

If you're getting zero fills specifically when latency simulation is enabled, your signal generator may be using wall-clock time instead of logical time. Check:

1. **Verify signal timestamps**: In the JSONL output, compare `timestamp` (signal creation time) with `logical_time` (event time). If they differ by days/weeks, signals are using wall-clock time.

2. **Update your generator**: Use `SignalMetadata::with_timestamp()` instead of `SignalMetadata::new()` and track logical time from events. See [Signal Timestamps and Logical Time](#signal-timestamps-and-logical-time).

3. **Check eligibility timestamps**: In `LimitOrder.OrderPlaced` events, `eligible_for_fills_at` should be close to the logical time + latency, not far in the future.

## References

- [Architecture Documentation](../architecture.md) - Overall system architecture
- [Intent Signals Guide](intent-signals.md) - Signal intent system
- [Orderbook Trading Guide](orderbook-trading.md) - Orderbook trading concepts
