"""stock-scoring CLI.

Subcommands
-----------
  score     Score a single ticker (or a universe) against the rubric.
  rank      Score the universe and print ranked list.
  size      Rank + downside + price-action → position sizing.
  sell      Evaluate sell triggers against a holdings file.
"""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import asdict
from datetime import date
from pathlib import Path

from .inputs import CompanyInputs, load_company, load_universe
from .price_action import PriceActionConfig, compute_price_action
from .scoring import Scorecard, load_rubric, rank_scorecards, score_ticker
from .sell import HoldingState, SellConfig, evaluate as evaluate_sell
from .sizing import Candidate, SizingConfig, allocate, compute_downside


REPO_CONFIGS = Path(__file__).resolve().parents[2] / "configs"
DEFAULT_RUBRIC = REPO_CONFIGS / "rubric.yaml"
DEFAULT_SIZING = REPO_CONFIGS / "sizing.yaml"
DEFAULT_PA = REPO_CONFIGS / "price_action.yaml"
DEFAULT_SELL = REPO_CONFIGS / "sell.yaml"


# ---------------------------------------------------------------------------
# score / rank
# ---------------------------------------------------------------------------


def _load_inputs(args: argparse.Namespace) -> list[CompanyInputs]:
    if args.ticker_file:
        return [load_company(args.ticker_file)]
    if args.universe:
        return load_universe(args.universe)
    raise SystemExit("must pass --ticker-file or --universe")


def _scorecard_to_dict(sc: Scorecard) -> dict:
    return {
        "ticker": sc.ticker,
        "total": round(sc.total, 4),
        "categories": [
            {
                "id": cat.id,
                "score": round(cat.score, 4),
                "weight": cat.weight,
                "criteria": [
                    {
                        "id": crit.id,
                        "name": crit.name,
                        "score": round(crit.score, 4),
                        "weight": crit.weight,
                        "signals": [
                            {
                                "id": sig.id,
                                "score": round(sig.score, 4),
                                "missing": sig.missing,
                            }
                            for sig in crit.signals
                        ],
                    }
                    for crit in cat.criteria
                ],
            }
            for cat in sc.categories
        ],
        "missing_signals": sc.missing_signals,
    }


def _handle_score(args: argparse.Namespace) -> int:
    rubric = load_rubric(args.rubric)
    companies = _load_inputs(args)
    scorecards = [score_ticker(rubric, c.signals, c.ticker) for c in companies]
    if args.json:
        print(json.dumps([_scorecard_to_dict(sc) for sc in scorecards], indent=2))
        return 0
    for sc in scorecards:
        _print_scorecard(sc)
    return 0


def _print_scorecard(sc: Scorecard) -> None:
    print(f"\n{sc.ticker}  total={sc.total:.3f}")
    for cat in sc.categories:
        print(f"  [{cat.id:>12}] {cat.score:.3f}  (weight={cat.weight:.2f})")
        for crit in cat.criteria:
            print(f"      {crit.name:<40} {crit.score:.3f}  (w={crit.weight:.3f})")
    if sc.missing_signals:
        print(f"  missing: {len(sc.missing_signals)} signals (defaults applied)")


def _handle_rank(args: argparse.Namespace) -> int:
    rubric = load_rubric(args.rubric)
    companies = load_universe(args.universe)
    scorecards = rank_scorecards(
        [score_ticker(rubric, c.signals, c.ticker) for c in companies]
    )
    if args.json:
        print(json.dumps([
            {"ticker": sc.ticker, "total": round(sc.total, 4)} for sc in scorecards
        ], indent=2))
        return 0
    print(f"{'rank':>4}  {'ticker':<8}  score")
    for i, sc in enumerate(scorecards, 1):
        print(f"{i:>4}  {sc.ticker:<8}  {sc.total:.3f}")
    return 0


# ---------------------------------------------------------------------------
# size
# ---------------------------------------------------------------------------


