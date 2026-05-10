# Bucket E — EMS, System Assembly & Test
**Tickers:** CLS, JBL, FLEX, FORM
**As of:** 2026-05-10
**Latest reported quarter set:** CLS Q1 CY26 (Mar-26), JBL FQ2 FY26 (Feb-26), FLEX FQ4 FY26 (Mar-26), FORM Q1 FY26 (Mar-26)

---

## CLS — Celestica Inc. (NYSE: CLS)

### 1. TTM revenue & last-Q revenue (YoY)
- **Fiscal calendar:** Calendar year. **Last-Q end:** March 31, 2026 (Q1 CY26).
- **Last-Q revenue:** **$4.05 B**, **+53% YoY** (vs $2.65 B in Q1 CY25).
- **TTM revenue (Q2'25–Q1'26):** **~$13.79 B**, calculated as FY2025 $12.39 B + Q1'26 $4.05 B − Q1'25 $2.65 B.
- Source: Celestica Q1 2026 release (Apr 27, 2026); FY25 release (Jan 2026).

### 2. Forward revenue
- **Q2 CY26 guide:** **$4.15–$4.45 B** (midpoint $4.30 B).
- **FY2026 guide (RAISED):** **$19.0 B** (up from $17.0 B prior).
- **FY2027 (street estimate, not company-issued):** Consensus tracking ~$22–24 B range (sell-side; not management). Not disclosed by company.
- Source: Q1 2026 release.

### 3. Earnings guidance
- **Q2 CY26:** Non-GAAP EPS $2.14–$2.34; non-GAAP op margin ~8.0% midpoint.
- **FY2026 (RAISED):** Non-GAAP EPS **$10.15** (up from $8.75); non-GAAP op margin **8.1%**; **FCF $500 M** (unchanged).

### 4. EPS (last quarter)
- **GAAP diluted EPS:** **$1.83** (vs $0.74 prior-year).
- **Non-GAAP adjusted EPS:** **$2.16** (vs $1.20 prior-year).
- **TTM non-GAAP EPS:** ~**$7.01** (FY25 $6.05 + Q1'26 $2.16 − Q1'25 $1.20).

### 5. Gross margin
- **Q1'26 GAAP GM:** **10.8%** (vs 10.3% YoY).
- **Q1'26 non-GAAP GM:** **11.3%** (vs 11.0% YoY).
- **4-Q trend (non-GAAP GM, approx):** Q2'25 ~11.0% → Q3'25 ~11.0% → Q4'25 ~11.2% → Q1'26 11.3%. Mix shift toward HPS and CCS hyperscaler programs is keeping GM range-bound but op-margin is expanding fast (operating leverage on SG&A).

