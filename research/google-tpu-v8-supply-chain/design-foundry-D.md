# Google TPU Ironwood (v7/v8) Supply Chain — Design / Foundry / Advanced Packaging

**As-of date:** 2026-05-10
**Bucket:** Design partner (AVGO) + foundry (TSM) + OSAT/advanced packaging (AMKR)
**Latest reported quarter:** Q1 calendar 2026 (AVGO Q1 FY26 ended 2026-02-01; TSM Q1 ended 2026-03-31; AMKR Q1 ended 2026-03-31)

Source-rule note: all figures with sources <120 days are "FRESH"; any older are flagged. Where the issuer does not disclose, the field is labeled **ESTIMATED** with stated math.

---

## AVGO — Broadcom Inc. (TPU ASIC design partner)

### 1) TTM revenue & last-Q revenue (YoY)
- **Last quarter:** Q1 FY26, fiscal-Q end **2026-02-01**. Revenue **$19.3B**, +29% YoY (DISCLOSED; press release).
- **TTM (Q2 FY25 + Q3 FY25 + Q4 FY25 + Q1 FY26):** $15.0B + $16.0B + $18.0B + $19.3B = **$68.3B TTM** (DISCLOSED; from issuer quarterly press releases).
- Semiconductor Solutions Q1 FY26: $12.5B (+52% YoY); Infrastructure Software $6.8B (+1% YoY).

### 2) Forward revenue
- **Q2 FY26 guide:** ~**$22.0B** (+47% YoY); semi solutions ~$14.8B; software ~$7.2B (DISCLOSED; CFO outlook).
- **AI semiconductor Q2 guide:** **$10.7B** (+140% YoY) (DISCLOSED).
- **FY26 implied (street, derived from Q1+Q2 guide + run-rate):** ~$86–92B (ESTIMATED, low confidence — AVGO does not give an FY guide).
- **FY27 line-of-sight commentary:** Hock Tan, "line of sight to AI chip revenue in excess of $100 billion in FY27" (DISCLOSED, call).

### 3) Earnings guidance
- Q2 FY26 **adjusted EBITDA margin guide: 68%** of revenue (DISCLOSED).
- AVGO does not give explicit EPS guidance; Q1 FY26 non-GAAP OpEx ~$2.05B; non-GAAP GM 77%.
- FCF Q1 FY26 = **$8.0B**, 41% of revenue (DISCLOSED).

### 4) EPS
- **Q1 FY26 GAAP diluted EPS:** **$1.50**.
- **Q1 FY26 non-GAAP diluted EPS:** **$2.05** (+28% YoY) (DISCLOSED).
- **TTM non-GAAP EPS:** approx **$7.10–7.20** (ESTIMATED from Q2 FY25 $1.58 + Q3 FY25 $1.69 + Q4 FY25 $1.93 + Q1 FY26 $2.05 ≈ $7.25 — confidence Medium).

### 5) Gross margin
- **Q1 FY26 GAAP GM:** 68.0% ($13.16B / $19.3B).
- **Q1 FY26 non-GAAP GM:** **77.0%** (DISCLOSED).
- **4-quarter trend (non-GAAP):** Q2 FY25 ~78% → Q3 FY25 ~78% → Q4 FY25 ~77% → Q1 FY26 77.0%. Mix shift toward semis (lower-margin XPU/networking) is gently compressing GM; Hock Tan flagged this on the call.

### 6) TPU supply history (CORE)
- **DISCLOSED**: AVGO is Google's ASIC design partner for TPU v4, v5e, v5p, v6 (Trillium), and v7 ("Ironwood"). Public confirmation goes back to Hock Tan's 2023 commentary; reinforced repeatedly through FY24/FY25/Q1 FY26 calls.
- **Q1 FY26 call (2026-03-04, Hock Tan, paraphrased / direct):**
  - "Three hyperscale customers" with multi-generational XPU roadmaps; "in 2027, each plans to deploy one-million-XPU clusters … serviceable AI revenue $60–90B in FY27 alone." (DISCLOSED, AVGO Q4 FY24 call repeated Q1 FY26.)
  - On Google/Anthropic: "off to a very good start in 2026 for **1 gigawatt** of TPU compute … for '27 demand is expected to surge in excess of **3 gigawatts**" — referencing Anthropic's stand-up on Google TPUs designed by AVGO. (DISCLOSED.)
  - Hock Tan noted **6 confirmed XPU customers** (3 in production: Google, Meta-MTIA, ByteDance per trade press; 3 in design / pre-revenue: OpenAI, Apple, fourth unidentified — INFERRED from trade press).
