"""Tests for the rubric loader and scoring engine."""

from __future__ import annotations

from pathlib import Path

import pytest

from stock_scoring.scoring import (
    Rubric,
    SignalSpec,
    load_rubric,
    percentile_rank,
    rank_scorecards,
    score_signal,
    score_ticker,
)


REPO_ROOT = Path(__file__).resolve().parents[1]
RUBRIC_PATH = REPO_ROOT / "configs" / "rubric.yaml"


def test_rubric_loads_and_has_expected_pillars():
    rubric = load_rubric(RUBRIC_PATH)
    assert isinstance(rubric, Rubric)
    pillar_ids = {c.id for c in rubric.categories}
    assert pillar_ids == {"fundamental", "marketing", "recent"}
    weights = {c.id: c.weight for c in rubric.categories}
    assert weights["fundamental"] == pytest.approx(0.60)
    assert weights["marketing"] == pytest.approx(0.20)
    assert weights["recent"] == pytest.approx(0.20)


def test_signal_scoring_bool():
    spec = SignalSpec(id="x", kind="bool", weight=1.0)
    assert score_signal(spec, True).score == 1.0
    assert score_signal(spec, False).score == 0.0
    # inverted
    spec_inv = SignalSpec(id="x", kind="bool", weight=1.0, invert=True)
    assert score_signal(spec_inv, True).score == 0.0
    assert score_signal(spec_inv, False).score == 1.0


def test_signal_scoring_linear():
    spec = SignalSpec(id="x", kind="linear", weight=1.0, min=0.0, max=10.0)
    assert score_signal(spec, 0.0).score == 0.0
    assert score_signal(spec, 10.0).score == 1.0
    assert score_signal(spec, 5.0).score == pytest.approx(0.5)
    # clamping
    assert score_signal(spec, -5.0).score == 0.0
    assert score_signal(spec, 25.0).score == 1.0


def test_signal_scoring_linear_inverted():
    spec = SignalSpec(id="x", kind="linear", weight=1.0, min=0.0, max=4.0, invert=True)
    assert score_signal(spec, 0.0).score == 1.0   # 0 debt = best
    assert score_signal(spec, 4.0).score == 0.0   # 4x = worst


def test_signal_scoring_bucket_min_style():
    spec = SignalSpec(
        id="x",
        kind="bucket",
        weight=1.0,
        buckets=(
            {"min": 10000000.0, "score": 1.0},
            {"min": 3000000.0, "score": 0.5},
            {"min": 0.0, "score": 0.0},
        ),
    )
    assert score_signal(spec, 50_000_000).score == 1.0
    assert score_signal(spec, 5_000_000).score == 0.5
    assert score_signal(spec, 500_000).score == 0.0


def test_signal_scoring_bucket_max_style():
    spec = SignalSpec(
        id="x",
        kind="bucket",
        weight=1.0,
        buckets=(
            {"max": 0.001, "score": 1.0},
            {"max": 0.01, "score": 0.5},
            {"max": 0.05, "score": 0.2},
            {"max": 1.0, "score": 0.0},
        ),
    )
    assert score_signal(spec, 0.0005).score == 1.0
    assert score_signal(spec, 0.005).score == 0.5
    assert score_signal(spec, 0.03).score == 0.2
    assert score_signal(spec, 0.5).score == 0.0


def test_missing_input_uses_default_and_is_flagged():
    spec = SignalSpec(id="x", kind="bool", weight=1.0, default=0.5)
    result = score_signal(spec, None)
    assert result.missing is True
    assert result.score == 0.5


def test_perfect_inputs_score_near_one():
    """Every signal answered maximally should yield a score near 1.0."""
    rubric = load_rubric(RUBRIC_PATH)
    inputs = _max_inputs(rubric)
    card = score_ticker(rubric, inputs, "TEST")
    assert card.total == pytest.approx(1.0, abs=1e-6)
    assert card.missing_signals == []


