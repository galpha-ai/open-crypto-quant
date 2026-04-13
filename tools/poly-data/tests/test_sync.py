from __future__ import annotations

from pathlib import Path

from poly_data import sync


def _touch(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b"x")


def test_sync_date_skip_existing_skips_all_present_files(
    tmp_path: Path, monkeypatch
) -> None:
    date = "2026-02-19"
    data_root = tmp_path / "data"
    partition = f"date={date}"
    _touch(data_root / "polymarket" / "snapshots" / partition / "snapshots.parquet")
    _touch(data_root / "polymarket" / "updates" / partition / "updates.parquet")
    _touch(data_root / "polymarket" / "trades" / partition / "trades.parquet")
    _touch(data_root / "polymarket" / "spot" / partition / "spot_prices.parquet")

    commands: list[list[str]] = []
    index_calls: list[tuple[str, Path]] = []

    monkeypatch.setattr(sync, "ensure_rsync", lambda: None)
    monkeypatch.setattr(sync, "run_cmd", lambda cmd, dry_run: commands.append(cmd))
    monkeypatch.setattr(
        sync,
        "index_date",
        lambda date_value, root_value: index_calls.append((date_value, root_value))
        or {"ticker_count": 0},
    )
    monkeypatch.setattr(sync, "delete_cache_entries", lambda _: 0)

    result = sync.sync_date(date, data_root=data_root, skip_existing=True)

    assert commands == []
    assert index_calls == [(date, data_root)]
    assert result["indexed"] is True


def test_sync_date_skip_existing_downloads_only_missing_files(
    tmp_path: Path, monkeypatch
) -> None:
    date = "2026-02-19"
    data_root = tmp_path / "data"
    partition = f"date={date}"
    _touch(data_root / "polymarket" / "snapshots" / partition / "snapshots.parquet")

    commands: list[list[str]] = []

    monkeypatch.setattr(sync, "ensure_rsync", lambda: None)
    monkeypatch.setattr(sync, "run_cmd", lambda cmd, dry_run: commands.append(cmd))
    monkeypatch.setattr(sync, "index_date", lambda *_: {"ticker_count": 0})
    monkeypatch.setattr(sync, "delete_cache_entries", lambda _: 0)

    sync.sync_date(date, data_root=data_root, skip_existing=True)

    assert len(commands) == 3
    assert all("--ignore-existing" in command for command in commands)
    assert any(f"/updates/date={date}/updates.parquet" in command[-2] for command in commands)
    assert any(f"/trades/date={date}/trades.parquet" in command[-2] for command in commands)
    assert any(
        f"spot_prices/daily/date={date}/spot_prices.parquet" in command[-2]
        for command in commands
    )
