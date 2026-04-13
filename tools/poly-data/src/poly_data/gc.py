from __future__ import annotations

import shutil
import sqlite3
from datetime import UTC, datetime
from pathlib import Path
from typing import Sequence

from .index import data_home, delete_cache_entries, delete_date, init_db


def _now_iso() -> str:
    return datetime.now(UTC).isoformat()


def _discover_local_dates(data_root: Path) -> list[str]:
    dates: set[str] = set()
    poly_root = data_root / "polymarket"
    for component in ("snapshots", "updates", "trades", "spot"):
        component_root = poly_root / component
        if not component_root.exists():
            continue
        for partition in component_root.glob("date=*"):
            if partition.is_dir() and "=" in partition.name:
                _, date = partition.name.split("=", 1)
                if date:
                    dates.add(date)
    return sorted(dates)


def _path_size_bytes(path: Path) -> int:
    if not path.exists():
        return 0
    if path.is_file():
        return path.stat().st_size
    return sum(child.stat().st_size for child in path.rglob("*") if child.is_file())


def _remove_path(path: Path, dry_run: bool) -> int:
    size = _path_size_bytes(path)
    if path.exists() and not dry_run:
        if path.is_dir():
            shutil.rmtree(path)
        else:
            path.unlink()
    return size


def _load_cache_dirs_for_dates(db_path: Path, dates: Sequence[str] | None) -> set[Path]:
    with sqlite3.connect(db_path) as con:
        if dates is None:
            rows = con.execute("SELECT DISTINCT cache_dir FROM cache_entries").fetchall()
        elif not dates:
            rows = []
        else:
            placeholders = ", ".join(["?"] * len(dates))
            rows = con.execute(
                f"SELECT DISTINCT cache_dir FROM cache_entries WHERE date IN ({placeholders})",
                list(dates),
            ).fetchall()
    return {Path(str(row[0])) for row in rows if row and row[0]}


def _clear_cache_entries_table(db_path: Path, dry_run: bool) -> None:
    if dry_run:
        return
    with sqlite3.connect(db_path) as con:
        con.execute("DELETE FROM cache_entries")


def _zero_out_update_rows(db_path: Path, date: str, dry_run: bool) -> None:
    if dry_run:
        return
    with sqlite3.connect(db_path) as con:
        con.execute(
            "UPDATE dates SET has_updates = 0, update_bytes = NULL, indexed_at = ? WHERE date = ?",
            (_now_iso(), date),
        )
        con.execute("UPDATE tickers SET update_rows = 0 WHERE date = ?", (date,))


def gc_data(
    *,
    keep_days: int | None = None,
    keep_dates: Sequence[str] | None = None,
    keep_days_updates: int | None = None,
    cache_only: bool = False,
    dry_run: bool = False,
    data_root: Path | None = None,
) -> dict[str, object]:
    root = (data_root or (data_home() / "data")).expanduser()
    db_path = init_db()
    all_dates = _discover_local_dates(root)

    keep_date_set = set(keep_dates or [])
    if keep_days is not None:
        keep_date_set.update(all_dates[-max(0, keep_days) :])

    full_delete_dates: set[str] = set()
    if not cache_only and (keep_dates is not None or keep_days is not None):
        full_delete_dates = set(all_dates) - keep_date_set

    update_delete_dates: set[str] = set()
    if not cache_only and keep_days_updates is not None:
        keep_updates = set(all_dates[-max(0, keep_days_updates) :])
        update_delete_dates = set(all_dates) - keep_updates
        update_delete_dates -= full_delete_dates

    cache_target_dates: set[str] | None
    if cache_only:
        cache_target_dates = None
    else:
        cache_target_dates = full_delete_dates | update_delete_dates

    cache_dirs = _load_cache_dirs_for_dates(db_path, None if cache_only else sorted(cache_target_dates))
    freed_bytes = 0

    for date in sorted(full_delete_dates):
        for component in ("snapshots", "updates", "trades", "spot"):
            component_dir = root / "polymarket" / component / f"date={date}"
            freed_bytes += _remove_path(component_dir, dry_run)
        if not dry_run:
            delete_date(date)
            delete_cache_entries(date)

    for date in sorted(update_delete_dates):
        updates_dir = root / "polymarket" / "updates" / f"date={date}"
        freed_bytes += _remove_path(updates_dir, dry_run)
        _zero_out_update_rows(db_path, date, dry_run)
        if not dry_run:
            delete_cache_entries(date)

    for cache_dir in sorted(cache_dirs, key=str):
        freed_bytes += _remove_path(cache_dir, dry_run)

    if cache_only:
        cache_root = data_home() / "cache"
        if cache_root.exists():
            for path in cache_root.iterdir():
                freed_bytes += _remove_path(path, dry_run)
        _clear_cache_entries_table(db_path, dry_run)

    return {
        "dry_run": dry_run,
        "cache_only": cache_only,
        "deleted_dates": sorted(full_delete_dates),
        "deleted_updates_dates": sorted(update_delete_dates),
        "cache_dirs_deleted": len(cache_dirs),
        "bytes_freed": int(freed_bytes),
    }
