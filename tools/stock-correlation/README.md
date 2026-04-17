# SNDK vs 285A.T (Kioxia) correlation analysis

Quick scripts to download daily prices for **SanDisk (SNDK)** and
**Kioxia Holdings (285A.T)** and compute their 3-month correlation,
lead/lag relationship, and peer-group context.

## Usage

```bash
python3 -m venv .venv && source .venv/bin/activate
pip install yfinance pandas numpy

python3 download.py    # writes data/SNDK.csv, data/285A_T.csv
python3 analyze.py     # prints summary + correlations
python3 peers.py       # MU, WDC, Nikkei, SOXX cross-check
```

## Findings (snapshot 2026-04-17)

Over trailing 3 months both names roughly doubled (SNDK +103%, 285A +101%)
on the NAND-memory up-cycle, with ~100% annualized vol on each.

| Metric | Value |
|---|---|
| Pearson, close prices | +0.911 (both trend up) |
| Pearson, same-day log returns | **+0.23** |
| SNDK_{t-1} → 285A_t corr | **+0.43** |
| 285A_{t-1} → SNDK_t corr | -0.13 (no signal) |
| Rolling-20d return corr (latest) | +0.44 (vs mean +0.18) |
| OLS: 285A_t ≈ 0.78% + 0.42·SNDK_{t-1} | R² = 0.18 |

Today's 285A -9.86% vs model expectation +2.07% (from SNDK +3.11%
the prior US session) → residual ≈ **-12%** idiosyncratic.
Nikkei today -1.75%, so ~8% is name/sector-specific.

Earlier in the week (2026-04-15) the whole NAND group sold off together
(SNDK -5.6%, 285A -7.4%, MU -2.0%). 4/16 both rebounded. 4/17 Kioxia
dumped while US peers have not yet printed.
