"""Bet-sizing and downside-risk model."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any

import yaml

from .scoring import SignalSpec, _clamp, score_signal


@dataclass
class DownsideFactorScore:
    id: str
    score: float
    weight: float
    missing: bool


@dataclass
class DownsideReport:
    ticker: str
    total: float                     # [0,1], higher = more downside
    factors: list[DownsideFactorScore]
    missing: list[str]


@dataclass
class SizingConfig:
    max_position_pct: float
    capital_deploy_pct: float
    min_quality_score: float
    top_n: int
    meme_dip_bonus: float
    factors: list[SignalSpec]

    @classmethod
    def load(cls, path: str | Path) -> "SizingConfig":
        raw = yaml.safe_load(Path(path).read_text())
        sizing = raw.get("sizing") or {}
        factors_raw = (raw.get("downside") or {}).get("factors") or {}
        factors = [
            _parse_factor(factor_id, factor_data)
            for factor_id, factor_data in factors_raw.items()
        ]
        return cls(
            max_position_pct=float(sizing.get("max_position_pct", 0.1)),
            capital_deploy_pct=float(sizing.get("capital_deploy_pct", 0.8)),
            min_quality_score=float(sizing.get("min_quality_score", 0.5)),
            top_n=int(sizing.get("top_n", 15)),
            meme_dip_bonus=float(sizing.get("meme_dip_bonus", 0.0)),
            factors=factors,
        )


def _parse_factor(fid: str, data: dict[str, Any]) -> SignalSpec:
    # Downside factors reuse the same SignalSpec scoring so we get buckets,
    # linear, scale, invert for free. Default value for missing = 0.5 (neutral).
    kind = str(data.get("kind", "scale"))
    weight = float(data.get("weight", 1.0))
    invert = bool(data.get("invert", False))
    default = float(data.get("default", 0.5))
    prompt = str(data.get("prompt", ""))
    buckets_raw = data.get("buckets") or ()
    buckets = tuple({k: float(v) for k, v in b.items()} for b in buckets_raw)
    min_v = data.get("min")
    max_v = data.get("max")
    return SignalSpec(
        id=fid,
        kind=kind,
        weight=weight,
        invert=invert,
        default=default,
        prompt=prompt,
        buckets=buckets,
        min=float(min_v) if min_v is not None else None,
        max=float(max_v) if max_v is not None else None,
    )


def compute_downside(
    config: SizingConfig,
    ticker: str,
    inputs: dict[str, Any],
) -> DownsideReport:
    factor_scores: list[DownsideFactorScore] = []
    missing: list[str] = []
    pairs: list[tuple[float, float]] = []
    for spec in config.factors:
        raw_input = inputs.get(spec.id)
        result = score_signal(spec, raw_input)
        factor_scores.append(
            DownsideFactorScore(
                id=spec.id, score=result.score, weight=spec.weight, missing=result.missing
            )
        )
        if result.missing:
            missing.append(spec.id)
        pairs.append((result.score, spec.weight))
    total = _weighted_avg(pairs)
    return DownsideReport(
        ticker=ticker,
        total=_clamp(total),
        factors=factor_scores,
        missing=missing,
    )


def _weighted_avg(pairs: list[tuple[float, float]]) -> float:
    total_w = sum(w for _, w in pairs)
    if total_w <= 0:
        return 0.0
    return sum(v * w for v, w in pairs) / total_w


# ---------------------------------------------------------------------------
# Portfolio sizing
# ---------------------------------------------------------------------------


@dataclass
class Allocation:
    ticker: str
    quality_score: float
    downside_risk: float
    price_action_score: float
    conviction: float
    raw_weight: float
    position_pct: float      # capped fraction of book
    reason: str = ""


@dataclass
class Candidate:
    ticker: str
    quality_score: float
    downside_risk: float
    price_action_score: float = 1.0
    conviction: float = 1.0
    meme_dip: bool = False


def allocate(config: SizingConfig, candidates: list[Candidate]) -> list[Allocation]:
    """Rank → filter → size.

    Steps:
      1. Drop candidates below min_quality_score.
      2. Sort by quality, keep top_n.
      3. raw = quality * pa * (1 - downside) * conviction (+ meme_dip_bonus)
      4. Normalize raw weights to sum to capital_deploy_pct
      5. Cap each at max_position_pct
    """
    filtered = [c for c in candidates if c.quality_score >= config.min_quality_score]
    filtered.sort(key=lambda c: c.quality_score, reverse=True)
    top = filtered[: config.top_n]

    allocations: list[Allocation] = []
    for cand in top:
        downside_adj = _clamp(1.0 - cand.downside_risk)
        raw = (
            cand.quality_score
            * cand.price_action_score
            * downside_adj
            * cand.conviction
        )
        # Meme-dip bonus: if downside is meme-driven and quality is strong,
        # treat dips as buyable and nudge size up.
        if cand.meme_dip and cand.quality_score >= config.min_quality_score:
            raw *= 1.0 + config.meme_dip_bonus
        allocations.append(
            Allocation(
                ticker=cand.ticker,
                quality_score=cand.quality_score,
                downside_risk=cand.downside_risk,
                price_action_score=cand.price_action_score,
                conviction=cand.conviction,
                raw_weight=raw,
                position_pct=0.0,
            )
        )

    total_raw = sum(a.raw_weight for a in allocations)
    if total_raw <= 0:
        return allocations

    # Normalize to deploy_pct, then cap per-name, then redistribute leftover
    # proportionally to uncapped names.
    deploy = config.capital_deploy_pct
    cap = config.max_position_pct

    weights = [a.raw_weight / total_raw * deploy for a in allocations]
    weights = _apply_cap_and_redistribute(weights, cap, deploy)
    for alloc, w in zip(allocations, weights):
        alloc.position_pct = w
    return allocations


def _apply_cap_and_redistribute(
    weights: list[float], cap: float, deploy: float
) -> list[float]:
    weights = list(weights)
    # Iteratively cap and redistribute overflow to uncapped names.
    for _ in range(len(weights) + 1):
        overflow = 0.0
        uncapped_indices: list[int] = []
        for i, w in enumerate(weights):
            if w > cap:
                overflow += w - cap
                weights[i] = cap
            else:
                uncapped_indices.append(i)
        if overflow <= 1e-12 or not uncapped_indices:
            break
        uncapped_sum = sum(weights[i] for i in uncapped_indices)
        if uncapped_sum <= 0:
            # spread evenly
            bonus = overflow / len(uncapped_indices)
            for i in uncapped_indices:
                weights[i] += bonus
        else:
            for i in uncapped_indices:
                weights[i] += overflow * (weights[i] / uncapped_sum)
    # If capping leaves us under deploy (every name hit the cap), accept it —
    # don't force leverage.
    total = sum(weights)
    if total > deploy + 1e-9:
        # defensive scale-down
        weights = [w * deploy / total for w in weights]
    return weights
