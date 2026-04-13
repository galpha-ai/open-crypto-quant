from __future__ import annotations

from bisect import bisect_left, bisect_right
import json
import random
import re
import sqlite3
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

import duckdb


def _sql_ident(name: str) -> str:
    return '"' + name.replace('"', '""') + '"'


def data_home() -> Path:
    return Path.home() / ".polysharp"


def ensure_data_dir() -> Path:
    root = data_home()
    directories = [
        root / "data" / "polymarket" / "snapshots",
        root / "data" / "polymarket" / "updates",
        root / "data" / "polymarket" / "trades",
        root / "data" / "polymarket" / "spot",
        root / "cache",
        root / "models",
    ]
    for directory in directories:
        directory.mkdir(parents=True, exist_ok=True)
    return root


def init_db() -> Path:
    root = ensure_data_dir()
    db_path = root / "index.db"

    with sqlite3.connect(db_path) as con:
        con.execute(
            """
            CREATE TABLE IF NOT EXISTS dates (
                date TEXT PRIMARY KEY,
                has_snapshots BOOLEAN NOT NULL DEFAULT 0,
                has_updates BOOLEAN NOT NULL DEFAULT 0,
                has_trades BOOLEAN NOT NULL DEFAULT 0,
                has_spot BOOLEAN NOT NULL DEFAULT 0,
                snapshot_bytes INTEGER,
                update_bytes INTEGER,
                trade_bytes INTEGER,
                spot_bytes INTEGER,
                indexed_at TEXT NOT NULL
            )
            """
        )
        con.execute(
            """
            CREATE TABLE IF NOT EXISTS tickers (
                date TEXT NOT NULL,
                ticker TEXT NOT NULL,
                snapshot_rows INTEGER,
                update_rows INTEGER,
                trade_rows INTEGER,
                market_open_ms INTEGER,
                market_close_ms INTEGER,
                duration_ms INTEGER,
                snapshot_in_window_rows INTEGER,
                spot_in_window_rows INTEGER,
                invalid_ticker_window BOOLEAN,
                PRIMARY KEY (date, ticker)
            )
            """
        )
        con.execute(
            """
            CREATE TABLE IF NOT EXISTS cache_entries (
                cache_key TEXT PRIMARY KEY,
                date TEXT NOT NULL,
                tickers TEXT NOT NULL,
                cache_dir TEXT NOT NULL,
                total_bytes INTEGER,
                created_at TEXT NOT NULL,
                source_snapshot_mtime TEXT,
                source_update_mtime TEXT,
                source_trade_mtime TEXT
            )
            """
        )
        con.execute("CREATE INDEX IF NOT EXISTS idx_tickers_date ON tickers(date)")
        con.execute("CREATE INDEX IF NOT EXISTS idx_tickers_ticker ON tickers(ticker)")

    return db_path


def _connect() -> sqlite3.Connection:
    con = sqlite3.connect(init_db())
    con.row_factory = sqlite3.Row
    return con


def _now_iso() -> str:
    return datetime.now(UTC).isoformat()


def _parse_after_threshold_ms(date: str, after: str) -> int:
    dt = datetime.strptime(f"{date} {after}", "%Y-%m-%d %H:%M").replace(tzinfo=UTC)
    return int(dt.timestamp() * 1000)


def _market_open_ms(ticker: str) -> int | None:
    parts = ticker.rsplit("-", 1)
    if len(parts) != 2:
        return None
    suffix = parts[1]
    if not suffix.isdigit():
        return None
    value = int(suffix)
    if value <= 0:
        return None
    return value * 1000


def _market_duration_ms(ticker: str) -> int | None:
    parts = ticker.split("-")
    if len(parts) < 4:
        return None
    duration = parts[-2]
    if not duration:
        return None
    unit = duration[-1]
    value_str = duration[:-1]
    if not value_str.isdigit():
        return None
    value = int(value_str)
    if value <= 0:
        return None
    if unit == "m":
        return value * 60_000
    if unit == "h":
        return value * 3_600_000
    if unit == "d":
        return value * 86_400_000
    return None


