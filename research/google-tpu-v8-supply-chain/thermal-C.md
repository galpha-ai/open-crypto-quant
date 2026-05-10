# Thermal Management & Liquid Cooling — Bucket C
**Tickers: VRT, MOD, NVT | As-of date: 2026-05-10**

Scope: liquid-cooling & thermal-management vendors with exposure to Google's 8th-gen TPU ("Ironwood") rack-scale buildout. The work these vendors do is largely accelerator-agnostic — a CDU or rack-PDU does not care whether it's cooling a TPU v7 or an NVDA GB300. Where vendors are named participants in Google's open-sourced **Project Deschutes CDU** ecosystem (OCP-spec 2 MW CDU developed by Google for TPU/AI workloads), that is the strongest TPU-supply evidence available. Project Deschutes ecosystem partners publicly listed by DCD and OCP include **Boyd, CoolerMaster, Delta, Envicool, Nidec, nVent, Stulz, and Vertiv** — Modine/Airedale is conspicuously absent from this list as of OCP Global Summit 2025.

---

## VRT — Vertiv Holdings

**Latest reported fiscal Q:** Q1 2026, ended **March 31, 2026** (reported April 22, 2026). Calendar fiscal year.

### 1. TTM revenue & last-Q revenue (YoY)
- **Last-Q (Q1 2026) net sales: $2,650M**, up **+30% YoY** vs $2,036M in Q1 2025; **+23% organic**, +4% acquisitions, +3% FX.
- **TTM revenue (Q2'25–Q1'26): ~$10.84B** (= FY25 $10,229.9M − Q1'25 $2,036M + Q1'26 $2,650M).
- Americas region drove growth at **+44% organic**; EMEA −29% organic (timing); APAC +12%.
- Source: Vertiv Q1 2026 release, investors.vertiv.com (2026-04-22).

### 2. Forward revenue
- **Q2 2026 guide:** $3,275–3,375M net sales (range from earnings release).
- **FY2026 guide (raised from initial $13.25–13.75B): $13,500–14,000M**, organic growth 29–31%.
- **Street consensus FY2027:** ~$17.0–17.5B implied from sell-side notes (Citi PT $340, Apr 2026), reflecting backlog-conversion roadmap; not directly disclosed by company.

