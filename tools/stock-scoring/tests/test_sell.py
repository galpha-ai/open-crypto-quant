"""Tests for the sell-trigger model."""

from __future__ import annotations

from datetime import date
from pathlib import Path

from stock_scoring.sell import (
    HoldingState,
    SellConfig,
    bump_grace_counters,
    evaluate,
)


REPO_ROOT = Path(__file__).resolve().parents[1]
SELL_PATH = REPO_ROOT / "configs" / "sell.yaml"


def test_config_loads():
    cfg = SellConfig.load(SELL_PATH)
    assert cfg.quality_percentile == 0.10
    assert cfg.price_percentile == 0.20
    assert cfg.quality_grace_days == 7
    assert cfg.price_grace_days == 14


def test_healthy_holding_triggers_nothing():
    cfg = SellConfig.load(SELL_PATH)
    state = HoldingState(
        ticker="NVDA", quality_percentile=0.98, price_percentile=0.95
    )
    assert evaluate(cfg, state, date(2026, 4, 1)) == []


def test_quality_drop_fires_after_grace_period():
    cfg = SellConfig.load(SELL_PATH)
    # below top 10% (percentile < 0.90)
    state = HoldingState(
        ticker="TSM",
        quality_percentile=0.80,
        price_percentile=0.95,
        days_below_quality=cfg.quality_grace_days,
    )
    sigs = evaluate(cfg, state, date(2026, 4, 1))
    triggers = {s.trigger for s in sigs}
    assert "quality_rank" in triggers


def test_quality_drop_respects_grace_period():
    cfg = SellConfig.load(SELL_PATH)
    state = HoldingState(
        ticker="TSM",
        quality_percentile=0.80,
        price_percentile=0.95,
        days_below_quality=cfg.quality_grace_days - 1,
    )
    sigs = evaluate(cfg, state, date(2026, 4, 1))
    triggers = {s.trigger for s in sigs}
    assert "quality_rank" not in triggers


def test_price_drop_fires_after_grace():
    cfg = SellConfig.load(SELL_PATH)
    state = HoldingState(
        ticker="VRT",
        quality_percentile=0.95,
        price_percentile=0.70,
        days_below_price=cfg.price_grace_days,
    )
    triggers = {s.trigger for s in evaluate(cfg, state, date(2026, 4, 1))}
    assert "price_rank" in triggers


def test_catalyst_news_fires_immediately():
    cfg = SellConfig.load(SELL_PATH)
    state = HoldingState(
        ticker="X", quality_percentile=0.99, price_percentile=0.99, catalyst_news=True
    )
    triggers = {s.trigger for s in evaluate(cfg, state, date(2026, 4, 1))}
    assert "catalyst_news" in triggers


def test_manual_override_fires():
    cfg = SellConfig.load(SELL_PATH)
    state = HoldingState(
        ticker="X",
        quality_percentile=0.99,
        price_percentile=0.99,
        manual_override=True,
    )
    triggers = {s.trigger for s in evaluate(cfg, state, date(2026, 4, 1))}
    assert "manual_override" in triggers


def test_multiple_triggers_aggregate():
    cfg = SellConfig.load(SELL_PATH)
    state = HoldingState(
        ticker="X",
        quality_percentile=0.50,
        price_percentile=0.50,
        days_below_quality=365,
        days_below_price=365,
        catalyst_news=True,
    )
    triggers = {s.trigger for s in evaluate(cfg, state, date(2026, 4, 1))}
    assert triggers == {"quality_rank", "price_rank", "catalyst_news"}


def test_grace_counter_bump_and_reset():
    cfg = SellConfig.load(SELL_PATH)
    state = HoldingState(
        ticker="X", quality_percentile=0.5, price_percentile=0.5
    )
    bump_grace_counters(state, cfg)
    bump_grace_counters(state, cfg)
    assert state.days_below_quality == 2
    assert state.days_below_price == 2
    # Now back above the threshold — counters should reset.
    state.quality_percentile = 0.99
    state.price_percentile = 0.99
    bump_grace_counters(state, cfg)
    assert state.days_below_quality == 0
    assert state.days_below_price == 0