def _market_window_ms(ticker: str) -> tuple[int, int] | None:
    open_ms = _market_open_ms(ticker)
    duration_ms = _market_duration_ms(ticker)
    if open_ms is None or duration_ms is None:
        return None
    return open_ms, open_ms + duration_ms


def _column_type_by_name(con: duckdb.DuckDBPyConnection, parquet_path: Path) -> dict[str, str]:
    rows = con.execute("DESCRIBE SELECT * FROM read_parquet(?)", [str(parquet_path)]).fetchall()
    return {str(name): str(dtype) for name, dtype, *_ in rows}


def _resolve_ts_column(
    *,
    column_types: dict[str, str],
    preferred_columns: list[str],
    label: str,
) -> tuple[str, str]:
    for column in preferred_columns:
        if column in column_types:
            return column, column_types[column]
    available = ", ".join(sorted(column_types.keys()))
    raise RuntimeError(
        f"Could not find timestamp column for {label}. "
        f"Tried {preferred_columns}; available columns: [{available}]"
    )


def _to_epoch_ms_expr(column_name: str, column_type: str) -> str:
    ident = _sql_ident(column_name)
    normalized = column_type.upper()
    if "TIMESTAMP" in normalized or normalized == "DATE":
        return f"CAST(epoch_ms({ident}) AS BIGINT)"
    return f"CAST({ident} AS BIGINT)"


def _snapshot_counts_in_window(
    con: duckdb.DuckDBPyConnection,
    *,
    snapshot_path: Path,
    ticker_windows: dict[str, tuple[int, int]],
) -> dict[str, int]:
    if not ticker_windows:
        return {}

    con.execute(
        "CREATE OR REPLACE TEMP TABLE ticker_windows("
        "ticker VARCHAR, window_start_ms BIGINT, window_end_ms BIGINT)"
    )
    con.executemany(
        "INSERT INTO ticker_windows VALUES (?, ?, ?)",
        [(ticker, start_ms, end_ms) for ticker, (start_ms, end_ms) in ticker_windows.items()],
    )

    snapshot_column_types = _column_type_by_name(con, snapshot_path)
    snapshot_ts_col, snapshot_ts_type = _resolve_ts_column(
        column_types=snapshot_column_types,
        preferred_columns=["ts", "timestamp", "timestamp_ms", "ts_ms"],
        label="snapshots",
    )
    snapshot_ts_expr = _to_epoch_ms_expr(snapshot_ts_col, snapshot_ts_type)

    rows = con.execute(
        f"""
        WITH snapshot_rows AS (
          SELECT ticker, {snapshot_ts_expr} AS ts_ms
          FROM read_parquet(?)
          WHERE ticker IN (SELECT ticker FROM ticker_windows)
        )
        SELECT
          w.ticker,
          CAST(
            coalesce(
              sum(
                CASE
                  WHEN s.ts_ms BETWEEN w.window_start_ms AND w.window_end_ms THEN 1
                  ELSE 0
                END
              ),
              0
            ) AS BIGINT
          ) AS snapshot_in_window
        FROM ticker_windows w
        LEFT JOIN snapshot_rows s ON s.ticker = w.ticker
        GROUP BY w.ticker
        ORDER BY w.ticker
        """,
        [str(snapshot_path)],
    ).fetchall()
    return {str(ticker): int(snapshot_in_window) for ticker, snapshot_in_window in rows}


def _spot_counts_in_window(
    con: duckdb.DuckDBPyConnection,
    *,
    spot_path: Path,
    ticker_windows: dict[str, tuple[int, int]],
) -> dict[str, int]:
    if not ticker_windows:
        return {}

    spot_column_types = _column_type_by_name(con, spot_path)
    spot_ts_col, spot_ts_type = _resolve_ts_column(
        column_types=spot_column_types,
        preferred_columns=["ts", "timestamp", "timestamp_ms", "ts_ms"],
        label="spot prices",
    )
    spot_ts_expr = _to_epoch_ms_expr(spot_ts_col, spot_ts_type)

    rows = con.execute(
        f"SELECT {spot_ts_expr} AS ts_ms FROM read_parquet(?) ORDER BY ts_ms",
        [str(spot_path)],
    ).fetchall()
    spot_ts_values = [int(ts_ms) for (ts_ms,) in rows if ts_ms is not None]
    counts: dict[str, int] = {}
    for ticker, (start_ms, end_ms) in ticker_windows.items():
        start_idx = bisect_left(spot_ts_values, start_ms)
        end_idx = bisect_right(spot_ts_values, end_ms)
        counts[ticker] = end_idx - start_idx
    return counts