def test_worst_inputs_score_zero():
    rubric = load_rubric(RUBRIC_PATH)
    inputs = _min_inputs(rubric)
    card = score_ticker(rubric, inputs, "TEST")
    assert card.total == pytest.approx(0.0, abs=1e-6)


def test_sample_nvda_beats_intc():
    from stock_scoring.inputs import load_company

    rubric = load_rubric(RUBRIC_PATH)
    nvda = load_company(REPO_ROOT / "data" / "sample" / "NVDA.yaml")
    intc = load_company(REPO_ROOT / "data" / "sample" / "INTC.yaml")
    nvda_card = score_ticker(rubric, nvda.signals, nvda.ticker)
    intc_card = score_ticker(rubric, intc.signals, intc.ticker)
    assert nvda_card.total > intc_card.total
    # sanity: NVDA reference case should be reasonably high
    assert nvda_card.total > 0.7


def test_rank_and_percentile():
    from stock_scoring.inputs import load_universe

    rubric = load_rubric(RUBRIC_PATH)
    universe = load_universe(REPO_ROOT / "data" / "sample")
    cards = [score_ticker(rubric, c.signals, c.ticker) for c in universe]
    ranked = rank_scorecards(cards)
    # monotonically descending
    for earlier, later in zip(ranked, ranked[1:]):
        assert earlier.total >= later.total
    top = ranked[0].ticker
    assert percentile_rank(cards, top) == pytest.approx(1.0, abs=1e-6)


def test_malformed_rubric_raises():
    from stock_scoring.scoring import RubricError
    import tempfile

    with tempfile.NamedTemporaryFile("w", suffix=".yaml", delete=False) as f:
        f.write("version: 1\nname: bad\n")  # no categories
        path = f.name
    with pytest.raises(RubricError):
        load_rubric(path)


# ---------------------------------------------------------------------------
# helpers
# ---------------------------------------------------------------------------


def _max_inputs(rubric: Rubric) -> dict:
    """Build an input dict that maxes every signal."""
    out = {}
    for cat in rubric.categories:
        for crit in cat.criteria:
            for sig in crit.signals:
                out[sig.id] = _max_value_for(sig)
    return out


def _min_inputs(rubric: Rubric) -> dict:
    out = {}
    for cat in rubric.categories:
        for crit in cat.criteria:
            for sig in crit.signals:
                out[sig.id] = _min_value_for(sig)
    return out


def _bucket_candidate_values(spec: SignalSpec) -> list[float]:
    """Reasonable candidate values to try for finding min/max bucket scores."""
    out: list[float] = [0.0, -1e9, 1e9]
    for b in spec.buckets:
        if "min" in b:
            out.extend([b["min"], b["min"] + 1e-6, b["min"] + 1.0])
        if "max" in b:
            out.extend([b["max"], max(0.0, b["max"] - 1e-6), b["max"] + 1.0])
    return out


def _bucket_value_for_extreme(spec: SignalSpec, *, maximize: bool) -> float:
    """Return a numeric value that yields min or max score through `spec`."""
    from stock_scoring.scoring import score_signal as _score

    best_val, best_score = 0.0, (-1.0 if maximize else 2.0)
    for v in _bucket_candidate_values(spec):
        s = _score(spec, v).score
        if maximize and s > best_score:
            best_score, best_val = s, v
        if (not maximize) and s < best_score:
            best_score, best_val = s, v
    return best_val


def _max_value_for(spec: SignalSpec):
    if spec.kind == "bool":
        return False if spec.invert else True
    if spec.kind == "scale":
        return 0.0 if spec.invert else 1.0
    if spec.kind == "linear":
        return spec.min if spec.invert else spec.max
    if spec.kind == "bucket":
        return _bucket_value_for_extreme(spec, maximize=True)
    return 1.0


def _min_value_for(spec: SignalSpec):
    if spec.kind == "bool":
        return True if spec.invert else False
    if spec.kind == "scale":
        return 1.0 if spec.invert else 0.0
    if spec.kind == "linear":
        return spec.max if spec.invert else spec.min
    if spec.kind == "bucket":
        return _bucket_value_for_extreme(spec, maximize=False)
    return 0.0
