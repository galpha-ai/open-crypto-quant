# Backtest Infrastructure

The trade server includes a comprehensive backtesting infrastructure for evaluating trading strategies against historical data. This enables strategy development and validation before deploying to production.

## Overview

The backtest system replays historical orderbook snapshots and trade events through the same signal generation and position management pipeline used in live trading. Limit orders are filled using a trade-based simulation model where pending orders fill when historical trades cross the order price.

## Components

```
src/backtest/
├── config.rs       # BacktestConfig, PositionConfig
├── runner.rs       # BacktestRunner orchestrator
├── processor.rs    # BacktestEventProcessor for event routing
├── loader.rs       # ParquetLoader for historical data
├── timeline.rs     # BacktestTimeline for event ordering
├── coordinator.rs  # BacktestEventCoordinator
├── collector.rs    # BacktestEventCollector for JSONL output
├── types.rs        # BacktestTick, CollectedEvent
└── error.rs        # BacktestError enum
```

### BacktestEventProcessor

Located in `src/backtest/processor.rs`. Encapsulates event handling logic extracted from `BacktestRunner::run()`.

Handles different event types:
- `OrderbookSnapshot`: Updates executor, generates signals, routes to appropriate handlers
- `PolymarketTrade`: Processes trade-based fill simulation
- `Timer`: Checks for position exits
- `LimitOrder`: Handles limit order lifecycle events
- `Execution`: Updates position state

Signal routing mirrors `PositionHandler` logic:
- Intent-based signals → `reconcile_intent()` on PositionManager
- Action-based signals routed by `signal_action()` (Entry/Exit/ModifyOrder/CancelOrder)

### ParquetLoader

Located in `src/backtest/loader.rs`.

- Loads orderbook snapshots and trade events from Parquet files
- Supports optional outcome filtering for multi-outcome markets (e.g., prediction markets)
- Parses JSON-encoded bid/ask levels from snapshot data
- If ticker patterns are exact Polymarket binary market tickers, derives a minimal time window
  (start/end plus a small buffer) and applies a timestamp filter to Parquet scans. This is
  controlled by `BacktestConfig.ticker_time_range_filter` and `ticker_time_range_buffer`.

### BacktestTimeline

Located in `src/backtest/timeline.rs`.

- Merges snapshots and trades chronologically
- Groups events into `BacktestTick` structures (snapshot + associated trades)
- Provides iterator interface for sequential processing

### BacktestEventCoordinator

Located in `src/backtest/coordinator.rs`. Implements `EventCoordinator` trait for backtest mode.

Delivers events in priority order:
1. Enqueued events (fill events, position events)
2. Orderbook snapshot for current tick
3. Buffered trade events
4. Timer events (at configured intervals)

Returns `NoMoreEvents` when timeline is exhausted.

### BacktestOrderExecutor

Public facade is in `src/execution/backtest/executor.rs` with internal modules in
`src/execution/backtest/executor/`.

- Tracks pending limit orders awaiting fill
- Implements trade-based fill simulation:
  - BID orders fill when SELL trades cross at or below bid price
  - ASK orders fill when BUY trades cross at or above ask price
  - Fills at order price (not trade price)
  - Supports partial fills based on trade size
- Internal module responsibilities:
  - `config.rs`: builder and configuration plumbing
  - `types.rs`: pending-order and quote-lane data structures
  - `latency.rs`: simulation clock and latency sampling helpers
  - `market_registry.rs`: market/asset complement mapping for mirrored fills
  - `quote_lifecycle.rs`: lane state transitions for place/cancel/replacement
  - `fill_engine.rs`: trade-driven fill matching and post-fill lane updates
- Uses shared `LifecycleEngine` as canonical lifecycle state authority (`SubmitPending`/`Open`/`CancelPending`/terminal).
  Quote-lane state is orchestration metadata only (scheduling/replacement), not a second lifecycle authority.
- Quote lane lifecycle uses a single canonical policy: once a lane enters canceling,
  cancellation is irreversible and later converged intent updates only change the
  pending replacement quote that will be placed after cancel completion.
- With latency simulation enabled, command-path place/cancel responses are deferred;
  terminal `OrderPlaced`/`OrderCancelled` events are emitted from lifecycle evidence
  during tick/trade processing.
- Tests are in `src/execution/backtest/executor_test.rs` and cover pricing, fills,
  latency/cancellation, inventory constraints, and redemption behavior.

### BacktestEventCollector

Located in `src/backtest/collector.rs`.

- Records all events during backtest execution
- Exports to JSONL format for post-hoc analysis
- Each line contains: timestamp, event_type, and full event data

### BacktestRunner

Located in `src/backtest/runner.rs`. Main entry point for executing backtests.

Orchestrates all components:
1. Loads data using ParquetLoader
2. Builds timeline from loaded data
3. Initializes coordinator, executor, and position manager
4. Creates `BacktestEventProcessor` for event handling
5. Runs event loop until timeline exhausted
6. Calculates final metrics and writes JSONL output

Delegates event processing to `BacktestEventProcessor` for cleaner separation of concerns.

