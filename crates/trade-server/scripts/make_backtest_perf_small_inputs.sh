#!/usr/bin/env bash
set -euo pipefail

# Generate a small Parquet fixture (snapshots/updates/trades) suitable for quickly
# running `tests/backtest_perf_test.rs` locally.
#
# This script intentionally keeps the schema compatible with `ParquetLoader`:
# - `ts` must be TIMESTAMP_MS
# - `end_date` must be TIMESTAMP_NS
#
# Required env vars (point at the *large* source Parquet files):
# - BACKTEST_PERF_SNAPSHOT_PATH
# - BACKTEST_PERF_UPDATE_PATH
# - BACKTEST_PERF_TRADE_PATH
#
# Optional env vars:
# - BACKTEST_PERF_SMALL_TICKER (default: btc-updown-15m-1768780800)
# - BACKTEST_PERF_SMALL_OUT_DIR (default: .local/backtest-perf-small/date=2026-01-19)
# - BACKTEST_PERF_SMALL_SNAPSHOTS_PER_OUTCOME (default: 200)
# - BACKTEST_PERF_SMALL_TRADES_PER_OUTCOME (default: 50)
# - BACKTEST_PERF_SMALL_UPDATES_LIMIT (default: 20000)

if ! command -v duckdb >/dev/null 2>&1; then
  echo "duckdb not found on PATH" >&2
  exit 1
fi

: "${BACKTEST_PERF_SNAPSHOT_PATH:?BACKTEST_PERF_SNAPSHOT_PATH is required}"
: "${BACKTEST_PERF_UPDATE_PATH:?BACKTEST_PERF_UPDATE_PATH is required}"
: "${BACKTEST_PERF_TRADE_PATH:?BACKTEST_PERF_TRADE_PATH is required}"

BACKTEST_PERF_SMALL_TICKER="${BACKTEST_PERF_SMALL_TICKER:-btc-updown-15m-1768780800}"
BACKTEST_PERF_SMALL_OUT_DIR="${BACKTEST_PERF_SMALL_OUT_DIR:-.local/backtest-perf-small/date=2026-01-19}"
BACKTEST_PERF_SMALL_SNAPSHOTS_PER_OUTCOME="${BACKTEST_PERF_SMALL_SNAPSHOTS_PER_OUTCOME:-200}"
BACKTEST_PERF_SMALL_TRADES_PER_OUTCOME="${BACKTEST_PERF_SMALL_TRADES_PER_OUTCOME:-50}"
BACKTEST_PERF_SMALL_UPDATES_LIMIT="${BACKTEST_PERF_SMALL_UPDATES_LIMIT:-20000}"

mkdir -p "${BACKTEST_PERF_SMALL_OUT_DIR}"

SNAP="${BACKTEST_PERF_SNAPSHOT_PATH}"
UPD="${BACKTEST_PERF_UPDATE_PATH}"
TRD="${BACKTEST_PERF_TRADE_PATH}"
T="${BACKTEST_PERF_SMALL_TICKER}"

OUT_SNAP="${BACKTEST_PERF_SMALL_OUT_DIR}/snapshots.parquet"
OUT_UPD="${BACKTEST_PERF_SMALL_OUT_DIR}/updates.parquet"
OUT_TRD="${BACKTEST_PERF_SMALL_OUT_DIR}/trades.parquet"

echo "Writing small Parquet fixtures to: ${BACKTEST_PERF_SMALL_OUT_DIR}"
echo "  ticker: ${T}"
echo "  snapshots/outcome: ${BACKTEST_PERF_SMALL_SNAPSHOTS_PER_OUTCOME}"
echo "  trades/outcome: ${BACKTEST_PERF_SMALL_TRADES_PER_OUTCOME}"
echo "  updates limit: ${BACKTEST_PERF_SMALL_UPDATES_LIMIT}"

duckdb -c "COPY (
  WITH snap AS (
    SELECT *, row_number() OVER (PARTITION BY outcome ORDER BY ts DESC) AS rn
    FROM read_parquet('${SNAP}')
    WHERE ticker='${T}'
  )
  SELECT
    CAST(ts AS TIMESTAMP_MS) AS ts,
    CAST(day AS TIMESTAMP_MS) AS day,
    queue,
    asset_id,
    market,
    bids,
    asks,
    hash,
    source,
    event_id,
    ticker,
    title,
    CAST(end_date AS TIMESTAMP_NS) AS end_date,
    outcome,
    date
  FROM snap
  WHERE rn <= ${BACKTEST_PERF_SMALL_SNAPSHOTS_PER_OUTCOME}
  ORDER BY ts
) TO '${OUT_SNAP}' (FORMAT 'parquet');"

duckdb -c "COPY (
  WITH tr AS (
    SELECT *, row_number() OVER (PARTITION BY outcome ORDER BY ts DESC) AS rn
    FROM read_parquet('${TRD}')
    WHERE ticker='${T}'
  )
  SELECT
    CAST(ts AS TIMESTAMP_MS) AS ts,
    CAST(day AS TIMESTAMP_MS) AS day,
    queue,
    trade_type,
    asset_id,
    market,
    price,
    size,
    side,
    fee_rate_bps,
    event_id,
    ticker,
    title,
    CAST(end_date AS TIMESTAMP_NS) AS end_date,
    outcome,
    date
  FROM tr
  WHERE rn <= ${BACKTEST_PERF_SMALL_TRADES_PER_OUTCOME}
  ORDER BY ts
) TO '${OUT_TRD}' (FORMAT 'parquet');"

duckdb -c "COPY (
  WITH snap AS (
    SELECT ts
    FROM (
      SELECT ts, row_number() OVER (PARTITION BY outcome ORDER BY ts DESC) AS rn
      FROM read_parquet('${SNAP}')
      WHERE ticker='${T}'
    )
    WHERE rn <= ${BACKTEST_PERF_SMALL_SNAPSHOTS_PER_OUTCOME}
  ),
  rng AS (SELECT MIN(ts) AS min_ts, MAX(ts) AS max_ts FROM snap),
  upd AS (
    SELECT *, row_number() OVER (ORDER BY ts) AS rn
    FROM read_parquet('${UPD}'), rng
    WHERE ticker='${T}' AND ts BETWEEN rng.min_ts AND rng.max_ts
  )
  SELECT
    CAST(ts AS TIMESTAMP_MS) AS ts,
    CAST(day AS TIMESTAMP_MS) AS day,
    queue,
    asset_id,
    market,
    price,
    size,
    side,
    hash,
    best_bid,
    best_ask,
    source,
    event_id,
    ticker,
    title,
    CAST(end_date AS TIMESTAMP_NS) AS end_date,
    outcome,
    date
  FROM upd
  WHERE rn <= ${BACKTEST_PERF_SMALL_UPDATES_LIMIT}
  ORDER BY ts
) TO '${OUT_UPD}' (FORMAT 'parquet');"

echo
echo "Generated:"
ls -lh "${OUT_SNAP}" "${OUT_UPD}" "${OUT_TRD}"

echo
echo "Run:"
cat <<EOF
BACKTEST_PERF_SNAPSHOT_PATH=${OUT_SNAP} \\
BACKTEST_PERF_UPDATE_PATH=${OUT_UPD} \\
BACKTEST_PERF_TRADE_PATH=${OUT_TRD} \\
BACKTEST_PERF_TICKER_PATTERNS=${T} \\
cargo test --test backtest_perf_test -- --ignored --nocapture
EOF