def upsert_date(
    date: str,
    *,
    has_snapshots: bool = False,
    has_updates: bool = False,
    has_trades: bool = False,
    has_spot: bool = False,
    snapshot_bytes: int | None = None,
    update_bytes: int | None = None,
    trade_bytes: int | None = None,
    spot_bytes: int | None = None,
    indexed_at: str | None = None,
) -> None:
    with _connect() as con:
        con.execute(
            """
            INSERT INTO dates (
                date,
                has_snapshots,
                has_updates,
                has_trades,
                has_spot,
                snapshot_bytes,
                update_bytes,
                trade_bytes,
                spot_bytes,
                indexed_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(date) DO UPDATE SET
                has_snapshots = excluded.has_snapshots,
                has_updates = excluded.has_updates,
                has_trades = excluded.has_trades,
                has_spot = excluded.has_spot,
                snapshot_bytes = excluded.snapshot_bytes,
                update_bytes = excluded.update_bytes,
                trade_bytes = excluded.trade_bytes,
                spot_bytes = excluded.spot_bytes,
                indexed_at = excluded.indexed_at
            """,
            (
                date,
                int(has_snapshots),
                int(has_updates),
                int(has_trades),
                int(has_spot),
                snapshot_bytes,
                update_bytes,
                trade_bytes,
                spot_bytes,
                indexed_at or _now_iso(),
            ),
        )


def upsert_tickers(date: str, ticker_rows: list[dict[str, Any]]) -> None:
    if not ticker_rows:
        return

    values: list[tuple[Any, ...]] = []
    for row in ticker_rows:
        ticker = row.get("ticker")
        if ticker is None:
            continue
        values.append(
            (
                date,
                str(ticker),
                row.get("snapshot_rows"),
                row.get("update_rows"),
                row.get("trade_rows"),
                row.get("market_open_ms"),
                row.get("market_close_ms"),
                row.get("duration_ms"),
                row.get("snapshot_in_window_rows"),
                row.get("spot_in_window_rows"),
                row.get("invalid_ticker_window"),
            )
        )

    if not values:
        return

    with _connect() as con:
        con.executemany(
            """
            INSERT INTO tickers (
                date,
                ticker,
                snapshot_rows,
                update_rows,
                trade_rows,
                market_open_ms,
                market_close_ms,
                duration_ms,
                snapshot_in_window_rows,
                spot_in_window_rows,
                invalid_ticker_window
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(date, ticker) DO UPDATE SET
                snapshot_rows = excluded.snapshot_rows,
                update_rows = excluded.update_rows,
                trade_rows = excluded.trade_rows,
                market_open_ms = excluded.market_open_ms,
                market_close_ms = excluded.market_close_ms,
                duration_ms = excluded.duration_ms,
                snapshot_in_window_rows = excluded.snapshot_in_window_rows,
                spot_in_window_rows = excluded.spot_in_window_rows,
                invalid_ticker_window = excluded.invalid_ticker_window
            """,
            values,
        )


def list_dates() -> list[dict[str, Any]]:
    with _connect() as con:
        rows = con.execute("SELECT * FROM dates ORDER BY date DESC").fetchall()
    return [dict(row) for row in rows]