## Data Flow

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────────┐
│  Parquet Files  │────▶│  ParquetLoader   │────▶│  BacktestTimeline   │
│ (snapshots.pq)  │     │                  │     │  (chronological)    │
│ (trades.pq)     │     └──────────────────┘     └──────────┬──────────┘
└─────────────────┘                                         │
                                                            ▼
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────────┐
│  JSONL Output   │◀────│ EventCollector   │◀────│ EventCoordinator    │
│                 │     │                  │     │  (priority queue)   │
└─────────────────┘     └──────────────────┘     └──────────┬──────────┘
                                                            │
                                                            ▼
                                               ┌─────────────────────────┐
                                               │ BacktestEventProcessor  │
                                               │   (signal routing)      │
                                               └──────────┬──────────────┘
                                                          │
                        ┌─────────────────────────────────┼─────────────────────────────────┐
                        │                                 │                                 │
                        ▼                                 ▼                                 ▼
               ┌────────────────┐               ┌────────────────┐               ┌────────────────┐
               │ SignalGenerator│               │ OrderExecutor  │               │ PositionManager│
               │ (user-provided)│               │   (backtest)   │               │  (in-memory)   │
               └────────────────┘               └────────────────┘               └────────────────┘
```

## Configuration

See `src/backtest/config.rs` for the full configuration structs.

**BacktestConfig** fields:
- `snapshot_path`: Path to orderbook snapshots Parquet file
- `update_path`: Path to orderbook updates Parquet file
- `trade_path`: Path to trade events Parquet file
- `spot_event_path`: Optional path to spot price events Parquet file
- `outcome_filter`: Optional filter by outcome (e.g., "Up")
- `ticker_patterns`: Optional glob patterns to filter tickers
- `ticker_time_range_filter`: Enable ticker-derived time-range filtering
- `ticker_time_range_buffer`: Buffer applied to derived time ranges
- `position`: Position management settings (PositionConfig)
- `timer_interval`: Timer event frequency
- `output_path`: JSONL output file path
- `buy_slippage` / `sell_slippage`: Market order slippage (0.0-1.0)
- `latency`: Optional latency simulation; `latency.min_quote_lifetime_ms` enables
  per-lane quote-update debouncing in reconciliation before cancel/replace dispatch

**PositionConfig** fields:
- `initial_balance`: Starting balance in quote currency
- `max_open_positions`: Maximum concurrent positions
- `max_holding_period`: Forced exit timeout
- `trade_amount`: Order size in quote currency
- `take_profit_threshold` / `stop_loss_threshold`: Exit thresholds as percentages

## Parquet Schema

**Orderbook Snapshots:**
| Column   | Type          | Description                       |
|----------|---------------|-----------------------------------|
| ts       | INT64         | Timestamp in milliseconds         |
| ticker   | STRING        | Market ticker identifier          |
| outcome  | STRING        | Outcome name (e.g., "Up", "Down") |
| bids     | STRING (JSON) | Array of {price, size} objects    |
| asks     | STRING (JSON) | Array of {price, size} objects    |
| end_date | STRING        | Market end/maturity date          |

**Orderbook Updates:**
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

**Trade Events:**
| Column  | Type    | Description                      |
|---------|---------|----------------------------------|
| ts      | INT64   | Timestamp in milliseconds        |
| ticker  | STRING  | Market ticker identifier         |
| outcome | STRING  | Outcome name                     |
| side    | STRING  | "BUY" or "SELL" (aggressor side) |
| price   | FLOAT64 | Trade price                      |
| size    | FLOAT64 | Trade size                       |

**Synthetic Snapshots Output:**
- Produced by `write_synthetic_bbo_snapshots_parquet_with_options()` in `src/backtest/synthetic_snapshots.rs`.
- Includes flat multi-level orderbook columns with configurable depth (`book_levels`, default 5).
- Bid columns follow `bid_price_i` / `bid_size_i` for `i = 0..N-1`.
- Ask columns follow `ask_price_i` / `ask_size_i` for `i = 0..N-1`.
- Base columns: `ts`, `observed_at`, `asset_id`, `ticker`, `outcome`, `end_date`, `hash`.

## Metrics

After backtest completion, `BacktestResult` contains `BacktestMetrics` with:
- `total_snapshots`, `total_trades`, `total_signals`: Event counts
- `total_orders_placed`, `total_fills`, `total_cancels`: Order lifecycle counts
- `final_pnl`: Realized PnL in quote currency
- `final_inventory`: Open position inventory at end
- `duration_ms`: Backtest execution time
- `data_start_time`, `data_end_time`: Time range of data processed

## Design Decisions

- **No Prometheus Metrics**: Backtest mode does not emit Prometheus metrics since they're designed for real-time observability. Instead, summary statistics are returned in `BacktestMetrics`.
- **Trade-Based Fill Model**: Orders fill when actual historical trades cross the order price, providing more realistic simulation than simple price-crossing models.
- **Reuses Existing Infrastructure**: Uses the same `SignalGenerator`, `PositionManager`, and event types as live trading for consistency.
- **Deterministic Execution**: Single-threaded execution ensures reproducible results given the same inputs.
- **JSONL Output**: Events are exported to JSONL for flexible post-hoc analysis with external tools.
