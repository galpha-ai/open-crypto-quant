"""Tests for the bet-sizing and downside-risk model."""

from __future__ import annotations

from pathlib import Path

import pytest

from stock_scoring.sizing import Candidate, SizingConfig, allocate, compute_downside


REPO_ROOT = Path(__file__).resolve().parents[1]
SIZING_PATH = REPO_ROOT / "configs" / "sizing.yaml"


def test_sizing_config_loads():
    cfg = SizingConfig.load(SIZING_PATH)
    assert cfg.max_position_pct == pytest.approx(0.10)
    assert cfg.capital_deploy_pct == pytest.approx(0.80)
    assert cfg.top_n >= 5
    # Every factor has a positive weight.
    assert all(s.weight > 0 for s in cfg.factors)


def test_downside_bigger_concentration_means_bigger_risk():
    cfg = SizingConfig.load(SIZING_PATH)
    low = compute_downside(
        cfg,
        "A",
        {"revenue_concentration_top_client": 0.05, "revenue_concentration_top5": 0.30},
    )
    high = compute_downside(
        cfg,
        "B",
        {"revenue_concentration_top_client": 0.45, "revenue_concentration_top5": 0.85},
    )
    assert high.total > low.total


def test_allocate_respects_max_position_cap():
    cfg = SizingConfig.load(SIZING_PATH)
    # One dominant candidate. Should be capped at max_position_pct.
    cands = [
        Candidate(ticker="A", quality_score=0.99, downside_risk=0.05,
                  price_action_score=1.0, conviction=1.0),
        Candidate(ticker="B", quality_score=0.60, downside_risk=0.1,
                  price_action_score=1.0, conviction=1.0),
        Candidate(ticker="C", quality_score=0.60, downside_risk=0.1,
                  price_action_score=1.0, conviction=1.0),
    ]
    allocs = allocate(cfg, cands)
    assert all(a.position_pct <= cfg.max_position_pct + 1e-9 for a in allocs)


def test_allocate_filters_below_min_quality():
    cfg = SizingConfig.load(SIZING_PATH)
    cands = [
        Candidate(ticker="GOOD", quality_score=0.80, downside_risk=0.1),
        Candidate(ticker="BAD", quality_score=0.20, downside_risk=0.1),
    ]
    allocs = allocate(cfg, cands)
    assert {a.ticker for a in allocs} == {"GOOD"}


def test_allocate_deploys_at_most_capital_pct():
    cfg = SizingConfig.load(SIZING_PATH)
    cands = [
        Candidate(ticker=f"T{i}", quality_score=0.7, downside_risk=0.2)
        for i in range(20)
    ]
    allocs = allocate(cfg, cands)
    deployed = sum(a.position_pct for a in allocs)
    assert deployed <= cfg.capital_deploy_pct + 1e-9
    # And should be close to cap when there are plenty of candidates.
    assert deployed >= cfg.capital_deploy_pct * 0.95


def test_higher_downside_reduces_weight():
    cfg = SizingConfig.load(SIZING_PATH)
    # Include filler candidates so the cap doesn't force both to the same size.
    cands = [
        Candidate(ticker="LOW", quality_score=0.80, downside_risk=0.10),
        Candidate(ticker="HIGH", quality_score=0.80, downside_risk=0.60),
    ] + [
        Candidate(ticker=f"F{i}", quality_score=0.60, downside_risk=0.3)
        for i in range(15)
    ]
    allocs = {a.ticker: a for a in allocate(cfg, cands)}
    assert allocs["LOW"].position_pct > allocs["HIGH"].position_pct


def test_meme_dip_bonus_applies():
    cfg = SizingConfig.load(SIZING_PATH)
    baseline = Candidate(ticker="A", quality_score=0.80, downside_risk=0.2)
    boosted = Candidate(ticker="B", quality_score=0.80, downside_risk=0.2, meme_dip=True)
    allocs = {a.ticker: a for a in allocate(cfg, [baseline, boosted])}
    assert allocs["B"].position_pct >= allocs["A"].position_pct