def list_tickers(
    date: str,
    *,
    pattern: str | None = None,
    regex: str | None = None,
    sample: int | None = None,
    latest: int | None = None,
    after: str | None = None,
    seed: int | None = None,
) -> list[str]:
    if sample is not None and sample <= 0:
        return []
    if latest is not None and latest <= 0:
        return []

    where = ["date = ?"]
    params: list[Any] = [date]
    if pattern:
        where.append("ticker GLOB ?")
        params.append(pattern)
    if after:
        where.append("market_open_ms >= ?")
        params.append(_parse_after_threshold_ms(date, after))

    sql = f"SELECT ticker FROM tickers WHERE {' AND '.join(where)}"

    can_use_sql_random_sample = (
        sample is not None and sample > 0 and seed is None and regex is None and latest is None
    )
    if can_use_sql_random_sample:
        sql += " ORDER BY RANDOM() LIMIT ?"
        params.append(sample)
    elif latest is not None:
        sql += " ORDER BY market_open_ms DESC LIMIT ?"
        params.append(latest)
    else:
        sql += " ORDER BY ticker"

    with _connect() as con:
        tickers = [str(row["ticker"]) for row in con.execute(sql, params).fetchall()]

    if regex:
        matcher = re.compile(regex)
        tickers = [ticker for ticker in tickers if matcher.search(ticker)]

    if sample is not None and not can_use_sql_random_sample and len(tickers) > sample:
        rng = random.Random(seed)
        tickers = rng.sample(tickers, sample)

    return tickers


def get_date_status(date: str) -> dict[str, Any] | None:
    with _connect() as con:
        row = con.execute("SELECT * FROM dates WHERE date = ?", (date,)).fetchone()
    return None if row is None else dict(row)


def delete_date(date: str) -> None:
    with _connect() as con:
        con.execute("DELETE FROM tickers WHERE date = ?", (date,))
        con.execute("DELETE FROM dates WHERE date = ?", (date,))


def lookup_cache_entry(cache_key: str) -> dict[str, Any] | None:
    with _connect() as con:
        row = con.execute("SELECT * FROM cache_entries WHERE cache_key = ?", (cache_key,)).fetchone()
    if row is None:
        return None

    payload = dict(row)
    try:
        payload["tickers"] = json.loads(payload["tickers"])
    except (TypeError, json.JSONDecodeError):
        pass
    return payload


def upsert_cache_entry(
    cache_key: str,
    *,
    date: str,
    tickers: list[str] | str,
    cache_dir: str,
    total_bytes: int | None = None,
    created_at: str | None = None,
    source_snapshot_mtime: str | None = None,
    source_update_mtime: str | None = None,
    source_trade_mtime: str | None = None,
) -> None:
    tickers_json = tickers if isinstance(tickers, str) else json.dumps(tickers)

    with _connect() as con:
        con.execute(
            """
            INSERT INTO cache_entries (
                cache_key,
                date,
                tickers,
                cache_dir,
                total_bytes,
                created_at,
                source_snapshot_mtime,
                source_update_mtime,
                source_trade_mtime
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(cache_key) DO UPDATE SET
                date = excluded.date,
                tickers = excluded.tickers,
                cache_dir = excluded.cache_dir,
                total_bytes = excluded.total_bytes,
                created_at = excluded.created_at,
                source_snapshot_mtime = excluded.source_snapshot_mtime,
                source_update_mtime = excluded.source_update_mtime,
                source_trade_mtime = excluded.source_trade_mtime
            """,
            (
                cache_key,
                date,
                tickers_json,
                cache_dir,
                total_bytes,
                created_at or _now_iso(),
                source_snapshot_mtime,
                source_update_mtime,
                source_trade_mtime,
            ),
        )


def delete_cache_entries(date: str) -> int:
    with _connect() as con:
        cursor = con.execute("DELETE FROM cache_entries WHERE date = ?", (date,))
        return int(cursor.rowcount)


def _component_file_paths(data_root: Path, date: str) -> dict[str, Path]:
    poly_root = data_root / "polymarket"
    partition = f"date={date}"
    return {
        "snapshots": poly_root / "snapshots" / partition / "snapshots.parquet",
        "updates": poly_root / "updates" / partition / "updates.parquet",
        "trades": poly_root / "trades" / partition / "trades.parquet",
        "spot": poly_root / "spot" / partition / "spot_prices.parquet",
    }


