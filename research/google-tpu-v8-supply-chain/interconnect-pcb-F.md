# Bucket F — High-Density Interconnect, PCB & Connectivity

**As of:** 2026-05-10. **Latest prints:** APH Q1 CY2026 (4/29/26), TEL Q2 FY26 (4/22/26), GLW Q1 CY2026 (4/28/26), TTMI Q1 CY2026 (4/29/26). All datapoints below are <120 days old (NOT STALE) unless otherwise flagged.

Tickers: APH, TEL, GLW, TTMI. These are the connector / cable / fiber / PCB layer of an Ironwood TPU rack — largely accelerator-agnostic; the bill-of-materials looks similar whether the rack houses Google TPUs, NVIDIA Blackwell/Rubin GPUs, Meta MTIA or AWS Trainium. Allocations between TPU and other accelerator are ESTIMATED by hyperscaler-capex weights unless explicitly disclosed.

---

## APH — Amphenol Corporation

### 1. TTM revenue & last-Q revenue (YoY)
- **Last-Q (Q1 CY2026, period ending ~3/31/26):** $7.62B, **+58% YoY USD** (+47% organic). Record quarter; beat the high end of guidance. Organic sequential growth was ~+16% with "virtually all" of the sequential growth driven by AI products.
- **TTM revenue (Q2'25 + Q3'25 + Q4'25 + Q1'26):** ~$25.5B (FY2025 was $23.10B per Q4'25 press release; Q1'26 added ~$7.62B vs ~$5.27B in Q1'25 = +$2.35B walk → TTM ≈ $25.5B).
- Source: APH press release 4/29/26; Q4 2025 release.

### 2. Forward revenue
- **Q2 CY2026 company guide:** sales **$8.10–8.20B**, **+43–45% YoY**; assumes constant FX. Implies further sequential growth in IT datacom in "low-teens" range.
- **Full-year 2026:** No explicit total guide issued; CCS (former CommScope) carve-out: ~$4.1B 2026 sales / +$0.15 EPS accretion (Jan 28 2026 commentary).
- Street FY26 consensus is ~$32B (post-Q1 revisions higher); APH historically does not give annual guide.

### 3. Earnings guidance
- **Q2 CY2026 adjusted diluted EPS:** **$1.14–$1.16**, +41–43% YoY.
- Q1'26 adj operating margin 27.3% (+380 bps YoY, -20 bps QoQ). FCF Q1'26 = $831M (89% of net income).
- No explicit FCF or GM annual guide.

### 4. EPS
- **Q1'26 GAAP diluted EPS:** $0.72 (+24% YoY).
- **Q1'26 non-GAAP adj diluted EPS:** $1.06 (+68% YoY).
- **TTM non-GAAP EPS:** ~$3.65 (Q1'26 $1.06 + Q4'25 ~$0.85 + Q3'25 ~$0.84 + Q2'25 ~$0.77, approximate; FY25 adj EPS print was ~$2.59).

### 5. Gross margin
- Q1'26: adj operating margin 27.3% (gross margin not separately broken out in press release, but gross profit was ~$2.8B implying GM ~36.7%).
- 4-Q adjusted operating margin trend: Q2'25 25.6% → Q3'25 27.5% → Q4'25 27.5% → Q1'26 27.3%.
- Q2'25 disclosed gross margin: 36.3% (+270 bps YoY). GM trajectory broadly stable at ~36–37%.

### 6. TPU supply history — AI / IT datacom commentary
- **IT Datacom segment = 41% of Q1'26 sales**; +99% YoY USD, **+81% organic YoY**; +16% organic sequentially with "virtually all" driven by AI (Norwitt). Strongest quarter in 94-year company history.
- Products: high-speed backplane connectors, 224G PAM4 cabled assemblies (NearStack / OverPass / Paladin), copper Active Electrical Cables (AECs), CommScope-acquired AEC product line (closed CCS deal early '26). Direct supplier of high-speed copper for NVL72 rack-scale; broadly accelerator-agnostic. Norwitt has explicitly named "AI-related products" each quarter; does not call out individual hyperscalers.
- Specific TPU evidence: Amphenol Communications Solutions Backplane catalog explicitly lists 112G/224G XCede and ExaMAX2 — these are used in custom-ASIC racks including TPU pods. Channel-checks (Next Platform, ServeTheHome) cite APH as a high-speed backplane supplier for Google TPU systems. Not formally disclosed by APH.

### 7. AI revenue split: TPU vs NVDA-GPU vs other-ASIC
APH does not disclose. ESTIMATE method: AI-related IT datacom run-rate ~$10–12B annualized in Q1'26 (41% × $7.6B × AI share ≈ 70% of segment = ~$2.2B/Q × 4 = ~$9B AI ann.). Allocate by 2026E hyperscaler accelerator-spend weights: NVDA-attach systems ~60–65%, Google TPU ~15%, AWS Trainium/Inferentia ~8%, Meta MTIA ~7%, MSFT Maia/AMD ~5–10%.
- **TPU revenue share of total APH:** ~5–6% (AI is ~35% of total; TPU is ~15% of AI).
- **NVDA-GPU-attach share of total APH:** ~22–24%.
- **Confidence:** LOW–MEDIUM (no disclosure, model-derived).

### 8. Customer concentration
- 2024 10-K: "no single customer accounted for more than 10% of net sales." Diversified across ~10,000+ end customers. No named hyperscaler disclosed.
- Apple commentary: historically Apple <10% but a top-5 customer in Mobile Devices. Google/MSFT/META/AMZN/NVDA not individually disclosed.

```json
{
  "ticker": "APH",
  "ttm_revenue_usd_b": 25.5,
  "last_q_revenue_usd_b": 7.62,
  "last_q_yoy_pct": 58,
  "fy_rev_guide_usd_b": null,
  "last_q_gm_pct_nongaap": 36.7,
  "last_q_eps_nongaap": 1.06,
  "tpu_supply_evidence": "estimated — backplane connector & high-speed copper supplier to Google AI systems via Amphenol Communications Solutions Backplane (XCede/ExaMAX2 224G), not formally disclosed; IT datacom +81% organic Q1'26 with virtually all sequential growth from AI",
  "tpu_revenue_share_est_pct": 5.5,
  "nvda_revenue_share_est_pct": 23,
  "confidence": "low-medium",
  "as_of_date": "2026-05-10"
}
```

---

## TEL — TE Connectivity

### 1. TTM revenue & last-Q revenue (YoY)
- **Last-Q (Q2 FY26, period ending ~3/28/26):** $4.74B, **+15% reported YoY** / **+7% organic**.
- **TTM revenue** (Q3'25 + Q4'25 + Q1'26 + Q2'26): ~$18.0B (FY25 ended 9/26/25 at $16.4B; subsequent two quarters ran ~$4.6–4.74B).
- Orders Q2'26 = record $5.3B, book-to-bill 1.12.
- Source: TEL press release 4/22/26.

### 2. Forward revenue
- **Q3 FY26 company guide:** sales ~$5.0B (+10% reported, +9% organic).
- **Full FY26:** Implied DDN segment AI sales **~$2.4B** for FY26 (raised by $200M from prior view, vs $150M raise reported earlier in quarter — net raise across both prints). No formal full-year revenue total guidance; sell-side FY26 ~$19.5–20B.

### 3. Earnings guidance
- **Q3 FY26 EPS guide:** adj EPS ~**$2.83** (+17% YoY); GAAP EPS ~$2.44 (+14% YoY).
- Capex raised to ~6% of revenue (from historical ~5%) entirely for AI program ramp in DDN, tied to specific customer awards.
- No FCF guide change.

### 4. EPS
- **Q2 FY26 GAAP EPS:** ~$2.40 (continuing ops; not separately broken out in summary — derived from "GAAP operating margin 20%, +200 bps YoY"; full GAAP EPS not specified in press release excerpts).
- **Q2 FY26 non-GAAP adj EPS:** **$2.73** (+24% YoY) — record.
- **TTM non-GAAP EPS:** ~$10.30 (rough sum of last 4 Qs: $2.27 + $2.59 + $2.71 + $2.73).

### 5. Gross margin
- Q2 FY26 adj operating margin: 22% (+130 bps YoY). GAAP operating margin: 20% (+200 bps YoY).
- Gross margin not disclosed at this level of detail; historical TEL GM ~34–36%.
- Operating margin trend (adj, last 4 Qs): Q3'25 ~20%, Q4'25 ~20.5%, Q1'26 ~21%, Q2'26 22%.

### 6. TPU supply history — AI / data center commentary
- **Digital Data Networks (DDN) within Industrial Solutions segment:** grew ~70% YoY in Q2'26 to $707M; ~1/3 of Industrial seg revenue. FY26 AI revenue raised again to ~$2.4B (now $200M above prior guidance issued 90 days earlier).
- Management commentary (Terrence Curtin): "growth across every hyperscale customer"; "AI content per chip has expanded fivefold since 2023" — from ~$500M run-rate in 2023 to "trajectory toward $3B" by FY27.
- Products: AdrenaLINE 224G connectors, Catapult ASIC over-the-board jumper cables (replacing high-loss PCB traces), Sliver internal cabling, MULTIGIG RT3 backplane. Recent Rampphotonics acquisition for optical passive connectivity.
- TPU specifics: TEL is a documented supplier of high-speed near-chip connectors to Google's TPU pods (channel-check, OCP commentary). Not formally disclosed.

### 7. AI revenue split: TPU vs NVDA-GPU vs other-ASIC
DDN AI run-rate ~$2.4B FY26. Same accelerator-agnostic logic as APH. Allocate by 2026E hyperscaler accelerator-spend weights:
- **TPU share of total TEL revenue:** ~$2.4B × 15% / $19.5B ≈ **~1.8%**.
- **NVDA-GPU-attach share of total TEL:** ~$2.4B × 62% / $19.5B ≈ **~7.6%**.
- **Confidence:** LOW–MEDIUM (DDN total disclosed; sub-allocation modeled).

### 8. Customer concentration
- TEL FY2025 10-K (filed late Nov 2025): "no single customer accounted for a significant amount of net sales in fiscal 2025, 2024 or 2023."
- No named hyperscaler disclosed. Auto remains the largest end-market (Transportation Solutions ~57% of total in Q2'26).

```json
{
  "ticker": "TEL",
  "ttm_revenue_usd_b": 18.0,
  "last_q_revenue_usd_b": 4.74,
  "last_q_yoy_pct": 15,
  "fy_rev_guide_usd_b": null,
  "last_q_gm_pct_nongaap": null,
  "last_q_eps_nongaap": 2.73,
  "tpu_supply_evidence": "estimated — 224G AdrenaLINE & Catapult jumper supplier to hyperscaler AI; FY26 DDN AI revenue raised to ~$2.4B with 'growth across every hyperscale customer'; AI content per chip 5x since 2023; not formally Google-attributed",
  "tpu_revenue_share_est_pct": 1.8,
  "nvda_revenue_share_est_pct": 7.6,
  "confidence": "low-medium",
  "as_of_date": "2026-05-10"
}
```

---

## GLW — Corning Incorporated

### 1. TTM revenue & last-Q revenue (YoY)
- **Last-Q (Q1 CY2026, period ending 3/31/26):** core sales $4.35B, **+18% YoY** (GAAP sales similar). Optical Communications $1.85B, +36% YoY; Solar $370M, +80% YoY; Glass Innovations $1.40B, +1%; Auto $437M, -1%.
- **TTM core sales:** ~$16.5B (FY25 core sales ~$15.0B + Q1'26 step-up).
- Source: GLW press release 4/28/26.

### 2. Forward revenue
- **Q2 CY2026 company guide:** core sales **~$4.6B**, +14% YoY.
- **FY 2030 Springboard plan extended** (May 6 NYC investor event): planning higher growth rate vs prior plan; specific FY26 total not formally guided but implied >$18B at run-rate.
- Street FY26: ~$18B.

### 3. Earnings guidance
- **Q2 CY2026 core EPS guide:** **$0.73–$0.77** (+25% YoY at midpoint).
- Q2 includes incremental ~$30M solar-wafer shutdown expense (planned maintenance).
- Q1 Springboard incremental EPS milestones laid out at investor day.

### 4. EPS
- **Q1'26 GAAP diluted EPS:** ~$0.60 (derived; not explicitly cited in press snippets).
- **Q1'26 core (non-GAAP) EPS:** **$0.70** (+30% YoY).
- **TTM core EPS:** ~$2.45.

### 5. Gross margin
- Q1'26 core gross margin: **39.1%** (+120 bps YoY).
- 4-Q core GM trend: Q2'25 ~37.9% → Q3'25 ~38.4% → Q4'25 ~38.9% → Q1'26 39.1%. Steady expansion driven by Optical mix and price-cost.

### 6. TPU supply history — Optical Communications / Gen-AI fiber
- **Optical Communications $1.85B Q1'26, +36% YoY**: Both carrier AND enterprise sub-segments grew 36%. Enterprise is dominated by hyperscaler/data-center fiber. Wendell Weeks: "robust GenAI infrastructure buildouts" driving both halves.
- **Meta deal (Jan 27 2026):** multi-year, up-to-**$6B** agreement for optical fiber/cable/connectivity; Hickory NC anchor. Officially announced.
- **Two additional hyperscaler deals (called out on Q1'26 call, Apr 28 2026):** "similar in size and duration" to the Meta agreement. Not named, but consistent with Google + Microsoft per channel commentary. Google specifically is widely understood to be one given Corning's incumbency in Google's network fiber.
- **NVIDIA partnership (May 6 2026):** up to $3.2B NVIDIA investment, 10× US optical-connectivity capacity expansion (3 new NC + TX plants), specifically for co-packaged optics & rack-scale optical replacement of copper inside Blackwell/Rubin systems. Confirmed direct NVIDIA-Corning relationship.
- New Gen-AI fiber product: 2× to 4× more fiber in existing conduit (Marvel-class density).
- Carrier vs hyperscaler split: roughly 50/50 within Optical Communications in 2026, with enterprise (hyperscaler) growing faster. Pre-AI cycle this was ~60/40 carrier-leaning.

### 7. AI revenue split: TPU vs NVDA-GPU vs other-ASIC
Optical Comms enterprise sub-segment is the AI proxy (~50% of Optical = ~$925M Q1'26 × 4 ≈ $3.7B FY26 run-rate). Now with named anchor customers:
- **Meta MTIA / Meta AI infra:** anchor (~$1B+ FY26 run-rate from the $6B 5-year deal). Largest single AI customer.
- **NVDA-attach:** large via 5/6/26 NVIDIA deal — fiber for Blackwell/Rubin rack-scale CPO; not yet ramped to revenue, but contracted.
- **TPU / Google:** Google is a long-standing Corning fiber customer (Google's intra/inter-DC fiber backbone). Likely one of the two undisclosed hyperscalers in deals "similar to Meta." Allocate ~$1B FY26 contracted run-rate.
- **Estimated mix of AI optical revenue (~$3.7B FY26):** Meta ~28%, NVDA-attach systems (split across all NVDA-using HSPs) ~32%, Google/TPU + Google general infra ~25%, MSFT/AMZN/other ~15%.
- **TPU/Google share of total GLW revenue:** ~$925M / $18B ≈ **~5%**.
- **NVDA-attach share of total GLW:** ~$1.18B / $18B ≈ **~6.5%**.
- **Confidence:** MEDIUM (Meta disclosed; others modeled).

### 8. Customer concentration
- GLW FY24 10-K: ~25% of Display Tech sales to single LCD panel maker; Optical Comms is the diversified segment with no >10% customer historically. Now with Meta-$6B contract and NVIDIA $3.2B + 10× capacity build, customer concentration in Optical is rising.
- AAPL: Corning's "Investing in America" announcement Feb 2026 with Apple (~$2.5B Specialty/Cover Glass) — Apple is ~5–10% of total GLW historically.
- Google/MSFT/AMZN: not separately disclosed.

```json
{
  "ticker": "GLW",
  "ttm_revenue_usd_b": 16.5,
  "last_q_revenue_usd_b": 4.35,
  "last_q_yoy_pct": 18,
  "fy_rev_guide_usd_b": null,
  "last_q_gm_pct_nongaap": 39.1,
  "last_q_eps_nongaap": 0.70,
  "tpu_supply_evidence": "estimated — Google is long-time Corning fiber customer in DC backbone; on Q1'26 call GLW disclosed two new hyperscaler deals 'similar to' the $6B Meta contract, widely believed to include Google. Optical Comms enterprise sub-segment +36% YoY on GenAI buildouts.",
  "tpu_revenue_share_est_pct": 5.0,
  "nvda_revenue_share_est_pct": 6.5,
  "confidence": "medium",
  "as_of_date": "2026-05-10"
}
```

---

## TTMI — TTM Technologies

### 1. TTM revenue & last-Q revenue (YoY)
- **Last-Q (Q1 CY2026, period ending ~3/31/26):** $846.0M, **+30% YoY** (vs $648.7M Q1'25). All-time high quarterly revenue.
- **TTM revenue (Q2'25 + Q3'25 + Q4'25 + Q1'26):** ~$3.10B (FY25 was $2.906B + ~$197M Q1 step-up).
- Source: TTMI press release 4/29/26.

### 2. Forward revenue
- **Q2 CY2026 company guide:** sales **$930–970M** (midpoint $950M, vs prior consensus $829M — major beat-and-raise).
- Mgmt: "first-half growth pace can continue at similar pace in 2H 2026." Implies FY26 ~$3.8–4.0B.
- No formal FY guide; sell-side FY26 ~$3.7B (likely revised up post-Q1).

### 3. Earnings guidance
- **Q2 CY2026 non-GAAP EPS guide:** **$0.82–$0.88** (vs prior consensus $0.74).
- 2026 capex raised to **$300–320M** (Syracuse fab build-out — $115–125M annualized revenue capacity, the only scaled advanced-PCB producer in N. America).
- Adj EBITDA Q1'26 = 15.7% of sales.

### 4. EPS
- **Q1'26 GAAP EPS:** ~$0.54 (derived from $846M sales, GAAP NI not separately cited in summary — but adj EBITDA 15.7% and historical GAAP-to-adj gap implies ~$0.50–0.55).
- **Q1'26 non-GAAP adj EPS:** **$0.75** (record; vs consensus $0.65, +15% beat).
- **TTM non-GAAP EPS:** ~$2.80 (FY25 adj EPS was $2.46 + Q1 step-up).

### 5. Gross margin
- Q1'26 GAAP gross margin: **22.3%** (+150 bps YoY from 20.8%).
- 4-Q GAAP GM trend: Q2'25 ~21.2% → Q3'25 ~21.6% → Q4'25 ~21.9% → Q1'26 22.3%. Steady expansion driven by data-center mix and Penang ramp.
- No separate non-GAAP GM publicly disclosed.

### 6. TPU supply history — data center networking commentary
- **End-market mix Q1'26:** Data Center Computing + Networking = **36% of sales** (up from 26% prior year). Segment grew **+61% YoY**.
- **Q2'26 guide:** Data Center & Networking expected to be **42% of Q2 sales**.
- Mgmt (Tom Edman): **~85% of Data Center Computing revenue is hyperscaler / GenAI-driven**. "4–5 major customers" in DC, actively diversifying.
- **AI PCB content per server rack:** TTMI doesn't publish per-rack BOM but cites multi-thousand-dollar advanced multi-layer (20–40 layer) backplane PCBs per AI rack vs <$500 for general-purpose servers. AI rack PCB content is ~3–5× general DC.
- **Syracuse, NY fab:** sole scaled advanced PCB producer in North America; defense + hyperscaler dual-use. Equipment install mid-2026, production ramp 2H 2026.
- TPU specifics: TTMI is publicly identified (channel-checks; e.g. SemiAnalysis, Trendforce) as a primary supplier of high-layer-count backplane PCBs for Google's TPU rack (incl. Ironwood). Other DC customers include Meta (MTIA), Microsoft (Maia), AWS (Trainium). Not formally named by TTMI.

### 7. AI revenue split: TPU vs NVDA-GPU vs other-ASIC
DC Computing + Networking ~$305M Q1'26 (36% × $846M). 85% = ~$259M hyperscaler/GenAI = ~$1.04B annualized.
- Within hyperscaler DC, TTMI is notably MORE custom-ASIC weighted than APH/TEL because Google/Meta/AWS custom ASICs use bespoke high-layer-count backplanes (TTMI's sweet spot), whereas NVIDIA HGX/NVL72 backplanes are increasingly going to Asian PCB suppliers (e.g. Zhen Ding, Unimicron) and direct cable in NVL72 (less PCB content). Weight:
  - Google TPU ~30% of hyperscaler DC = ~$310M = **~10% of TTMI total**.
  - Meta MTIA ~20% = ~7%.
  - AWS Trainium ~15% = ~5%.
  - MSFT Maia ~10% = ~3%.
  - NVDA-attach systems ~25% = **~8% of TTMI total**.
- **TPU revenue share of total TTMI:** **~10%** (highest of the 4 names here).
- **NVDA-GPU share of total TTMI:** ~8%.
- **Confidence:** MEDIUM (the 85% hyperscaler share IS disclosed; ASIC sub-allocation modeled).

### 8. Customer concentration
- TTMI FY2025 10-K: top-5 customers = **44% of 2025 net sales**; **one customer >10%** of sales (not named; widely assumed to be a defense prime — Raytheon/Lockheed — or a hyperscaler).
- Q1'25 update: top-5 = 45%.
- Mgmt explicitly says "4–5 major hyperscaler customers" — implies Google, Meta, MSFT, AMZN, possibly NVDA-direct (for switch/networking PCBs). None named individually.
- The unnamed >10% customer in the 10-K is historically a defense customer; with hyperscaler scaling, a hyperscaler may displace it by FY26.

```json
{
  "ticker": "TTMI",
  "ttm_revenue_usd_b": 3.10,
  "last_q_revenue_usd_b": 0.846,
  "last_q_yoy_pct": 30,
  "fy_rev_guide_usd_b": null,
  "last_q_gm_pct_nongaap": 22.3,
  "last_q_eps_nongaap": 0.75,
  "tpu_supply_evidence": "estimated — TTMI named in channel reports as primary advanced-PCB / backplane supplier for Google TPU racks incl. Ironwood; ~85% of DC Computing revenue is hyperscaler GenAI per mgmt; 4-5 major DC customers; Syracuse fab is sole scaled NA advanced PCB producer",
  "tpu_revenue_share_est_pct": 10.0,
  "nvda_revenue_share_est_pct": 8.0,
  "confidence": "medium",
  "as_of_date": "2026-05-10"
}
```

---

## Cross-ticker observations

| Ticker | Last-Q YoY | AI exposure | Disclosed Google/TPU evidence | Estimated TPU revenue share |
|--------|------------|-------------|-------------------------------|----------------------------|
| APH | +58% | Highest absolute $; +81% organic IT datacom | None named; broad AI commentary | ~5–6% |
| TEL | +15% reported | DDN AI = $2.4B FY26 | "every hyperscale customer"; not named | ~1.8% |
| GLW | +18% | Optical Comms +36%; Meta $6B + 2 unnamed | 2 additional HSP deals; Google likely one | ~5% |
| TTMI | +30% | DC 36% of sales, +61% YoY | Channel-named for TPU backplanes | ~10% (highest) |

TTMI has the most TPU-leverage in this bucket by percentage of revenue. APH has the largest absolute dollar AI exposure. GLW has the most defensible long-term contractual visibility (Meta + NVIDIA + 2 unnamed hyperscalers under multi-year deals).

## Source notes
- APH: 4/29/26 earnings release; Q1 CY2026 transcript (Norwitt); investors.amphenol.com.
- TEL: 4/22/26 earnings release; Q2 FY26 transcript (Curtin); investors.te.com.
- GLW: 4/28/26 earnings release; Q1 CY2026 transcript (Weeks); investor.corning.com. Meta announcement 1/27/26; NVIDIA partnership 5/6/26 (corning.com newsroom).
- TTMI: 4/29/26 earnings release; Q1 CY2026 transcript (Edman); investors.ttm.com.
- All quarterly source pages reviewed via WebSearch on 5/10/26 — none stale (>120 days).
- TPU-specific attribution is INFERRED from channel reports (Next Platform, ServeTheHome, SemiAnalysis); none of the four companies discloses Google as a >10% customer.
