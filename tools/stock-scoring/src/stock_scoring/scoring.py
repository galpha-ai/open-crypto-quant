"""Rubric-driven scoring engine.

The rubric is a tree: categories → criteria → signals. Each signal takes
an input value from a company-data dict and produces a score in [0, 1].
Sibling scores are combined with weighted averages, with weights renormalized
to sum to 1 at every level (so weights in the YAML are relative, not
absolute).
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import yaml


SignalKind = str  # "bool" | "bucket" | "linear" | "scale" | "categorical"


class RubricError(ValueError):
    """Raised when a rubric YAML is malformed."""


@dataclass(frozen=True)
class SignalSpec:
    id: str
    kind: SignalKind
    weight: float
    invert: bool = False
    default: float = 0.5
    prompt: str = ""
    # kind-specific:
    buckets: tuple[dict[str, float], ...] = ()
    min: float | None = None
    max: float | None = None
    values: dict[str, float] = field(default_factory=dict)
    group: str | None = None


@dataclass(frozen=True)
class CriterionSpec:
    id: str
    name: str
    weight: float
    signals: tuple[SignalSpec, ...]


@dataclass(frozen=True)
class CategorySpec:
    id: str
    weight: float
    criteria: tuple[CriterionSpec, ...]


@dataclass(frozen=True)
class Rubric:
    version: int
    name: str
    categories: tuple[CategorySpec, ...]


@dataclass
class SignalScore:
    id: str
    raw_input: Any
    score: float
    missing: bool


@dataclass
class CriterionScore:
    id: str
    name: str
    score: float
    weight: float
    signals: list[SignalScore]


@dataclass
class CategoryScore:
    id: str
    score: float
    weight: float
    criteria: list[CriterionScore]


@dataclass
class Scorecard:
    ticker: str
    total: float
    categories: list[CategoryScore]
    missing_signals: list[str]


# ---------------------------------------------------------------------------
# Loading
# ---------------------------------------------------------------------------


def load_rubric(path: str | Path) -> Rubric:
    raw = yaml.safe_load(Path(path).read_text())
    if not isinstance(raw, dict):
        raise RubricError(f"Rubric root must be a mapping, got {type(raw).__name__}")
    try:
        version = int(raw["version"])
        name = str(raw.get("name", "unnamed"))
        categories_raw = raw["categories"]
    except KeyError as exc:
        raise RubricError(f"Rubric missing required field: {exc}") from exc
    if not isinstance(categories_raw, dict) or not categories_raw:
        raise RubricError("Rubric `categories` must be a non-empty mapping")
    categories = tuple(
        _parse_category(cid, cdata) for cid, cdata in categories_raw.items()
    )
    return Rubric(version=version, name=name, categories=categories)


def _parse_category(cid: str, data: dict[str, Any]) -> CategorySpec:
    weight = float(data.get("weight", 1.0))
    criteria_raw = data.get("criteria")
    if not isinstance(criteria_raw, dict) or not criteria_raw:
        raise RubricError(f"Category `{cid}` requires `criteria` mapping")
    criteria = tuple(
        _parse_criterion(crit_id, crit_data)
        for crit_id, crit_data in criteria_raw.items()
    )
    return CategorySpec(id=cid, weight=weight, criteria=criteria)


def _parse_criterion(crit_id: str, data: dict[str, Any]) -> CriterionSpec:
    weight = float(data.get("weight", 1.0))
    name = str(data.get("name", crit_id))
    signals_raw = data.get("signals")
    if not isinstance(signals_raw, dict) or not signals_raw:
        raise RubricError(f"Criterion `{crit_id}` requires `signals` mapping")
    signals = tuple(
        _parse_signal(sig_id, sig_data) for sig_id, sig_data in signals_raw.items()
    )
    return CriterionSpec(id=crit_id, name=name, weight=weight, signals=signals)


def _parse_signal(sig_id: str, data: dict[str, Any]) -> SignalSpec:
    kind = str(data.get("kind", "scale"))
    if kind not in {"bool", "bucket", "linear", "scale", "categorical"}:
        raise RubricError(f"Signal `{sig_id}` has unknown kind `{kind}`")
    weight = float(data.get("weight", 1.0))
    invert = bool(data.get("invert", False))
    default = float(data.get("default", 0.5))
    prompt = str(data.get("prompt", ""))
    buckets_raw = data.get("buckets") or ()
    buckets = tuple(
        {k: float(v) for k, v in b.items()} for b in buckets_raw
    )
    min_v = data.get("min")
    max_v = data.get("max")
    values = {str(k): float(v) for k, v in (data.get("values") or {}).items()}
    group = data.get("group")
    if kind == "linear" and (min_v is None or max_v is None):
        raise RubricError(f"Signal `{sig_id}` kind=linear requires min and max")
    return SignalSpec(
        id=sig_id,
        kind=kind,
        weight=weight,
        invert=invert,
        default=default,
        prompt=prompt,
        buckets=buckets,
        min=float(min_v) if min_v is not None else None,
        max=float(max_v) if max_v is not None else None,
        values=values,
        group=group,
    )


# ---------------------------------------------------------------------------
# Scoring
# ---------------------------------------------------------------------------


def _clamp(x: float, lo: float = 0.0, hi: float = 1.0) -> float:
    return max(lo, min(hi, x))


def score_signal(spec: SignalSpec, value: Any) -> SignalScore:
    """Score a single signal. `value` may be None (treated as missing)."""
    if value is None:
        return SignalScore(id=spec.id, raw_input=None, score=spec.default, missing=True)

    if spec.kind == "bool":
        if isinstance(value, bool):
            raw = 1.0 if value else 0.0
        else:
            raw = 1.0 if str(value).lower() in {"true", "yes", "y", "1"} else 0.0
    elif spec.kind == "scale":
        raw = _clamp(float(value))
    elif spec.kind == "linear":
        assert spec.min is not None and spec.max is not None
        lo, hi = spec.min, spec.max
        if hi == lo:
            raw = 1.0
        else:
            raw = _clamp((float(value) - lo) / (hi - lo))
    elif spec.kind == "bucket":
        raw = _score_bucket(spec, float(value))
    elif spec.kind == "categorical":
        raw = _clamp(spec.values.get(str(value), spec.default))
    else:  # unreachable — validated at load time
        raise RubricError(f"Unknown kind {spec.kind}")

    if spec.invert:
        raw = 1.0 - raw
    return SignalScore(id=spec.id, raw_input=value, score=raw, missing=False)


def _score_bucket(spec: SignalSpec, x: float) -> float:
    """Bucket rules — each {min: X, score: S} or {max: X, score: S}.

    `min` buckets are checked in descending order of threshold: first `x >= min`
    wins. `max` buckets are checked in ascending order: first `x <= max` wins.
    Mixing keys in one rubric is allowed but discouraged; we evaluate in the
    order given in YAML.
    """
    for bucket in spec.buckets:
        if "min" in bucket and x >= bucket["min"]:
            return _clamp(bucket["score"])
        if "max" in bucket and x <= bucket["max"]:
            return _clamp(bucket["score"])
    return spec.default


def _weighted_avg(pairs: list[tuple[float, float]]) -> float:
    """Given (value, weight) pairs, return the renormalized weighted average."""
    total_w = sum(w for _, w in pairs)
    if total_w <= 0:
        return 0.0
    return sum(v * w for v, w in pairs) / total_w


def score_ticker(
    rubric: Rubric,
    inputs: dict[str, Any],
    ticker: str = "",
) -> Scorecard:
    """Score a single ticker. `inputs` is a flat dict keyed by signal id."""
    category_scores: list[CategoryScore] = []
    missing: list[str] = []

    for category in rubric.categories:
        criterion_scores: list[CriterionScore] = []
        for criterion in category.criteria:
            signal_scores = [score_signal(s, inputs.get(s.id)) for s in criterion.signals]
            for spec, result in zip(criterion.signals, signal_scores):
                if result.missing:
                    missing.append(f"{category.id}.{criterion.id}.{spec.id}")
            pairs = [(r.score, s.weight) for s, r in zip(criterion.signals, signal_scores)]
            crit_score = _weighted_avg(pairs)
            criterion_scores.append(
                CriterionScore(
                    id=criterion.id,
                    name=criterion.name,
                    score=crit_score,
                    weight=criterion.weight,
                    signals=signal_scores,
                )
            )
        cat_pairs = [(c.score, c.weight) for c in criterion_scores]
        cat_score = _weighted_avg(cat_pairs)
        category_scores.append(
            CategoryScore(
                id=category.id,
                score=cat_score,
                weight=category.weight,
                criteria=criterion_scores,
            )
        )

    total_pairs = [(c.score, c.weight) for c in category_scores]
    total = _weighted_avg(total_pairs)
    return Scorecard(
        ticker=ticker,
        total=total,
        categories=category_scores,
        missing_signals=missing,
    )


def rank_scorecards(scorecards: list[Scorecard]) -> list[Scorecard]:
    """Return scorecards sorted by total score descending (stable)."""
    return sorted(scorecards, key=lambda s: s.total, reverse=True)


def percentile_rank(scorecards: list[Scorecard], ticker: str) -> float:
    """Return the percentile rank of `ticker` (1.0 = top, 0.0 = bottom).

    Ties share the mean rank.
    """
    if not scorecards:
        return 0.0
    target = next((s for s in scorecards if s.ticker == ticker), None)
    if target is None:
        raise KeyError(f"ticker {ticker!r} not in scorecards")
    scores = sorted((s.total for s in scorecards), reverse=True)
    # index of the first score <= target.total in the descending list
    above = sum(1 for s in scores if s > target.total)
    equal = sum(1 for s in scores if s == target.total)
    # mean of the ranks of the tied group, converted to a top-percentile
    mean_rank = above + (equal + 1) / 2  # 1-indexed
    return 1.0 - (mean_rank - 1) / len(scores)
