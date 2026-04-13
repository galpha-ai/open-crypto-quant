from __future__ import annotations

import csv
import json
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any

import duckdb

from poly_data.completeness.config import DataCompletenessConfig

MANIFEST_VERSION = 2


@dataclass(frozen=True)
class OutcomeStats:
    ticker: str
    outcome: str
    asset_id: str | None
    asset_id_count: int
    rows: int
    valid_rows: int
    invalid_rows: int
    min_ts: datetime | None
    max_ts: datetime | None
    null_best_bid_rows: int
    null_best_ask_rows: int
    crossed_rows: int
    bid_out_of_bounds_rows: int
    ask_out_of_bounds_rows: int
    last_valid_ts: datetime | None
    max_inter_event_gap_ms: int | None


@dataclass(frozen=True)
class EventStats:
    ticker: str
    outcome: str
    events_total: int
    usable_events: int
    usable_ratio: float


@dataclass(frozen=True)
class SpotStats:
    symbol: str
    rows: int
    valid_rows: int
    invalid_rows: int
    min_ts: datetime | None
    max_ts: datetime | None


@dataclass(frozen=True)
class ManifestPaths:
    synthetic_bbo: Path
    spot_prices: Path | None
    out_dir: Path
    allowlist_csv: Path
    exclusions_csv: Path
    summary_json: Path


def resolve_paths(
    *,
    config: DataCompletenessConfig,
    date: str,
    data_root: Path | None = None,
    synthetic_bbo: Path | None = None,
    spot_prices: Path | None = None,
    out_dir: Path | None = None,
) -> ManifestPaths:
    root = data_root or config.inputs.data_root

    if synthetic_bbo is not None:
        synthetic = synthetic_bbo
    elif config.inputs.synthetic_bbo_path.strip():
        synthetic = Path(config.inputs.synthetic_bbo_path)
    else:
        synthetic = Path(
            config.inputs.synthetic_bbo_template.format(
                data_root=str(root),
                date=date,
            )
        )

    if config.spot.enabled:
        if spot_prices is not None:
            spot_path = spot_prices
        elif config.inputs.spot_prices_path.strip():
            spot_path = Path(config.inputs.spot_prices_path)
        else:
            spot_path = Path(
                config.inputs.spot_prices_template.format(
                    data_root=str(root),
                    date=date,
                )
            )
    else:
        spot_path = None

    resolved_out_dir = (
        out_dir
        or Path(
            config.outputs.out_dir_template.format(
                date=date,
            )
        )
    )

    return ManifestPaths(
        synthetic_bbo=synthetic,
        spot_prices=spot_path,
        out_dir=resolved_out_dir,
        allowlist_csv=resolved_out_dir / config.outputs.manifest_csv,
        exclusions_csv=resolved_out_dir / config.outputs.exclusions_csv,
        summary_json=resolved_out_dir / config.outputs.summary_json,
    )


