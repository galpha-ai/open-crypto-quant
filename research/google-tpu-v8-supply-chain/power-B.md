# Server Power Supply — Bucket B (MPWR / AEIS / VICR)

Author: Subagent B
As-of date: 2026-05-10
Coverage period: Q1 FY2026 prints (Mar-Q for all three; calendar-quarter reporters)

Notes on sourcing:
- Primary sources: company press releases (GlobeNewswire / IR), 10-Q / 10-K, and earnings call transcripts (Motley Fool, Investing.com transcripts).
- Where transcript-only commentary is the source, that's flagged.
- For TPU/GPU revenue splits, only MPWR discloses an "enterprise data" segment that maps cleanly to AI accelerator power. VICR and AEIS do NOT disaggregate by AI customer.
- Anything > 120 days old is flagged STALE.

---

## MPWR — Monolithic Power Systems

### 1. TTM revenue & last-Q revenue (YoY)
- **Last-Q (Mar-Q 2026, fiscal quarter ended Mar 31, 2026):** $804.2M, +26.1% YoY, +7.1% QoQ. (Source: MPWR Q1 2026 press release, Apr 30, 2026.)
- **TTM (Apr-2025 through Mar-2026):** ≈ $2.99B. Build: FY2025 $2.79B − Q1-2025 ($638.1M, implied from +26% YoY off $804.2M) + Q1-2026 $804.2M ≈ $2.96B–$2.99B. (Derived from FY2025 press release and Q1-2026 PR.)

### 2. Forward revenue
- **Q2 2026 guide (company):** $890M–$910M (midpoint $900M), implying ~+30% YoY. (Source: Q1-2026 PR.)
- **FY2026 (company):** No formal full-year revenue guide. Management raised the enterprise-data segment growth floor to ~85% YoY (from prior 50% floor); other segments not re-guided to specific dollars.
- **Street FY2026/FY2027 consensus:** Not gathered from primary source; sell-side estimates not reproduced here.

### 3. Earnings guidance
- **Q2 2026 GAAP GM:** 55.1%–55.7%; **non-GAAP GM:** 55.3%–55.9%.
- **Q2 2026 GAAP OpEx:** $219.1M–$225.1M; **non-GAAP OpEx:** $167.0M–$171.0M.
- **EPS range:** Not explicitly disclosed by company (MPWR guides revenue and GM, not EPS).
- **FCF guide:** Not provided. Q1 2026 operating cash flow $250.3M (disclosed).

### 4. EPS
- **Q1 2026 GAAP diluted EPS:** $3.92.
- **Q1 2026 non-GAAP diluted EPS:** $5.10 (vs. street $4.90).
- **TTM non-GAAP EPS:** ≈ $19.7 (FY2025 $17.77 + Q1-2026 $5.10 − Q1-2025 ≈ $3.1 implied).

### 5. Gross margin
- **Q1 2026:** GAAP 55.3%, non-GAAP 55.5%.
- **4-Q trend (non-GAAP):**
  - Q2 2025: 55.5%
  - Q3 2025: 55.5%
  - Q4 2025: ~55.5% (FY2025 GAAP 55.2%; guide had been 55.2–55.8%)
  - Q1 2026: 55.5%
  - Trend: flat ~55.5%; mgmt cautious on H2 2026 — possible product-mix headwind from ramping AI/communications mix.

### 6. TPU supply history
- **Direct TPU evidence:** NONE PUBLICLY DISCLOSED. MPWR does not name Google or TPU in earnings calls.
- **Strong-signal AI relationship:** MPWR's "enterprise data" segment exploded in 2024–2025 on NVIDIA H100/H200 power-stage (PMIC + multiphase VRM) wins. The 2024 "GB200 socket loss" saga — Edgewater Research and SemiAnalysis reported MPWR was largely supplanted by Renesas / Infineon on Nvidia's GB200 PDB modules — was a major share-price overhang. However, mgmt confirmed continued inclusion in NVIDIA's BOM for later platforms, and Q1-2026 enterprise data of $262.8M (+97.7% YoY) suggests share has stabilized or shifted (likely a mix of NVIDIA, hyperscaler-custom-ASIC including Google TPU, and AMD MI300/MI355).
- **Conclusion:** MPWR's TPU supply is INFERRED (no public statement). Most likely role is point-of-load/multiphase VRM on TPU board, but unconfirmed.