- **TPU v7 Ironwood process:** TSMC **N3E** (DISCLOSED at Google Cloud Next 2025, reaffirmed at HotChips). 9,216-chip superpod architecture.
- Trade-press estimate (HSBC, semianalysis): Google TPU contributes ~**75–80% of AVGO's custom-XPU revenue** in FY26 (ESTIMATED).

### 7) AI revenue split: TPU vs NVDA vs other-ASIC
- **Q1 FY26 AI revenue: $8.4B** (+106% YoY) (DISCLOSED).
  - **Custom XPU: ~67% = ~$5.6B** (AI networking ~1/3 of AI rev, so XPU = 2/3). DISCLOSED on call.
  - **AI Networking (Tomahawk 6, Jericho 3, Bailly optical, NICs): ~$2.8B** (DISCLOSED).
- **Of XPU ~$5.6B, TPU share ESTIMATED at 75–80% = ~$4.2–4.5B for TPU specifically in Q1 FY26** (ESTIMATED, Medium confidence, based on HSBC + semianalysis trade-press splits and Hock Tan's "three customers" commentary where Google is the largest and longest-running).
- **AVGO does not compete in NVDA-merchant-GPU territory** — its AI revenue is essentially zero NVDA. Frame: AVGO's TPU exposure is the single largest non-NVDA AI accelerator program globally.
- **TPU share of AVGO TTM total revenue (ESTIMATED):** TPU ~$4.3B × 4 (annualized) ≈ $17B / $68B TTM = **~25% of AVGO TTM revenue is TPU**. Confidence **Medium-Low**; AVGO does not disclose by customer.

### 8) Customer concentration
- AVGO's most recent 10-K (FY25) discloses **one customer >10%** (Apple, ~20% — wireless/touch). No hyperscaler crossed the 10% disclosure line in FY24 filings, but with XPU run-rate now at $5.6B/qtr Google likely exceeds 10% in FY26 (INFERRED — watch FY26 10-K).
- Disclosed 10%+ customers: **Apple** (~20% TTM). **Google, Meta, OpenAI, Anthropic** are disclosed as XPU customers by program but not by 10-K threshold.

```json
{
  "ticker": "AVGO",
  "ttm_revenue_usd_b": 68.3,
  "last_q_revenue_usd_b": 19.3,
  "last_q_yoy_pct": 29,
  "fy_rev_guide_usd_b": null,
  "last_q_gm_pct_nongaap": 77.0,
  "last_q_eps_nongaap": 2.05,
  "tpu_supply_evidence": "DISCLOSED ASIC partner v4/v5e/v5p/v6/v7; Hock Tan Q1 FY26 call confirmed Google + Anthropic 1GW TPU 2026, 3GW 2027; TPU v7 Ironwood on TSMC N3E",
  "tpu_revenue_share_est_pct": 25,
  "nvda_revenue_share_est_pct": 0,
  "confidence": "Medium",
  "as_of_date": "2026-05-10"
}
```

---

## TSM — Taiwan Semiconductor Manufacturing (TPU wafer foundry)

### 1) TTM revenue & last-Q revenue (YoY)
- **Last quarter:** Q1 2026, fiscal-Q end **2026-03-31**. Revenue **$35.9B (TWD 1.13T)**, +**35.1% YoY** (DISCLOSED).
- **FY2025 (calendar):** $122.4B, +35.9% YoY.
- **TTM (Q2'25 + Q3'25 + Q4'25 + Q1'26):** Q1 2025 was ~$25.5B; TTM ≈ $122.4B − $25.5B + $35.9B = **~$132.8B TTM** (computed).
- April 2026 monthly revenue: **NT$410.73B**, +17.5% YoY — a moderation vs the Q1 print but YTD cumulative still +29.9% YoY.

### 2) Forward revenue
- **Q2 2026 guide:** **$39.0–40.2B**, +27%+ YoY (DISCLOSED, midpoint ~$39.6B).
- **FY2026 guide:** "revenue growth above 30% in USD terms" (raised from "mid-20s"); implied **~$160B+** (DISCLOSED).
- **Street FY27 consensus:** ~$185–200B (ESTIMATED from CoWoS capacity glide).

### 3) Earnings guidance
- **Q2 2026 GM guide: 65.5–67.5%** (DISCLOSED).
- Q1 2026 GM actual: **66.2%** (+3.9 pp QoQ).
- Op-margin Q1 2026: ~57%; net margin record.
- **CapEx 2026: raised to $52–56B (high end)** (DISCLOSED).

### 4) EPS
- **Q1 2026 GAAP EPS: TWD 22.08** (~$0.68/share TWD-denominated; ADR ratio 5:1 → ~$3.40/ADR) (DISCLOSED). Net income +58.3% YoY.
- TSMC does not report non-GAAP EPS (IFRS reporter).
- **TTM EPS:** ~TWD 70–72 (ESTIMATED, Medium confidence).

### 5) Gross margin
- **Q1 2026 GM (IFRS, only metric): 66.2%** (DISCLOSED).
- **4-quarter trend:** Q2 2025 ~58% → Q3 2025 ~60% → Q4 2025 ~62.3% → Q1 2026 66.2% (rising on advanced-node price increases + N3 yield maturation + N5 reuse). DISCLOSED.

### 6) TPU supply history (CORE)
- **DISCLOSED**: TSMC is the wafer foundry for all Google TPU generations including TPU v6 (Trillium) on **N5/N4P** and TPU v7 (Ironwood) on **N3E** (DISCLOSED via Google Cloud Next 2025; TrendForce 2025-11-07; Jon Peddie research). 8th-gen TPU split into training + inference reportedly targets **N2** (DISCLOSED, TheNextWeb 2026, "TSMC 2nm split").
- TSMC does **not** name customers in IR disclosure. CC Wei (CEO) Q1 2026 call: "HPC platform 61% of revenue, up from 55% Q/Q on AI accelerators and AI server-CPU strength" (DISCLOSED).
- **CoWoS / advanced packaging:** Doug Yu / CC Wei: "CoWoS capacity remains extremely tight; ramping with OSAT partners." TSMC building toward **~130k CoWoS wafers/mo by late 2026** (DISCLOSED, raised from prior ~80–90k target). 5.5-reticle CoWoS at >98% yield (Q1 2026 disclosure).
- **CoWoS 2026 allocation (trade-press, Morgan Stanley + DigiTimes, ESTIMATED):** Nvidia ~60% (~595k wafers), Broadcom ~15% (~150k wafers, of which **~90k for Google TPU**, ~50k Meta MTIA, ~10k OpenAI), AMD ~11%, others ~14%.

### 7) AI revenue split: TPU vs NVDA vs other-ASIC
- **HPC segment Q1 2026: 61% of $35.9B = $21.9B** (DISCLOSED).
- HPC ≠ AI accelerator; it also includes server CPUs (AMD EPYC, Apple M-series Mac, Google Axion CPU) and networking silicon. AI-accelerator subset ESTIMATED ~70% of HPC = **~$15.3B in Q1 2026** (ESTIMATED).
- **NVDA share of TSMC revenue:** ~22–25% (trade-press estimate; ESTIMATED). At Q1 2026 ≈ **$7.9–9.0B NVDA wafer revenue/quarter**.
- **Google TPU share of TSMC revenue (BUILD):**
  - Math: ~90k CoWoS wafers/yr × ~24 dies/wafer (large reticle TPU v7) ≈ 2.2M dies; but use wafer revenue not die count.
  - 90k CoWoS wafers ≈ **~90k 12-inch front-end wafers/yr** for TPU v7 die alone (excludes HBM logic die). N3E wafer ASP ~$18–22k. → **~$1.6–2.0B/yr in front-end wafer revenue** plus packaging fees (TSMC CoWoS-S/L revenue per wafer ~$10–15k → another ~$1.1–1.4B).
  - **TPU front-end + advanced-packaging revenue to TSMC ≈ $2.7–3.4B/yr in 2026 (ESTIMATED)**, equal to **~2% of TSMC FY26 revenue**. Confidence **Low-Medium** — wafer ASP and CoWoS pricing are inferred.
- TPU share of HPC segment: ~$3B / ~$80B HPC FY26 = **~3.5–4% of HPC** (ESTIMATED).
- Conclusion: **NVDA dwarfs TPU at the TSMC level (~10x)**; TPU is the largest non-NVDA accelerator program but still small in TSMC mix.

### 8) Customer concentration
- TSMC FY25 10-K: **one customer >10%** disclosed (Apple, ~25% — INFERRED, trade press). NVDA likely also >10% in FY25 but TSMC labels by "Customer A/B" — Customer A widely understood to be Apple, Customer B to be NVDA.
- Hyperscaler-direct customers (Google, Meta, AWS): via AVGO/MediaTek/Marvell intermediation. None likely >10% directly.

```json
{
  "ticker": "TSM",
  "ttm_revenue_usd_b": 132.8,
  "last_q_revenue_usd_b": 35.9,
  "last_q_yoy_pct": 35.1,
  "fy_rev_guide_usd_b": 160.0,
  "last_q_gm_pct_nongaap": 66.2,
  "last_q_eps_nongaap": null,
  "tpu_supply_evidence": "DISCLOSED foundry for all TPU gens; TPU v6 N5/N4P, TPU v7 Ironwood N3E, v8 split N2; CoWoS-S/L allocation ~90k wafers/yr to Google TPU via AVGO per Morgan Stanley/DigiTimes",
  "tpu_revenue_share_est_pct": 2,
  "nvda_revenue_share_est_pct": 23,
  "confidence": "Medium",
  "as_of_date": "2026-05-10"
}
```

---

## AMKR — Amkor Technology (OSAT / advanced packaging)

### 1) TTM revenue & last-Q revenue (YoY)
- **Last quarter:** Q1 2026, fiscal-Q end **2026-03-31**. Revenue **$1.685B**, +**27% YoY** (DISCLOSED).
- **FY2025:** $6.71B, +6% YoY (DISCLOSED).
- **TTM (Q2'25 + Q3'25 + Q4'25 + Q1'26):** $6.71B − Q1'25 (~$1.32B) + $1.685B ≈ **~$7.07B TTM** (computed).
- Q1 2026 advanced packaging revenue at record level; advanced products ~83% of mix per FY25 10-K.

### 2) Forward revenue
- **Q2 2026 guide:** **$1.75–1.85B** (midpoint $1.80B), +7% sequential, +~24% YoY (DISCLOSED).
- **FY2026:** AMKR does not give a formal FY revenue guide. Implied FY26 from Q1 actual + Q2 guide + 2H seasonal step-up: ~**$7.6–8.0B** (ESTIMATED, Medium confidence).
- New HDFO (CoWoS-equivalent) data-center CPU program ramps Q2 with meaningful revenue Q3 — points to back-half acceleration.

### 3) Earnings guidance
- **Q2 2026 EPS guide:** **$0.42–0.52 diluted** (DISCLOSED).
- **Q2 GM guide:** **14.5–15.5%** (DISCLOSED).
- **Net income Q2 guide:** $105–130M.
- **FY26 CapEx:** **$2.5–3.0B** (DISCLOSED — vs $0.85B FY25, a major step-up funding Arizona Phase 1).
- Arizona Phase 1 completion 2027; production 2028; "Arizona can be in the $1B run-rate range, ~10%+ of 2025 revenue" (DISCLOSED).

### 4) EPS
- **Q1 2026 GAAP diluted EPS:** **$0.33** (DISCLOSED).
- AMKR does not separately publish non-GAAP EPS most quarters; treat GAAP as primary.
- **TTM EPS:** ~$1.55–1.65 (computed from quarterly disclosures, Medium confidence).

### 5) Gross margin
- **Q1 2026 GAAP GM: 14.2%** (DISCLOSED, above-guide).
- **4-quarter trend:** Q2'25 ~14.8% → Q3'25 ~16.1% → Q4'25 ~15.4% → Q1'26 14.2% (Q1 seasonally weakest; advanced packaging mix lifting trend GM vs historical 12–13% pre-AI).

### 6) TPU supply history (CORE)
- **AMKR's role:** OSAT overflow for CoWoS-S/L when TSMC's in-house CoWoS capacity is saturated, **FO-PLP** (fan-out panel-level packaging) for cost-reduced versions, plus traditional substrate and test for ASIC programs. Amkor's **HDFO platform** is positioned as the CoWoS-R / CoWoS-L equivalent ("technologies similar to TSMC's CoWoS-R or CoWoS-L"; >5 customers in qualification — DISCLOSED on Q1 2026 call).
- **Google/AVGO as customer:** **NOT explicitly disclosed** by AMKR. AMKR Q1 2026 call did **not** name Google by name. Trade press (DigiTimes, semianalysis): AVGO uses Amkor for back-end test + select advanced packaging on TPU when TSMC CoWoS is tight — primarily **substrate + final test**, not the CoWoS interposer itself. (INFERRED, Low-Medium confidence.)
- **Arizona facility:**
  - $7B total investment, Peoria AZ campus.
  - **TSMC partnership:** "Amkor and TSMC to Expand Partnership and Collaborate on Advanced Packaging in Arizona" (DISCLOSED 2024). Joint definition of InFO and CoWoS technologies for common customers.
  - **Lead customers disclosed:** **Apple** ("first and largest") and **NVIDIA**. **Google NOT named** for Arizona. (DISCLOSED.)
  - Phase 1 completion 2027; production early 2028.
- **Q1 2026 commentary:** New **HDFO data-center CPU program** starts ramping Q2 2026 with meaningful revenue Q3. This program is widely understood (trade press) to be AMD Venice CPU packaging, not TPU.

### 7) AI revenue split: TPU vs NVDA vs other-ASIC
- AMKR is materially smaller than ASE and dwarfed by TSMC's in-house CoWoS. Frame: AMKR captures the **packaging overflow + non-HBM test + substrate** business.
- AMKR does not disclose AI revenue separately. Trade-press ESTIMATE: AI/HPC-related packaging ~$1.0–1.2B of FY25 revenue (~15–18%).
- **NVDA share of AMKR revenue:** ESTIMATED **~5–10%** — NVDA is a "lead customer" for Arizona which is pre-production, and a current customer for select Hopper/Blackwell substrate/test. Confidence **Low**.
- **TPU share of AMKR revenue:** ESTIMATED **~1–3%** — Google/TPU is not named as a top customer; Amkor's TPU exposure (if any) is via AVGO ASIC build, primarily substrate and test. Confidence **Low**.
- Bulk of AMKR revenue remains **Apple (~30%) + Qualcomm (~11%)** smartphone-driven.

### 8) Customer concentration
- **Disclosed 10%+ customers (FY25 10-K):**
  - **Apple: 29.8%** of FY25 net sales.
  - **Qualcomm: 11.1%** of FY25 net sales.
  - Top 10 customers: 72% of FY25.
- **No disclosed 10%+ exposure to NVDA, Google/Alphabet, MSFT, META, AMZN.** AVGO not disclosed at 10%+.
- This is a notable structural risk: Apple concentration ~30% means any iPhone slowdown hits AMKR directly. Arizona ramp is intended to deepen Apple share but also onboard NVDA/AMD/AVGO.

```json
{
  "ticker": "AMKR",
  "ttm_revenue_usd_b": 7.07,
  "last_q_revenue_usd_b": 1.685,
  "last_q_yoy_pct": 27,
  "fy_rev_guide_usd_b": null,
  "last_q_gm_pct_nongaap": 14.2,
  "last_q_eps_nongaap": 0.33,
  "tpu_supply_evidence": "INFERRED only; AMKR Q1 2026 call did NOT name Google. AMKR Arizona lead customers disclosed as Apple + NVIDIA. AMKR HDFO platform is CoWoS-L equivalent with 5+ customers in qual; trade press suggests AVGO uses AMKR for substrate/test overflow on TPU",
  "tpu_revenue_share_est_pct": 2,
  "nvda_revenue_share_est_pct": 7,
  "confidence": "Low",
  "as_of_date": "2026-05-10"
}
```

---

## Sources

- Broadcom Q1 FY26 press release (2026-03-04): https://investors.broadcom.com/news-releases/news-release-details/broadcom-inc-announces-first-quarter-fiscal-year-2026-financial
- Broadcom Q1 FY26 transcript (Hock Tan): https://www.stockinsights.ai/us/AVGO/earnings-transcript/fy26-q1-afd5
- TSMC Q1 2026 results: https://investor.tsmc.com/english/quarterly-results/2026/q1
- TSMC April 2026 monthly revenue: https://pr.tsmc.com/english/news/3305
- TSMC Q1 2026 earnings call transcript: https://www.investing.com/news/transcripts/earnings-call-transcript-tsmcs-q1-2026-shows-strong-growth-and-margin-gains-93CH-4617167
- Amkor Q1 2026 release: https://ir.amkor.com/news-releases/news-release-details/amkor-technology-reports-financial-results-first-quarter-2026
- Amkor Q1 2026 transcript: https://www.fool.com/earnings/call-transcripts/2026/04/27/amkor-amkr-q1-2026-earnings-call-transcript/
- Amkor + TSMC Arizona partnership: https://ir.amkor.com/news-releases/news-release-details/amkor-and-tsmc-expand-partnership-and-collaborate-advanced
- TPU Ironwood / TSMC N3E: https://www.trendforce.com/news/2025/11/07/news-google-unveils-7th-gen-tpu-ironwood-with-9216-chip-superpod-taking-aim-at-nvidia/
- TSMC CoWoS allocation (Morgan Stanley / DigiTimes): https://www.digitimes.com/news/a20251210PD218/tsmc-cowos-capacity-nvidia-equipment.html
- Amkor 2025 10-K customer concentration: https://www.stocktitan.net/sec-filings/AMKR/10-k-amkor-technology-inc-files-annual-report-df084542c304.html