def build_manifest(
    *,
    config: DataCompletenessConfig,
    config_path: Path,
    date: str,
    data_root: Path | None = None,
    synthetic_bbo: Path | None = None,
    spot_prices: Path | None = None,
    out_dir: Path | None = None,
) -> ManifestPaths:
    paths = resolve_paths(
        config=config,
        date=date,
        data_root=data_root,
        synthetic_bbo=synthetic_bbo,
        spot_prices=spot_prices,
        out_dir=out_dir,
    )

    if not paths.synthetic_bbo.exists():
        raise FileNotFoundError(f"synthetic_bbo parquet not found: {paths.synthetic_bbo}")
    if config.spot.enabled and (paths.spot_prices is None or not paths.spot_prices.exists()):
        raise FileNotFoundError(f"spot_prices parquet not found: {paths.spot_prices}")

    paths.out_dir.mkdir(parents=True, exist_ok=True)

    con = duckdb.connect(database=":memory:")
    con.execute("SET enable_progress_bar=false;")

    con.execute(
        f"CREATE OR REPLACE VIEW synthetic_bbo_raw AS SELECT * FROM read_parquet({_sql_quote(str(paths.synthetic_bbo))});"
    )

    raw_cols = con.execute("PRAGMA table_info('synthetic_bbo_raw');").fetchall()
    raw_present = {str(r[1]) for r in raw_cols}
    if {"best_bid", "best_ask"}.issubset(raw_present):
        con.execute("CREATE OR REPLACE VIEW synthetic_bbo AS SELECT * FROM synthetic_bbo_raw;")
    elif {"bid_price_0", "ask_price_0"}.issubset(raw_present):
        con.execute(
            """
            CREATE OR REPLACE VIEW synthetic_bbo AS
            SELECT
              *,
              bid_price_0 AS best_bid,
              ask_price_0 AS best_ask
            FROM synthetic_bbo_raw;
            """
        )
    else:
        raise ValueError(
            "synthetic_bbo parquet must include either (best_bid, best_ask) "
            "or (bid_price_0, ask_price_0) columns"
        )

    _assert_required_columns(
        con,
        view="synthetic_bbo",
        required=["ts", "ticker", "outcome", "asset_id", "best_bid", "best_ask"],
    )

    spot_stats = None
    if config.spot.enabled and paths.spot_prices is not None:
        con.execute(
            f"CREATE OR REPLACE VIEW spot_prices AS SELECT * FROM read_parquet({_sql_quote(str(paths.spot_prices))});"
        )
        _assert_required_columns(
            con,
            view="spot_prices",
            required=["ts", "symbol", "price"],
        )
        spot_stats = _load_spot_stats(con, config)

    stats = _load_outcome_stats(con, config)
    event_stats = _load_event_stats(con, config)
    end_dates = _load_end_dates(con, config)
    by_ticker: dict[str, dict[str, OutcomeStats]] = {}
    for s in stats:
        by_ticker.setdefault(s.ticker, {})[s.outcome] = s
    by_ticker_event: dict[str, dict[str, EventStats]] = {}
    for s in event_stats:
        by_ticker_event.setdefault(s.ticker, {})[s.outcome] = s

    required_outcomes = list(config.universe.required_outcomes)
    tickers = sorted(by_ticker.keys())

    allowlist_rows: list[dict[str, object]] = []
    exclusion_rows: list[dict[str, object]] = []

    exclusion_reason_counts: dict[str, int] = {}

    for ticker in tickers:
        row, reasons = _build_ticker_row(
            date=date,
            ticker=ticker,
            required_outcomes=required_outcomes,
            outcome_stats=by_ticker[ticker],
            event_stats=by_ticker_event.get(ticker, {}),
            end_date=end_dates.get(ticker),
            spot_stats=spot_stats,
            config=config,
        )

        if reasons:
            row_with_reason = dict(row)
            row_with_reason["reason"] = ";".join(reasons)
            exclusion_rows.append(row_with_reason)
            for reason in reasons:
                exclusion_reason_counts[reason] = exclusion_reason_counts.get(reason, 0) + 1
        else:
            allowlist_rows.append(row)

    allowlist_fieldnames = _manifest_fieldnames(required_outcomes, include_reason=False)
    exclusions_fieldnames = _manifest_fieldnames(required_outcomes, include_reason=True)
    _write_csv(paths.allowlist_csv, allowlist_rows, fieldnames=allowlist_fieldnames)
    _write_csv(paths.exclusions_csv, exclusion_rows, fieldnames=exclusions_fieldnames)

    summary = _build_summary(
        config=config,
        config_path=config_path,
        date=date,
        paths=paths,
        allowlist_rows=allowlist_rows,
        exclusion_rows=exclusion_rows,
        exclusion_reason_counts=exclusion_reason_counts,
        outcome_stats=stats,
        event_stats=event_stats,
        end_dates=end_dates,
        spot_stats=spot_stats,
    )
    paths.summary_json.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")

    return paths