def index_date(date: str, data_root: Path | None = None) -> dict[str, Any]:
    root = (data_root or (data_home() / "data")).expanduser()
    component_paths = _component_file_paths(root, date)
    presence = {name: path.exists() for name, path in component_paths.items()}
    byte_sizes: dict[str, int | None] = {
        name: path.stat().st_size if presence[name] else None for name, path in component_paths.items()
    }

    ticker_counts: dict[str, dict[str, int]] = {"snapshots": {}, "updates": {}, "trades": {}}
    snapshot_in_window_counts: dict[str, int] = {}
    spot_in_window_counts: dict[str, int] = {}
    with duckdb.connect() as con:
        for component in ("snapshots", "updates", "trades"):
            parquet_path = component_paths[component]
            if not parquet_path.exists():
                continue
            rows = con.execute(
                "SELECT ticker, COUNT(*) AS row_count FROM read_parquet(?) GROUP BY ticker",
                [str(parquet_path)],
            ).fetchall()
            ticker_counts[component] = {str(ticker): int(row_count) for ticker, row_count in rows}

    all_tickers = set().union(*ticker_counts.values())
    ticker_windows: dict[str, tuple[int, int]] = {}
    for ticker in all_tickers:
        window = _market_window_ms(ticker)
        if window is not None:
            ticker_windows[ticker] = window

    with duckdb.connect() as con:
        if presence["snapshots"]:
            snapshot_in_window_counts = _snapshot_counts_in_window(
                con,
                snapshot_path=component_paths["snapshots"],
                ticker_windows=ticker_windows,
            )
        if presence["spot"]:
            spot_in_window_counts = _spot_counts_in_window(
                con,
                spot_path=component_paths["spot"],
                ticker_windows=ticker_windows,
            )

    ticker_rows: list[dict[str, Any]] = []
    for ticker in sorted(all_tickers):
        window = ticker_windows.get(ticker)
        market_open_ms: int | None = None
        market_close_ms: int | None = None
        duration_ms: int | None = None
        invalid_ticker_window = window is None
        if window is not None:
            market_open_ms, market_close_ms = window
            duration_ms = market_close_ms - market_open_ms

        ticker_rows.append(
            {
                "ticker": ticker,
                "snapshot_rows": ticker_counts["snapshots"].get(ticker, 0),
                "update_rows": ticker_counts["updates"].get(ticker, 0),
                "trade_rows": ticker_counts["trades"].get(ticker, 0),
                "market_open_ms": market_open_ms,
                "market_close_ms": market_close_ms,
                "duration_ms": duration_ms,
                "snapshot_in_window_rows": snapshot_in_window_counts.get(ticker, 0),
                "spot_in_window_rows": spot_in_window_counts.get(ticker, 0),
                "invalid_ticker_window": int(invalid_ticker_window),
            }
        )

    with _connect() as con:
        con.execute("DELETE FROM tickers WHERE date = ?", (date,))
    upsert_tickers(date, ticker_rows)
    upsert_date(
        date,
        has_snapshots=presence["snapshots"],
        has_updates=presence["updates"],
        has_trades=presence["trades"],
        has_spot=presence["spot"],
        snapshot_bytes=byte_sizes["snapshots"],
        update_bytes=byte_sizes["updates"],
        trade_bytes=byte_sizes["trades"],
        spot_bytes=byte_sizes["spot"],
        indexed_at=_now_iso(),
    )

    return {
        "date": date,
        "has_snapshots": presence["snapshots"],
        "has_updates": presence["updates"],
        "has_trades": presence["trades"],
        "has_spot": presence["spot"],
        "ticker_count": len(ticker_rows),
    }


def count_tickers(date: str) -> int:
    with _connect() as con:
        row = con.execute("SELECT COUNT(*) AS n FROM tickers WHERE date = ?", (date,)).fetchone()
    return int(row["n"]) if row is not None else 0


def get_component_ticker_counts(date: str) -> dict[str, int]:
    with _connect() as con:
        row = con.execute(
            """
            SELECT
                SUM(CASE WHEN snapshot_rows > 0 THEN 1 ELSE 0 END) AS snapshots,
                SUM(CASE WHEN update_rows > 0 THEN 1 ELSE 0 END) AS updates,
                SUM(CASE WHEN trade_rows > 0 THEN 1 ELSE 0 END) AS trades
            FROM tickers
            WHERE date = ?
            """,
            (date,),
        ).fetchone()
    if row is None:
        return {"snapshots": 0, "updates": 0, "trades": 0}
    return {
        "snapshots": int(row["snapshots"] or 0),
        "updates": int(row["updates"] or 0),
        "trades": int(row["trades"] or 0),
    }
