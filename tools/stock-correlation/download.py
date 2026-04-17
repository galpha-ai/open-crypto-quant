"""Download daily OHLCV for SNDK (SanDisk) and 285A.T (Kioxia Holdings).

Writes CSVs to ./data/ and prints a short summary.
"""
from __future__ import annotations

import sys
from pathlib import Path

import pandas as pd
import yfinance as yf

TICKERS = {
    "SNDK": "SNDK",      # SanDisk Corp (NASDAQ)
    "285A.T": "285A.T",  # Kioxia Holdings (Tokyo)
}

OUT = Path(__file__).parent / "data"
OUT.mkdir(parents=True, exist_ok=True)


def fetch(ticker: str, period: str = "6mo") -> pd.DataFrame:
    df = yf.download(ticker, period=period, interval="1d",
                     auto_adjust=False, progress=False)
    if df.empty:
        raise RuntimeError(f"No data for {ticker}")
    if isinstance(df.columns, pd.MultiIndex):
        df.columns = df.columns.get_level_values(0)
    df.index = pd.to_datetime(df.index).tz_localize(None)
    return df


def main() -> int:
    for label, tkr in TICKERS.items():
        df = fetch(tkr)
        path = OUT / f"{label.replace('.', '_')}.csv"
        df.to_csv(path)
        print(f"{label}: {len(df)} rows  "
              f"{df.index.min().date()} .. {df.index.max().date()}  "
              f"last_close={df['Close'].iloc[-1]:.2f}  -> {path.name}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