### 7. AI revenue split: TPU vs NVDA GPU vs other-ASIC (Q1 2026)
- **Disclosed segment:** Enterprise Data = $262.8M (32.7% of Q1 revenue), +97.7% YoY. DISCLOSED.
- **Build:**
  - Total enterprise data: $262.8M.
  - NVIDIA-platform-clear (H100/H200/B200 PMIC + multiphase VRM, despite GB200 socket loss): est. $150–180M (INFERRED, mid confidence). NVIDIA remains MPWR's largest single AI customer per channel checks.
  - Residual ~$80–110M is allocated across Google TPU, AMD MI300/MI355, AWS Trainium/Inferentia, Microsoft Maia. With no socket disclosures, a flat-share allocation puts TPU at ~$20–30M of Q1, ~2.5–4% of total revenue. ESTIMATED, LOW confidence.
- **Direct quote (Q1 2026 call):** Mgmt raised 2026 enterprise-data growth floor "from 50% to approximately 85% year-over-year" — directly attributed to AI / data-center customer extended ordering patterns. No customer named.

### 8. Customer concentration
- **2025 10-K disclosure:** Specific 10% customers not located in this research pass. Geographic concentration disclosed: 92% of FY2025 revenue from Asia-based customers (distributors / OEMs).
- **Named customers in narrative:** NVIDIA implicitly the largest AI customer per GB200-saga IR commentary; Google/MSFT/META/AMZN not disclosed by name.
- **Risk:** High customer / distributor concentration (typical for fabless AAA-spec analog co.).

```json
{
  "ticker": "MPWR",
  "ttm_revenue_usd_b": 2.96,
  "last_q_revenue_usd_b": 0.8042,
  "last_q_yoy_pct": 26.1,
  "fy_rev_guide_usd_b": null,
  "last_q_gm_pct_nongaap": 55.5,
  "last_q_eps_nongaap": 5.10,
  "tpu_supply_evidence": "inferred",
  "tpu_revenue_share_est_pct": 3.0,
  "nvda_revenue_share_est_pct": 20.0,
  "confidence": "mid",
  "as_of_date": "2026-05-10"
}
```

---

## AEIS — Advanced Energy Industries

### 1. TTM revenue & last-Q revenue (YoY)
- **Last-Q (Mar-Q 2026, fiscal quarter ended Mar 31, 2026):** $511M, +26% YoY. (Source: AEIS Q1 2026 press release / 10-Q, May 5, 2026.)
- **TTM (Apr-2025 through Mar-2026):** ≈ $1.91B. Build: FY2025 $1.80B − Q1-2025 ≈ $405M + Q1-2026 $511M = $1.906B.

### 2. Forward revenue
- **Q2 2026 guide (company):** $540M ± $20M (midpoint $540M).
- **FY2026 (company commentary, Q4-2025 call, Feb 2026):** "high-teens" YoY growth; data-center segment growth revised to >30% YoY (implies data-center FY2026 ≈ $760M+). Implied FY2026 ≈ $2.1B at +17%; >$2.1B if data-center accelerates.
- **Street consensus FY+1 (2027):** Not gathered from primary source.

### 3. Earnings guidance
- **Q2 2026 non-GAAP EPS:** $2.18 ± $0.25.
- **Q2 2026 GAAP EPS:** $1.54 ± $0.23.
- **Gross margin:** Mgmt targeting >40% non-GAAP in 2026, with long-term target 43%.
- **CapEx:** Plans 2026 capex sized to support >$2.5B of revenue-generating capacity (incl. Thailand site).
- **FCF guide:** Not specifically stated; Q1-2026 adj. EBITDA $108M (record).

### 4. EPS
- **Q1 2026 GAAP diluted EPS:** $1.59.
- **Q1 2026 non-GAAP diluted EPS:** $2.09 (+70% YoY).
- **TTM non-GAAP EPS:** est. $7.10 (FY2025 non-GAAP ≈ $6.30 + Q1-26 $2.09 − Q1-25 ≈ $1.23). DERIVED — not from a single primary source.

### 5. Gross margin
- **Q1 2026:** GAAP 39.3%, non-GAAP 40.1% (highest since Artesyn acquisition in 2019).
- **4-Q trend (non-GAAP-comparable):**
  - Q2 2025: ~38.0% (FY2025 38.7% avg)
  - Q3 2025: ~38.5%
  - Q4 2025: 39.7% (best in 5 years at the time)
  - Q1 2026: 40.1%
  - Trend: 220 bps YoY expansion; driven by China-factory-closure mix benefit + higher data-center volume; tariff drag noted.

