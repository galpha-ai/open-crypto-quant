from __future__ import annotations

import argparse
import json
import sqlite3
from collections import Counter
from pathlib import Path

from poly_data.completeness.config import DataCompletenessConfig
from poly_data.completeness.manifest_builder import build_manifest

from .index import (
    count_tickers,
    data_home,
    get_component_ticker_counts,
    get_date_status,
    index_date,
    list_dates,
    list_tickers,
)
from .gc import gc_data
from .sync import (
    DEFAULT_COMPONENTS,
    DEFAULT_HOST,
    DEFAULT_REMOTE_ROOT,
    DEFAULT_USER,
    discover_remote_dates,
    parse_date_range,
    sync_date,
)
from .prescreen import prescreen_tickers
from .resolve import resolve_data


def _format_bytes(size: int) -> str:
    if size < 1024:
        return f"{size} B"
    if size < 1024**2:
        return f"{size / 1024:.1f} KB"
    if size < 1024**3:
        return f"{size / 1024**2:.1f} MB"
    return f"{size / 1024**3:.1f} GB"


def _default_data_root() -> Path:
    return data_home() / "data"


def _discover_local_dates(data_root: Path) -> list[str]:
    poly_root = data_root / "polymarket"
    dates: set[str] = set()
    for component in ("snapshots", "updates", "trades", "spot"):
        base = poly_root / component
        if not base.exists():
            continue
        for path in base.glob("date=*"):
            if not path.is_dir():
                continue
            if "=" not in path.name:
                continue
            _, value = path.name.split("=", 1)
            if value:
                dates.add(value)
    return sorted(dates)


def _ticker_pattern(ticker: str) -> str:
    parts = ticker.split("-")
    if len(parts) < 2:
        return ticker
    return "-".join(parts[:-1]) + "-*"


def _unique_preserving_order(items: list[str]) -> list[str]:
    out: list[str] = []
    seen: set[str] = set()
    for item in items:
        if item in seen:
            continue
        seen.add(item)
        out.append(item)
    return out


def _parse_dates_for_sync(args: argparse.Namespace) -> list[str]:
    specified = int(bool(args.date)) + int(bool(args.date_range)) + int(args.latest is not None)
    if specified != 1:
        raise SystemExit("sync requires exactly one of --date, --date-range, or --latest.")

    if args.date:
        return [args.date]
    if args.latest is not None:
        if args.latest <= 0:
            raise SystemExit("--latest must be > 0.")
        remote_dates = discover_remote_dates(args.host, args.user, args.remote_root)
        return remote_dates[-args.latest :]
    try:
        return parse_date_range(args.date_range)
    except ValueError as exc:
        raise SystemExit(str(exc)) from exc


def _handle_sync(args: argparse.Namespace) -> int:
    dates = _parse_dates_for_sync(args)
    for date in dates:
        result = sync_date(
            date,
            host=args.host,
            user=args.user,
            remote_root=args.remote_root,
            components=args.components,
            data_root=args.data_root,
            skip_existing=args.skip_existing,
            dry_run=args.dry_run,
        )
        print(
            f"[sync] date={date} indexed={result['indexed']} cache_entries_deleted={result['cache_entries_deleted']}"
        )
    if args.purge:
        summary = gc_data(
            keep_dates=dates,
            dry_run=args.dry_run,
            data_root=args.data_root,
        )
        print(
            f"[purge] deleted_dates={len(summary['deleted_dates'])} "
            f"cache_dirs_deleted={summary['cache_dirs_deleted']} "
            f"bytes_freed={summary['bytes_freed']}"
        )
    return 0


def _handle_index(args: argparse.Namespace) -> int:
    if args.date:
        result = index_date(args.date, args.data_root)
        print(f"[index] date={args.date} tickers={result['ticker_count']}")
        return 0

    dates = _discover_local_dates(args.data_root)
    for date in dates:
        result = index_date(date, args.data_root)
        print(f"[index] date={date} tickers={result['ticker_count']}")
    print(f"[index] indexed {len(dates)} dates")
    return 0


