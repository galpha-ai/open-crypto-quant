# Architecture

Codemap for `poly-data` — a Python CLI for managing Polymarket data for backtesting and model training.

## Overview

poly-data manages the lifecycle of Polymarket orderbook data used by both backtesting and model training pipelines. It handles: syncing from remote storage, indexing locally, filtering/prescreening by quality, building completeness manifests, resolving filtered paths for consumers, and garbage-collecting old data.

**Data source**: GCS (Google Cloud Storage) on GCP. The previous ewr1-3 rsync-based sync is deprecated.

**Tech stack**: Python 3.12+, DuckDB (parquet analysis), SQLite (local index), PyYAML (config). Packaged with hatchling, managed with uv.

**Entry point**: `poly_data.cli:main` (installed as `poly-data` CLI).

## Directory Layout

```
src/poly_data/
├── cli.py                        # CLI router — argparse with 8 subcommands
├── sync.py                       # Remote data download from GCS
├── index.py                      # SQLite indexing, ticker discovery, metric computation
├── resolve.py                    # Ticker filtering + parquet caching with DuckDB
├── prescreen.py                  # Quality filtering using indexed metrics
├── gc.py                         # Storage cleanup (data + cache)
└── completeness/
    ├── config.py                 # YAML config dataclasses for completeness gating
    └── manifest_builder.py       # Build allowlist/exclusions from synthetic BBO analysis

configs/
└── data_completeness.yaml        # Default completeness gating config

tests/
├── test_sync.py
├── test_resolve.py
├── test_prescreen.py
└── test_completeness_cli.py

docs/
├── backtest-data.md              # Usage guide for backtest data workflow
└── data-completeness.md          # Completeness gating spec
```

## Data Storage

All local data lives under `~/.polysharp/`:

```
~/.polysharp/
├── data/polymarket/
│   ├── snapshots/date={DATE}/snapshots.parquet
│   ├── updates/date={DATE}/updates.parquet
│   ├── trades/date={DATE}/trades.parquet
│   └── spot/date={DATE}/spot_prices.parquet
├── cache/{sha256_key}/           # Filtered parquet cache
│   ├── snapshots.parquet
│   ├── updates.parquet
│   ├── trades.parquet
│   └── manifest.json
├── models/                       # ML model artifacts
└── index.db                      # SQLite metadata index
```

## Module Responsibilities

### `cli.py`

CLI orchestration. Routes 8 subcommands to handler functions, formats output (human-readable or JSON), handles errors.

**Subcommands**: `sync`, `index`, `status`, `tickers`, `resolve`, `prescreen`, `completeness`, `gc`.

### `sync.py`

Downloads parquet data from GCS to local storage. Automatically indexes after sync.

- `sync_date()` — download components (snapshots, updates, trades) for a date
- `discover_remote_dates()` — list available dates from remote storage

### `index.py`

SQLite-backed metadata index for fast ticker discovery without re-scanning parquets.

- `index_date()` — scan parquets, compute per-ticker metrics (row counts, market window stats), store in SQLite
- `list_tickers()` — query with pattern/regex/sample/latest/after filters
- `list_dates()`, `get_date_status()`, `count_tickers()` — status queries

**SQLite schema**:
- `dates` — per-date presence flags and byte sizes for each component
- `tickers` — per-(date, ticker) row counts, market window times, in-window coverage
- `cache_entries` — cache metadata for invalidation

**Ticker window encoding**: tickers like `btc-updown-15m-1704067200` encode market open time (unix suffix) and duration (`15m`) in the ticker name itself. `index.py` parses this to compute `market_open_ms`, `market_close_ms`, `duration_ms`.

### `resolve.py`

Resolves data paths for a selected set of tickers. Filters large parquets down to selected tickers using DuckDB and caches the results.

- `resolve_data()` — main entry: select tickers, filter parquets, return manifest with paths
- `filter_parquet()` — DuckDB-based parquet filtering
- Cache key: SHA256(date + sorted tickers), first 12 chars
- Cache invalidation: compares source file mtimes

### `prescreen.py`

Lightweight quality gating using metrics already stored in the SQLite index (no parquet reads).

- `prescreen_tickers()` — filter tickers by: valid ticker window, minimum snapshot count, window coverage ratio, spot availability
- Drop reasons: `invalid_ticker_window`, `no_snapshots_in_window`, `insufficient_snapshots_in_window`, `insufficient_window_coverage`, `no_spot_in_window`, `missing_spot_file`

### `completeness/` (config.py + manifest_builder.py)

Heavy-duty completeness analysis using the synthetic BBO parquet. Config-driven via `configs/data_completeness.yaml`.

**config.py** — dataclass hierarchy: `DataCompletenessConfig` with sections for inputs, universe, event_time, thresholds, validation, spot, outputs.

**manifest_builder.py** — the core analysis engine:

1. Load synthetic BBO parquet (+ optional spot prices)
2. Filter to universe (ticker regex + required outcomes)
3. Evaluate quote validity per row (non-null bid/ask, non-negative spread, price bounds)
4. Event-time completeness: for each timestamp `t`, check if a fresh valid quote exists at `t + h` for all configured horizons
5. Compute `usable_ratio = labelable_rows / total_rows` per (ticker, outcome)
6. Score per ticker: `min(usable_ratio(outcome) for outcome in required_outcomes)`
7. Gate: score >= 0.98, usable_events >= 1000, gap checks, spot coverage
8. Output: `allowlist.csv`, `exclusions.csv`, `summary.json`

### `gc.py`

Storage cleanup. Supports keeping N days, specific dates, or cache-only cleanup. Tracks bytes freed.

## Data Flow

```
GCS (Google Cloud Storage)
    │ sync
    ▼
~/.polysharp/data/  ──► index.py ──► index.db (SQLite)
    │                                    │
    │                          ┌─────────┼──────────┐
    │                          ▼         ▼          ▼
    │                      tickers   prescreen   status
    │
    ├──► resolve.py ──► ~/.polysharp/cache/ ──► backtest / model training
    │
    └──► completeness/manifest_builder.py
              │
              ▼
         allowlist.csv / exclusions.csv / summary.json
              │
              ▼
         backtest / model training ticker selection
```

## Typical Workflow

```bash
poly-data sync --latest 3 --purge        # 1. Fetch data
poly-data status                          # 2. Inspect coverage
poly-data tickers --date DATE --pattern "btc-updown-15m-*"  # 3. Discover tickers
poly-data prescreen --date DATE --pattern "btc-updown-15m-*" --require-spot  # 4. Quality filter
poly-data completeness --date DATE        # 5. Build completeness manifest
poly-data resolve --date DATE --tickers T1 T2 --json  # 6. Get filtered paths
poly-data gc --keep-days 7                # 7. Cleanup
```

## External Dependencies

- **GCS**: Google Cloud Storage — source of parquet data produced by the data pipeline
- **Synthetic BBO**: generated upstream by `poly-mm-backtest` (Rust, in polysharp repo)
- **Spot prices**: Binance BTCUSDT data from data-pipeline
- **Downstream consumers**: polysharp backtesting engine and model training pipelines read resolved parquet paths
