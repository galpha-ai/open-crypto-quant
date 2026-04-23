"""Tests for the price-action filter."""

from __future__ import annotations

from pathlib import Path

import pytest

from stock_scoring.price_action import PriceActionConfig, compute_price_action


REPO_ROOT = Path(__file__).resolve().parents[1]
PA_PATH = REPO_ROOT / "configs" / "price_action.yaml"


def test_pa_config_loads():
    cfg = PriceActionConfig.load(PA_PATH)
    assert len(cfg.factors) > 0


def test_pa_missing_inputs_produces_neutral_score():
    cfg = PriceActionConfig.load(PA_PATH)
    report = compute_price_action(cfg, "X", {})
    # Every factor uses default=0.5 → weighted average = 0.5
    assert report.score == pytest.approx(0.5, abs=1e-9)
    assert len(report.missing) == len(cfg.factors)


def test_pa_bullish_inputs_score_high():
    cfg = PriceActionConfig.load(PA_PATH)
    bullish = {
        "iv_regime": 0.9,
        "options_flow_bullish": 0.9,
        "short_interest": 0.25,
        "yuan_demand": 0.9,
        "cds_default_rate": 0.0,
        "hy_debt_volume": 0.9,
        "otm_call_oi_surge": 0.9,
        "leaps_being_bought": True,
        "put_oi_insider_signal": 0.0,
        "trend_filter": 0.9,
    }
    report = compute_price_action(cfg, "X", bullish)
    assert report.score > 0.8
    assert report.missing == []


def test_pa_bearish_inputs_score_low():
    cfg = PriceActionConfig.load(PA_PATH)
    bearish = {
        "iv_regime": 0.1,
        "options_flow_bullish": 0.1,
        "short_interest": 0.0,
        "yuan_demand": 0.1,
        "cds_default_rate": 0.05,
        "hy_debt_volume": 0.1,
        "otm_call_oi_surge": 0.0,
        "leaps_being_bought": False,
        "put_oi_insider_signal": 0.9,
        "trend_filter": 0.0,
    }
    report = compute_price_action(cfg, "X", bearish)
    assert report.score < 0.2
