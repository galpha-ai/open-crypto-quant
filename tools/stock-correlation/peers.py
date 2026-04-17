"""Short cross-check: pull NAND peers + Nikkei/SOXX for the last few days.

Purpose: decide whether today's 285A.T -9% print is idiosyncratic to Kioxia,
Japan-market-driven, or NAND-sector driven (implication for SNDK tonight).
"""
from __future__ import annotations

import pandas as pd
import yfinance as yf

TICKERS = {
    "SNDK": "SNDK",
    "MU": "MU",
    "WDC": "WDC",
    "285A": "285A.T",
    "NIKKEI": "^N225",
    "SOXX": "SOXX",
}


def main() -> None:
    frames = []
    for label, tkr in TICKERS.items():
        df = yf.download(tkr, period="10d", interval="1d",
                         auto_adjust=False, progress=False)
        if isinstance(df.columns, pd.MultiIndex):
            df.columns = df.columns.get_level_values(0)
        frames.append(df[["Close"]].rename(columns={"Close": label}))
    prices = pd.concat(frames, axis=1)
    rets = prices.pct_change().mul(100)

    print("Last 5 closes:")
    print(prices.tail(5).round(2))
    print("\nDaily % change (last 5 sessions):")
    print(rets.tail(5).round(2))


if __name__ == "__main__":
    main()
