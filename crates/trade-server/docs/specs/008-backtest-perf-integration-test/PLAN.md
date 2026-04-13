# Backtest Performance Integration Test

## Problem

When using trade-server as a library to run backtests:
- With ticker filter (single ticker): works fine
- Without ticker filter (all tickers): causes memory issues, dev environment becomes unresponsive

## Goal

Create an integration test for the backtest framework to:
1. Load real parquet data (configured via env vars)
2. Run event loop with a no-op strategy
3. Collect performance metrics (memory, timing, event counts)
4. Help identify memory bottlenecks

## File Location

```
tests/backtest_perf_test.rs
```

## Environment Variables

| Variable | Description | Required |
|----------|-------------|----------|
| `BACKTEST_PERF_SNAPSHOT_PATH` | Path to snapshots parquet | Yes |
| `BACKTEST_PERF_UPDATE_PATH` | Path to updates parquet | Yes |
| `BACKTEST_PERF_TRADE_PATH` | Path to trades parquet | No (derives from snapshot path) |
| `BACKTEST_PERF_SPOT_PATH` | Path to spot prices parquet | No |
| `BACKTEST_PERF_TICKER_PATTERNS` | Comma-separated ticker patterns | No |
| `BACKTEST_PERF_OUTCOME_FILTER` | Outcome filter (e.g., "Up") | No |

## Test Data

```
# Snapshots (424 MB)
~/popeyes/data-pipeline/local-data/polymarket/snapshots/date=2026-01-19/snapshots.parquet

# Updates (2.5 GB)
~/popeyes/data-pipeline/local-data/polymarket/updates/date=2026-01-19/updates.parquet

# Spot prices (35 MB)
~/popeyes/data-pipeline/local-data/spot_prices/daily/date=2026-01-19/spot_prices.parquet
```

## Architecture

```
tests/backtest_perf_test.rs
├── PerfMetrics struct
│   ├── data_load_duration_ms
│   ├── event_loop_duration_ms
│   ├── total_duration_ms
│   ├── snapshot_count
│   ├── update_count
│   ├── trade_count
│   ├── events_processed
│   ├── peak_memory_mb
│   └── unique_tickers
│
├── test_backtest_performance()
│   ├── Load config from env vars
│   ├── Run backtest with NoopSignalGenerator
│   ├── Collect and print metrics
│   └── Assert no panic/error
│
└── Helper functions
    ├── get_memory_usage_mb() -> f64
    ├── load_config_from_env() -> BacktestConfig
    └── print_metrics()
```

## Key Design Decisions

1. **Use `JsonlFileEventCollector`** - Streams to disk, avoids memory buildup from event capture

2. **Disable all event capture** - Use `CaptureConfig` with all fields `false` to minimize I/O and memory

3. **Use streaming mode** - `BacktestEventCoordinator::new_from_data()` uses iterators internally

4. **Memory measurement** - Read `/proc/self/statm` (Linux) for RSS memory at key points

5. **Phased metrics** - Measure separately:
   - Phase 1: Data loading (ParquetLoader)
   - Phase 2: Event loop execution
   - Total wall-clock time

6. **Use existing `NoopSignalGenerator`** - Already exists in `src/signal/generator.rs`

## Running the Test

```bash
# (Optional) Generate a small fixture for quick iteration
BACKTEST_PERF_SNAPSHOT_PATH=~/popeyes/data-pipeline/local-data/polymarket/snapshots/date=2026-01-19/snapshots.parquet \
BACKTEST_PERF_UPDATE_PATH=~/popeyes/data-pipeline/local-data/polymarket/updates/date=2026-01-19/updates.parquet \
BACKTEST_PERF_TRADE_PATH=~/popeyes/data-pipeline/local-data/polymarket/trades/date=2026-01-19/trades.parquet \
BACKTEST_PERF_SMALL_TICKER=btc-updown-15m-1768780800 \
BACKTEST_PERF_SMALL_OUT_DIR=.local/backtest-perf-small/date=2026-01-19 \
./scripts/make_backtest_perf_small_inputs.sh

# With all data (likely to cause memory issues)
BACKTEST_PERF_SNAPSHOT_PATH=~/popeyes/data-pipeline/local-data/polymarket/snapshots/date=2026-01-19/snapshots.parquet \
BACKTEST_PERF_UPDATE_PATH=~/popeyes/data-pipeline/local-data/polymarket/updates/date=2026-01-19/updates.parquet \
BACKTEST_PERF_SPOT_PATH=~/popeyes/data-pipeline/local-data/spot_prices/daily/date=2026-01-19/spot_prices.parquet \
cargo test --test backtest_perf_test -- --ignored --nocapture

# With ticker filter (baseline - should work)
BACKTEST_PERF_SNAPSHOT_PATH=~/popeyes/data-pipeline/local-data/polymarket/snapshots/date=2026-01-19/snapshots.parquet \
BACKTEST_PERF_UPDATE_PATH=~/popeyes/data-pipeline/local-data/polymarket/updates/date=2026-01-19/updates.parquet \
BACKTEST_PERF_TICKER_PATTERNS="btc-updown-15m-*" \
cargo test --test backtest_perf_test -- --ignored --nocapture
```

## Dependencies

Add to `Cargo.toml`:

```toml
[dev-dependencies]
shellexpand = "3.1"  # For tilde expansion in paths
```

## Suspected Memory Bottlenecks

Based on code analysis:

1. **`ParquetLoader::load_*()` methods** (`src/backtest/loader.rs`)
   - Loads entire dataset into `Vec<>` before any filtering
   - Updates file is 2.5GB

2. **Initial data loading** (`BacktestRunner::load_data()`)
   - All snapshots, updates, trades loaded into memory simultaneously
   - No streaming at the parquet level

3. **`OrderbookTracker`** (`src/orderbook_tracker/`)
   - Maintains orderbook state per asset_id
   - Could grow significantly with many tickers

4. **Event collector** (if using `InMemoryEventCollector`)
   - Holds all events until export
   - `JsonlFileEventCollector` mitigates this

## Next Steps After Profiling

Depending on findings:

1. **Streaming parquet reads** - Use row-group-level filtering before full materialization
2. **Lazy loading updates** - Only load updates when needed per ticker
3. **Memory-mapped files** - For large parquets
4. **Chunked processing** - Process data in batches with explicit memory limits
5. **Ticker-based partitioning** - Process one ticker at a time to bound memory

## Additional Profiling Tools

For deeper analysis:

- **heaptrack**: `heaptrack cargo test ...` for allocation traces
- **valgrind massif**: Detailed heap profiling
- **DHAT**: Rust-native heap profiler via `dhat` crate
