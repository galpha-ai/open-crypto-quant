"""Price-action filter. Produces a price_action_score in [0, 1]."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any

import yaml

from .scoring import SignalSpec, _clamp, score_signal


@dataclass
class PriceActionReport:
    ticker: str
    score: float
    factor_scores: dict[str, float]
    missing: list[str]


@dataclass
class PriceActionConfig:
    factors: list[SignalSpec]

    @classmethod
    def load(cls, path: str | Path) -> "PriceActionConfig":
        raw = yaml.safe_load(Path(path).read_text())
        factors_raw = raw.get("factors") or {}
        factors = [_parse_pa_factor(fid, fdata) for fid, fdata in factors_raw.items()]
        return cls(factors=factors)


def _parse_pa_factor(fid: str, data: dict[str, Any]) -> SignalSpec:
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


def compute_price_action(
    config: PriceActionConfig,
    ticker: str,
    inputs: dict[str, Any],
) -> PriceActionReport:
    factor_scores: dict[str, float] = {}
    missing: list[str] = []
    pairs: list[tuple[float, float]] = []
    for spec in config.factors:
        result = score_signal(spec, inputs.get(spec.id))
        factor_scores[spec.id] = result.score
        if result.missing:
            missing.append(spec.id)
        pairs.append((result.score, spec.weight))
    total_w = sum(w for _, w in pairs)
    score = sum(v * w for v, w in pairs) / total_w if total_w > 0 else 0.0
    return PriceActionReport(
        ticker=ticker,
        score=_clamp(score),
        factor_scores=factor_scores,
        missing=missing,
    )