def _load_outcome_stats(con: duckdb.DuckDBPyConnection, config: DataCompletenessConfig) -> list[OutcomeStats]:
    ticker_filter = _ticker_universe_where_clause(config.universe.ticker_regexes)
    valid_expr = _valid_quote_sql_expr(config)
    _ensure_ticker_window_table(con, ticker_filter)

    query = f"""
    WITH raw AS (
      SELECT
        ticker,
        outcome,
        asset_id,
        ts,
        CAST(epoch_ms(ts) AS BIGINT) AS ts_ms,
        CAST(epoch_ms(end_date) AS BIGINT) AS end_date_ms,
        best_bid,
        best_ask,
        (best_bid IS NULL)::INT AS is_null_bid,
        (best_ask IS NULL)::INT AS is_null_ask,
        (best_bid IS NOT NULL AND best_ask IS NOT NULL AND best_bid > best_ask)::INT AS is_crossed,
        (best_bid IS NOT NULL AND (best_bid < {config.validation.min_price} OR best_bid > {config.validation.max_price}))::INT
          AS is_bid_oob,
        (best_ask IS NOT NULL AND (best_ask < {config.validation.min_price} OR best_ask > {config.validation.max_price}))::INT
          AS is_ask_oob,
        ({valid_expr})::INT AS is_valid
      FROM synthetic_bbo
      WHERE {ticker_filter}
    ),
    base AS (
      SELECT
        raw.*,
        tw.open_ms,
        tw.window_end_ms,
        CASE
          WHEN tw.window_end_ms IS NOT NULL AND raw.end_date_ms IS NOT NULL THEN least(tw.window_end_ms, raw.end_date_ms)
          WHEN tw.window_end_ms IS NOT NULL THEN tw.window_end_ms
          ELSE raw.end_date_ms
        END AS effective_end_ms
      FROM raw
      LEFT JOIN ticker_window tw USING (ticker)
      WHERE (tw.open_ms IS NULL OR raw.ts_ms >= tw.open_ms)
        AND (
          CASE
            WHEN tw.window_end_ms IS NOT NULL AND raw.end_date_ms IS NOT NULL THEN least(tw.window_end_ms, raw.end_date_ms)
            WHEN tw.window_end_ms IS NOT NULL THEN tw.window_end_ms
            ELSE raw.end_date_ms
          END IS NULL
          OR raw.ts_ms <= (
            CASE
              WHEN tw.window_end_ms IS NOT NULL AND raw.end_date_ms IS NOT NULL THEN least(tw.window_end_ms, raw.end_date_ms)
              WHEN tw.window_end_ms IS NOT NULL THEN tw.window_end_ms
              ELSE raw.end_date_ms
            END
          )
        )
    ),
    gaps_base AS (
      SELECT
        ticker,
        outcome,
        COALESCE(
          ts_ms - lag(ts_ms) OVER (PARTITION BY ticker, outcome ORDER BY ts_ms),
          0
        ) AS gap_ms
      FROM base
    ),
    gaps AS (
      SELECT
        ticker,
        outcome,
        max(gap_ms)::BIGINT AS max_inter_event_gap_ms
      FROM gaps_base
      GROUP BY 1, 2
    )
    SELECT
      ticker,
      outcome,
      min(asset_id) AS asset_id,
      count(DISTINCT asset_id)::BIGINT AS asset_id_count,
      count(*)::BIGINT AS rows,
      sum(is_valid)::BIGINT AS valid_rows,
      (count(*) - sum(is_valid))::BIGINT AS invalid_rows,
      min(ts) AS min_ts,
      max(ts) AS max_ts,
      sum(is_null_bid)::BIGINT AS null_best_bid_rows,
      sum(is_null_ask)::BIGINT AS null_best_ask_rows,
      sum(is_crossed)::BIGINT AS crossed_rows,
      sum(is_bid_oob)::BIGINT AS bid_out_of_bounds_rows,
      sum(is_ask_oob)::BIGINT AS ask_out_of_bounds_rows,
      max(CASE WHEN is_valid THEN ts ELSE NULL END) AS last_valid_ts,
      max(gaps.max_inter_event_gap_ms) AS max_inter_event_gap_ms
    FROM base
    LEFT JOIN gaps USING (ticker, outcome)
    GROUP BY 1, 2
    ORDER BY 1, 2;
    """

    rows = con.execute(query).fetchall()
    out: list[OutcomeStats] = []
    for (
        ticker,
        outcome,
        asset_id,
        asset_id_count,
        n_rows,
        valid_rows,
        invalid_rows,
        min_ts,
        max_ts,
        null_bid,
        null_ask,
        crossed,
        bid_oob,
        ask_oob,
        last_valid_ts,
        max_inter_event_gap_ms,
    ) in rows:
        out.append(
            OutcomeStats(
                ticker=str(ticker),
                outcome=str(outcome),
                asset_id=str(asset_id) if asset_id is not None else None,
                asset_id_count=int(asset_id_count),
                rows=int(n_rows),
                valid_rows=int(valid_rows),
                invalid_rows=int(invalid_rows),
                min_ts=min_ts,
                max_ts=max_ts,
                null_best_bid_rows=int(null_bid),
                null_best_ask_rows=int(null_ask),
                crossed_rows=int(crossed),
                bid_out_of_bounds_rows=int(bid_oob),
                ask_out_of_bounds_rows=int(ask_oob),
                last_valid_ts=last_valid_ts,
                max_inter_event_gap_ms=int(max_inter_event_gap_ms)
                if max_inter_event_gap_ms is not None
                else None,
            )
        )
    return out


def _load_spot_stats(con: duckdb.DuckDBPyConnection, config: DataCompletenessConfig) -> SpotStats:
    valid_expr = _spot_valid_sql_expr()
    symbol = config.spot.symbol

    query = f"""
    SELECT
      count(*)::BIGINT AS rows,
      sum(({valid_expr})::INT)::BIGINT AS valid_rows,
      (count(*) - sum(({valid_expr})::INT))::BIGINT AS invalid_rows,
      min(ts) AS min_ts,
      max(ts) AS max_ts
    FROM spot_prices
    WHERE symbol = {_sql_quote(symbol)};
    """
    rows, valid_rows, invalid_rows, min_ts, max_ts = con.execute(query).fetchone()
    return SpotStats(
        symbol=symbol,
        rows=int(rows or 0),
        valid_rows=int(valid_rows or 0),
        invalid_rows=int(invalid_rows or 0),
        min_ts=min_ts,
        max_ts=max_ts,
    )