def _handle_status(args: argparse.Namespace) -> int:
    if args.date:
        row = get_date_status(args.date)
        if row is None:
            print(f"No index entry for date={args.date}")
            return 1

        component_counts = get_component_ticker_counts(args.date)
        tickers = list_tickers(args.date)
        pattern_counts = Counter(_ticker_pattern(ticker) for ticker in tickers)

        print(f"Date: {args.date}")
        print("Components:")
        print(
            f"  snapshots: {component_counts['snapshots']} tickers, {_format_bytes(int(row.get('snapshot_bytes') or 0))}"
        )
        print(
            f"  updates:   {component_counts['updates']} tickers, {_format_bytes(int(row.get('update_bytes') or 0))}"
        )
        print(
            f"  trades:    {component_counts['trades']} tickers, {_format_bytes(int(row.get('trade_bytes') or 0))}"
        )
        spot_state = "present" if row.get("has_spot") else "missing"
        print(f"  spot:      {spot_state}, {_format_bytes(int(row.get('spot_bytes') or 0))}")
        print("Ticker patterns:")
        for pattern, count in pattern_counts.most_common(20):
            print(f"  {pattern}: {count}")
        return 0

    rows = list_dates()
    print("Date        Snap  Upd  Trade Spot  Tickers  Size")
    total_bytes = 0
    for row in rows:
        date = str(row["date"])
        ticker_count = count_tickers(date)
        size = int(row.get("snapshot_bytes") or 0)
        size += int(row.get("update_bytes") or 0)
        size += int(row.get("trade_bytes") or 0)
        size += int(row.get("spot_bytes") or 0)
        total_bytes += size

        print(
            f"{date:<10}  "
            f"{'yes' if row.get('has_snapshots') else 'no ':<4}  "
            f"{'yes' if row.get('has_updates') else 'no ':<3}  "
            f"{'yes' if row.get('has_trades') else 'no ':<5}  "
            f"{'yes' if row.get('has_spot') else 'no ':<4}  "
            f"{ticker_count:<7}  "
            f"{_format_bytes(size)}"
        )
    print(f"Total: {len(rows)} dates, {_format_bytes(total_bytes)}")
    return 0


def _handle_tickers(args: argparse.Namespace) -> int:
    tickers = list_tickers(
        args.date,
        pattern=args.pattern,
        regex=args.regex,
        sample=args.sample,
        latest=args.latest,
        after=args.after,
        seed=args.seed,
    )
    if args.json:
        print(json.dumps(tickers))
        return 0

    for ticker in tickers:
        print(ticker)
    return 0


def _handle_resolve(args: argparse.Namespace) -> int:
    if args.tickers and any(
        option is not None for option in (args.pattern, args.regex, args.sample, args.latest, args.after)
    ):
        raise SystemExit(
            "--tickers cannot be combined with --pattern/--regex/--sample/--latest/--after"
        )

    manifest = resolve_data(
        date=args.date,
        tickers=args.tickers,
        pattern=args.pattern,
        regex=args.regex,
        sample=args.sample,
        latest=args.latest,
        after=args.after,
        seed=args.seed,
        no_filter=args.no_filter,
        data_root=args.data_root,
    )

    if args.json:
        print(json.dumps(manifest))
        return 0

    print(f"Date: {manifest['date']}")
    print(f"Tickers: {len(manifest['tickers'])}")
    print(f"snapshot_path: {manifest['snapshot_path']}")
    print(f"update_path: {manifest['update_path']}")
    print(f"trade_path: {manifest['trade_path']}")
    print(f"spot_event_path: {manifest['spot_event_path']}")
    return 0


def _handle_gc(args: argparse.Namespace) -> int:
    if not args.cache_only and args.keep_days is None and not args.keep_dates and args.keep_days_updates is None:
        raise SystemExit(
            "gc requires one of --keep-days, --keep-dates, --keep-days-updates, or --cache-only"
        )

    summary = gc_data(
        keep_days=args.keep_days,
        keep_dates=args.keep_dates,
        keep_days_updates=args.keep_days_updates,
        cache_only=args.cache_only,
        dry_run=args.dry_run,
        data_root=args.data_root,
    )
    print(
        f"[gc] deleted_dates={len(summary['deleted_dates'])} "
        f"deleted_updates_dates={len(summary['deleted_updates_dates'])} "
        f"cache_dirs_deleted={summary['cache_dirs_deleted']} "
        f"bytes_freed={summary['bytes_freed']}"
    )
    return 0


def _handle_completeness(args: argparse.Namespace) -> int:
    config = DataCompletenessConfig.load(args.config)
    paths = build_manifest(
        config=config,
        config_path=args.config,
        date=args.date,
        data_root=args.data_root,
        synthetic_bbo=args.synthetic_bbo,
        spot_prices=args.spot_prices,
        out_dir=args.out_dir,
    )

    if args.json:
        payload = {
            "date": args.date,
            "allowlist_csv": str(paths.allowlist_csv),
            "exclusions_csv": str(paths.exclusions_csv),
            "summary_json": str(paths.summary_json),
        }
        print(json.dumps(payload, sort_keys=True))
        return 0

    print("Wrote:")
    print(f"- allowlist:   {paths.allowlist_csv}")
    print(f"- exclusions:  {paths.exclusions_csv}")
    print(f"- summary:     {paths.summary_json}")
    return 0


