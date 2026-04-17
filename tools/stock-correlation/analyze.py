"""Correlation + drawdown analysis for SNDK vs 285A.T (Kioxia).

Reads CSVs written by download.py. Prints:
  - summary stats over trailing ~3 months
  - Pearson correlation on close prices and log returns
  - rolling 20d return correlation
  - beta of 285A.T returns on SNDK returns (lag 0 and lag 1)
  - today / last-day move comparison
"""
from __future__ import annotations

from pathlib import Path

import numpy as np
import pandas as pd

DATA = Path(__file__).parent / "data"
LOOKBACK_DAYS = 90  # ~3 calendar months


def load(label: str) -> pd.Series:
    df = pd.read_csv(DATA / f"{label}.csv", index_col=0, parse_dates=True)
    return df["Close"].rename(label)


def summarize(s: pd.Series) -> dict:
    last = s.iloc[-1]
    first = s.iloc[0]
    mx, mn = s.max(), s.min()
    ret = s.pct_change().dropna()
    return {
        "first_date": s.index[0].date(),
        "last_date": s.index[-1].date(),
        "first": first,
        "last": last,
        "total_return_%": (last / first - 1) * 100,
        "max": mx,
        "min": mn,
        "drawdown_from_peak_%": (last / mx - 1) * 100,
        "ann_vol_%": ret.std() * np.sqrt(252) * 100,
        "last_day_ret_%": ret.iloc[-1] * 100,
    }


def main() -> None:
    sndk = load("SNDK")
    kio = load("285A_T")

    # Restrict to last 3 months
    cutoff = max(sndk.index.max(), kio.index.max()) - pd.Timedelta(days=LOOKBACK_DAYS)
    sndk = sndk[sndk.index >= cutoff]
    kio = kio[kio.index >= cutoff]

    print("=" * 72)
    print(f"SANDISK (SNDK) — trailing {LOOKBACK_DAYS}d")
    print("=" * 72)
    for k, v in summarize(sndk).items():
        print(f"  {k:<22} {v}")

    print()
    print("=" * 72)
    print(f"KIOXIA  (285A.T) — trailing {LOOKBACK_DAYS}d")
    print("=" * 72)
    for k, v in summarize(kio).items():
        print(f"  {k:<22} {v}")

    # Align on calendar date (timezones differ; use date index)
    s1 = sndk.copy(); s1.index = s1.index.normalize()
    s2 = kio.copy(); s2.index = s2.index.normalize()

    # Pair "same calendar date" -- but note JP closes before US opens.
    # For co-movement sensitivity, also shift SNDK +1 day (prior US close
    # is the information set when Tokyo opens the next morning).
    prices = pd.concat({"SNDK": s1, "285A": s2}, axis=1).dropna()
    rets = np.log(prices).diff().dropna()

    print()
    print("=" * 72)
    print(f"ALIGNED PAIRS: {len(prices)} trading days  "
          f"({prices.index.min().date()} .. {prices.index.max().date()})")
    print("=" * 72)
    print(f"  Pearson corr (close prices)          : "
          f"{prices['SNDK'].corr(prices['285A']):+.3f}")
    print(f"  Pearson corr (daily log-returns, t=0): "
          f"{rets['SNDK'].corr(rets['285A']):+.3f}")

    # Lead/lag: SNDK close feeds into 285A next day
    shifted = pd.concat(
        {"SNDK_prev": rets["SNDK"].shift(1), "285A": rets["285A"]}, axis=1
    ).dropna()
    print(f"  Corr (SNDK_{{t-1}} vs 285A_t)          : "
          f"{shifted['SNDK_prev'].corr(shifted['285A']):+.3f}")

    # Reverse
    shifted2 = pd.concat(
        {"SNDK": rets["SNDK"], "285A_prev": rets["285A"].shift(1)}, axis=1
    ).dropna()
    print(f"  Corr (285A_{{t-1}} vs SNDK_t)          : "
          f"{shifted2['285A_prev'].corr(shifted2['SNDK']):+.3f}")

    # Rolling 20d return correlation
    roll = rets["SNDK"].rolling(20).corr(rets["285A"]).dropna()
    if len(roll):
        print(f"  Rolling-20d return corr: last={roll.iloc[-1]:+.3f}  "
              f"mean={roll.mean():+.3f}  min={roll.min():+.3f}  "
              f"max={roll.max():+.3f}")

    # Beta of 285A on SNDK_prev (closer to actual information flow)
    x = shifted["SNDK_prev"].values
    y = shifted["285A"].values
    beta = np.cov(x, y, ddof=0)[0, 1] / np.var(x)
    alpha = y.mean() - beta * x.mean()
    yhat = alpha + beta * x
    ss_res = ((y - yhat) ** 2).sum()
    ss_tot = ((y - y.mean()) ** 2).sum()
    r2 = 1 - ss_res / ss_tot
    print(f"  OLS  285A_t = {alpha*100:+.3f}% + {beta:+.3f}*SNDK_{{t-1}}   "
          f"R²={r2:.3f}")

    # Last-move diagnostic
    print()
    print("=" * 72)
    print("LAST-DAY CO-MOVEMENT")
    print("=" * 72)
    last_sndk_ret = sndk.pct_change().iloc[-1] * 100
    last_kio_ret = kio.pct_change().iloc[-1] * 100
    print(f"  SNDK  last close = {sndk.iloc[-1]:>10,.2f}  "
          f"move = {last_sndk_ret:+.2f}%  on {sndk.index[-1].date()}")
    print(f"  285A  last close = {kio.iloc[-1]:>10,.2f}  "
          f"move = {last_kio_ret:+.2f}%  on {kio.index[-1].date()}")

    # Expected 285A move given SNDK_{t-1} and the beta
    if len(sndk) >= 2:
        expected = alpha + beta * (last_sndk_ret / 100)
        resid = last_kio_ret / 100 - expected
        print(f"  Expected 285A from SNDK_{{t-1}}={last_sndk_ret:+.2f}%: "
              f"{expected*100:+.2f}%  (residual {resid*100:+.2f}%)")

    # Worst 5 days for each over the window, to spot co-crashes
    print()
    print("=" * 72)
    print("WORST 5 DAYS (ret %) FOR EACH NAME — look for same-date clusters")
    print("=" * 72)
    sndk_ret = sndk.pct_change().dropna() * 100
    kio_ret = kio.pct_change().dropna() * 100
    print("  SNDK:")
    for dt, r in sndk_ret.nsmallest(5).items():
        print(f"    {dt.date()}  {r:+.2f}%")
    print("  285A:")
    for dt, r in kio_ret.nsmallest(5).items():
        print(f"    {dt.date()}  {r:+.2f}%")


if __name__ == "__main__":
    main()