def _ensure_ticker_window_table(con: duckdb.DuckDBPyConnection, ticker_filter: str) -> None:
    tickers = [
        str(row[0])
        for row in con.execute(f"SELECT DISTINCT ticker FROM synthetic_bbo WHERE {ticker_filter};").fetchall()
        if row and row[0] is not None
    ]
    rows: list[tuple[str, int | None, int | None]] = []
    for ticker in tickers:
        open_ms = _market_open_ms(ticker)
        duration_ms = _market_duration_ms(ticker)
        window_end_ms = None
        if open_ms is not None and duration_ms is not None:
            window_end_ms = int(open_ms + duration_ms)
        rows.append(
            (
                ticker,
                int(open_ms) if open_ms is not None else None,
                window_end_ms,
            )
        )

    con.execute(
        "CREATE OR REPLACE TEMP TABLE ticker_window(ticker VARCHAR, open_ms BIGINT, window_end_ms BIGINT);"
    )
    if rows:
        con.executemany("INSERT INTO ticker_window VALUES (?, ?, ?);", rows)


def _load_end_dates(
    con: duckdb.DuckDBPyConnection, config: DataCompletenessConfig
) -> dict[str, datetime]:
    ticker_filter = _ticker_universe_where_clause(config.universe.ticker_regexes)
    query = f"""
    SELECT ticker, min(end_date) AS end_date
    FROM synthetic_bbo
    WHERE {ticker_filter}
    GROUP BY 1;
    """
    rows = con.execute(query).fetchall()
    out: dict[str, datetime] = {}
    for ticker, end_date in rows:
        if ticker is None:
            continue
        effective_end = _effective_end_date(str(ticker), end_date)
        if effective_end is not None:
            out[str(ticker)] = effective_end
    return out


def _load_event_stats(
    con: duckdb.DuckDBPyConnection, config: DataCompletenessConfig
) -> list[EventStats]:
    required_outcomes = list(config.universe.required_outcomes)
    if not required_outcomes:
        return []

    h_max_s = max(config.event_time.horizons_s, default=0.0)
    h_max_ms = int(round(h_max_s * 1000.0))
    if h_max_ms < 0:
        raise ValueError(f"max horizon must be non-negative, got {h_max_s}")

    max_gap_ms = int(config.event_time.max_gap_ms)
    require_forward = config.event_time.require_forward_freshness_for_max_horizon

    ticker_filter = _ticker_universe_where_clause(config.universe.ticker_regexes)
    valid_expr = _valid_quote_sql_expr(config)

    con.execute("CREATE OR REPLACE TEMP TABLE required_outcomes(outcome VARCHAR);")
    con.executemany("INSERT INTO required_outcomes VALUES (?);", [(o,) for o in required_outcomes])

    _ensure_ticker_window_table(con, ticker_filter)

    con.execute(
        f"""
        CREATE OR REPLACE TEMP VIEW event_base AS
        SELECT
          ticker,
          outcome,
          ts,
          CAST(epoch_ms(ts) AS BIGINT) AS ts_ms,
          end_date,
          CAST(epoch_ms(end_date) AS BIGINT) AS end_date_ms,
          ({valid_expr}) AS is_valid
        FROM synthetic_bbo
        WHERE {ticker_filter}
          AND outcome IN (SELECT outcome FROM required_outcomes);
        """
    )

    con.execute(
        """
        CREATE OR REPLACE TEMP VIEW event_base_with_window AS
        SELECT eb.*, tw.open_ms, tw.window_end_ms
        FROM event_base eb
        LEFT JOIN ticker_window tw USING (ticker)
        WHERE tw.open_ms IS NULL OR eb.ts_ms >= tw.open_ms;
        """
    )

    con.execute(
        f"""
        CREATE OR REPLACE TEMP VIEW event_eval AS
        SELECT
          *,
          CASE
            WHEN window_end_ms IS NOT NULL AND end_date_ms IS NOT NULL THEN least(window_end_ms, end_date_ms)
            WHEN window_end_ms IS NOT NULL THEN window_end_ms
            ELSE end_date_ms
          END AS effective_end_ms,
          ts_ms + {h_max_ms} AS ts_h_ms
        FROM event_base_with_window
        WHERE effective_end_ms IS NULL OR ts_ms + {h_max_ms} <= effective_end_ms;
        """
    )

    con.execute(
        """
        CREATE OR REPLACE TEMP VIEW event_future AS
        SELECT
          ticker,
          outcome,
          ts_ms AS fwd_ts_ms,
          is_valid AS fwd_valid
        FROM event_base_with_window;
        """
    )

    spot_join = ""
    spot_condition = "TRUE"
    if config.spot.enabled:
        spot_valid_expr = _spot_valid_sql_expr()
        con.execute(
            f"""
            CREATE OR REPLACE TEMP VIEW spot_prices_ms AS
            SELECT
              CAST(epoch_ms(ts) AS BIGINT) AS ts_ms,
              ({spot_valid_expr}) AS is_valid
            FROM spot_prices
            WHERE symbol = {_sql_quote(config.spot.symbol)};
            """
        )
        con.execute(
            """
            CREATE OR REPLACE TEMP VIEW spot_prices_with_dummy AS
            SELECT * FROM spot_prices_ms
            UNION ALL
            SELECT -1 AS ts_ms, FALSE AS is_valid;
            """
        )
        spot_join = """
        ASOF JOIN spot_prices_with_dummy s
          ON e.ts_ms >= s.ts_ms
        """
        spot_condition = f"(s.is_valid AND (e.ts_ms - s.ts_ms) <= {int(config.spot.max_gap_ms)})"

    forward_condition = ""
    if require_forward:
        forward_condition = (
            f"AND fwd.fwd_valid AND (e.ts_h_ms - fwd.fwd_ts_ms) <= {max_gap_ms}"
        )

    query = f"""
    SELECT
      e.ticker,
      e.outcome,
      count(*)::BIGINT AS events_total,
      sum(
        CASE
          WHEN e.is_valid
           {forward_condition}
           AND {spot_condition}
          THEN 1 ELSE 0 END
      )::BIGINT AS usable_events
    FROM event_eval e
    ASOF JOIN event_future fwd
      ON fwd.ticker = e.ticker
     AND fwd.outcome = e.outcome
     AND e.ts_h_ms >= fwd.fwd_ts_ms
    {spot_join}
    GROUP BY 1, 2
    ORDER BY 1, 2;
    """

    rows = con.execute(query).fetchall()
    out: list[EventStats] = []
    for ticker, outcome, events_total, usable_events in rows:
        total = int(events_total or 0)
        usable = int(usable_events or 0)
        ratio = round(usable / total, 6) if total > 0 else 0.0
        out.append(
            EventStats(
                ticker=str(ticker),
                outcome=str(outcome),
                events_total=total,
                usable_events=usable,
                usable_ratio=ratio,
            )
        )

    return out


