from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Any, Sequence

import duckdb

from .index import (
    count_tickers,
    data_home,
    get_date_status,
    list_tickers,
    lookup_cache_entry,
    upsert_cache_entry,
)


def compute_cache_key(date: str, tickers: Sequence[str]) -> str:
    payload = f"{date}\0" + "\0".join(sorted(tickers))
    return hashlib.sha256(payload.encode("utf-8")).hexdigest()[:12]


def cache_dir_path(cache_key: str) -> Path:
    return data_home() / "cache" / cache_key


def _source_paths_for_date(date: str, data_root: Path | None = None) -> dict[str, Path]:
    root = (data_root or (data_home() / "data")).expanduser()
    partition = f"date={date}"
    base = root / "polymarket"
    return {
        "snapshot_path": base / "snapshots" / partition / "snapshots.parquet",
        "update_path": base / "updates" / partition / "updates.parquet",
        "trade_path": base / "trades" / partition / "trades.parquet",
        "spot_event_path": base / "spot" / partition / "spot_prices.parquet",
    }


def _mtime_string(path: Path) -> str | None:
    if not path.exists():
        return None
    return str(path.stat().st_mtime_ns)


def is_cache_valid(cache_key: str, source_mtimes: dict[str, str | None]) -> bool:
    entry = lookup_cache_entry(cache_key)
    if entry is None:
        return False

    cache_dir = Path(str(entry["cache_dir"]))
    if not cache_dir.exists():
        return False
    if not (cache_dir / "snapshots.parquet").exists():
        return False
    if not (cache_dir / "updates.parquet").exists():
        return False
    if not (cache_dir / "trades.parquet").exists():
        return False

    if entry.get("source_snapshot_mtime") != source_mtimes.get("source_snapshot_mtime"):
        return False
    if entry.get("source_update_mtime") != source_mtimes.get("source_update_mtime"):
        return False
    if entry.get("source_trade_mtime") != source_mtimes.get("source_trade_mtime"):
        return False
    return True


def filter_parquet(source_path: Path, dest_path: Path, tickers: Sequence[str]) -> None:
    if not tickers:
        raise ValueError("filter_parquet requires at least one ticker")

    placeholders = ", ".join(["?"] * len(tickers))
    escaped_dest = str(dest_path).replace("'", "''")

    dest_path.parent.mkdir(parents=True, exist_ok=True)
    with duckdb.connect() as con:
        # DuckDB widens TIMESTAMP_MILLIS to TIMESTAMP (us) internally, so
        # the Parquet writer emits TIMESTAMP_US by default.  Downstream Rust
        # loaders require the original TIMESTAMP_MS precision.  Detect
        # millisecond columns from Parquet physical metadata and cast them
        # back to TIMESTAMP_MS in the COPY query.
        ms_cols: set[str] = set()
        for row in con.execute(
            "SELECT name, converted_type FROM parquet_schema(?)", [str(source_path)]
        ).fetchall():
            if row[1] == "TIMESTAMP_MILLIS":
                ms_cols.add(row[0])

        if ms_cols:
            all_cols = [
                row[0]
                for row in con.execute(
                    "SELECT column_name FROM (DESCRIBE SELECT * FROM read_parquet(?))",
                    [str(source_path)],
                ).fetchall()
            ]
            select_parts = [
                f'CAST("{c}" AS TIMESTAMP_MS) AS "{c}"' if c in ms_cols else f'"{c}"'
                for c in all_cols
            ]
            select_clause = ", ".join(select_parts)
        else:
            select_clause = "*"

        sql = (
            f"COPY (SELECT {select_clause} FROM read_parquet(?) WHERE ticker IN ({placeholders})) "
            f"TO '{escaped_dest}' (FORMAT PARQUET)"
        )
        params: list[Any] = [str(source_path), *tickers]
        con.execute(sql, params)


def _build_manifest(
    date: str,
    *,
    snapshot_path: Path,
    update_path: Path,
    trade_path: Path,
    spot_event_path: Path,
    tickers: list[str],
) -> dict[str, Any]:
    return {
        "date": date,
        "snapshot_path": str(snapshot_path),
        "update_path": str(update_path),
        "trade_path": str(trade_path),
        "spot_event_path": str(spot_event_path),
        "tickers": tickers,
    }