### 6. TPU supply history
- **Direct TPU evidence:** NONE PUBLICLY DISCLOSED. AEIS does not name Google or any hyperscaler in earnings calls.
- **Strong-signal AI relationship:** Data Center Computing segment grew from ~$120M in Q1 2024 to $194.2M in Q1 2026 (+102% YoY in Q1-26 alone; full-year 2025 segment +107% YoY to $587M). This is AEIS's high-density rack-PSU and bus-converter franchise (largely the Artesyn legacy). Industry coverage (data-center-knowledge, Simply Wall St narratives) cites AEIS's 800V / 1000V PSU programs being designed in with hyperscalers including Microsoft and Google for 2026+ deployments.
- **Conclusion:** AEIS's role on TPU racks = STRONG-SIGNAL (front-end AC-DC and 48V bus PSU level), but no Google-specific socket has been disclosed in any 10-Q/K. The growth math is consistent with multi-hyperscaler AI PSU wins but the company does not break out by customer.

### 7. AI revenue split: TPU vs NVDA GPU vs other-ASIC (Q1 2026)
- **Disclosed segment:** Data Center Computing = $194.2M (38.0% of Q1 revenue), +102% YoY. DISCLOSED.
- **Build:**
  - AEIS sells front-end rack PSUs / bus converters — much less GPU-specific than MPWR. Demand is driven by total rack power, not specific accelerator socket.
  - Approximate allocation by hyperscaler 2025 capex weights (AMZN ~30%, MSFT ~25%, GOOG ~22%, META ~18%, ORCL/others ~5%): NVIDIA-GPU racks (largely Hopper/Blackwell deployed across all four) ≈ 60–70% of segment; Google TPU custom racks ≈ 10–15%; other-ASIC (Trainium, Maia, MTIA) ≈ 15–20%. INFERRED, low-mid confidence.
  - As % of total AEIS Q1 revenue: NVDA-platform ≈ 23–27%, Google-TPU ≈ 4–6%, other-ASIC ≈ 6–8%. ESTIMATED.
- **Direct quote (Q1 2026 call):** "Second consecutive record quarter" in Data Center Computing; mgmt called out 800V/1000V design wins. No specific hyperscaler named.

### 8. Customer concentration
- **2024 10-K (filed Feb 2025):** Applied Materials = 26% of revenue; Lam Research = 11% (both via semiconductor equipment segment, NOT data-center). STALE for 2025 figures — 2025 10-K should refresh but is not parsed here.
- **Hyperscaler / GPU customers:** No 10% threshold breach disclosed. Data Center Computing customers (rack OEMs / hyperscaler-direct) are NOT named individually in filings.

```json
{
  "ticker": "AEIS",
  "ttm_revenue_usd_b": 1.91,
  "last_q_revenue_usd_b": 0.511,
  "last_q_yoy_pct": 26.0,
  "fy_rev_guide_usd_b": 2.10,
  "last_q_gm_pct_nongaap": 40.1,
  "last_q_eps_nongaap": 2.09,
  "tpu_supply_evidence": "strong-signal",
  "tpu_revenue_share_est_pct": 5.0,
  "nvda_revenue_share_est_pct": 25.0,
  "confidence": "low",
  "as_of_date": "2026-05-10"
}
```

---

## VICR — Vicor Corporation

### 1. TTM revenue & last-Q revenue (YoY)
- **Last-Q (Mar-Q 2026, fiscal quarter ended Mar 31, 2026):** $113.0M total product + royalty, +20.2% YoY, +5.3% QoQ. (Source: VICR Q1 2026 press release / 8-K, May 1, 2026.)
- **TTM (Apr-2025 through Mar-2026):** ≈ $431M. Build: FY2025 ≈ $407M (derived from 10-K R&D of $78.57M = 19.3% of net revenues; cross-checks vs. Q3-2025 $110.4M and Q4-2025 ~$107M) − Q1-2025 ~$94M + Q1-2026 $113M ≈ $426–431M. NOTE: FY2025 included a $45M Q2 patent-litigation settlement booked to licensing — base TTM ex-settlement ≈ $385M.

### 2. Forward revenue
- **Q2 2026 guide (company):** "Near $126M" (≈ +18% YoY).
- **FY2026 guide (company):** "Near $570M" — explicitly excludes potential new license deals pending the ongoing ITC litigation. Implies ≈ +40% YoY on the reported base, or ≈ +48% ex-settlement base.
- **Street consensus FY+1 (2027):** Not gathered.