def _build_ticker_row(
    *,
    date: str,
    ticker: str,
    required_outcomes: list[str],
    outcome_stats: dict[str, OutcomeStats],
    event_stats: dict[str, EventStats],
    end_date: datetime | None,
    spot_stats: SpotStats | None,
    config: DataCompletenessConfig,
) -> tuple[dict[str, object], list[str]]:
    outcomes_present = sorted(outcome_stats.keys())

    reasons: list[str] = []
    missing = [o for o in required_outcomes if o not in outcome_stats]
    if missing:
        reasons.append("missing_required_outcomes")

    bad_asset_id = [
        o
        for o in required_outcomes
        if o in outcome_stats and outcome_stats[o].asset_id_count != 1
    ]
    if bad_asset_id:
        reasons.append("multiple_asset_ids_for_ticker_outcome")

    no_valid = [o for o in required_outcomes if o in outcome_stats and outcome_stats[o].valid_rows == 0]
    if no_valid:
        reasons.append("no_valid_quotes_for_required_outcome")

    min_ts = min((s.min_ts for s in outcome_stats.values() if s.min_ts is not None), default=None)
    max_ts = max((s.max_ts for s in outcome_stats.values() if s.max_ts is not None), default=None)
    last_valid_ts = min(
        (s.last_valid_ts for s in outcome_stats.values() if s.last_valid_ts is not None),
        default=None,
    )

    rows_total = sum(s.rows for s in outcome_stats.values())
    valid_rows_total = sum(s.valid_rows for s in outcome_stats.values())
    invalid_rows_total = sum(s.invalid_rows for s in outcome_stats.values())

    end_gap_ms = ""
    if end_date is not None and last_valid_ts is not None:
        end_gap_ms = max(0, int((end_date - last_valid_ts).total_seconds() * 1000))

    events_total = sum(s.events_total for s in event_stats.values())
    usable_events_total = sum(s.usable_events for s in event_stats.values())

    row: dict[str, object] = {
        "manifest_version": MANIFEST_VERSION,
        "date": date,
        "ticker": ticker,
        "outcomes_present": "|".join(outcomes_present),
        "min_ts": _ts_to_iso(min_ts),
        "max_ts": _ts_to_iso(max_ts),
        "rows_total": rows_total,
        "valid_rows_total": valid_rows_total,
        "invalid_rows_total": invalid_rows_total,
        "events_total": events_total,
        "usable_events_total": usable_events_total,
        "end_date": _ts_to_iso(end_date),
        "last_valid_ts": _ts_to_iso(last_valid_ts),
        "end_gap_ms": end_gap_ms,
        "spot_symbol": config.spot.symbol if config.spot.enabled else "",
        "spot_rows": spot_stats.rows if spot_stats is not None else 0,
        "spot_valid_rows": spot_stats.valid_rows if spot_stats is not None else 0,
        "spot_invalid_rows": spot_stats.invalid_rows if spot_stats is not None else 0,
        "spot_min_ts": _ts_to_iso(spot_stats.min_ts) if spot_stats is not None else "",
        "spot_max_ts": _ts_to_iso(spot_stats.max_ts) if spot_stats is not None else "",
    }

    usable_ratio_by_outcome: dict[str, float] = {}
    for outcome in required_outcomes:
        s = outcome_stats.get(outcome)
        e = event_stats.get(outcome)
        key_prefix = _sanitize_outcome(outcome)
        row[f"{key_prefix}_asset_id"] = s.asset_id if s is not None else ""
        row[f"{key_prefix}_asset_id_count"] = s.asset_id_count if s is not None else 0
        row[f"{key_prefix}_rows"] = s.rows if s is not None else 0
        row[f"{key_prefix}_valid_rows"] = s.valid_rows if s is not None else 0
        row[f"{key_prefix}_invalid_rows"] = s.invalid_rows if s is not None else 0
        events = e.events_total if e is not None else 0
        usable_events = e.usable_events if e is not None else 0
        usable_ratio = e.usable_ratio if e is not None else 0.0
        max_inter_event_gap_ms = s.max_inter_event_gap_ms if s is not None else None
        row[f"{key_prefix}_events"] = events
        row[f"{key_prefix}_usable_events"] = usable_events
        row[f"{key_prefix}_usable_ratio"] = usable_ratio
        row[f"{key_prefix}_max_inter_event_gap_ms"] = (
            max_inter_event_gap_ms if max_inter_event_gap_ms is not None else ""
        )
        usable_ratio_by_outcome[outcome] = usable_ratio

    usable_ratio_score = min(usable_ratio_by_outcome.values(), default=0.0)
    row["usable_ratio_score"] = usable_ratio_score

    if (
        end_gap_ms != ""
        and isinstance(end_gap_ms, int)
        and end_gap_ms > config.thresholds.max_end_gap_ms
    ):
        reasons.append("quotes_end_before_market_end")

    for outcome in required_outcomes:
        e = event_stats.get(outcome)
        if e is None:
            continue
        if e.usable_events < config.thresholds.min_usable_events:
            reasons.append("insufficient_usable_events")
            break

    if usable_ratio_score < config.thresholds.min_usable_ratio:
        reasons.append("usable_ratio_below_threshold")

    for outcome in required_outcomes:
        s = outcome_stats.get(outcome)
        if s is None or s.max_inter_event_gap_ms is None:
            continue
        if s.max_inter_event_gap_ms > config.thresholds.max_inter_event_gap_ms:
            reasons.append("max_inter_event_gap_exceeded")
            break

    if config.spot.enabled:
        if spot_stats is None or spot_stats.rows <= 0:
            reasons.append("spot_missing_symbol")
        elif spot_stats.valid_rows <= 0:
            reasons.append("spot_no_valid_prices")

        if spot_stats is not None and spot_stats.valid_rows > 0:
            for outcome in required_outcomes:
                e = event_stats.get(outcome)
                if e is None:
                    continue
                if e.usable_events < config.spot.min_usable_events:
                    reasons.append("spot_insufficient_usable_events")
                    break
            if usable_ratio_score < config.spot.min_usable_ratio:
                reasons.append("spot_usable_ratio_below_threshold")

    return row, reasons


