# Optical & High-Speed Interconnect Bucket — Google TPU v8 (Ironwood-class) Supply Chain

**Research as-of date:** 2026-05-10
**Bucket:** Optical & High-Speed Interconnect
**Tickers:** COHR, AAOI, LITE, MRVL, CRDO

**Methodology notes**
- All figures cross-referenced from company IR press releases, 8-K filings, and earnings-call transcripts.
- Fiscal calendars differ: COHR & LITE (FY = July-Jun), MRVL & CRDO (FY = Feb-Jan), AAOI (FY = calendar).
- "Last reported quarter" varies: COHR Q3 FY26 (Mar 2026), LITE Q3 FY26 (Mar 2026), AAOI Q1 CY26 (Mar 2026), MRVL Q4 FY26 (Jan 2026) — note MRVL's January-ending fiscal year means Q4 FY26 results were the latest available; Q1 FY27 will report ~late May 2026. CRDO Q3 FY26 (Jan 2026) — Q4 FY26 reports Jun 1, 2026.
- TPU supply evidence for v7 "Ironwood" is partly inferred from trade press (LightCounting, SemiAnalysis, Converge Digest) because hyperscalers do not name vendors and vendors do not name hyperscalers.
- Where a figure is older than 120 days as of 2026-05-10 (i.e., before 2026-01-10), it is flagged **STALE**.

---

## COHR — Coherent Corp.