### 3. Earnings guidance
- **FY26 Adj diluted EPS: $6.30–6.40** (raised from $5.97–6.07 at Q4'25); GAAP EPS $5.60–5.70.
- **FY26 Adj operating margin: 22.8–23.8%**.
- **FY26 Adj free cash flow: $2,100–2,300M.**
- Q2'26 Adj operating margin guide: ~21.5–22.5%.

### 4. EPS — last-Q & TTM
- **Q1 2026 GAAP diluted EPS: $0.99** (+136% YoY).
- **Q1 2026 Adj diluted EPS: $1.17** (+83% YoY).
- **TTM Adj diluted EPS: ~$4.73** (FY25 $4.20 − Q1'25 $0.64 + Q1'26 $1.17).
- **TTM GAAP EPS: ~$3.83** (FY25 $3.41 − Q1'25 $0.42 + Q1'26 $0.99 — approx, share-count drift small).

### 5. Gross margin
- **Q1 2026 GAAP gross margin: 37.7%.** Company does not report a separate "adjusted" gross margin — non-GAAP adjustments flow through SG&A/other lines. **Adjusted operating margin: 20.8%** (+430 bps YoY).
- 4-Q GAAP gross-margin trend (approximate, from cost-of-sales math):
  - Q2 2025: ~36.5%
  - Q3 2025: ~37.5%
  - Q4 2025: ~37.0%
  - Q1 2026: 37.7%
  - Trend: stable-to-up mid-37% range; mix benefit (liquid-cooling, services) offset by tariff drag.

### 6. TPU supply history
- **Strong, explicit, named.** Vertiv's **CoolChip CDU – Project Deschutes 5** is a publicly listed product on the OCP marketplace (opencompute.org/products/736), built to Google's 5th-generation Project Deschutes 2 MW CDU specification — the exact reference design for Ironwood-class TPU racks.
- Vertiv showcased Deschutes-compliant CDUs at OCP Global Summit 2025 and SC25; ecosystem confirmed by Dell'Oro Group post-OCP 2025 writeup.
- Q1 2026 commentary (per release): backlog **$12.45B** as of Mar-31-2026 (vs $15.0B at Dec-31-2025; $7.0B Mar-31-2025 — **+78% YoY**). Orders/backlog momentum driven by "hyperscale AI demand."
- Q4 2025: organic orders **+252% YoY**, book-to-bill ~2.9x — the AI-orders-to-revenue conversion narrative. Vertiv has reiterated multi-quarter backlog visibility through 2027.
- 2025 acquisitions: **CoolTera** (CDU IP, 2023), **PurgeRite** ($1B, 2025 — pipe-flushing/hydraulics services for hyperscale liquid cooling), **Strategic Thermal Labs** (2026) — all reinforce TPU/AI-rack thermal positioning.
- NVIDIA GB300 NVL72 reference architecture co-developed (June 2025) — same plant footprint serves Google TPU racks.

### 7. AI revenue split: TPU vs NVDA-GPU vs other-ASIC
- Vertiv discloses **no breakout** by accelerator. Data-center revenue is the dominant segment but no AI-attach % is disclosed.
- Work is **largely accelerator-agnostic**: CDUs, busways, switchgear, UPS serve any AI rack. The TPU vs GPU split is set by where Vertiv's hyperscale customers deploy spend.
- **ESTIMATED** Google TPU share of AI-attach revenue: Google ~14–18% of Big-4 hyperscaler AI capex (GOOG ~$95B vs MSFT ~$110B + META ~$70B + AMZN ~$115B 2026 capex). Apply to Vertiv's ~70% AI-data-center share of mix → Google TPU exposure roughly **8–12% of total revenue**.
- NVDA-GPU-attached revenue (i.e., serving GB200/GB300 racks at MSFT/META/AMZN/Oracle/CoreWeave): **ESTIMATED ~45–55%** of total revenue.
- Other-ASIC (AWS Trainium, Microsoft Maia, Meta MTIA): **ESTIMATED 5–10%**.
- Confidence: **LOW-MEDIUM** on the split (estimated, not disclosed); HIGH on directional Google exposure given confirmed Deschutes-5 product participation.

### 8. Customer concentration
- Per Vertiv 2024 10-K (Item 1A, filed Feb 2025): "no single customer accounted for 10% or more of net sales" in 2024, 2023, or 2022.
- The 2025 10-K (filed Feb 2026) repeats this disclosure — **no 10% customer**, though hyperscaler concentration is acknowledged as a risk factor.
- Known but undisclosed-% customers: Microsoft, Alphabet/Google, Meta, AWS, Oracle, CoreWeave, Compass Datacenters (named partner, Dec 2024 announcement), Equinix, Digital Realty.

```json
{
  "ticker": "VRT",
  "ttm_revenue_usd_b": 10.84,
  "last_q_revenue_usd_b": 2.65,
  "last_q_yoy_pct": 30.2,
  "fy_rev_guide_usd_b": 13.75,
  "last_q_gm_pct_nongaap": 37.7,
  "last_q_eps_nongaap": 1.17,
  "tpu_supply_evidence": "Named OCP vendor for Google Project Deschutes 5 CDU (2MW, Ironwood-class); CoolChip CDU listed on OCP marketplace; Deschutes ecosystem partner per OCP/DCD; backlog $12.45B Mar-31-2026 (+78% YoY)",
  "tpu_revenue_share_est_pct": 10,
  "nvda_revenue_share_est_pct": 50,
  "confidence": "low-medium on accelerator split, high on directional Google exposure",
  "as_of_date": "2026-05-10"
}
```

---

## MOD — Modine Manufacturing

**Latest reported fiscal Q:** Q3 FY2026, ended **December 31, 2025** (reported Feb 5, 2026). Modine's fiscal year ends March; Q4 FY2026 (ending Mar-31-2026) **reports May 27, 2026 — not yet available as of 2026-05-10**.

**STALE FLAG:** Q3 FY2026 release dated 2026-02-05 = ~94 days old as of 2026-05-10; under 120 days. Q4 FY2026 will be the next print.

### 1. TTM revenue & last-Q revenue (YoY)
- **Last-Q (Q3 FY26) net sales: $805.0M**, up **+31% YoY** vs $616.8M Q3 FY25.
- **TTM revenue (Q4 FY25 + Q1–Q3 FY26): ~$3.05B** (FY25 $2,576.4M − Q3 FY25 $616.8M + Q3 FY26 $805.0M − Q2 FY25 ~$648M ... cleaner: 9M FY26 $2,226.7M + Q4 FY25 ~$641M ≈ **$2.87B TTM** through Dec-31-2025).
- Climate Solutions Q3: $544.6M (+51%), Data Center sales +78% YoY (≈ $130M dollar growth → data-center segment ~$295–305M in Q3 FY26 alone).
- Performance Technologies Q3: $266.0M (+1%).
- Source: prnewswire.com/Modine Q3 FY26 release; investors.modine.com (2026-02-05).

### 2. Forward revenue
- **FY2026 (ending Mar 2026) raised guide:** total sales growth **+20–25%** (implies FY26 revenue ~$3.09–3.22B); Climate Solutions **+40–45%**.
- **Data Center FY26 guide: +70%+ YoY** (implies data-center segment ~$1.10–1.15B+ in FY26 vs $644M in FY25).
- **Multi-year FY27/FY28 guide (set at Q3 FY26 call):** data-center sales **+50–70% annually**; **>$2B target by FY28** (raised from prior $2B by FY28 plan).
- **Q4 FY26 implied:** ~$825–940M total sales (back-solve from guide vs $2.227B YTD).
- Street consensus FY27 (Apr 2026 sell-side): ~$3.8–4.1B revenue.

### 3. Earnings guidance
- **FY26 Adj EBITDA range: $455–475M** (raised from $420–450M), +16–21% YoY.
- **FY26 Adj EBITDA margin guide: ~14.5–15.0%.**
- No explicit EPS range or FCF guide in current outlook; capex elevated (~$200M+ for data-center capacity).
- 9M FY26 actual Adj EBITDA: $324.8M (14.6% margin).

### 4. EPS — last-Q & TTM
- **Q3 FY26 GAAP EPS: $(0.90)** (includes non-cash pension-termination charge; net loss $46.8M).
- **Q3 FY26 Adj EPS: $1.19** (+29% YoY vs $0.92).
- **9M FY26 GAAP EPS: $0.90**; Adj EPS not aggregated in release but ~$3.10 implied.
- **TTM Adj EPS: ~$3.85–4.00** (rough — FY25 was $3.49 Adj EPS).

### 5. Gross margin
- **Q3 FY26 GAAP gross margin: 23.1%** (down 120 bps YoY) — capacity-ramp drag in Climate Solutions.
- Modine does **not** publish an adjusted gross margin.
- 4-Q GAAP gross-margin trend:
  - Q4 FY25 (Mar-25): ~24.7%
  - Q1 FY26 (Jun-25): **24.2%** (−40 bps YoY)
  - Q2 FY26 (Sep-25): **22.3%** (−290 bps YoY) — capacity-ramp cost peak
  - Q3 FY26 (Dec-25): **23.1%** (−120 bps YoY)
  - Trend: down 100–290 bps YoY through capacity buildout for data-center cooling lines; management expects normalization by Q4 FY26 / early FY27.

### 6. TPU supply history
- **Indirect / weaker named evidence than VRT-NVT.** Modine sells data-center cooling under the **Airedale by Modine** brand: chillers, CDUs, dry coolers, CRAC/CRAH units, modular DC.
- October 2025: Airedale launched **skid-based 400 kW – 2 MW+ CDU** — capacity envelope matches Project Deschutes spec.
- **HOWEVER:** Modine/Airedale is **NOT named** in the publicly disclosed Project Deschutes ecosystem (Boyd, CoolerMaster, Delta, Envicool, Nidec, **nVent**, Stulz, **Vertiv**) per DCD coverage and OCP marketplace listings — gap relative to VRT and NVT.
- Modine's data-center growth is more concentrated in **chillers and air-cooling for white-space/gray-space** plus CDUs for direct-to-chip — strong hyperscaler exposure but not specifically tied to TPU racks vs GPU racks.
- Feb 2025: announced largest order in company history (~$180M) — not customer-named, widely speculated to be a top-3 US hyperscaler.
- FY25 data-center segment revenue: **$644M (+119% YoY)**; FY26 tracking to >$1.1B (+70%).
- New 155,000-sqft Franklin, Wisconsin DC-cooling facility opened 2025; $100M N.A. capacity expansion announced 2025.

### 7. AI revenue split: TPU vs NVDA-GPU vs other-ASIC
- No accelerator-level disclosure. Modine reports a **Climate Solutions** segment that includes Data Centers, HVAC, refrigeration.
- Cooling product mix (chillers, CRAC, CDUs) serves any AI accelerator type — **accelerator-agnostic**.
- **ESTIMATED** Google TPU share of Modine's data-center revenue: **5–10%**, materially lower than VRT/NVT estimates because (a) Modine is not in the named Deschutes vendor list, (b) Modine's largest known footprint is chillers/dry-coolers/HVAC for the broader DC envelope which is hyperscaler-mix-weighted (MSFT ~30%, AMZN ~30%, GOOG ~15%, META ~15% of hyperscale capex).
- NVDA-GPU-attached share (servicing GB200/GB300 racks at all hyperscalers + neoclouds): **ESTIMATED 50–60%**.
- Other-ASIC: **ESTIMATED ~10%**.
- Confidence: **LOW** — all ESTIMATED; Modine discloses no customer/accelerator splits.

### 8. Customer concentration
- Per Modine FY25 10-K (filed May 2025, item 1): **"In fiscal 2025, 2024 and 2023, our largest customer accounted for less than 10% of our net sales."** No 10% customer.
- Top customers operate in data-center cooling, commercial AC, refrigeration, commercial vehicle, off-highway, automotive markets.
- Modine has not named hyperscaler customers publicly; the Feb 2025 $180M order was unattributed. Likely customers (per industry channel checks): Microsoft, Google, AWS, Meta, Oracle, plus large colos (Equinix, Digital Realty). No CoreWeave/Crusoe disclosure.

```json
{
  "ticker": "MOD",
  "ttm_revenue_usd_b": 2.87,
  "last_q_revenue_usd_b": 0.805,
  "last_q_yoy_pct": 31.0,
  "fy_rev_guide_usd_b": 3.15,
  "last_q_gm_pct_nongaap": 23.1,
  "last_q_eps_nongaap": 1.19,
  "tpu_supply_evidence": "Airedale by Modine CDU portfolio (400kW-2MW+, Oct 2025), $180M largest-ever order Feb 2025 (customer unnamed), data-center revenue +119% in FY25 to $644M and +78% in Q3 FY26. NOT in named Project Deschutes vendor ecosystem — TPU evidence weaker than VRT/NVT",
  "tpu_revenue_share_est_pct": 7,
  "nvda_revenue_share_est_pct": 55,
  "confidence": "low — all estimated; no accelerator or customer disclosures",
  "as_of_date": "2026-05-10"
}
```

---

## NVT — nVent Electric

**Latest reported fiscal Q:** Q1 2026, ended **March 31, 2026** (reported May 1, 2026). Calendar fiscal year.

### 1. TTM revenue & last-Q revenue (YoY)
- **Last-Q (Q1 2026) net sales: $1,242M**, up **+53% reported / +34% organic** YoY vs $810M Q1 2025.
- Systems Protection: $894.8M (+76% reported, +50% organic) — data-center heavy.
- Electrical Connections: $347M (+15% reported, +8% organic).
- **TTM revenue (Q2'25–Q1'26): ~$4.36B** (= FY25 $3.93B − Q1'25 $0.81B + Q1'26 $1.24B).
- FY25 full-year revenue: **$3.93B** (+30% reported / +13% organic).
- Backlog: **$2.6B** at Mar-31-2026 (vs $2.35B at Dec-31-2025; $749M at Dec-31-2024 — **+248% YoY**).
- Source: stocktitan.net NVT 10-Q; fool.com Q1 2026 transcript (2026-05-01).

### 2. Forward revenue
- **Q2 2026 guide:** organic sales +23–25%; reported growth higher.
- **FY2026 guide (raised):** **reported growth +26–28%**, organic **+21–23%**. Implies FY26 revenue **~$4.95–5.03B**.
- Street consensus FY2027: ~$5.6–5.9B (sell-side post-Q1 prints).

### 3. Earnings guidance
- **FY26 Adj diluted EPS: $4.45–4.55** (raised from $4.00–4.15 at Q4'25). Implies ~30% growth.
- **Q2 2026 Adj EPS guide: $1.12–1.15.**
- **FY26 FCF conversion: 90–95%** of adj net income.
- **FY26 capex: $130M** (+40% YoY).
- Op-margin guide: ROS ~20%+ (Q1 was 20%).
- Tariff headwind: **$80M incremental in 2026** (after $90M in 2025).

### 4. EPS — last-Q & TTM
- **Q1 2026 GAAP diluted EPS (continuing ops): $0.86.**
- **Q1 2026 Adj diluted EPS: $1.09** (+63% YoY).
- Q1 2026 net income (continuing ops): $142.4M.
- TTM Adj EPS: ~$3.94 (FY25 ~$3.27 + Q1'26 $1.09 − Q1'25 $0.67 ≈ $3.69 — rough; share count drift).

### 5. Gross margin
- **Q1 2026 GAAP gross margin: 35.9%** (down ~290 bps from 38.8% YoY — copper inflation + mix + integration costs from EPG acquisition).
- nVent does not report an adjusted gross margin separately.
- 4-Q GAAP gross-margin trend:
  - Q2 2025: ~38.0%
  - Q3 2025: ~37.5%
  - Q4 2025: **36.5%** (full-year FY25 GM 36.46% per company)
  - Q1 2026: **35.9%** (−290 bps YoY)
  - Trend: declining; **copper inflation + EPG acquisition lower-GM mix + tariffs** are the headwinds; offset by volume leverage on operating margin (ROS held at 20%).

### 6. TPU supply history
- **Strong, explicit, named.** **nVent Project Deschutes Open CDU** is a publicly listed product on the OCP marketplace (opencompute.org/products/733) — direct Google Ironwood-class TPU evidence.
- nVent named in DCD coverage of the Deschutes ecosystem (along with Boyd, CoolerMaster, Delta, Envicool, Nidec, Stulz, **Vertiv**).
- SC25 (Nov 2025): nVent unveiled new liquid cooling and power portfolio — Deschutes-compliant.
- Q1 2026 commentary (per transcript): "data center demand drove record sales, orders and backlog"; "white space growth led by liquid cooling, power distribution units, and cable management"; organic orders **+40%** in Q1 2026.
- **Rack PDU & busway exposure:** core nVent products (ERIFLEX busbars, SCHROFF cable management, Hoffman enclosures, Trachte switchgear via 2025 EPG acquisition). EPG acquisition ($979.6M, May 2025) added enclosures/switchgear/bus systems with ~85% of sales targeting power utilities/data centers/renewables.
- **Data center revenue 2025: ~$1.0B** (up from ~$600M in 2024 — +67%). Full-year 2026 tracking materially higher given +40% Q1 organic orders.

### 7. AI revenue split: TPU vs NVDA-GPU vs other-ASIC
- nVent does not disclose accelerator-level mix.
- Products (Deschutes CDU, rack PDU, busway, enclosures) are accelerator-agnostic.
- **ESTIMATED** Google TPU share of nVent AI-attach revenue: **10–15%** — direct evidence of Deschutes participation supports above-average Google exposure for thermal vendors.
- NVDA-GPU-attached share: **ESTIMATED 50–55%** (MSFT/META/AMZN/Oracle/CoreWeave GB200/GB300 racks).
- Other-ASIC: **ESTIMATED 5–10%**.
- Infrastructure is 55% of total Q1'26 revenue (up from 45% prior, 12% at 2018 spin-off) — data-center heavy infrastructure.
- Confidence: **LOW-MEDIUM** on the split (estimated); **HIGH** on directional Google participation given product listed on OCP marketplace.

### 8. Customer concentration
- Per nVent 2024 10-K (filed Feb 2025) and 2025 10-K (filed Feb 2026): **no 10%+ customer disclosed**. Concentration risk acknowledged in risk factors.
- Q3 2025 earnings call: "most of [organic order] increase came from large liquid cooling orders for hyperscaler programs"; named end-customers in industry coverage include **Microsoft, Google, Amazon, Oracle, Meta**. No CoreWeave/Crusoe-specific disclosure.

```json
{
  "ticker": "NVT",
  "ttm_revenue_usd_b": 4.36,
  "last_q_revenue_usd_b": 1.242,
  "last_q_yoy_pct": 53.5,
  "fy_rev_guide_usd_b": 4.99,
  "last_q_gm_pct_nongaap": 35.9,
  "last_q_eps_nongaap": 1.09,
  "tpu_supply_evidence": "Named OCP vendor for Google Project Deschutes Open CDU (listed on OCP marketplace product 733); Deschutes ecosystem partner per DCD; data-center revenue ~$1B in FY25 (+67% YoY); Q1'26 organic orders +40% with hyperscaler liquid-cooling programs cited; backlog $2.6B (+248% YoY)",
  "tpu_revenue_share_est_pct": 12,
  "nvda_revenue_share_est_pct": 52,
  "confidence": "low-medium on split, high on Deschutes participation",
  "as_of_date": "2026-05-10"
}
```

---

## Cross-ticker notes & caveats

- **STALE FLAG:** MOD's most recent print is Q3 FY26 (Dec-31-2025), reported Feb-05-2026 — ~94 days old; under 120-day threshold but the next data point (Q4 FY26, ending Mar-31-2026) prints May 27, 2026. VRT and NVT both prints are <30 days old.
- **TPU-vs-GPU split numbers are ESTIMATES.** No vendor in this bucket discloses revenue by accelerator. The work is largely accelerator-agnostic — a CDU, busway, or rack-PDU cools/powers an Ironwood TPU rack the same way it cools/powers a GB300 rack. The TPU share estimates flow from (a) confirmed participation in Google's Project Deschutes ecosystem (VRT, NVT — yes; MOD — no), and (b) Google's ~15% share of named-Big-4 hyperscale AI capex.
- **Project Deschutes is the strongest available TPU-supply evidence** because it is Google's open-sourced 2 MW CDU spec for Ironwood-class racks, with a publicly disclosed vendor list. **VRT and NVT both have OCP-marketplace-listed Deschutes products. Modine/Airedale does not.**
- **Backlog conversion is the VRT key narrative:** $7.0B → $12.45B → reported $15B at Dec-31-2025 → $12.45B at Mar-31-2026 (working down as Q1 revenue converts). Watch FY26 H2 ramp.
- **Customer concentration:** None of the three discloses a 10% customer. All three serve a similar hyperscaler customer set (MSFT, GOOG, AMZN, META, ORCL) plus colos and neoclouds. CoreWeave and Crusoe are likely customers but undisclosed in filings.
- **Margin pressure mostly tariff/mix/copper:** NVT GM down 290 bps YoY on copper + EPG mix; MOD GM down 120–290 bps on capacity ramp; VRT GM stable in mid-37%. None of these is a demand signal — they are input cost / integration phenomena.