### 3. Earnings guidance
- **EPS guide:** None — Vicor does not give EPS guidance.
- **Margin commentary:** "Margin expansion expected" as capacity comes online; Gen-5 VPD ramp expected H2 2026.
- **FCF guide:** Not provided.

### 4. EPS
- **Q1 2026 GAAP diluted EPS:** $0.44 (47.3M diluted shares; $20.7M net income).
- **Non-GAAP EPS:** Vicor does not report a separate non-GAAP EPS.
- **TTM EPS:** est. $2.60 (FY2025 net income $118.6M = ~$2.51/sh, but boosted by $45M Q2 settlement; ex-settlement TTM EPS ≈ $1.85). DERIVED.

### 5. Gross margin
- **Q1 2026 GAAP:** 55.2%.
- **No non-GAAP reported.**
- **4-Q trend (GAAP):**
  - Q1 2025: 47.2%
  - Q2 2025: 65.3% (boosted by $45M patent-litigation settlement — recurring base ~50–52%)
  - Q3 2025: 57.5%
  - Q4 2025: 55.4%
  - Q1 2026: 55.2%
  - Trend: structural improvement from 47% base to 55%+ run-rate driven by Advanced Products mix and capacity utilization; Q2-2025 spike is non-recurring.

### 6. TPU supply history (THE KEY SECTION FOR VICR)
This requires a careful, evidence-graded read because Vicor is the most-cited "AI power" hyperscaler-pick name and is consistently called out for vertical / factorized power.

- **Confirmed historical TPU relationship:** Vicor was the **first** 48V-direct-to-load power supplier to Google's TPU program in the v2/v3 era (2017–2019), per multiple secondary sources (SemiAnalysis, Vicor IR archive materials, public conference talks). Google's adoption of 48V in OpenCompute (2016) coincided with Vicor's NBM/PRM/VTM ramp. Confidence: HIGH for v2/v3.
- **TPU v4/v5e/v5p:** Public evidence weaker. Industry write-ups suggest Google moved toward more vertically-integrated power (with multi-source supply incl. Delta, Lite-On, FSP for rack PSUs, and Renesas/Infineon plus Vicor for board-level). Confidence: MEDIUM that Vicor retained some 48V→PoL content; LOW that it remained sole-source.
- **TPU v6 (Trillium) / v7 (Ironwood-class):** No public confirmation. Vicor does not name Google in any 2024-2026 filing or transcript.
- **Q1 2026 lead VPD customer identity:** Per Q1 2026 transcript and Photoncap analysis, the "lead Gen-5 VPD customer" is described as a "maker of a wafer-scale-engine-based AI accelerator" — almost certainly **Cerebras**, NOT Google. Gen-4→Gen-5 ramp begins H2 2026.
- **Hyperscaler commentary in Q1 2026 call:** Mgmt referenced "hyperscalers" in plural as a Q1 backlog driver. No Google-specific quote.
- **Conclusion:** Vicor's TPU exposure for v7 / Ironwood = INFERRED at best. The dominant Q1-2026 AI growth driver is Cerebras (wafer-scale VPD), not Google TPU. The "Vicor = vertical power for TPU" narrative is partially STALE; for current TPU racks, the supplier mix is fragmented and Vicor is not the publicly confirmed VPD vendor for TPU v7.

### 7. AI revenue split: TPU vs NVDA GPU vs other-ASIC (Q1 2026)
- **Disclosure:** Vicor does NOT segment by customer or accelerator. Reports two product lines only: Advanced Products (61% of FY2025 incl. licensing) and Brick Products (39%).
- **Build:**
  - Q1 2026 product+royalty revenue $113M. Advanced Products ≈ ~$60M (Q1 2026 Advanced "increased by 3.7% sequentially").
  - Of Advanced Products, the "high-performance computing" sub-segment is the AI-accelerator power line. Mgmt has said this is >50% of high-performance revenue.
  - Lead VPD customer (likely Cerebras) is the single biggest concentration — estimated $20–30M of Q1.
  - NVIDIA: Vicor LOST the H100 socket to MPWR per SemiAnalysis; minimal direct NVIDIA-platform exposure (some legacy NBM in 12V→48V conversion modules on older racks).
  - Google TPU: estimated <$5M (legacy 48V bus converters on v4/v5 racks, if still designed in). ESTIMATED, LOW confidence.
  - Other-ASIC (Cerebras, Tesla Dojo, AMD MI300, OEM HPC): bulk of AI exposure.