def resolve_data(
    *,
    date: str,
    tickers: Sequence[str] | None = None,
    pattern: str | None = None,
    regex: str | None = None,
    sample: int | None = None,
    latest: int | None = None,
    after: str | None = None,
    seed: int | None = None,
    no_filter: bool = False,
    data_root: Path | None = None,
) -> dict[str, Any]:
    source_paths = _source_paths_for_date(date, data_root)

    if tickers:
        requested = list(dict.fromkeys(tickers))
        available = set(list_tickers(date))
        missing = [ticker for ticker in requested if ticker not in available]
        if missing:
            raise ValueError(f"Unknown tickers for date={date}: {', '.join(missing)}")
        selected_tickers = requested
    else:
        selected_tickers = list_tickers(
            date,
            pattern=pattern,
            regex=regex,
            sample=sample,
            latest=latest,
            after=after,
            seed=seed,
        )

    if not selected_tickers:
        if get_date_status(date) is None:
            raise ValueError(
                f"No index entry for date={date}. "
                f"Run `uv run poly-data sync --date {date}` "
                f"or choose an indexed date from `uv run poly-data status`."
            )

        selector_parts: list[str] = []
        if tickers:
            selector_parts.append(f"tickers={len(tickers)}")
        if pattern:
            selector_parts.append(f"pattern={pattern}")
        if regex:
            selector_parts.append(f"regex={regex}")
        if sample is not None:
            selector_parts.append(f"sample={sample}")
        if latest is not None:
            selector_parts.append(f"latest={latest}")
        if after:
            selector_parts.append(f"after={after}")
        selector_text = ", ".join(selector_parts) if selector_parts else "none"
        available_count = count_tickers(date)
        raise ValueError(
            f"No tickers resolved for date={date} with selectors: {selector_text}. "
            f"Indexed tickers available on date={date}: {available_count}."
        )

    if no_filter:
        return _build_manifest(
            date,
            snapshot_path=source_paths["snapshot_path"],
            update_path=source_paths["update_path"],
            trade_path=source_paths["trade_path"],
            spot_event_path=source_paths["spot_event_path"],
            tickers=selected_tickers,
        )

    cache_key = compute_cache_key(date, selected_tickers)
    cache_dir = cache_dir_path(cache_key)
    cache_snapshot = cache_dir / "snapshots.parquet"
    cache_update = cache_dir / "updates.parquet"
    cache_trade = cache_dir / "trades.parquet"
    source_mtimes = {
        "source_snapshot_mtime": _mtime_string(source_paths["snapshot_path"]),
        "source_update_mtime": _mtime_string(source_paths["update_path"]),
        "source_trade_mtime": _mtime_string(source_paths["trade_path"]),
    }

    if is_cache_valid(cache_key, source_mtimes):
        return _build_manifest(
            date,
            snapshot_path=cache_snapshot,
            update_path=cache_update,
            trade_path=cache_trade,
            spot_event_path=source_paths["spot_event_path"],
            tickers=selected_tickers,
        )

    filter_parquet(source_paths["snapshot_path"], cache_snapshot, selected_tickers)
    filter_parquet(source_paths["update_path"], cache_update, selected_tickers)
    filter_parquet(source_paths["trade_path"], cache_trade, selected_tickers)

    manifest = _build_manifest(
        date,
        snapshot_path=cache_snapshot,
        update_path=cache_update,
        trade_path=cache_trade,
        spot_event_path=source_paths["spot_event_path"],
        tickers=selected_tickers,
    )
    cache_dir.mkdir(parents=True, exist_ok=True)
    (cache_dir / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True))

    total_bytes = (
        cache_snapshot.stat().st_size + cache_update.stat().st_size + cache_trade.stat().st_size
    )
    upsert_cache_entry(
        cache_key,
        date=date,
        tickers=selected_tickers,
        cache_dir=str(cache_dir),
        total_bytes=total_bytes,
        source_snapshot_mtime=source_mtimes["source_snapshot_mtime"],
        source_update_mtime=source_mtimes["source_update_mtime"],
        source_trade_mtime=source_mtimes["source_trade_mtime"],
    )
    return manifest
