from __future__ import annotations

import sqlite3
from typing import Any

from .index import init_db

_REQUIRED_TICKER_COLUMNS = {
    "ticker",
    "market_open_ms",
    "market_close_ms",
    "snapshot_rows",
    "snapshot_in_window_rows",
    "spot_in_window_rows",
    "invalid_ticker_window",
}


def market_open_ms(ticker: str) -> int | None:
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


def market_duration_ms(ticker: str) -> int | None:
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


def market_window_ms(ticker: str) -> tuple[int, int] | None:
    open_ms = market_open_ms(ticker)
    duration_ms = market_duration_ms(ticker)
    if open_ms is None or duration_ms is None:
        return None
    return open_ms, open_ms + duration_ms


def _connect_index() -> sqlite3.Connection:
    con = sqlite3.connect(init_db())
    con.row_factory = sqlite3.Row
    return con


def _chunked(items: list[str], size: int) -> list[list[str]]:
    return [items[i : i + size] for i in range(0, len(items), size)]


def _load_indexed_metrics(
    *,
    date: str,
    tickers: list[str],
) -> tuple[bool, dict[str, sqlite3.Row]]:
    unique_tickers: list[str] = []
    seen: set[str] = set()
    for ticker in tickers:
        if ticker in seen:
            continue
        seen.add(ticker)
        unique_tickers.append(ticker)

    with _connect_index() as con:
        row = con.execute("SELECT has_spot FROM dates WHERE date = ?", (date,)).fetchone()
        if row is None:
            raise RuntimeError(
                f"No indexed metadata found for date={date}. "
                f"Run `uv run poly-data index --date {date}` first."
            )

        table_columns = {str(column_row[1]) for column_row in con.execute("PRAGMA table_info(tickers)")}
        missing_columns = sorted(_REQUIRED_TICKER_COLUMNS - table_columns)
        if missing_columns:
            raise RuntimeError(
                "Index schema is missing prescreen columns: "
                f"{', '.join(missing_columns)}. "
                f"Run `uv run poly-data index --date {date}` after updating local parquet data."
            )

        metric_rows: dict[str, sqlite3.Row] = {}
        for batch in _chunked(unique_tickers, 500):
            placeholders = ",".join("?" for _ in batch)
            query = (
                "SELECT "
                "ticker, market_open_ms, market_close_ms, snapshot_rows, "
                "snapshot_in_window_rows, spot_in_window_rows, invalid_ticker_window "
                "FROM tickers "
                "WHERE date = ? AND ticker IN (" + placeholders + ")"
            )
            params: list[object] = [date, *batch]
            for metric_row in con.execute(query, params).fetchall():
                metric_rows[str(metric_row["ticker"])] = metric_row

    stale_tickers = [
        ticker
        for ticker, metric_row in metric_rows.items()
        if metric_row["snapshot_rows"] is None
        or metric_row["snapshot_in_window_rows"] is None
        or metric_row["spot_in_window_rows"] is None
        or metric_row["invalid_ticker_window"] is None
    ]
    if stale_tickers:
        sample = ", ".join(stale_tickers[:3])
        if len(stale_tickers) > 3:
            sample += ", ..."
        raise RuntimeError(
            "Indexed prescreen metrics are missing for one or more tickers "
            f"on date={date} ({sample}). "
            f"Re-run `uv run poly-data index --date {date}` to refresh indexed metrics."
        )

    return bool(row["has_spot"]), metric_rows


def prescreen_tickers(
    *,
    date: str,
    tickers: list[str],
    require_spot: bool,
    min_snapshots: int,
    min_window_coverage: float | None,
) -> tuple[list[str], dict[str, Any]]:
    has_spot, metric_rows = _load_indexed_metrics(date=date, tickers=tickers)

    report_rows: list[dict[str, Any]] = []
    eligible: list[str] = []
    dropped_by_reason: dict[str, int] = {}
    spot_file_missing = require_spot and not has_spot

    for ticker in tickers:
        reason: str | None = None
        window_start_ms: int | None = None
        window_end_ms: int | None = None
        snapshot_total = 0
        snapshot_in_window = 0
        spot_in_window = 0
        window_coverage = 0.0

        metric_row = metric_rows.get(ticker)
        if metric_row is None:
            window = market_window_ms(ticker)
            invalid_ticker_window = window is None
            if window is not None:
                window_start_ms, window_end_ms = window
        else:
            window_start_ms = (
                int(metric_row["market_open_ms"]) if metric_row["market_open_ms"] is not None else None
            )
            window_end_ms = (
                int(metric_row["market_close_ms"])
                if metric_row["market_close_ms"] is not None
                else None
            )
            snapshot_total = int(metric_row["snapshot_rows"] or 0)
            snapshot_in_window = int(metric_row["snapshot_in_window_rows"] or 0)
            spot_in_window = int(metric_row["spot_in_window_rows"] or 0)
            invalid_ticker_window = bool(metric_row["invalid_ticker_window"])

        if snapshot_total > 0:
            window_coverage = snapshot_in_window / snapshot_total

        if invalid_ticker_window:
            reason = "invalid_ticker_window"
        elif spot_file_missing:
            reason = "missing_spot_file"
        elif snapshot_in_window == 0:
            reason = "no_snapshots_in_window"
        elif snapshot_in_window < min_snapshots:
            reason = "insufficient_snapshots_in_window"
        elif min_window_coverage is not None and window_coverage < min_window_coverage:
            reason = "insufficient_window_coverage"
        elif require_spot and spot_in_window == 0:
            reason = "no_spot_in_window"

        is_eligible = reason is None
        if is_eligible:
            eligible.append(ticker)
        else:
            assert reason is not None
            dropped_by_reason[reason] = dropped_by_reason.get(reason, 0) + 1

        report_rows.append(
            {
                "ticker": ticker,
                "eligible": is_eligible,
                "reason": reason,
                "window_start_ms": window_start_ms,
                "window_end_ms": window_end_ms,
                "snapshot_total": snapshot_total,
                "snapshot_in_window": snapshot_in_window,
                "spot_in_window": spot_in_window,
                "window_coverage": round(window_coverage, 6),
            }
        )

    report: dict[str, Any] = {
        "prescreen_enabled": True,
        "inputs": {
            "date": date,
            "index_db": str(init_db()),
            "has_spot": has_spot,
        },
        "criteria": {
            "require_spot": require_spot,
            "min_snapshots": min_snapshots,
            "min_window_coverage": min_window_coverage,
        },
        "summary": {
            "candidate_tickers": len(tickers),
            "eligible_tickers": len(eligible),
            "dropped_tickers": len(tickers) - len(eligible),
            "dropped_by_reason": dict(sorted(dropped_by_reason.items())),
        },
        "tickers": report_rows,
    }
    return eligible, report