def _handle_size(args: argparse.Namespace) -> int:
    rubric = load_rubric(args.rubric)
    sizing_cfg = SizingConfig.load(args.sizing)
    pa_cfg = PriceActionConfig.load(args.price_action)
    companies = load_universe(args.universe)

    candidates: list[Candidate] = []
    details: list[dict] = []
    for c in companies:
        scorecard = score_ticker(rubric, c.signals, c.ticker)
        downside = compute_downside(sizing_cfg, c.ticker, c.downside)
        pa = compute_price_action(pa_cfg, c.ticker, c.price_action)
        conviction = float(c.meta.get("conviction", 1.0))
        meme_dip = bool(c.meta.get("meme_dip", False))
        candidates.append(
            Candidate(
                ticker=c.ticker,
                quality_score=scorecard.total,
                downside_risk=downside.total,
                price_action_score=pa.score,
                conviction=conviction,
                meme_dip=meme_dip,
            )
        )
        details.append(
            {
                "ticker": c.ticker,
                "quality_score": round(scorecard.total, 4),
                "downside_risk": round(downside.total, 4),
                "price_action_score": round(pa.score, 4),
                "conviction": conviction,
                "meme_dip": meme_dip,
            }
        )

    allocations = allocate(sizing_cfg, candidates)

    if args.json:
        print(json.dumps(
            {
                "details": details,
                "allocations": [asdict(a) for a in allocations],
            },
            indent=2,
        ))
        return 0

    print(f"{'rank':>4}  {'ticker':<8}  {'quality':>8}  {'downside':>8}  {'pa':>6}  "
          f"{'conv':>5}  {'pos %':>7}")
    for i, a in enumerate(allocations, 1):
        print(
            f"{i:>4}  {a.ticker:<8}  {a.quality_score:>8.3f}  {a.downside_risk:>8.3f}  "
            f"{a.price_action_score:>6.3f}  {a.conviction:>5.2f}  {a.position_pct * 100:>6.2f}%"
        )
    total_deployed = sum(a.position_pct for a in allocations) * 100
    print(f"\ndeployed: {total_deployed:.2f}% of book")
    return 0


# ---------------------------------------------------------------------------
# sell
# ---------------------------------------------------------------------------


def _handle_sell(args: argparse.Namespace) -> int:
    cfg = SellConfig.load(args.sell)
    import yaml
    holdings = yaml.safe_load(Path(args.holdings).read_text()) or {}
    today = date.fromisoformat(args.as_of) if args.as_of else date.today()

    signals = []
    for row in holdings.get("positions", []):
        state = HoldingState(
            ticker=str(row["ticker"]).upper(),
            quality_percentile=float(row.get("quality_percentile", 1.0)),
            price_percentile=float(row.get("price_percentile", 1.0)),
            days_below_quality=int(row.get("days_below_quality", 0)),
            days_below_price=int(row.get("days_below_price", 0)),
            catalyst_news=bool(row.get("catalyst_news", False)),
            manual_override=bool(row.get("manual_override", False)),
        )
        signals.extend(evaluate_sell(cfg, state, today))

    if args.json:
        print(json.dumps([
            {
                "ticker": s.ticker,
                "trigger": s.trigger,
                "reason": s.reason,
                "as_of": s.as_of.isoformat(),
            } for s in signals
        ], indent=2))
        return 0

    if not signals:
        print("no sell signals")
        return 0
    for s in signals:
        print(f"{s.ticker:<8}  {s.trigger:<18}  {s.reason}")
    return 0


# ---------------------------------------------------------------------------
# parser
# ---------------------------------------------------------------------------


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="stock-scoring",
        description="Rubric-driven stock scoring & sizing for AI/DC/semi names.",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    sp_score = sub.add_parser("score", help="Score a ticker or universe")
    sp_score.add_argument("--rubric", type=Path, default=DEFAULT_RUBRIC)
    sp_score.add_argument("--ticker-file", type=Path)
    sp_score.add_argument("--universe", type=Path)
    sp_score.add_argument("--json", action="store_true")
    sp_score.set_defaults(func=_handle_score)

    sp_rank = sub.add_parser("rank", help="Score and rank the universe")
    sp_rank.add_argument("--rubric", type=Path, default=DEFAULT_RUBRIC)
    sp_rank.add_argument("--universe", type=Path, required=True)
    sp_rank.add_argument("--json", action="store_true")
    sp_rank.set_defaults(func=_handle_rank)

    sp_size = sub.add_parser("size", help="Rank + downside + PA → allocations")
    sp_size.add_argument("--rubric", type=Path, default=DEFAULT_RUBRIC)
    sp_size.add_argument("--sizing", type=Path, default=DEFAULT_SIZING)
    sp_size.add_argument("--price-action", type=Path, default=DEFAULT_PA)
    sp_size.add_argument("--universe", type=Path, required=True)
    sp_size.add_argument("--json", action="store_true")
    sp_size.set_defaults(func=_handle_size)

    sp_sell = sub.add_parser("sell", help="Evaluate sell triggers for holdings")
    sp_sell.add_argument("--sell", type=Path, default=DEFAULT_SELL)
    sp_sell.add_argument("--holdings", type=Path, required=True)
    sp_sell.add_argument("--as-of", help="YYYY-MM-DD (default: today)")
    sp_sell.add_argument("--json", action="store_true")
    sp_sell.set_defaults(func=_handle_sell)

    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        return int(args.func(args))
    except (FileNotFoundError, ValueError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
