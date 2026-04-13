# Backtest Data Guide

This guide covers input data requirements, synthetic BBO exports, and manifest-driven ticker selection.

## Required Input Files

Backtests require three Parquet files, plus spot data for BS/ML modes.

| File | Required | Purpose |
|---|---|---|
| `snapshots.parquet` | Yes | Seed orderbook state |
| `updates.parquet` | Yes | Incremental orderbook deltas |
| `trades.parquet` | Yes | Historical trades used by fill simulation |
| `spot.parquet` | Only for `bs_lead_lag` or spot-dependent ML features | Spot price stream |

## Sync and Discover Data with `poly-data`

Use `poly-data` as the canonical data workflow instead of ad-hoc download scripts.

```bash
DATE=YYYY-MM-DD

# Sync one date from ewr1-3 into the local poly-data store.
uv run poly-data sync --date "${DATE}"

# Sync latest 3 available remote dates and purge older local dates.
uv run poly-data sync --latest 3 --purge

# Inspect local data/index coverage.
uv run poly-data status
uv run poly-data status --date "${DATE}"

# Enumerate available tickers without scanning parquet manually.
uv run poly-data tickers --date "${DATE}" --pattern "btc-updown-15m-*"

# Resolve filtered (or raw) data paths for a selected ticker set.
uv run poly-data resolve --date "${DATE}" --pattern "btc-updown-15m-*" --json

# Prescreen tickers using coverage metrics stored in the local index DB.
uv run poly-data prescreen --date "${DATE}" --pattern "btc-updown-15m-*" --json
```

Default local layout is currently:
- `~/.polysharp/data/polymarket/snapshots/date=YYYY-MM-DD/snapshots.parquet`
- `~/.polysharp/data/polymarket/updates/date=YYYY-MM-DD/updates.parquet`
- `~/.polysharp/data/polymarket/trades/date=YYYY-MM-DD/trades.parquet`
- `~/.polysharp/data/polymarket/spot/date=YYYY-MM-DD/spot_prices.parquet`

## Minimum Columns

### `snapshots.parquet`

| Column | Type |
|---|---|
| `ts` | TIMESTAMP_MS |
| `ticker` | STRING |
| `outcome` | STRING |
| `bids` | STRING (JSON array) |
| `asks` | STRING (JSON array) |
| `end_date` | TIMESTAMP_NS |

### `updates.parquet`

| Column | Type |
|---|---|
| `ts` | TIMESTAMP_MS |
| `asset_id` | STRING |
| `price` | FLOAT64 |
| `size` | FLOAT64 |
| `side` | STRING (`BUY` / `SELL`) |
| `ticker` | STRING |
| `outcome` | STRING |

### `trades.parquet`

| Column | Type |
|---|---|
| `ts` | TIMESTAMP_MS |
| `ticker` | STRING |
| `outcome` | STRING |
| `side` | STRING (`BUY` / `SELL`) |
| `price` | FLOAT64 |
| `size` | FLOAT64 |

### `spot.parquet` (BS/ML)

| Column | Type |
|---|---|
| `ts` | TIMESTAMP_MS |
| `symbol` | STRING |
| `price` | FLOAT64 |

## Dump Updates-Applied Synthetic BBO

`poly-mm-backtest` can materialize the updates-applied snapshot stream (same `OrderbookTracker` semantics as live) into `synthetic_bbo.parquet`.

```bash
cargo run --bin poly-mm-backtest -- \
  --config crates/poly-market-maker/config.backtest.xgb.yaml \
  --dump-synthetic-bbo-parquet ./output/synthetic_bbo.parquet \
  --dump-only
```

Notes:
- Respects `outcome_filter` and `strategy.ticker_patterns` from the config.
- Uses snapshot-first semantics for equal timestamps.
- Updates before the first snapshot seed are ignored.

Reference: `docs/data-completeness.md`.

## Manifest-Driven Ticker Selection

Backtests are often biased by missing markets/outcomes or long quote gaps. Use a completeness manifest and run on `allowlist.csv`.

### 1) Build manifest files

Outputs:
- `allowlist.csv`
- `exclusions.csv`
- `summary.json`

If synthetic BBO is already present in data-pipeline local data
(`local-data/polymarket/synthetic_bbo/date=${DATE}/synthetic_bbo.parquet`):

```bash
DATE=YYYY-MM-DD
uv run poly-data completeness --date "${DATE}"
```

If you generated synthetic BBO locally:

```bash
DATE=YYYY-MM-DD
uv run poly-data completeness \
  --date "${DATE}" \
  --synthetic-bbo ./output/synthetic_bbo.parquet
```

### 2) Use allowlist tickers in `strategy.ticker_patterns`

```bash
DATE=YYYY-MM-DD
uv run python - <<'PY'
import csv
import os
from pathlib import Path

date = os.environ["DATE"]
allowlist = Path(f"output/data_completeness/date={date}/allowlist.csv")
tickers = [row["ticker"] for row in csv.DictReader(allowlist.read_text().splitlines())]

print("ticker_patterns:")
for ticker in tickers:
    print(f'  - "{ticker}"')
PY
```

Paste the generated list into your backtest config and run normally.

### 3) Debug exclusions

```bash
DATE=YYYY-MM-DD
jq '.exclusion_reason_counts' "output/data_completeness/date=${DATE}/summary.json"
rg "usable_ratio_below_threshold" "output/data_completeness/date=${DATE}/exclusions.csv" | head
```