- **As % of Q1 2026 revenue:** NVDA-platform ≈ <5%, Google-TPU ≈ <5%, other-ASIC (Cerebras-dominant) ≈ 25–35%. ESTIMATED, LOW confidence.

### 8. Customer concentration
- **2025 10-K:** "the Company has derived the majority of its revenue from Advanced Products in any given year from either one customer or a limited number of customers." No customer names disclosed.
- **Inferred top customer:** Lead VPD customer (likely Cerebras based on Q1-2026 call language about wafer-scale-engine AI accelerator).
- **No named disclosure of Google, NVIDIA, MSFT, META, AMZN, or AMD.**
- **Backlog:** $300.6M one-year backlog (+70% sequential, +93% YoY from $155.5M at YE2024 → $176.9M YE2025 → $300.6M Mar-26). Book-to-bill >2.0.

```json
{
  "ticker": "VICR",
  "ttm_revenue_usd_b": 0.43,
  "last_q_revenue_usd_b": 0.113,
  "last_q_yoy_pct": 20.2,
  "fy_rev_guide_usd_b": 0.57,
  "last_q_gm_pct_nongaap": 55.2,
  "last_q_eps_nongaap": 0.44,
  "tpu_supply_evidence": "inferred",
  "tpu_revenue_share_est_pct": 3.0,
  "nvda_revenue_share_est_pct": 3.0,
  "confidence": "low",
  "as_of_date": "2026-05-10"
}
```

---

## Cross-ticker takeaways

| | MPWR | AEIS | VICR |
|---|---|---|---|
| TTM rev | $2.96B | $1.91B | $0.43B |
| Last-Q YoY | +26% | +26% | +20% |
| Non-GAAP GM | 55.5% | 40.1% | 55.2% (GAAP only) |
| Disclosed AI/DC segment | Enterprise Data $263M (33% of rev) | Data Center Computing $194M (38%) | None (Advanced Products line) |
| TPU evidence | INFERRED | STRONG-SIGNAL (PSU level) | INFERRED for v7; CONFIRMED historical v2/v3 |
| Biggest AI customer (likely) | NVIDIA (board-level VRM) | NVIDIA-platform racks | Cerebras (wafer-scale VPD) |
| Customer concentration disclosed | Geographic only (92% Asia); no 10% customer named | Applied Materials 26%, Lam 11% (semi-equipment side only; STALE 2024 10-K) | "One customer or limited customers" — no names |

## Key caveats & confidence flags
- All three companies decline to name hyperscaler customers, so all TPU/Google/NVDA splits are INFERRED or ESTIMATED with mid-to-low confidence.
- MPWR's narrative on the NVIDIA GB200 socket loss to Renesas/Infineon (2024–2025) is now stale; mgmt has confirmed continued BOM inclusion and Q1-26 enterprise-data growth of 97.7% YoY confirms share recovery somewhere.
- VICR's "AI hyperscaler vertical power delivery" story is real but the lead customer for Gen-5 VPD is almost certainly Cerebras, not Google. The original Google-TPU-Vicor 48V story dates to 2017–2019 and is not refreshed in current filings.
- AEIS is the cleanest hyperscaler-PSU exposure but is socket-agnostic (front-end rack PSU, not GPU-board PoL), so the TPU vs NVDA split is fundamentally allocation-by-hyperscaler-capex, not allocation-by-supplier-socket.

## Sources
- MPWR Q1 2026 PR (GlobeNewswire, Apr 30 2026)
- MPWR Q4/FY2025 PR (Feb 2026)
- AEIS Q1 2026 PR (StockTitan, May 5 2026) and Motley Fool transcript
- AEIS Q4/FY2025 PR (Feb 10 2026) and Motley Fool transcript
- VICR Q1 2026 PR (GlobeNewswire, May 1 2026) and Motley Fool transcript
- VICR FY2025 10-K (filed Feb-Mar 2026)
- SemiAnalysis "Energizing AI: Power Delivery Competition" (Vicor / MPS / Delta / ADI / Renesas / Infineon)
- Photoncap "The Last 1.5mm of AI Power: Three Numbers from Vicor's Q1 2026 Earnings Call"
- AEIS 2024 10-K (customer concentration: AMAT 26%, LRCX 11%) — STALE for 2025 figures.