def _build_summary(
    *,
    config: DataCompletenessConfig,
    config_path: Path,
    date: str,
    paths: ManifestPaths,
    allowlist_rows: list[dict[str, object]],
    exclusion_rows: list[dict[str, object]],
    exclusion_reason_counts: dict[str, int],
    outcome_stats: list[OutcomeStats],
    event_stats: list[EventStats],
    end_dates: dict[str, datetime],
    spot_stats: SpotStats | None,
) -> dict[str, Any]:
    per_outcome_valid_rates: dict[str, list[float]] = {}
    for s in outcome_stats:
        if s.rows <= 0:
            continue
        per_outcome_valid_rates.setdefault(s.outcome, []).append(s.valid_rows / s.rows)

    def quantiles(xs: list[float]) -> dict[str, float]:
        if not xs:
            return {}
        xs_sorted = sorted(xs)
        n = len(xs_sorted)

        def at(p: float) -> float:
            idx = int(round(p * (n - 1)))
            return float(xs_sorted[max(0, min(n - 1, idx))])

        return {
            "min": float(xs_sorted[0]),
            "p10": at(0.10),
            "p50": at(0.50),
            "p90": at(0.90),
            "max": float(xs_sorted[-1]),
        }

    usable_ratio_by_outcome: dict[str, list[float]] = {}
    events_total_by_outcome: dict[str, list[float]] = {}
    usable_events_by_outcome: dict[str, list[float]] = {}
    for s in event_stats:
        usable_ratio_by_outcome.setdefault(s.outcome, []).append(float(s.usable_ratio))
        events_total_by_outcome.setdefault(s.outcome, []).append(float(s.events_total))
        usable_events_by_outcome.setdefault(s.outcome, []).append(float(s.usable_events))

    max_gap_by_outcome: dict[str, list[float]] = {}
    last_valid_by_ticker: dict[str, datetime] = {}
    for s in outcome_stats:
        if s.max_inter_event_gap_ms is not None:
            max_gap_by_outcome.setdefault(s.outcome, []).append(float(s.max_inter_event_gap_ms))
        if s.last_valid_ts is not None:
            prev = last_valid_by_ticker.get(s.ticker)
            if prev is None or s.last_valid_ts < prev:
                last_valid_by_ticker[s.ticker] = s.last_valid_ts

    end_gap_values: list[float] = []
    for ticker, end_date in end_dates.items():
        last_valid_ts = last_valid_by_ticker.get(ticker)
        if end_date is None or last_valid_ts is None:
            continue
        end_gap_values.append(float(max(0, int((end_date - last_valid_ts).total_seconds() * 1000))))

    return {
        "manifest_version": MANIFEST_VERSION,
        "date": date,
        "config_path": str(config_path),
        "inputs": {
            "synthetic_bbo": str(paths.synthetic_bbo),
            "spot_prices": str(paths.spot_prices) if paths.spot_prices is not None else "",
        },
        "universe": {
            "ticker_regexes": list(config.universe.ticker_regexes),
            "required_outcomes": list(config.universe.required_outcomes),
        },
        "event_time": {
            "horizons_s": list(config.event_time.horizons_s),
            "max_gap_ms": config.event_time.max_gap_ms,
            "require_forward_freshness_for_max_horizon": config.event_time.require_forward_freshness_for_max_horizon,
        },
        "thresholds": {
            "min_usable_ratio": config.thresholds.min_usable_ratio,
            "min_usable_events": config.thresholds.min_usable_events,
            "max_end_gap_ms": config.thresholds.max_end_gap_ms,
            "max_inter_event_gap_ms": config.thresholds.max_inter_event_gap_ms,
        },
        "validation": {
            "require_best_bid_ask_non_null": config.validation.require_best_bid_ask_non_null,
            "require_non_negative_spread": config.validation.require_non_negative_spread,
            "enforce_price_bounds": config.validation.enforce_price_bounds,
            "min_price": config.validation.min_price,
            "max_price": config.validation.max_price,
        },
        "spot": {
            "enabled": config.spot.enabled,
            "symbol": config.spot.symbol,
            "max_gap_ms": config.spot.max_gap_ms,
            "min_usable_ratio": config.spot.min_usable_ratio,
            "min_usable_events": config.spot.min_usable_events,
            "rows": spot_stats.rows if spot_stats is not None else 0,
            "valid_rows": spot_stats.valid_rows if spot_stats is not None else 0,
            "invalid_rows": spot_stats.invalid_rows if spot_stats is not None else 0,
            "min_ts": _ts_to_iso(spot_stats.min_ts) if spot_stats is not None else "",
            "max_ts": _ts_to_iso(spot_stats.max_ts) if spot_stats is not None else "",
        },
        "outputs": {
            "out_dir": str(paths.out_dir),
            "allowlist_csv": str(paths.allowlist_csv),
            "exclusions_csv": str(paths.exclusions_csv),
            "summary_json": str(paths.summary_json),
        },
        "counts": {
            "universe_tickers": len(allowlist_rows) + len(exclusion_rows),
            "allowlisted_tickers": len(allowlist_rows),
            "excluded_tickers": len(exclusion_rows),
        },
        "excluded_by_reason": dict(sorted(exclusion_reason_counts.items())),
        "valid_row_rate_by_outcome": {
            outcome: quantiles(rates) for outcome, rates in sorted(per_outcome_valid_rates.items())
        },
        "event_distributions": {
            "usable_ratio_by_outcome": {
                outcome: quantiles(rates) for outcome, rates in sorted(usable_ratio_by_outcome.items())
            },
            "events_total_by_outcome": {
                outcome: quantiles(values) for outcome, values in sorted(events_total_by_outcome.items())
            },
            "usable_events_by_outcome": {
                outcome: quantiles(values) for outcome, values in sorted(usable_events_by_outcome.items())
            },
            "max_inter_event_gap_ms_by_outcome": {
                outcome: quantiles(values) for outcome, values in sorted(max_gap_by_outcome.items())
            },
            "end_gap_ms": quantiles(end_gap_values),
        },
    }


