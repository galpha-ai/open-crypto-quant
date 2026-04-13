from __future__ import annotations

import json
import sqlite3
from pathlib import Path

import duckdb
import pytest

from poly_data import index
from poly_data.cli import main as poly_data_main
from poly_data.prescreen import prescreen_tickers


@pytest.fixture
def poly_data_home(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    home = tmp_path / ".polysharp"
    monkeypatch.setattr(index, "data_home", lambda: home)
    index.ensure_data_dir()
    index.init_db()
    return home


def _write_snapshot_parquet(path: Path, rows: list[tuple[str, int]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    con = duckdb.connect()
    con.execute("CREATE TABLE rows (ticker VARCHAR, ts_ms BIGINT)")
    if rows:
        con.executemany("INSERT INTO rows (ticker, ts_ms) VALUES (?, ?)", rows)
    con.execute("COPY rows TO ? (FORMAT PARQUET)", [str(path)])
    con.close()


def _write_spot_parquet(path: Path, rows: list[int]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    con = duckdb.connect()
    con.execute("CREATE TABLE rows (ts_ms BIGINT)")
    if rows:
        con.executemany("INSERT INTO rows (ts_ms) VALUES (?)", [(value,) for value in rows])
    con.execute("COPY rows TO ? (FORMAT PARQUET)", [str(path)])
    con.close()


def _reason_map(report: dict[str, object]) -> dict[str, str | None]:
    rows = report["tickers"]
    assert isinstance(rows, list)
    out: dict[str, str | None] = {}
    for row in rows:
        assert isinstance(row, dict)
        ticker = row["ticker"]
        reason = row["reason"]
        assert isinstance(ticker, str)
        assert reason is None or isinstance(reason, str)
        out[ticker] = reason
    return out


def _paths_for_date(data_root: Path, date: str) -> tuple[Path, Path]:
    partition = f"date={date}"
    snapshot_path = data_root / "polymarket" / "snapshots" / partition / "snapshots.parquet"
    spot_path = data_root / "polymarket" / "spot" / partition / "spot_prices.parquet"
    return snapshot_path, spot_path


def _load_ticker_row(date: str, ticker: str) -> sqlite3.Row:
    con = sqlite3.connect(index.init_db())
    con.row_factory = sqlite3.Row
    row = con.execute("SELECT * FROM tickers WHERE date = ? AND ticker = ?", (date, ticker)).fetchone()
    con.close()
    assert row is not None
    return row


def test_index_date_persists_prescreen_metrics_and_refreshes_on_reindex(poly_data_home: Path) -> None:
    date = "2026-02-11"
    data_root = poly_data_home / "data"
    snapshot_path, spot_path = _paths_for_date(data_root, date)
    ticker = "btc-updown-15m-1000"

    _write_snapshot_parquet(snapshot_path, [(ticker, 1_010_000), (ticker, 2_100_000)])
    _write_spot_parquet(spot_path, [1_050_000])

    index.index_date(date, data_root)

    row = _load_ticker_row(date, ticker)
    assert row["snapshot_rows"] == 2
    assert row["snapshot_in_window_rows"] == 1
    assert row["spot_in_window_rows"] == 1
    assert row["invalid_ticker_window"] == 0

    _write_snapshot_parquet(snapshot_path, [(ticker, 2_000_000), (ticker, 2_100_000)])
    _write_spot_parquet(spot_path, [3_000_000])

    index.index_date(date, data_root)

    row_after = _load_ticker_row(date, ticker)
    assert row_after["snapshot_rows"] == 2
    assert row_after["snapshot_in_window_rows"] == 0
    assert row_after["spot_in_window_rows"] == 0


def test_prescreen_reasons_without_spot_requirements(poly_data_home: Path) -> None:
    date = "2026-02-12"
    data_root = poly_data_home / "data"
    snapshot_path, spot_path = _paths_for_date(data_root, date)
    eligible = "btc-updown-15m-1000"
    no_snapshots = "btc-updown-15m-2000"
    invalid = "bad-ticker"

    _write_snapshot_parquet(
        snapshot_path,
        [
            (eligible, 1_010_000),
            (eligible, 1_020_000),
            (eligible, 900_000),
            (no_snapshots, 1_000_000),
            (invalid, 1_010_000),
        ],
    )
    _write_spot_parquet(spot_path, [])
    index.index_date(date, data_root)

    filtered, report = prescreen_tickers(
        date=date,
        tickers=[eligible, no_snapshots, invalid],
        require_spot=False,
        min_snapshots=1,
        min_window_coverage=None,
    )

    reasons = _reason_map(report)
    assert filtered == [eligible]
    assert reasons[eligible] is None
    assert reasons[no_snapshots] == "no_snapshots_in_window"
    assert reasons[invalid] == "invalid_ticker_window"


def test_prescreen_reason_insufficient_snapshots_in_window(poly_data_home: Path) -> None:
    date = "2026-02-13"
    data_root = poly_data_home / "data"
    snapshot_path, spot_path = _paths_for_date(data_root, date)
    ticker = "btc-updown-15m-3000"

    _write_snapshot_parquet(snapshot_path, [(ticker, 3_010_000)])
    _write_spot_parquet(spot_path, [])
    index.index_date(date, data_root)

    filtered, report = prescreen_tickers(
        date=date,
        tickers=[ticker],
        require_spot=False,
        min_snapshots=2,
        min_window_coverage=None,
    )

    reasons = _reason_map(report)
    assert filtered == []
    assert reasons[ticker] == "insufficient_snapshots_in_window"


def test_prescreen_reason_insufficient_window_coverage(poly_data_home: Path) -> None:
    date = "2026-02-14"
    data_root = poly_data_home / "data"
    snapshot_path, spot_path = _paths_for_date(data_root, date)
    ticker = "btc-updown-15m-4000"

    _write_snapshot_parquet(
        snapshot_path,
        [
            (ticker, 4_010_000),
            (ticker, 3_500_000),
            (ticker, 5_100_000),
            (ticker, 5_200_000),
        ],
    )
    _write_spot_parquet(spot_path, [])
    index.index_date(date, data_root)

    filtered, report = prescreen_tickers(
        date=date,
        tickers=[ticker],
        require_spot=False,
        min_snapshots=1,
        min_window_coverage=0.5,
    )

    reasons = _reason_map(report)
    assert filtered == []
    assert reasons[ticker] == "insufficient_window_coverage"


def test_prescreen_reason_missing_spot_file(poly_data_home: Path) -> None:
    date = "2026-02-15"
    data_root = poly_data_home / "data"
    snapshot_path, _spot_path = _paths_for_date(data_root, date)
    ticker = "btc-updown-15m-5000"

    _write_snapshot_parquet(snapshot_path, [(ticker, 5_010_000)])
    index.index_date(date, data_root)

    filtered, report = prescreen_tickers(
        date=date,
        tickers=[ticker],
        require_spot=True,
        min_snapshots=1,
        min_window_coverage=None,
    )

    reasons = _reason_map(report)
    assert filtered == []
    assert reasons[ticker] == "missing_spot_file"


def test_prescreen_reason_no_spot_in_window(poly_data_home: Path) -> None:
    date = "2026-02-16"
    data_root = poly_data_home / "data"
    snapshot_path, spot_path = _paths_for_date(data_root, date)
    ticker = "btc-updown-15m-6000"

    _write_snapshot_parquet(snapshot_path, [(ticker, 6_010_000), (ticker, 6_020_000)])
    _write_spot_parquet(spot_path, [5_000_000, 7_000_000])
    index.index_date(date, data_root)

    filtered, report = prescreen_tickers(
        date=date,
        tickers=[ticker],
        require_spot=True,
        min_snapshots=1,
        min_window_coverage=None,
    )

    reasons = _reason_map(report)
    assert filtered == []
    assert reasons[ticker] == "no_spot_in_window"


def test_prescreen_fails_fast_when_indexed_metrics_are_missing(poly_data_home: Path) -> None:
    date = "2026-02-17"
    ticker = "btc-updown-15m-7000"

    index.upsert_date(date, has_snapshots=True, has_spot=True)
    index.upsert_tickers(
        date,
        [
            {
                "ticker": ticker,
                "snapshot_rows": 1,
                "update_rows": 0,
                "trade_rows": 0,
                "market_open_ms": 7_000_000,
                "market_close_ms": 7_900_000,
                "duration_ms": 900_000,
            }
        ],
    )

    with pytest.raises(RuntimeError, match="Re-run `uv run poly-data index --date 2026-02-17`"):
        prescreen_tickers(
            date=date,
            tickers=[ticker],
            require_spot=False,
            min_snapshots=1,
            min_window_coverage=None,
        )


def test_poly_data_prescreen_json_output_is_deterministic(poly_data_home: Path, capsys) -> None:
    date = "2026-02-18"
    data_root = poly_data_home / "data"
    snapshot_path, spot_path = _paths_for_date(data_root, date)
    eligible = "btc-updown-15m-8000"
    dropped = "btc-updown-15m-8100"

    _write_snapshot_parquet(snapshot_path, [(eligible, 8_010_000), (dropped, 7_000_000)])
    _write_spot_parquet(spot_path, [])

    assert poly_data_main(["index", "--date", date, "--data-root", str(data_root)]) == 0
    capsys.readouterr()

    args = [
        "prescreen",
        "--date",
        date,
        "--tickers",
        eligible,
        dropped,
        "--json",
        "--data-root",
        str(data_root),
    ]
    assert poly_data_main(args) == 0
    first_payload = json.loads(capsys.readouterr().out)

    assert poly_data_main(args) == 0
    second_payload = json.loads(capsys.readouterr().out)

    assert first_payload == second_payload
    assert first_payload["eligible_tickers"] == [eligible]
    assert first_payload["summary"]["candidate_tickers"] == 2
    assert first_payload["summary"]["eligible_tickers"] == 1
    reasons = {row["ticker"]: row["reason"] for row in first_payload["tickers"]}
    assert reasons[dropped] == "no_snapshots_in_window"