### 1. TTM revenue & last-Q revenue (YoY)
- **Last reported quarter:** Q3 FY2026, fiscal-quarter end **March 31, 2026**.
- **Q3 FY26 revenue: $1.81B, +21% YoY** (+27% pro-forma). Source: Coherent press release, May 6, 2026 (https://www.coherent.com/news/press-releases/third-quarter-fiscal-year-2026-results).
- **TTM revenue (Q4 FY25 + Q1–Q3 FY26):** $1.53B + $1.58B + $1.69B + $1.81B = **~$6.61B**. Sources: COHR FY25 Q4 release (Aug 13, 2025), Q1 FY26 release (Nov 6, 2025), Q2 FY26 release (Feb 4, 2026), Q3 FY26 release (May 6, 2026).
- Datacenter & communications segment Q3 FY26: **$1.36B** (+37% YoY, +13% QoQ), ~75% of revenue. Source: Converge Digest summary May 6, 2026.

### 2. Forward revenue
- **Q4 FY26 guide: $1.91B – $2.05B** (midpoint $1.98B). Source: COHR Q3 FY26 press release.
- **Full-year FY27 directional:** "FY27 growth rate to exceed FY26 growth rate" (no point estimate). Source: Q3 FY26 call commentary.
- **Street consensus FY27** (calendar 2026 ≈ FY27): ~$8.2–8.5B (StockAnalysis.com/Yahoo Finance, May 2026).

### 3. Earnings guidance
- **Q4 FY26 non-GAAP EPS: $1.52 – $1.72.** Source: Q3 FY26 press release.
- Implied non-GAAP gross margin direction: continued sequential expansion (no specific point guide disclosed for GM beyond Q4 trajectory).
- **FCF guide:** not provided as a point estimate; capex elevated due to InP capacity doubling.

### 4. EPS
- **Q3 FY26 GAAP diluted EPS: $0.97**; **non-GAAP $1.41**. Source: Q3 FY26 press release.
- **TTM non-GAAP EPS** (Q4 FY25 $1.00 + Q1 FY26 ~$1.10 + Q2 FY26 ~$1.25 + Q3 FY26 $1.41) ≈ **$4.75**. Source: aggregated press releases.

### 5. Gross margin (last-Q + 4-quarter trend, non-GAAP unless noted)
- **Q3 FY26 GAAP GM 37.7% / non-GAAP 39.6%.** Source: Q3 FY26 press release.
- 4-quarter non-GAAP GM trend: **Q4 FY25 38.1% → Q1 FY26 38.7% → Q2 FY26 39.0% → Q3 FY26 39.6%.**

### 6. TPU supply history
- **Status: strong-signal (not confirmed by company).**
- Coherent is one of two large-scale suppliers of 100G/200G EML lasers used in 800G and 1.6T transceivers consumed by Google's Apollo OCS network. Coherent has publicly stated it has shipped **Optical Circuit Switch (OCS) systems to seven customers**, and trade press (LightCounting, SemiAnalysis, Converge Digest) identify Google's Ironwood TPU v7 architecture as the validating workload for OCS at scale (https://convergedigest.com/coherent-posts-strong-q3-fy2026-growth-on-ai-data-center-demand/).
- Coherent also produces transceiver modules (400G/800G/1.6T) shipped into hyperscale AI clusters. No press release explicitly names Google.
- Historical: Coherent (and predecessor II-VI) supplied EML chips and InP wafers underlying optics generations consumed across TPU v4 / v5e / v5p / v6 ("Trillium") and now v7 ("Ironwood") — **inferred from market-share commentary** (Coherent holds ~30–40% of EML supply share).

### 7. AI revenue split (last-Q): TPU vs NVDA-GPU vs other-ASIC
- **DISCLOSED:** Datacom segment $1.36B in Q3 FY26 (~75% of total revenue).
- **Vendor disclosure:** Coherent announced a $2B NVIDIA equity stake plus a multiyear CPO supply agreement in Q3 FY26 — confirming material NVIDIA exposure.
- **INFERRED allocation (low/mid confidence):**
  - NVDA-platform optics + CPO + GPU-spec 800G/1.6T transceivers: **~45–55%** of datacom (~$610–750M).
  - Google-Ironwood-OCS + EMLs going into Google-spec transceivers: **~15–25%** of datacom (~$200–340M).
  - Other ASIC (AWS Trainium, Microsoft Maia, Meta MTIA) + traditional cloud/telecom: **~25–35%** (~$340–475M).
- **Allocation logic:** NVIDIA equity stake + recent CPO supply deal strongly tilts mix to NVDA; Coherent's heavy EML share into Google's OCS and Apollo network drives the TPU bucket; AWS/MSFT custom optics consume the residual. Confidence: **low/mid** — Coherent does not break out by customer.

### 8. Customer concentration
- No customer disclosed as ≥10% in most recent 10-Q (FY25 10-K filed Aug 2025 disclosed concentration but did not name a single customer at the 10% threshold). Source: COHR FY25 10-K, Aug 2025 (**STALE**).
- NVIDIA now a material customer post-CPO supply deal (May 2026 announcement) and ~$2B equity holder.
- Apple, Cisco, and major hyperscalers historically inferred to be present but not named.

```json
{
  "ticker": "COHR",
  "ttm_revenue_usd_b": 6.61,
  "last_q_revenue_usd_b": 1.81,
  "last_q_yoy_pct": 21,
  "fy_rev_guide_usd_b": 1.98,
  "last_q_gm_pct_nongaap": 39.6,
  "last_q_eps_nongaap": 1.41,
  "tpu_supply_evidence": "strong-signal",
  "tpu_revenue_share_est_pct": 15,
  "nvda_revenue_share_est_pct": 38,
  "confidence": "low",
  "as_of_date": "2026-05-10"
}
```

---

## AAOI — Applied Optoelectronics

### 1. TTM revenue & last-Q revenue (YoY)
- **Last reported quarter:** Q1 CY2026, calendar-quarter end **March 31, 2026**.
- **Q1 CY26 revenue: $151.1M, +51.3% YoY** (vs $99.9M Q1 CY25). Source: AAOI press release, May 7, 2026 (https://investors.ao-inc.com/news-releases/news-release-details/applied-optoelectronics-reports-first-quarter-2026-results).
- Datacenter $81.4M (+154% YoY, +9% QoQ); CATV $66.8M. Source: same.
- **TTM revenue (Q2 CY25 + Q3 CY25 + Q4 CY25 + Q1 CY26):** $103.0M + $118.6M + $134.3M + $151.1M = **~$507M**. Sources: AAOI Q2-Q4 CY25 and Q1 CY26 press releases.

### 2. Forward revenue
- **Q2 CY26 guide: $180M – $198M** (midpoint $189M). Source: AAOI Q1 CY26 release.
- **FY2026 full-year revenue guide raised to "over $1.1B"** (from prior $1B). Source: AAOI Q1 CY26 earnings call commentary, May 7, 2026.
- **Street consensus FY26:** ~$1.05–1.15B (Yahoo Finance, May 2026).

### 3. Earnings guidance
- **Q2 CY26 guide:** non-GAAP GM **29%–30%**; non-GAAP net result between **$2.5M loss and $2.8M profit**.
- **FY2026:** management targets non-GAAP operating profit **>$140M**.
- **FCF guide:** not disclosed as a point estimate.

### 4. EPS
- **Q1 CY26 GAAP EPS: $(0.19)**; **non-GAAP $(0.07)**. Source: AAOI Q1 CY26 press release.
- **TTM non-GAAP EPS:** approximately **$(0.10)–$0.00** range, net loss as company invests in 800G/1.6T capacity. Source: aggregated quarterly releases.

### 5. Gross margin (last-Q + 4-quarter trend, non-GAAP)
- **Q1 CY26 GAAP GM 29.1% / non-GAAP 29.2%.** Source: Q1 CY26 release.
- 4-quarter non-GAAP GM trend: **Q2 CY25 ~29.5% → Q3 CY25 31.0% → Q4 CY25 31.4% → Q1 CY26 29.2%.** Source: AAOI quarterly press releases.

### 6. TPU supply history
- **Status: inferred (presence on this list is speculative).**
- AAOI announced **first volume shipment of 800G data center transceivers to one major hyperscale customer in Q1 CY26**; press release does not name the customer.
- Trade press (TrendForce, BMF Reports) identifies **Microsoft as AAOI's primary hyperscaler anchor** (~28.8% of 2025 revenue) and notes Google's 800G+ orders have been heavily captured by Innolight and Eoptolink rather than AAOI.
- **No public evidence** of AAOI shipping into Google TPU v4/v5/v6/v7. Press releases reference "qualifying with three additional hyperscalers" but do not name Google.
- A reported $200M+ 1.6T transceiver order in March 2026 has been speculatively attributed to one of Microsoft/Amazon (BMF Reports, Mar 2026) — **not Google**.

### 7. AI revenue split (last-Q): TPU vs NVDA-GPU vs other-ASIC
- **DISCLOSED:** Datacenter Q1 CY26 $81.4M.
- **INFERRED allocation (low confidence):**
  - NVDA-spec optics: **~10–20%** of datacenter — AAOI has limited NVIDIA design wins relative to Innolight.
  - TPU (Google): **~0–5%** — minimal/none.
  - Other-ASIC + general hyperscale (Microsoft/Amazon-spec): **~75–90%** ($60–75M).
- **Allocation logic:** AAOI's hyperscale revenue is dominated by Microsoft per trade-press; remaining datacenter sales go to other CSPs for 200G/400G short-reach optics. Google TPU presence is **negligible**. Confidence: **low**.

### 8. Customer concentration
- **2025 disclosed customers:** Digicomm 53.1% (CATV distributor), **Microsoft 28.8%** of revenue. Source: AAOI FY25 10-K filed March 2026 (per trade-press cite; **STALE** on absolute number once Q1 CY26 reported).
- Google: **not named as a 10% customer**.
- Amazon, Meta: **not named**.

```json
{
  "ticker": "AAOI",
  "ttm_revenue_usd_b": 0.507,
  "last_q_revenue_usd_b": 0.1511,
  "last_q_yoy_pct": 51.3,
  "fy_rev_guide_usd_b": 1.1,
  "last_q_gm_pct_nongaap": 29.2,
  "last_q_eps_nongaap": -0.07,
  "tpu_supply_evidence": "none",
  "tpu_revenue_share_est_pct": 2,
  "nvda_revenue_share_est_pct": 10,
  "confidence": "low",
  "as_of_date": "2026-05-10"
}
```

---

## LITE — Lumentum Holdings

### 1. TTM revenue & last-Q revenue (YoY)
- **Last reported quarter:** Q3 FY2026, fiscal-quarter end **March 28, 2026**.
- **Q3 FY26 revenue: $808.4M, +90.1% YoY** (vs $425.2M Q3 FY25). Source: Lumentum press release, May 6, 2026 (https://investor.lumentum.com/financial-news-releases/news-details/2026/Lumentum-Announces-Third-Quarter-of-Fiscal-Year-2026-Financial-Results/default.aspx).
- Components $533.3M (+77% YoY); Systems $275.1M (+121% YoY). Source: same.
- **TTM revenue (Q4 FY25 + Q1–Q3 FY26):** ~$455M (Q4 FY25 estimate) + $533.8M + $665.5M + $808.4M = **~$2.46B**. Sources: Lumentum Q1 FY26 release (Nov 4, 2025), Q2 FY26 release (Feb 4, 2026), Q3 FY26 release.

### 2. Forward revenue
- **Q4 FY26 guide: $960M – $1.01B** (midpoint $985M, new record). Source: Q3 FY26 release.
- **Implied FY26 revenue: ~$2.97B** (Q1 $533.8M + Q2 $665.5M + Q3 $808.4M + Q4 mid $985M).
- **Street consensus FY27 (Jul 2026–Jun 2027):** ~$4.0–4.5B (Yahoo Finance, May 2026).

### 3. Earnings guidance
- **Q4 FY26 non-GAAP EPS: $2.85 – $3.05.**
- **Q4 FY26 non-GAAP operating margin: 35%–36%.**
- **GM directional:** continued sequential expansion (non-GAAP GM was 47.9% in Q3, implied above 48% in Q4).
- **FCF guide:** not disclosed as a point estimate.

### 4. EPS
- **Q3 FY26 GAAP diluted EPS: $1.50**; **non-GAAP $2.37.** Source: Q3 FY26 release.
- **TTM non-GAAP EPS** (rough): Q4 FY25 ~$0.50 + Q1 FY26 ~$1.10 + Q2 FY26 ~$1.65 + Q3 FY26 $2.37 ≈ **~$5.62**.

### 5. Gross margin (last-Q + 4-quarter trend, non-GAAP)
- **Q3 FY26 GAAP GM 44.2% / non-GAAP 47.9%.** Source: Q3 FY26 release.
- 4-quarter non-GAAP GM trend: **Q4 FY25 ~36.5% → Q1 FY26 39.4% → Q2 FY26 42.5% → Q3 FY26 47.9%.** Source: quarterly press releases.

### 6. TPU supply history
- **Status: strong-signal (not confirmed by company).**
- Lumentum holds an estimated **50–60% global share of EML laser chips** and is the **only supplier shipping 200G-per-lane EMLs at volume** — these are the chips inside 1.6T transceivers serving Google's Ironwood TPU v7 optical fabric (Compound Semiconductor Magazine, viksnewsletter.com, TrendForce Dec 2025).
- Mizuho identifies Lumentum as a likely **supplier of Optical Circuit Switch (OCS) components** used in Google TPU clusters, although there is no formal Google-Lumentum partnership announcement.
- Lumentum disclosed shipments to "all three announced hyperscale customers" in Q4 FY25 cloud-module commentary; Google is widely assumed to be one of them given the EML share dynamics.
- Historical: Lumentum/Oclaro chips have been inside Google's optics generations since TPU v4 (400G era) — **inferred** from EML market share data; no individual TPU generation is named in filings.

### 7. AI revenue split (last-Q): TPU vs NVDA-GPU vs other-ASIC
- **DISCLOSED:** Cloud & Networking segment dominates revenue (~80%+); transceivers + laser chips are the largest growth drivers. NVIDIA $2B preferred equity investment + multiyear supply agreement announced in Q2/Q3 FY26.
- **INFERRED allocation (mid confidence):**
  - NVDA-spec (laser chips into NVDA-platform 800G/1.6T transceivers + NVDA preferred-supply commitment): **~40–50%** of cloud-AI (~$320–400M of $808M total).
  - TPU (Google Apollo OCS + EMLs into Google-spec optics): **~15–25%** (~$120–200M).
  - Other-ASIC + ZR coherent telecom + industrial laser: **~25–40%** (~$200–325M).
- **Allocation logic:** NVIDIA preferred equity + multiyear deal sharply tilts mix; Lumentum's dominant 200G EML share is critical to both NVDA and Google 1.6T networks. Cisco remains a meaningful systems customer. Confidence: **mid** — Lumentum gave more granular customer color than peers but did not name customers explicitly.

### 8. Customer concentration
- **FY25 10-K (Aug 2025, STALE on data through Mar 2026):** disclosed 1 customer >10% of revenue, not named publicly in summaries reviewed.
- NVIDIA: now material via $2B preferred equity + supply agreement (announced late 2025/early 2026).
- Cisco: historically a top-3 customer in Systems segment (per 10-K risk factor language).
- Google, Microsoft, Amazon, Meta: **not specifically named**, but each implied present via cloud-module commentary.

```json
{
  "ticker": "LITE",
  "ttm_revenue_usd_b": 2.46,
  "last_q_revenue_usd_b": 0.808,
  "last_q_yoy_pct": 90.1,
  "fy_rev_guide_usd_b": 2.97,
  "last_q_gm_pct_nongaap": 47.9,
  "last_q_eps_nongaap": 2.37,
  "tpu_supply_evidence": "strong-signal",
  "tpu_revenue_share_est_pct": 18,
  "nvda_revenue_share_est_pct": 42,
  "confidence": "mid",
  "as_of_date": "2026-05-10"
}
```

---

## MRVL — Marvell Technology

### 1. TTM revenue & last-Q revenue (YoY)
- **Last reported quarter:** Q4 FY2026, fiscal-quarter end **January 31, 2026** (reported March 11, 2026; Q1 FY27 will report late May 2026 — **not yet available as of 2026-05-10**).
- **Q4 FY26 revenue: $2.219B, +22% YoY.** Source: Marvell press release, March 11, 2026 (https://investor.marvell.com/news-events/press-releases/detail/1011/marvell-technology-inc-reports-fourth-quarter-and-fiscal-year-2026-financial-results).
- **FY26 full-year revenue: $8.195B, +42% YoY.** Source: same.
- **TTM = FY26 = $8.195B.**
- Data center segment Q4 FY26: **$1.651B, +21% YoY**, 74% of revenue. FY26 data center: **>$6B, +46% YoY.**

### 2. Forward revenue
- **Q1 FY27 guide: $2.40B ±5%** (~$2.28B–$2.52B). Source: Q4 FY26 release.
- **FY27 commentary:** "year-over-year revenue growth to accelerate each quarter in FY27"; street consensus implies **~$10.5–11.0B for FY27** (per Yahoo Finance / 247wallst.com summaries May 2026).
- Note: TheNextWeb / CNBC (April 2026) report Marvell is in talks with Google to add a **memory processing unit + an inference-optimized TPU** alongside Broadcom and MediaTek — material upside if signed.

### 3. Earnings guidance
- **Q1 FY27 non-GAAP EPS: $0.79 ±$0.05.**
- **Q1 FY27 GAAP EPS: $0.31 ±$0.05.**
- **Q1 FY27 non-GAAP GM: 58.25%–59.25%; GAAP 51.4%–52.4%.**
- **FCF guide:** not provided as a point estimate.

### 4. EPS
- **Q4 FY26 GAAP diluted EPS: $0.46**; **non-GAAP $0.80.** Source: Q4 FY26 release.
- **FY26 full-year GAAP EPS: $3.07; non-GAAP $2.84** (+81% YoY).
- **TTM non-GAAP EPS = $2.84.**

### 5. Gross margin (last-Q + 4-quarter trend, non-GAAP)
- **Q4 FY26 GAAP GM 51.7% / non-GAAP 59.0%.**
- 4-quarter non-GAAP GM trend: **Q1 FY26 59.8% → Q2 FY26 59.4% → Q3 FY26 59.7% → Q4 FY26 59.0%.** Source: Marvell Q1, Q2, Q3, Q4 FY26 press releases.

### 6. TPU supply history
- **Status: strong-signal (no signed contract publicly disclosed for TPU itself).**
- **Confirmed Google business:** Marvell co-designs the **Axion ARM CPU** with Google (custom-ASIC partnership disclosed by Marvell). Source: TheNextWeb April 2026.
- **April 2026 reports (CNBC, TheNextWeb):** Google in talks with Marvell for **memory processing unit (MPU) and inference-optimized TPU** alongside Broadcom and MediaTek — discussions have **not yet produced a signed TPU contract** as of May 2026.
- **Confirmed AWS Trainium 2/3 design wins** (heavily ramping in FY26).
- **Confirmed Microsoft Maia 2** custom-silicon engagement.
- Marvell **electro-optics** business (PAM4 DSP, 800G/1.6T optical DSPs) ships into transceivers consumed by Google TPU pods and NVDA H100/H200/Blackwell systems. Marvell does not name customers.
- Historical: Marvell PAM4 DSPs have been in Google-spec optics across TPU v4/v5e/v5p/v6 — **inferred** from DSP market share.

### 7. AI revenue split (last-Q): TPU vs NVDA-GPU vs other-ASIC
- **DISCLOSED:** Data center $1.651B Q4 FY26. CEO has stated AI is "majority of data center revenue and on track to be majority of company revenue."
- **NVDA partnership:** $2B Marvell equity investment by NVIDIA late March 2026 + NVLink Fusion integration with Marvell custom ASICs.
- **INFERRED allocation (mid confidence):**
  - Other-ASIC (AWS Trainium dominant + Microsoft Maia): **~50–60%** of data center (~$830M–1.0B).
  - NVDA-platform (PAM4 DSPs into NVDA-spec optical modules + NVLink Fusion): **~20–25%** (~$330M–415M).
  - Google (Axion CPU today + DSPs into Google-spec optics): **~10–15%** (~$165M–250M); MPU/TPU still pre-revenue.
  - Telecom/storage residual: **~5–10%**.
- **Allocation logic:** AWS Trainium remains Marvell's largest custom-ASIC program by far (Bank of America estimates $1–2B annually); Google Axion is meaningful but smaller; NVDA exposure is largely via electro-optics DSPs rather than custom silicon, until NVLink Fusion ramps. Confidence: **mid**.

### 8. Customer concentration
- **FY26 10-K (filed March 11, 2026):** discloses dependence on a small number of customers including AWS, Microsoft, Google, Meta. Specific 10% customers named in 10-K but redacted from summary articles reviewed; AWS implied largest single customer (analyst consensus ~30%+ of revenue).
- **NVIDIA:** new ~$2B equity holder + NVLink Fusion partner as of late March 2026.
- **Google:** confirmed Axion design partner; TPU/MPU discussions ongoing.

```json
{
  "ticker": "MRVL",
  "ttm_revenue_usd_b": 8.195,
  "last_q_revenue_usd_b": 2.219,
  "last_q_yoy_pct": 22,
  "fy_rev_guide_usd_b": 10.7,
  "last_q_gm_pct_nongaap": 59.0,
  "last_q_eps_nongaap": 0.80,
  "tpu_supply_evidence": "strong-signal",
  "tpu_revenue_share_est_pct": 12,
  "nvda_revenue_share_est_pct": 22,
  "confidence": "mid",
  "as_of_date": "2026-05-10"
}
```

---

## CRDO — Credo Technology

### 1. TTM revenue & last-Q revenue (YoY)
- **Last reported quarter:** Q3 FY2026, fiscal-quarter end **January 31, 2026** (reported March 2026; Q4 FY26 reports June 1, 2026 — **not yet available as of 2026-05-10**).
- **Q3 FY26 revenue: $407.0M, +201.5% YoY, +51.9% QoQ.** Source: Credo press release, March 2026 (https://investors.credosemi.com/news-events/news/news-details/2026/Credo-Technology-Group-Holding-Ltd-Reports-Third-Quarter-of-Fiscal-Year-2026-Financial-Results/default.aspx).
- **TTM revenue (Q4 FY25 + Q1–Q3 FY26):** ~$170M (Q4 FY25 estimate) + $223.1M + $268.0M + $407.0M = **~$1.07B**. Sources: Credo Q1 FY26 release (Sept 2025), Q2 FY26 release (Dec 2025), Q3 FY26 release.

### 2. Forward revenue
- **Q4 FY26 revenue guide: $425M – $435M** (midpoint $430M). Source: Q3 FY26 release.
- **Implied FY26 revenue: ~$1.32B** ($223.1M + $268.0M + $407.0M + $430M mid).
- Management has framed FY26 as ">200% YoY growth" trajectory.
- **Street consensus FY27 (Feb 2026–Jan 2027):** ~$1.9–2.2B (Yahoo Finance / Stockanalysis.com, May 2026).

### 3. Earnings guidance
- **Q4 FY26 non-GAAP GM: 64.0%–66.0%** (down sequentially from 68.6% — mix shift to lower-margin AECs and optics).
- **Q4 FY26 GAAP GM: 63.9%–65.9%.**
- **EPS guide:** not given as point estimate in initial release summaries; non-GAAP EPS implied $0.90–$1.05 based on revenue and margin guide.
- **FCF guide:** not disclosed as a point estimate.

### 4. EPS
- **Q3 FY26 GAAP diluted EPS: $0.82**; **non-GAAP $1.07.** Source: Q3 FY26 release.
- **TTM non-GAAP EPS** (rough): Q4 FY25 ~$0.27 + Q1 FY26 $0.52 + Q2 FY26 $0.67 + Q3 FY26 $1.07 ≈ **~$2.53**.

### 5. Gross margin (last-Q + 4-quarter trend, non-GAAP)
- **Q3 FY26 GAAP GM 68.5% / non-GAAP 68.6%.** Source: Q3 FY26 release.
- 4-quarter non-GAAP GM trend: **Q4 FY25 ~65.0% → Q1 FY26 67.6% → Q2 FY26 67.7% → Q3 FY26 68.6%.** Source: Credo quarterly press releases.

### 6. TPU supply history
- **Status: inferred — no public evidence of direct TPU supply.**
- Credo's core products are **Active Electrical Cables (AECs)**, **SerDes IP**, **PAM4 DSPs**, and emerging **ZeroFlap optical transceivers**. AECs are predominantly used in scale-out copper interconnect within hyperscale racks.
- Per multiple sell-side notes (Seeking Alpha, Simply Wall St): **Broadcom owns the Google TPU custom-ASIC interconnect**; Credo's AEC traction is concentrated at **AWS, Microsoft, and xAI**, not Google.
- Credo did acquire **DustPhotonics** (silicon photonics) to expand optical roadmap, which could position it for future Google business but no design wins publicly disclosed.
- **No public evidence of Credo content in TPU v4/v5/v6/v7.**

### 7. AI revenue split (last-Q): TPU vs NVDA-GPU vs other-ASIC
- **DISCLOSED:** Customer concentration in Q3 FY26: top 3 customers represented **39%, 32%, and 17%** of revenue (88% combined). Q2 FY26 prior disclosure: top 4 hyperscalers each >10% (42%, 24%, 16%, 11%).
- **Per trade press (Seeking Alpha, AInvest):** largest customer typically identified as **Amazon (AWS)**; **Microsoft and xAI** are also material; **ByteDance** has been growing.
- **INFERRED allocation (mid confidence):**
  - Other-ASIC + general hyperscale scale-out (AWS Trainium racks, Microsoft Maia + GPU racks, Meta racks): **~80–90%** ($325M–365M).
  - NVDA-platform (AECs in NVDA-spec H100/H200/GB200 racks at AWS/MSFT/Meta): **~10–15%** ($40M–60M) — indirect via hyperscaler builds.
  - TPU (Google): **~0–5%** — no direct evidence.
- **Allocation logic:** Credo's biggest customer is AWS (likely 39% number), Microsoft/xAI/ByteDance fill in. Google's TPU is served by Broadcom; Credo's AEC adoption inside Google is reportedly limited. Confidence: **mid**.

### 8. Customer concentration
- **Q3 FY26 (filed in 10-Q):** top customers 39%, 32%, 17% of revenue — three customers >10%.
- **Q2 FY26 10-Q:** top 4 hyperscalers each >10%: 42%, 24%, 16%, 11%.
- **Named in press/analyst reports** (not disclosed by Credo directly): **Amazon (largest)**, **Microsoft**, **xAI**, **Meta**.
- **Google: not identified as a 10% customer** in any disclosure reviewed.

```json
{
  "ticker": "CRDO",
  "ttm_revenue_usd_b": 1.07,
  "last_q_revenue_usd_b": 0.407,
  "last_q_yoy_pct": 201.5,
  "fy_rev_guide_usd_b": 1.32,
  "last_q_gm_pct_nongaap": 68.6,
  "last_q_eps_nongaap": 1.07,
  "tpu_supply_evidence": "none",
  "tpu_revenue_share_est_pct": 2,
  "nvda_revenue_share_est_pct": 12,
  "confidence": "mid",
  "as_of_date": "2026-05-10"
}
```

---

## Cross-Ticker Summary Table

| Ticker | Last-Q Rev | YoY% | Non-GAAP GM | TPU Evidence | TPU est% | NVDA est% | Confidence |
|--------|-----------:|-----:|------------:|--------------|---------:|----------:|-----------|
| COHR   | $1.81B    | +21% | 39.6%       | strong-signal| ~15%     | ~38%      | low       |
| AAOI   | $0.151B   | +51% | 29.2%       | none         | ~2%      | ~10%      | low       |
| LITE   | $0.808B   | +90% | 47.9%       | strong-signal| ~18%     | ~42%      | mid       |
| MRVL   | $2.219B   | +22% | 59.0%       | strong-signal| ~12%     | ~22%      | mid       |
| CRDO   | $0.407B   | +202%| 68.6%       | none         | ~2%      | ~12%      | mid       |

## Key Sources (URLs)
- Coherent Q3 FY26 release: https://www.coherent.com/news/press-releases/third-quarter-fiscal-year-2026-results
- Coherent Q3 FY26 transcript (Motley Fool): https://www.fool.com/earnings/call-transcripts/2026/05/06/coherent-cohr-q3-2026-earnings-transcript/
- AAOI Q1 CY26 release: https://investors.ao-inc.com/news-releases/news-release-details/applied-optoelectronics-reports-first-quarter-2026-results
- Lumentum Q3 FY26 release: https://investor.lumentum.com/financial-news-releases/news-details/2026/Lumentum-Announces-Third-Quarter-of-Fiscal-Year-2026-Financial-Results/default.aspx
- Marvell Q4 FY26 release: https://investor.marvell.com/news-events/press-releases/detail/1011/marvell-technology-inc-reports-fourth-quarter-and-fiscal-year-2026-financial-results
- Marvell 10-K FY26: https://investor.marvell.com/sec-filings/all-sec-filings/content/0001835632-26-000011/mrvl-20260131.htm
- Credo Q3 FY26 release: https://investors.credosemi.com/news-events/news/news-details/2026/Credo-Technology-Group-Holding-Ltd-Reports-Third-Quarter-of-Fiscal-Year-2026-Financial-Results/default.aspx
- Marvell-Google TPU/MPU talks (CNBC, Apr 2026): https://www.cnbc.com/2026/04/20/marvell-stock-google-custom-ai-chips.html
- Ironwood TPU supply chain (TrendForce + LightCounting summaries): https://www.trendforce.com/presscenter/news/20251208-12823.html
- Coherent Ironwood OCS validation: https://convergedigest.com/coherent-posts-strong-q3-fy2026-growth-on-ai-data-center-demand/

## Caveats
- Every "TPU revenue share" estimate is an inference; no company in this bucket discloses Google TPU revenue separately. Reported confidence is at most **mid**.
- NVIDIA-equity-investment language can be misread as a customer relationship; for COHR and LITE the NVDA stakes are paired with multiyear supply agreements, so revenue exposure is real.
- AAOI and CRDO 10% customer disclosures lag the latest reported quarter — refreshed numbers come with each 10-Q.
- Marvell's Q1 FY27 results are due in late May 2026 and will supersede the Q4 FY26 figures.
- Coherent's CPO supply agreement with NVIDIA may materially shift the FY27 NVDA-revenue share above the ~38% estimated here.
