from __future__ import annotations

from pathlib import Path

import duckdb
import pytest

from poly_data import index
from poly_data import resolve as resolve_mod
from poly_data.resolve import compute_cache_key, resolve_data


@pytest.fixture
def poly_data_home(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    home = tmp_path / ".polysharp"
    monkeypatch.setattr(index, "data_home", lambda: home)
    monkeypatch.setattr(resolve_mod, "data_home", lambda: home)
    index.ensure_data_dir()
    index.init_db()
    return home


def _write_ticker_parquet(path: Path, tickers: list[str]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    con = duckdb.connect()
    con.execute("CREATE TABLE rows (ticker VARCHAR, value INTEGER)")
    con.executemany(
        "INSERT INTO rows (ticker, value) VALUES (?, ?)",
        [(ticker, i) for i, ticker in enumerate(tickers)],
    )
    con.execute("COPY rows TO ? (FORMAT PARQUET)", [str(path)])
    con.close()


def _prepare_date_data(home: Path, date: str, tickers: list[str]) -> dict[str, Path]:
    data_root = home / "data"
    partition = f"date={date}"
    paths = {
        "snapshot_path": data_root / "polymarket" / "snapshots" / partition / "snapshots.parquet",
        "update_path": data_root / "polymarket" / "updates" / partition / "updates.parquet",
        "trade_path": data_root / "polymarket" / "trades" / partition / "trades.parquet",
        "spot_event_path": data_root / "polymarket" / "spot" / partition / "spot_prices.parquet",
    }
    for path in paths.values():
        _write_ticker_parquet(path, tickers)
    return paths


def _seed_index(date: str, tickers: list[str]) -> None:
    index.upsert_date(
        date,
        has_snapshots=True,
        has_updates=True,
        has_trades=True,
        has_spot=True,
    )
    index.upsert_tickers(
        date,
        [
            {
                "ticker": ticker,
                "snapshot_rows": 1,
                "update_rows": 1,
                "trade_rows": 1,
                "market_open_ms": int(ticker.rsplit("-", 1)[1]) * 1000,
            }
            for ticker in tickers
        ],
    )


def test_compute_cache_key_is_deterministic() -> None:
    date = "2026-02-09"
    tickers_a = ["btc-updown-15m-1770645600", "btc-updown-15m-1770646500"]
    tickers_b = ["btc-updown-15m-1770646500", "btc-updown-15m-1770645600"]
    assert compute_cache_key(date, tickers_a) == compute_cache_key(date, tickers_b)


def test_resolve_cache_hit_and_mtime_invalidation(poly_data_home: Path) -> None:
    date = "2026-02-09"
    tickers = ["btc-updown-15m-1770645600", "btc-updown-15m-1770646500"]
    paths = _prepare_date_data(poly_data_home, date, tickers)
    _seed_index(date, tickers)

    manifest_1 = resolve_data(date=date, tickers=[tickers[0]])
    cache_key = compute_cache_key(date, [tickers[0]])
    entry_1 = index.lookup_cache_entry(cache_key)
    assert entry_1 is not None

    manifest_2 = resolve_data(date=date, tickers=[tickers[0]])
    entry_2 = index.lookup_cache_entry(cache_key)
    assert entry_2 is not None
    assert manifest_1 == manifest_2
    assert entry_1["source_update_mtime"] == entry_2["source_update_mtime"]

    _write_ticker_parquet(paths["update_path"], [tickers[0], tickers[0], tickers[1]])
    manifest_3 = resolve_data(date=date, tickers=[tickers[0]])
    entry_3 = index.lookup_cache_entry(cache_key)
    assert entry_3 is not None
    assert manifest_3["update_path"] == manifest_2["update_path"]
    assert entry_3["source_update_mtime"] != entry_2["source_update_mtime"]


def test_resolve_sample_latest_and_manifest_schema(poly_data_home: Path) -> None:
    date = "2026-02-09"
    tickers = [
        "btc-updown-15m-1770645600",
        "btc-updown-15m-1770646500",
        "btc-updown-15m-1770647400",
    ]
    _prepare_date_data(poly_data_home, date, tickers)
    _seed_index(date, tickers)

    latest_manifest = resolve_data(date=date, pattern="btc-updown-15m-*", latest=1, no_filter=True)
    assert latest_manifest["tickers"] == ["btc-updown-15m-1770647400"]

    sample_a = resolve_data(
        date=date,
        pattern="btc-updown-15m-*",
        sample=2,
        seed=7,
        no_filter=True,
    )
    sample_b = resolve_data(
        date=date,
        pattern="btc-updown-15m-*",
        sample=2,
        seed=7,
        no_filter=True,
    )
    assert len(sample_a["tickers"]) == 2
    assert sample_a["tickers"] == sample_b["tickers"]

    assert set(sample_a.keys()) == {
        "date",
        "snapshot_path",
        "update_path",
        "trade_path",
        "spot_event_path",
        "tickers",
    }


def test_resolve_raises_clear_error_for_unindexed_date(poly_data_home: Path) -> None:
    with pytest.raises(ValueError, match="No index entry for date=2026-02-20"):
        resolve_data(date="2026-02-20", pattern="btc-updown-15m-*")


def test_resolve_raises_clear_error_for_selector_miss(poly_data_home: Path) -> None:
    date = "2026-02-19"
    tickers = [
        "btc-updown-15m-1770645600",
        "btc-updown-15m-1770646500",
    ]
    _prepare_date_data(poly_data_home, date, tickers)
    _seed_index(date, tickers)

    with pytest.raises(
        ValueError,
        match=(
            "No tickers resolved for date=2026-02-19 with selectors: pattern=eth-updown-15m-\\*. "
            "Indexed tickers available on date=2026-02-19: 2."
        ),
    ):
        resolve_data(date=date, pattern="eth-updown-15m-*")