def _handle_prescreen(args: argparse.Namespace) -> int:
    if args.tickers and any(
        option is not None for option in (args.pattern, args.regex, args.sample, args.latest, args.after)
    ):
        raise SystemExit(
            "--tickers cannot be combined with --pattern/--regex/--sample/--latest/--after"
        )
    if args.min_snapshots <= 0:
        raise SystemExit("--min-snapshots must be > 0")
    if args.min_window_coverage is not None and (
        args.min_window_coverage < 0.0 or args.min_window_coverage > 1.0
    ):
        raise SystemExit("--min-window-coverage must be in [0, 1]")

    if args.tickers:
        selected_tickers = _unique_preserving_order(args.tickers)
    else:
        selected_tickers = list_tickers(
            args.date,
            pattern=args.pattern,
            regex=args.regex,
            sample=args.sample,
            latest=args.latest,
            after=args.after,
            seed=args.seed,
        )

    if not selected_tickers:
        raise SystemExit(f"No tickers resolved for date={args.date}")

    eligible_tickers, report = prescreen_tickers(
        date=args.date,
        tickers=selected_tickers,
        require_spot=bool(args.require_spot),
        min_snapshots=args.min_snapshots,
        min_window_coverage=args.min_window_coverage,
    )
    payload: dict[str, object] = {
        "date": args.date,
        "eligible_tickers": eligible_tickers,
        **report,
    }

    if args.json:
        print(json.dumps(payload, sort_keys=True))
        return 0

    summary = report["summary"]
    print(
        "[prescreen] "
        f"date={args.date} "
        f"candidates={summary['candidate_tickers']} "
        f"eligible={summary['eligible_tickers']} "
        f"dropped={summary['dropped_tickers']}"
    )
    if summary["dropped_by_reason"]:
        reasons = ", ".join(
            f"{reason}={count}" for reason, count in summary["dropped_by_reason"].items()
        )
        print(f"[prescreen] dropped_by_reason: {reasons}")
    if eligible_tickers:
        print("[prescreen] eligible tickers:")
        for ticker in eligible_tickers:
            print(ticker)

    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="poly-data", description="Backtest data manager")
    subparsers = parser.add_subparsers(dest="command", required=True)

    sync_parser = subparsers.add_parser("sync", help="Download and index backtest parquet data")
    sync_parser.add_argument("--date", help="Date in YYYY-MM-DD")
    sync_parser.add_argument("--date-range", help="Date range: YYYY-MM-DD..YYYY-MM-DD")
    sync_parser.add_argument("--latest", type=int, help="Sync latest N dates from remote listing")
    sync_parser.add_argument(
        "--components",
        nargs="+",
        choices=sorted(DEFAULT_COMPONENTS),
        default=list(DEFAULT_COMPONENTS),
    )
    sync_parser.add_argument("--host", default=DEFAULT_HOST)
    sync_parser.add_argument("--user", default=DEFAULT_USER)
    sync_parser.add_argument("--remote-root", default=DEFAULT_REMOTE_ROOT)
    sync_parser.add_argument("--data-root", type=Path, default=_default_data_root())
    sync_parser.add_argument(
        "--skip-existing",
        action="store_true",
        help="Skip downloads for parquet files that already exist locally.",
    )
    sync_parser.add_argument("--dry-run", action="store_true")
    sync_parser.add_argument("--purge", action="store_true")
    sync_parser.set_defaults(func=_handle_sync)

    index_parser = subparsers.add_parser("index", help="Rebuild index from local files")
    index_parser.add_argument("--date", help="Date in YYYY-MM-DD")
    index_parser.add_argument("--data-root", type=Path, default=_default_data_root())
    index_parser.set_defaults(func=_handle_index)

    status_parser = subparsers.add_parser("status", help="Show indexed data availability")
    status_parser.add_argument("--date", help="Date in YYYY-MM-DD")
    status_parser.set_defaults(func=_handle_status)

    tickers_parser = subparsers.add_parser("tickers", help="List indexed tickers")
    tickers_parser.add_argument("--date", required=True, help="Date in YYYY-MM-DD")
    tickers_parser.add_argument("--pattern")
    tickers_parser.add_argument("--regex")
    tickers_parser.add_argument("--sample", type=int)
    tickers_parser.add_argument("--latest", type=int)
    tickers_parser.add_argument("--after", help="HH:MM (UTC)")
    tickers_parser.add_argument("--seed", type=int)
    tickers_parser.add_argument("--json", action="store_true")
    tickers_parser.set_defaults(func=_handle_tickers)

    resolve_parser = subparsers.add_parser("resolve", help="Resolve data paths for a ticker set")
    resolve_parser.add_argument("--date", required=True, help="Date in YYYY-MM-DD")
    resolve_parser.add_argument("--tickers", nargs="+")
    resolve_parser.add_argument("--pattern")
    resolve_parser.add_argument("--regex")
    resolve_parser.add_argument("--sample", type=int)
    resolve_parser.add_argument("--latest", type=int)
    resolve_parser.add_argument("--after", help="HH:MM (UTC)")
    resolve_parser.add_argument("--seed", type=int)
    resolve_parser.add_argument("--no-filter", action="store_true")
    resolve_parser.add_argument("--data-root", type=Path, default=_default_data_root())
    resolve_parser.add_argument("--json", action="store_true")
    resolve_parser.set_defaults(func=_handle_resolve)

    prescreen_parser = subparsers.add_parser(
        "prescreen", help="Prescreen tickers by market-window snapshot/spot coverage"
    )
    prescreen_parser.add_argument("--date", required=True, help="Date in YYYY-MM-DD")
    prescreen_parser.add_argument("--tickers", nargs="+")
    prescreen_parser.add_argument("--pattern")
    prescreen_parser.add_argument("--regex")
    prescreen_parser.add_argument("--sample", type=int)
    prescreen_parser.add_argument("--latest", type=int)
    prescreen_parser.add_argument("--after", help="HH:MM (UTC)")
    prescreen_parser.add_argument("--seed", type=int)
    prescreen_parser.add_argument(
        "--require-spot",
        dest="require_spot",
        action="store_const",
        const=True,
        default=False,
        help="Require at least one spot row in the ticker's market window.",
    )
    prescreen_parser.add_argument(
        "--no-require-spot",
        dest="require_spot",
        action="store_const",
        const=False,
        help="Do not require spot coverage in the ticker's market window (default).",
    )
    prescreen_parser.add_argument("--min-snapshots", type=int, default=1)
    prescreen_parser.add_argument("--min-window-coverage", type=float)
    prescreen_parser.add_argument("--data-root", type=Path, default=_default_data_root())
    prescreen_parser.add_argument("--json", action="store_true")
    prescreen_parser.set_defaults(func=_handle_prescreen)

    gc_parser = subparsers.add_parser("gc", help="Garbage collect old data/cache")
    gc_parser.add_argument("--keep-days", type=int)
    gc_parser.add_argument("--keep-dates", nargs="*")
    gc_parser.add_argument("--keep-days-updates", type=int)
    gc_parser.add_argument("--cache-only", action="store_true")
    gc_parser.add_argument("--dry-run", action="store_true")
    gc_parser.add_argument("--data-root", type=Path, default=_default_data_root())
    gc_parser.set_defaults(func=_handle_gc)

    completeness_parser = subparsers.add_parser(
        "completeness",
        help="Build data completeness manifest artifacts (allowlist/exclusions/summary)",
    )
    completeness_parser.add_argument("--date", required=True, help="Date in YYYY-MM-DD")
    completeness_parser.add_argument(
        "--config",
        type=Path,
        default=Path("configs/data_completeness.yaml"),
        help="Path to data completeness config YAML.",
    )
    completeness_parser.add_argument(
        "--data-root",
        type=Path,
        default=None,
        help="Override config.inputs.data_root for path templates.",
    )
    completeness_parser.add_argument(
        "--synthetic-bbo",
        type=Path,
        default=None,
        help="Override synthetic BBO parquet path.",
    )
    completeness_parser.add_argument(
        "--spot-prices",
        type=Path,
        default=None,
        help="Override spot prices parquet path.",
    )
    completeness_parser.add_argument(
        "--out-dir",
        type=Path,
        default=None,
        help="Override output directory for generated artifacts.",
    )
    completeness_parser.add_argument("--json", action="store_true")
    completeness_parser.set_defaults(func=_handle_completeness)

    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        return int(args.func(args))
    except RuntimeError as exc:
        raise SystemExit(str(exc)) from exc
    except sqlite3.Error as exc:
        raise SystemExit(f"SQLite error: {exc}") from exc


if __name__ == "__main__":
    raise SystemExit(main())
