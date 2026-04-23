"""Sell model with four triggers."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import date, timedelta
from pathlib import Path

import yaml


@dataclass
class SellConfig:
    quality_percentile: float
    quality_grace_days: int
    price_percentile: float
    price_grace_days: int

    @classmethod
    def load(cls, path: str | Path) -> "SellConfig":
        raw = yaml.safe_load(Path(path).read_text())
        triggers = raw.get("triggers") or {}
        q = triggers.get("quality_rank") or {}
        p = triggers.get("price_rank") or {}
        return cls(
            quality_percentile=float(q.get("percentile_threshold", 0.10)),
            quality_grace_days=int(q.get("grace_period_days", 0)),
            price_percentile=float(p.get("percentile_threshold", 0.20)),
            price_grace_days=int(p.get("grace_period_days", 0)),
        )


@dataclass
class SellSignal:
    ticker: str
    trigger: str        # "quality_rank" | "price_rank" | "catalyst_news" | "manual_override"
    reason: str
    as_of: date


@dataclass
class HoldingState:
    """What we track per position to evaluate sell triggers."""
    ticker: str
    quality_percentile: float
    price_percentile: float
    days_below_quality: int = 0
    days_below_price: int = 0
    catalyst_news: bool = False
    manual_override: bool = False


def evaluate(config: SellConfig, state: HoldingState, today: date) -> list[SellSignal]:
    """Return any sell signals firing for this holding."""
    signals: list[SellSignal] = []

    # Catalyst news & manual override fire immediately, no grace period.
    if state.catalyst_news:
        signals.append(
            SellSignal(
                ticker=state.ticker,
                trigger="catalyst_news",
                reason="sell-catalyst news event",
                as_of=today,
            )
        )
    if state.manual_override:
        signals.append(
            SellSignal(
                ticker=state.ticker,
                trigger="manual_override",
                reason="manual override (nervous)",
                as_of=today,
            )
        )

    # Quality rank: must stay in top `quality_percentile`.
    if (
        state.quality_percentile < (1.0 - config.quality_percentile)
        and state.days_below_quality >= config.quality_grace_days
    ):
        signals.append(
            SellSignal(
                ticker=state.ticker,
                trigger="quality_rank",
                reason=(
                    f"quality rank {state.quality_percentile:.2%} dropped below top "
                    f"{config.quality_percentile:.0%} for {state.days_below_quality}d"
                ),
                as_of=today,
            )
        )

    # Price rank: must stay in top `price_percentile`.
    if (
        state.price_percentile < (1.0 - config.price_percentile)
        and state.days_below_price >= config.price_grace_days
    ):
        signals.append(
            SellSignal(
                ticker=state.ticker,
                trigger="price_rank",
                reason=(
                    f"price rank {state.price_percentile:.2%} dropped below top "
                    f"{config.price_percentile:.0%} for {state.days_below_price}d"
                ),
                as_of=today,
            )
        )

    return signals


def bump_grace_counters(state: HoldingState, config: SellConfig) -> None:
    """Advance grace-period counters by one day; reset when back above threshold."""
    if state.quality_percentile < (1.0 - config.quality_percentile):
        state.days_below_quality += 1
    else:
        state.days_below_quality = 0
    if state.price_percentile < (1.0 - config.price_percentile):
        state.days_below_price += 1
    else:
        state.days_below_price = 0


def next_quality_check(last_check: date) -> date:
    """Quality is evaluated monthly per the spec."""
    return last_check + timedelta(days=30)


def next_price_check(last_check: date) -> date:
    """Price is evaluated weekly per the spec."""
    return last_check + timedelta(days=7)