### 6. TPU supply history & HPS segment
- **HPS segment Q1'26 revenue: ~$1.7 B, +63% YoY**, representing **42% of total revenue**. HPS is the home of Google business and is the segment where Celestica books hyperscaler design-build programs.
- **Direct CEO commentary (Q1'26 call):** *"Strong demand and ramping programs for our 800G networking switches across our largest hyperscaler customers."*
- **Communications end market revenue +69% YoY**, "exceeding the low-60s growth outlook, driven by 800G switch program ramps for hyperscaler customers."
- **Enterprise end market revenue +101% YoY**, driven by *"ramping of a next-generation AI/ML compute program with a hyperscaler customer."* This is widely understood to refer to Google's next-gen TPU compute boards.
- **New award (forward-looking):** *"CPO Ethernet switch program optimized for AI scale-out networks with a hyperscaler customer; HPS program expected to begin ramping production in 2027."* — consistent with a Google TPU v8/v9-era network.
- **Prior gen evidence:** Celestica has been publicly named as TPU motherboard/system integrator across TPU v4/v5e/v5p/v6 (Trillium) generations; HPS segment was launched/scaled in part to house this Google business.
- **Confidence:** HIGH that CLS is a primary integrator on Google TPU systems.

### 7. AI revenue split: TPU vs NVDA GPU vs other-ASIC
- HPS revenue ($1.7 B last Q, ~$6.8 B annualized run-rate) is dominated by **Google + 1–2 other hyperscalers**. Of HPS:
  - **800G switch programs** (Communications end market): accelerator-agnostic — built for *all* AI fabrics (TPU clusters AND GPU clusters). Likely the larger of the two streams in Q1'26.
  - **AI/ML compute program** (Enterprise end market): the TPU motherboard/compute board piece for the largest hyperscaler.
- **Estimated AI revenue split (Q1'26, ESTIMATED, confidence MEDIUM):**
  - **Google TPU systems (motherboards + integration):** ~25–35% of total CLS revenue (~$1.0–1.4 B of $4.05 B).
  - **Google-network 800G switches + adjacent (accelerator-agnostic, but Google-channel):** ~10–15% additional CLS revenue.
  - **NVDA-GPU-direct exposure:** Small/modest — CLS is not a top NVDA HGX/MGX ODM (those go to Foxconn/Quanta/Wistron). Likely <5%.
  - **Other-ASIC (Meta MTIA, AWS Trainium):** Limited disclosure; the "two 15% customers" likely include another hyperscaler (Meta or AMD ML cluster).
- Marker: **ESTIMATED**, confidence MEDIUM.

### 8. Customer concentration (DISCLOSED)
- **10-Q disclosure (Q1'26):** *"Three customers each accounted for at least 10% of total revenue, representing 35%, 15% and 15% of revenue, respectively"* → **65% of revenue** from top 3.
- **Customer #1 at 35%** is widely understood (channel + IR commentary) to be **Google / Alphabet**.
- Customers #2 and #3 at 15% each are not named in filings; consistent with prior years, candidates include AMD, Meta, and Cisco-equivalent or another hyperscaler.
- **Disclosed Google %: not by name**, but >$1.4 B/quarter and growing >60%.

```json
{
  "ticker": "CLS",
  "ttm_revenue_usd_b": 13.79,
  "last_q_revenue_usd_b": 4.05,
  "last_q_yoy_pct": 53.0,
  "fy_rev_guide_usd_b": 19.0,
  "last_q_gm_pct_nongaap": 11.3,
  "last_q_eps_nongaap": 2.16,
  "tpu_supply_evidence": "HPS segment $1.7B, +63% YoY; Enterprise +101% YoY from 'AI/ML compute program with a hyperscaler customer' (Google TPU); Communications +69% YoY on 800G switch ramps; CPO Ethernet switch program awarded for 2027 ramp; named publicly as TPU v4/v5/v6 system integrator",
  "tpu_revenue_share_est_pct": 30.0,
  "nvda_revenue_share_est_pct": 3.0,
  "confidence": "MEDIUM-HIGH (TPU exposure DISCLOSED-by-implication; exact $ ESTIMATED)",
  "as_of_date": "2026-05-10"
}
```

---

## JBL — Jabil Inc. (NYSE: JBL)

### 1. TTM revenue & last-Q revenue (YoY)
- **Fiscal calendar:** Fiscal year ends ~Aug 31. **Last-Q end:** Feb 28, 2026 (Q2 FY26).
- **Last-Q revenue:** **$8.30 B** (exceeded guidance by ~$500 M; not separately broken out YoY in transcript, but ~+15–20% YoY est. given $7B prior-year Q2).
- **Q1 FY26 (ended Nov 30, 2025):** $8.3 B, +19% YoY.
- **TTM revenue (Q3'25–Q2'26):** **~$31.5–32.0 B** estimate (running on raised FY26 guide of $34.0 B with H1 FY26 ~$16.6 B).
- Source: Jabil Q2 FY26 release (Mar 2026); Q1 FY26 release (Dec 2025).

### 2. Forward revenue
- **Q3 FY26 guide:** **$8.1–$8.9 B** (midpoint $8.5 B); core EPS $2.83–$3.23.
- **FY2026 guide (RAISED):** **~$34.0 B** (up $1.6 B from prior $32.4 B).
- **FY2026 AI-related revenue outlook (RAISED):** **$13.1 B** (+46% YoY), up $1 B vs December.
- **FY2027 street:** ~$37–39 B (sell-side; not company-issued).

### 3. Earnings guidance
- **FY2026 (RAISED):** Core EPS **$12.25** (up from $11.55); core op margin **5.7%** (unchanged); FCF **>$1.3 B**.
- **Q3 FY26:** Core EPS $2.83–$3.23.

### 4. EPS (last quarter)
- **GAAP diluted EPS Q2 FY26:** **$2.08**.
- **Core (non-GAAP) diluted EPS Q2 FY26:** **$2.69**.
- **TTM core EPS estimate:** ~$10.7 (Q1 $2.85 + Q2 $2.69 + ~$5.1 across F2H FY25 = ~$10.6–10.8).

### 5. Gross margin
- **Q2 FY26 GAAP GM:** Not separately disclosed in transcript (10-Q level).
- **Q2 FY26 core/adjusted GM:** ~**9.0–9.5%** range (implied by 5.3% core op margin and historical SG&A run-rate; not directly disclosed in the Motley Fool transcript extract).
- **Trend:** Stable to up. FY25 core op margin ~5.4%; FY26 guide 5.7% suggests modest expansion.
- **Note:** Jabil reports GM less prominently than operating margin; reliable GM figures require 10-Q (filed but not extracted here).

### 6. TPU supply history & Intelligent Infrastructure segment
- **Intelligent Infrastructure segment Q2 FY26 revenue: $4.0 B, +52% YoY**, 5.7% core operating margin.
- Composition: cloud/data-center infra +$600M YoY, networking/comms +$400M, capital equipment +$100M.
- **Management commentary:** *"Major East Coast retrofit ahead of schedule, enabling expanded participation in both liquid- and air-cooled server opportunities"*; *"robust ramp of a second hyperscaler in Mexico"*; *"imminent discussions with a third hyperscaler."*
- **No explicit Google or TPU mention in the transcript.** Jabil's hyperscaler positioning is generally NVDA-GPU-rack-attached (custom cabinet/rack/manifold assembly for cloud server OEMs and direct-to-hyperscaler).
- **TPU exposure assessment:** Possible but small. Jabil has not been publicly identified as a TPU motherboard integrator (that's CLS). Jabil's "AI-related revenue" of $13.1 B FY26 is primarily NVDA-GPU-attached rack/cabinet/manifold/optical-transceiver assembly work, plus some networking.

### 7. AI revenue split: TPU vs NVDA GPU vs other-ASIC
- **FY26 AI-related revenue (DISCLOSED):** $13.1 B (~38% of company revenue).
- **Estimated split (ESTIMATED, confidence LOW–MEDIUM):**
  - **NVDA-GPU-attached (racks, manifolds, optical, capital equipment):** ~60–75% of the AI bucket (~$8–10 B).
  - **TPU / Google-ASIC exposure:** Small. <5% of AI bucket. Possibly via subcomponent assembly but not motherboard integration.
  - **AMD MI300/MI350 and other-ASIC racks:** ~10–15%.
  - **Networking / capital equipment (lithography customer-specific, semi-cap ATE assembly):** ~15–20%.
- **The "second hyperscaler in Mexico" ramp is more likely a Microsoft/Meta/AWS racks program than Google TPU motherboards.**
- Marker: **ESTIMATED**, confidence LOW–MEDIUM. Jabil discloses AI-revenue aggregate but not customer split.

### 8. Customer concentration
- **No 10%+ customer named in transcript.** Jabil historically has had Apple as the most concentrated single customer (~15–20% of revenue in prior years, since reduced post-iPhone case-and-mechanical work moves). FY25 10-K disclosed one customer >10% (Apple).
- **No Google, NVDA, MSFT, META mentioned by name** in transcript Q&A. Jabil prefers anonymous "hyperscaler" / "cloud customer" descriptors.

```json
{
  "ticker": "JBL",
  "ttm_revenue_usd_b": 31.7,
  "last_q_revenue_usd_b": 8.3,
  "last_q_yoy_pct": 17.0,
  "fy_rev_guide_usd_b": 34.0,
  "last_q_gm_pct_nongaap": 9.2,
  "last_q_eps_nongaap": 2.69,
  "tpu_supply_evidence": "No direct TPU disclosure. Intelligent Infrastructure +52% YoY ($4B/quarter). AI-related FY26 revenue $13.1B (+46%). 'Second hyperscaler in Mexico' ramping; 'third hyperscaler' in discussions. Likely NVDA-GPU rack-attached, not TPU motherboard.",
  "tpu_revenue_share_est_pct": 1.5,
  "nvda_revenue_share_est_pct": 22.0,
  "confidence": "LOW-MEDIUM (AI aggregate DISCLOSED; customer split ESTIMATED)",
  "as_of_date": "2026-05-10"
}
```

---

## FLEX — Flex Ltd. (NASDAQ: FLEX)

### 1. TTM revenue & last-Q revenue (YoY)
- **Fiscal calendar:** Fiscal year ends late March. **Last-Q end:** Mar 31, 2026 (Q4 FY26).
- **Q4 FY26 revenue:** **$7.5 B, +17% YoY**.
- **FY2026 full-year revenue:** **$27.9 B, +8% YoY**.
- **TTM = FY2026 = $27.9 B.**
- Source: Flex Q4 FY26 release (May 6, 2026).

### 2. Forward revenue
- **FY2027 guide:** **$32.3–$33.8 B** (midpoint $33.05 B, **+18% YoY**).
- **CPI (Cloud & Power Infrastructure) FY27 guide:** **+65 to +75% revenue growth** (i.e., $10.9–11.6 B from $6.6 B in FY26).
- **CPI FY28 outlook:** **>+80% growth**.
- **Q1 FY27 guide:** Not separately broken out in extracts; full-year only disclosed.

### 3. Earnings guidance
- **FY2027 adj EPS guide:** **$4.21–$4.51** (+32% midpoint vs $3.30 in FY26).
- **FY2026 adj op margin (actual):** **6.3%** (up 70 bps).
- **FCF:** Not directly extracted; FY27 should exceed FY26.

### 4. EPS (last quarter)
- **Q4 FY26 GAAP EPS:** **$0.67**.
- **Q4 FY26 adjusted EPS:** **$0.93**.
- **FY26 GAAP EPS:** **$2.33**.
- **FY26 adjusted EPS (TTM):** **$3.30**, +25% YoY.

### 5. Gross margin
- **Q4 FY26 GAAP GM:** **9.4%**.
- **Q4 FY26 adjusted GM:** **9.9%** (record, +50 bps YoY).
- **FY26 adjusted GM:** **9.5%** (+70 bps YoY).
- **4-Q trend:** Steady upward — FY25 GM ~8.8% → FY26 9.5% → Q4 FY26 9.9% (record).

### 6. TPU supply history & CPI segment
- **CPI segment Q4 FY26 revenue: $1.8 B, +31% YoY**; full-year FY26 **$6.6 B, +38% YoY** (beat company's own 35% target).
- **CEO Revathi Advaithi (Q4 FY26 call) direct quote:** *"We've recently secured substantial incremental business with several hyperscaler and data center customers, **including Google**. These are not single-product manufacturing engagements. They span power infrastructure, thermal systems and complex hardware manufacturing deployed at scale across our global footprint."*
- **Spin-off announced:** Flex will **spin off CPI into an independent public company in Q1 CY27**, focused on end-to-end digital, power, and thermal infrastructure for AI data centers and utilities.
- **TPU-specific evidence:** Flex's Google relationship is **power infrastructure / thermal / rack-level assembly**, NOT TPU motherboard integration. Flex makes power shelves, busbars, BBUs, transformers, and contributes to NVIDIA reference rack power modules.
- **Per task spec:** Flex has explicit GPU/AI-server power-system commentary but **less direct TPU-board exposure** than CLS. Confirmed.

### 7. AI revenue split: TPU vs NVDA GPU vs other-ASIC
- **CPI segment = ~24% of FY26 revenue ($6.6 B of $27.9 B).** This is the AI-leveraged segment.
- **Estimated CPI split (ESTIMATED, confidence MEDIUM):**
  - **NVDA-GPU-rack-attached power/thermal (incl. neocloud builds):** ~60–70% of CPI (largest stream).
  - **Google power / thermal / rack assembly (mostly accelerator-agnostic but flowing into TPU clusters):** ~15–25% of CPI.
  - **Other hyperscalers (MSFT, AWS, Meta) + utilities + colos:** balance.
- **Direct TPU-motherboard exposure: minimal.** Power-shelf and thermal exposure to TPU racks is real but not separately disclosed.
- Marker: **ESTIMATED**, confidence MEDIUM. Google contract DISCLOSED at the customer level (CEO named on call); product detail not broken out.

### 8. Customer concentration
- **No 10%+ customer named in Q4 FY26 release.**
- **Google contract DISCLOSED on Q4 FY26 call** as a multi-year, multi-product (power + thermal + complex hardware) engagement, but not sized.
- Diversified hyperscaler / neocloud / colo / utility customer mix; CPI segment "booked out in terms of capacity and backlog for the next couple of years."
- No specific NVDA, MSFT, META disclosures by name in extracts.

```json
{
  "ticker": "FLEX",
  "ttm_revenue_usd_b": 27.9,
  "last_q_revenue_usd_b": 7.5,
  "last_q_yoy_pct": 17.0,
  "fy_rev_guide_usd_b": 33.05,
  "last_q_gm_pct_nongaap": 9.9,
  "last_q_eps_nongaap": 0.93,
  "tpu_supply_evidence": "Multi-year contract with Google for power/thermal/complex hardware DISCLOSED on Q4 FY26 call. CPI segment $6.6B (+38% YoY) being spun off Q1 CY27. Exposure is power infrastructure and thermal at AI-rack level, NOT TPU motherboard integration.",
  "tpu_revenue_share_est_pct": 4.0,
  "nvda_revenue_share_est_pct": 14.0,
  "confidence": "MEDIUM (Google relationship DISCLOSED; TPU vs GPU split ESTIMATED)",
  "as_of_date": "2026-05-10"
}
```

---

## FORM — FormFactor, Inc. (NASDAQ: FORM)

### 1. TTM revenue & last-Q revenue (YoY)
- **Fiscal calendar:** Fiscal year ends late December (52/53-week). **Last-Q end:** Mar 28, 2026 (Q1 FY26).
- **Q1 FY26 revenue:** **$226.1 M, +32.0% YoY** (vs $171.4 M in Q1 FY25).
- **FY2025 revenue:** **$785 M** (+2.8% YoY, per 10-K).
- **TTM revenue (Q2'25–Q1'26):** **~$839.7 M** ($785 M + $226.1 M − $171.4 M).
- Source: FormFactor Q1 FY26 release (Apr 29, 2026); FY25 10-K.

### 2. Forward revenue
- **Q2 FY26 guide:** **$240 M ± $5 M** (midpoint, +6% sequential, sets up another record).
- **FY2026 company guide:** Not formal; management forecasts **"record revenue in DRAM probe cards in the current quarter, driven by another step-up in HBM demand"**; CPO revenue $10–20 M (high end).
- **FY2026 street estimate:** ~$960–1,000 M (implied from trajectory; not company-issued).

### 3. Earnings guidance
- **Q2 FY26 non-GAAP EPS guide:** **$0.61 ± $0.04**.
- **Q2 FY26 non-GAAP GM guide:** **49.5% ± 150 bps**.
- **FCF guidance:** Not disclosed quarterly.

### 4. EPS (last quarter)
- **Q1 FY26 GAAP EPS:** **$0.26** (vs $0.08 prior-year).
- **Q1 FY26 non-GAAP EPS:** **$0.56** (exceeded high end of outlook).
- **TTM non-GAAP EPS estimate:** ~**$1.75–1.85** (Q1 $0.56 + sub-$0.45 average across prior 3 Q's).

### 5. Gross margin
- **Q1 FY26 GAAP GM:** **38.4%** (down from 42.2% in Q4'25 due to mix/restructuring items).
- **Q1 FY26 non-GAAP GM:** **49.0%** (+510 bps sequentially, 250 bps above guidance).
- **4-Q trend (non-GAAP):** Q2'25 ~41% → Q3'25 ~43% → Q4'25 ~44% → Q1'26 **49.0%** — significant expansion driven by HBM/Smart Matrix mix.

### 6. TPU supply history & probe card commentary
- Probe cards are **accelerator-agnostic** — FORM sells to memory (HBM at SK Hynix, Samsung, Micron) and to foundry-and-logic test (TSMC, Intel, Samsung foundry, plus design houses).
- **DRAM probe card revenue:** Record in Q1'26; **DRAM total revenue +69.7% YoY** in segment commentary. Driven by **HBM for generative AI**.
- **Smart Matrix:** *"Provides a unique combination of high parallelism productivity and high-speed performance, enabling our customers to test hundreds of completed HBM decks simultaneously at the 10 Gbit+ I/O data rate of HBM4."* — critical for **TSMC CoWoS final-test insertion** where the HBM stack is paired with GPU/custom ASIC.
- **Foundry & Logic:** Q1'26 increase *"driven primarily by growth in probe cards for networking applications"* (i.e., 800G switch ASICs, retimers, optical DSPs). Q2'26 expected to grow further on **data-center CPU applications**.
- **CPO probe revenue:** $10–20 M FY26 (high end), via Keystone Photonics acquisition + Advantest/TEL partnerships.
- **Direct TPU statement: None.** But: advanced-package probe cards (HBM stack-test, RDL probing) directly benefit when **CoWoS-S/L volumes rise** — which is true for **both NVIDIA Blackwell/Rubin and Google TPU v8 (Ironwood)** which is on TSMC CoWoS-L.

### 7. AI revenue split: TPU vs NVDA GPU vs other-ASIC
- FormFactor does **not** disclose by end-customer chip. Disclosed by segment:
  - **DRAM probe (mostly HBM): ~45–50% of revenue Q1'26** (record).
  - **Foundry & Logic probe: ~40–45%** (networking, CPU, accelerator SoC test).
  - **Systems (engineering test, MEMS, photonics): ~10%**.
- **Estimated mapping to end-accelerator (ESTIMATED, confidence LOW–MEDIUM):**
  - **HBM probe** is consumed by SK Hynix / Micron / Samsung — output **flows to NVDA HBM3e/HBM4 (>50%), Google TPU HBM3e (~15–20%), Broadcom-AVGO custom ASICs (~10–15%), AMD MI3xx (~10%)**.
  - **Advanced-package probe (CoWoS stack test):** lifts with **NVDA + AVGO/TPU + AMD** CoWoS volumes simultaneously. **TPU v8 Ironwood** is on TSMC N3 + CoWoS-L, so each TPU package consumed adds to FORM's advanced-package probe demand at TSMC's test partners.
  - **Networking F&L probe:** Tied to 800G switch silicon (Broadcom Tomahawk 5/6, Marvell), retimers (Astera, Credo), optical DSPs (Marvell, Broadcom) — **accelerator-agnostic but AI-leveraged**.
- **TPU-attributable revenue share (Q1'26): ~6–10% of FORM revenue (ESTIMATED, LOW confidence).**
- **NVDA-GPU-attributable revenue share (incl. HBM and CoWoS for NVDA): ~30–40% of FORM revenue (ESTIMATED, MEDIUM confidence).**

### 8. Customer concentration
- **FY2025 10-K disclosure:** *"One customer contributed 22.9% of fiscal 2025 revenues."* (Widely understood to be **SK Hynix**, the primary HBM customer.)
- **Additional context:** **SK Hynix** has been reported at up to **15.5% of quarterly revenue** in some quarters; **Intel** is a long-standing major customer for F&L probe cards.
- **Q1 FY26 disclosure (10-Q level, per earnings transcript):** *"We have 2, 10% customers in this quarter,"* and *"the first quarter growth in probe cards for networking applications caused **a leader in high-performance compute** to become a 10% customer for the first time."* — strongly implies **NVIDIA** (or possibly a custom-ASIC vendor's foundry test partner) becoming a top-tier customer.
- **No direct Google/Alphabet customer disclosure.** Google's TPU exposure to FORM is indirect via TSMC's probe test partners and via HBM vendors.

```json
{
  "ticker": "FORM",
  "ttm_revenue_usd_b": 0.84,
  "last_q_revenue_usd_b": 0.226,
  "last_q_yoy_pct": 32.0,
  "fy_rev_guide_usd_b": 0.98,
  "last_q_gm_pct_nongaap": 49.0,
  "last_q_eps_nongaap": 0.56,
  "tpu_supply_evidence": "Probe cards are accelerator-agnostic. Smart Matrix HBM4 probe is the final-test insertion for HBM stacks paired with GPU or custom ASIC in CoWoS — directly benefits TPU v8/Ironwood (TSMC N3 + CoWoS-L). DRAM probe record Q1'26, +69.7% YoY in DRAM. F&L growth from networking probe (800G switch ASICs). Q1'26 'leader in high-performance compute' became 10% customer for first time (likely NVIDIA).",
  "tpu_revenue_share_est_pct": 8.0,
  "nvda_revenue_share_est_pct": 35.0,
  "confidence": "LOW-MEDIUM (segment splits DISCLOSED; end-accelerator attribution INFERRED via HBM/CoWoS volume share)",
  "as_of_date": "2026-05-10"
}
```

---

## Cross-ticker summary table

| Ticker | TTM Rev ($B) | Last-Q Rev ($B) | YoY% | FY Guide ($B) | NG-GM% | NG-EPS | TPU share (est) | NVDA share (est) | Confidence |
|--------|--------------|-----------------|------|---------------|--------|--------|------------------|-------------------|------------|
| CLS    | 13.79        | 4.05            | +53% | 19.0          | 11.3   | $2.16  | ~30%             | ~3%               | MED-HIGH   |
| JBL    | 31.7         | 8.30            | +17% | 34.0          | ~9.2   | $2.69  | ~1.5%            | ~22%              | LOW-MED    |
| FLEX   | 27.9         | 7.50            | +17% | 33.05         | 9.9    | $0.93  | ~4%              | ~14%              | MEDIUM     |
| FORM   | 0.84         | 0.226           | +32% | ~0.98         | 49.0   | $0.56  | ~8%              | ~35%              | LOW-MED    |

**Staleness check:** All four data points are within last 60 days (CLS Apr 27, JBL Mar 18, FLEX May 6, FORM Apr 29). **None flagged STALE.**

## Sources
- Celestica Q1 2026 release: https://corporate.celestica.com/news-releases/news-release-details/celestica-announces-first-quarter-2026-financial-results
- Celestica Q1 2026 transcript: https://www.fool.com/earnings/call-transcripts/2026/04/28/celestica-cls-q1-2026-earnings-transcript/
- Celestica FY2025 release: https://corporate.celestica.com/news-releases/news-release-details/celestica-announces-fourth-quarter-and-fy-2025-financial-results
- Jabil Q2 FY26 transcript: https://www.fool.com/earnings/call-transcripts/2026/03/25/jabil-jbl-q2-2026-earnings-call-transcript/
- Jabil Q1 FY26 release: https://investors.jabil.com/news/news-details/2025/Jabil-Posts-First-Quarter-Results/
- Flex Q4 FY26 release: https://investors.flex.com/news/news-details/2026/FLEX-REPORTS-FOURTH-QUARTER-AND-FISCAL-2026-RESULTS/
- Flex Q4 FY26 transcript: https://www.fool.com/earnings/call-transcripts/2026/05/06/flex-flex-q4-2026-earnings-call-transcript/
- FormFactor Q1 FY26 release: https://www.stocktitan.net/news/FORM/form-factor-inc-reports-2026-first-quarter-ptznafssjeh9.html
- FormFactor Q1 FY26 transcript: https://www.fool.com/earnings/call-transcripts/2026/04/29/formfactor-form-q1-2026-earnings-transcript/
- FormFactor FY2025 10-K (customer concentration): https://last10k.com/sec-filings/form