def _write_csv(path: Path, rows: list[dict[str, object]], *, fieldnames: list[str]) -> None:
    with path.open("w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows)


def _sanitize_outcome(outcome: str) -> str:
    return "".join(ch.lower() if ch.isalnum() else "_" for ch in outcome).strip("_")


def _sql_quote(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


def _ticker_universe_where_clause(regexes: list[str]) -> str:
    if not regexes:
        return "TRUE"
    parts = [f"regexp_matches(ticker, {_sql_quote(pat)})" for pat in regexes]
    return "(" + " OR ".join(parts) + ")"


def _valid_quote_sql_expr(config: DataCompletenessConfig) -> str:
    has_bid = "(best_bid IS NOT NULL)"
    has_ask = "(best_ask IS NOT NULL)"
    has_both = f"({has_bid} AND {has_ask})"
    has_any = f"({has_bid} OR {has_ask})"

    if config.validation.require_best_bid_ask_non_null:
        side_ok = has_both
    else:
        side_ok = has_any

    checks = [side_ok]

    if config.validation.require_non_negative_spread:
        checks.append(f"(NOT {has_both} OR best_ask - best_bid >= 0)")

    if config.validation.enforce_price_bounds:
        checks.append(
            "("
            f"(best_bid IS NULL OR (best_bid >= {config.validation.min_price} AND best_bid <= {config.validation.max_price}))"
            " AND "
            f"(best_ask IS NULL OR (best_ask >= {config.validation.min_price} AND best_ask <= {config.validation.max_price}))"
            ")"
        )

    return "(" + " AND ".join(checks) + ")"


def _spot_valid_sql_expr() -> str:
    return "(price IS NOT NULL AND price > 0)"


def _assert_required_columns(
    con: duckdb.DuckDBPyConnection, *, view: str, required: list[str]
) -> None:
    cols = con.execute(f"PRAGMA table_info({_sql_quote(view)});").fetchall()
    present = {str(r[1]) for r in cols}
    missing = [c for c in required if c not in present]
    if missing:
        raise ValueError(f"{view} missing columns: {missing}")


def _manifest_fieldnames(required_outcomes: list[str], *, include_reason: bool) -> list[str]:
    fields = [
        "manifest_version",
        "date",
        "ticker",
        "outcomes_present",
        "min_ts",
        "max_ts",
        "rows_total",
        "valid_rows_total",
        "invalid_rows_total",
        "events_total",
        "usable_events_total",
        "end_date",
        "last_valid_ts",
        "end_gap_ms",
        "spot_symbol",
        "spot_rows",
        "spot_valid_rows",
        "spot_invalid_rows",
        "spot_min_ts",
        "spot_max_ts",
        "usable_ratio_score",
    ]
    for outcome in required_outcomes:
        key_prefix = _sanitize_outcome(outcome)
        fields.extend(
            [
                f"{key_prefix}_asset_id",
                f"{key_prefix}_asset_id_count",
                f"{key_prefix}_rows",
                f"{key_prefix}_valid_rows",
                f"{key_prefix}_invalid_rows",
                f"{key_prefix}_events",
                f"{key_prefix}_usable_events",
                f"{key_prefix}_usable_ratio",
                f"{key_prefix}_max_inter_event_gap_ms",
            ]
        )
    if include_reason:
        fields.append("reason")
    return fields


def _ts_to_iso(value: object) -> str:
    iso = getattr(value, "isoformat", None)
    if callable(iso):
        return str(iso())
    return ""


def _ms_to_iso(value: int | None) -> str:
    if value is None:
        return ""
    return _ts_to_iso(datetime.utcfromtimestamp(value / 1000.0))


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


def _effective_end_date(ticker: str, end_date: datetime | None) -> datetime | None:
    open_ms = _market_open_ms(ticker)
    duration_ms = _market_duration_ms(ticker)
    duration_end = None
    if open_ms is not None and duration_ms is not None:
        duration_end = datetime.utcfromtimestamp((open_ms + duration_ms) / 1000.0)

    if end_date is None:
        return duration_end
    if duration_end is None:
        return end_date
    return min(end_date, duration_end)
