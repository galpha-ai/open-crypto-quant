# INTC vs AMD — CPU Trend Deep-Dive: Key Questions for Subagent Dispatch

## Thesis anchors (from prior discussion)
- **Secular:** CPU is a strong long trend (agents/inference workloads push per-core + memory bandwidth demand back onto CPUs).
- **Tail risk:** Taiwan / TSMC disruption is discounted, not actively priced; trade only on confirmation signals.
- **INTC microstructure:** governance weak, capable engineers have left, Chinese-American CEO under board friction, but sitting on three good narratives (Terafab / Elon angle, packaging capacity scarcity, US-gov foundry preference).
- **AMD microstructure:** stronger data-center CPU execution, mediocre but capacity-scarcity-bid GPU business.
- **Suggested base trade:** long INTC, de-risk before each earnings (narrative > print for the next several quarters); pair or overweight AMD on DC-CPU fundamentals.

The objective of the question set below is to **falsify or strengthen each anchor** with primary-source data, and to produce *pairwise* (INTC vs AMD) read-outs rather than two separate monographs.

---

## 1. Fundamentals — product, P&L, roadmap

Each question should produce a side-by-side table (INTC | AMD | delta | direction of surprise).

### 1.1 Revenue decomposition & mix
1. Decompose the last 12 quarters of revenue by reported segment for both names (INTC: CCG, DCAI, NEX, Mobileye, Altera, IFS; AMD: Client, Data Center, Gaming, Embedded). What is the trajectory of **DC-CPU revenue specifically** (strip out GPU/accelerator contribution from AMD's DC segment)?
2. Within DC, estimate **server-CPU units × ASP** for each. What fraction of the DC gap is pricing vs volume?
3. Quantify **custom-silicon cannibalization** (Graviton, Axion, Cobalt, Maia, Trainium) by hyperscaler. Express as "share of hyperscaler CPU sockets lost to in-house ARM" per year, 2022-2026e.
4. What % of AMD DC revenue is MI300/MI325/MI350 (accelerator) vs EPYC (CPU)? Isolate the CPU line to compare apples-to-apples vs Xeon.
5. Decompose **client CPU** (desktop + mobile) share at Mercury Research granularity — is AMD still taking share in premium mobile post-Strix / Strix Halo, and is Intel defending with Lunar Lake / Arrow Lake / Panther Lake?

### 1.2 Margins & capital structure
6. Rebuild INTC gross margin bridge: what % of the margin collapse is **fab underutilization** vs **product mix** vs **inventory write-downs**? Under what 18A utilization does GM normalize to >50%?
7. AMD's GM is levered to TSMC wafer pricing and mix. Sensitivity: +10% N3 wafer price → AMD GM impact? What does TSMC's 2026 price list imply?
8. Compare **FCF conversion** (FCF / net income) over 5 years. INTC's CHIPS-Act-net capex schedule vs AMD's asset-light — when (if ever) does INTC FCF inflect positive ex-grants?
9. Net-debt / EBITDA and interest coverage at INTC post Mobileye/Altera monetization. At what stress level does the dividend/credit rating come under renewed pressure?
10. Share-count trajectory: AMD buyback pace vs INTC dilution (employee equity + any converts). Net EPS tailwind/headwind from capital structure alone.

### 1.3 Roadmap execution & technology
11. **Process node race:** Intel 18A PDK 1.0 maturity, defect density, and early customer silicon (Panther Lake, Clearwater Forest) vs TSMC N2 HVM timing. Who has the superior *usable* leading-edge node in 2026 and 2027?
12. **Packaging:** Intel Foveros / Foveros Direct / EMIB vs TSMC CoWoS-L / SoIC. Throughput, yield, and design-win comparison for 2026 AI servers. Can Intel sell packaging as a service credibly?
13. **Server CPU SKU-level benchmarks:** Granite Rapids (128P, 256P) vs Turin (Zen 5 EPYC 9005, up to 192c) on (a) SPECint rate, (b) LLM inference tokens/sec/W for 7B-70B models, (c) vector DB workloads, (d) memory-bandwidth-bound analytics. Where does each *actually* win?
14. **AI-on-CPU differentiators:** AMX (Intel) vs AVX-512 + VNNI (AMD) for small-batch inference and agent workloads. Which ISA capability is being adopted by Llama.cpp, vLLM, TensorRT-LLM CPU paths, and ONNX Runtime?
15. **Foundry external wins:** list every publicly confirmed IFS customer, node, volume commitment, and revenue ramp year. Distinguish "MOU/LOI" from "tape-out" from "HVM." How much is Microsoft/AWS/DoD actually worth in 2026-2028 revenue?
16. **R&D productivity:** R&D $/transistor-shipped and R&D / (rev × GM%) trends. Is INTC out-spending to catch up, or structurally wasteful?

### 1.4 Historical patterns for trade design
17. **Earnings drift study:** For the last 40 earnings prints (INTC + AMD), measure T-5 to T-1 day drift, overnight gap, and T+1 to T+5 drift. Is there a statistically significant "sell before INTC earnings" edge after controlling for SOXX beta? Same test on AMD.
18. **Narrative vs print divergence:** For each INTC print since 2022, tag the pre-print narrative (positive / neutral / negative) from sell-side notes and news sentiment and measure realized move. Is the "narrative good, print bad" pattern real and persistent?
19. **Pair-trade history:** INTC/AMD ratio z-score over 10 years, with regime labels (2017 Zen launch, 2020 7nm miss, 2022 Pat era, 2024 transition). What drawdown would a naive long-INTC/short-AMD pair have endured, and what is the current percentile?
20. **Options-implied vs realized** earnings move for both names last 12 quarters. Is the straddle rich enough to express the "sell before earnings" view via puts / put spreads / calendar rather than stock?

---

## 2. Institutional flows — SEC filings & position-flow primary sources

### 2.1 13F (long-only and hedge funds)
21. Pull last 8 quarters of 13Fs; rank holders by $ change for INTC and AMD separately. Flag **directional divergence** (funds buying AMD and selling INTC, or vice versa) — these are the clearest "signal" flows.
22. **Smart-money cohort:** Baillie Gifford, Capital Group, Jennison, Primecap, T. Rowe, Fidelity Contrafund, Viking, Tiger Global, Coatue, Lone Pine, Maverick, D1, Light Street, Whale Rock, Altimeter. Who owns, who trimmed, who initiated?
23. **Concentration:** count of funds with INTC as top-10 holding vs same for AMD, last 5 years. Has INTC been de-institutionalized into a retail/passive name?
24. **Activist stakes (13D/13G):** any >5% filer in INTC? History of Third Point, Elliott, Starboard involvement and any renewed chatter. Any activist precedent for forcing a foundry spin (IFS separation)?
25. Berkshire / Scion / Burry holdings check — any semis exposure, since both have historically commented on the space.

### 2.2 Insider transactions (Form 4)
26. All Form 4s for INTC NEOs and directors since the new CEO took over: buys vs sells, 10b5-1 plan adoption dates, and aggregate net $. Particularly scrutinize **director open-market buys** — rarer and higher-signal than exec sales.
27. Same for AMD: Lisa Su, CFO, Rick Bergman etc. Distinguish 10b5-1 scheduled sales from discretionary. Compute net insider $ / market cap vs sector.
28. Compare the **ratio of insider-buy-days to insider-sell-days** across both tickers last 24 months — a classic but underused signal.
29. Any 10b5-1 plan amendments (especially shortening cooling-off) that could indicate information asymmetry.

### 2.3 Congressional & government trades
30. STOCK Act periodic transaction reports: identify every member of Congress who traded INTC or AMD in the last 24 months. Weight by committee (House Armed Services, Senate Intel, Commerce, Science) — those are the information-adjacent committees for CHIPS Act and export-control flows.
31. Nancy Pelosi / Paul Pelosi specifically — any semis positions or dated trades around CHIPS disbursement milestones.
32. Federal official / agency trades disclosed under STOCK Act or OGE 278 — any Commerce, DoD, or NSC officials with positions.
33. Cross-reference trade dates against public CHIPS Act announcement timestamps to detect pre-announcement accumulation.

### 2.4 Short interest, options, and derivatives flow
34. Short-interest %float and days-to-cover trend for both names; compare to 5-year distribution. Is the INTC short base "tired"?
35. Large-block options prints (>$5M premium) from UW/cboe feeds for last 6 months; categorize as directional bullish/bearish and tag around catalyst dates (earnings, CHIPS milestones, 18A tape-outs).
36. Gamma positioning: dealer gamma profile around current spot for both names; identify gamma-flip levels that would amplify moves around earnings.
37. Skew and term structure: is the INTC put skew steeper than AMD's, consistent with tail-risk pricing?
38. ETF flow attribution: SOXX / SMH / XSD creations/redemptions and their pro-rata impact on INTC and AMD. Any idiosyncratic pressure vs index beta?

### 2.5 Credit & cross-asset
39. INTC senior unsecured bond OAS and CDS spread history vs IG index and vs AMD (AMD has minimal CDS so use bond proxy). Is credit signaling equity-market distress that equity hasn't priced?
40. 5y CDS for TSMC and for INTC — crossover points are informative about perceived foundry risk transfer.

### 2.6 Hedge fund commentary & letters
41. Scrape 13F-filing hedge-fund quarterly letters for mentions of INTC or AMD since 2023. Tag thesis (long foundry optionality / short execution / pair trade). Who has the cleanest articulated variant of our thesis?
42. Ark / Ark Invest and similar high-turnover disclosed funds — any position additions that could indicate crowded-side of our trade.

---

## 3. Supply chain — from wafer to server rack

### 3.1 Foundry & wafer supply
43. TSMC capacity allocation by node for 2025 and 2026: what % of N4/N3/N2 is booked by AMD vs Apple vs NVDA vs MediaTek vs Qualcomm? AMD's growth-capacity-constrained scenario?
44. Under a TSMC Taiwan disruption scenario (3-month, 12-month), what is AMD's near-term revenue-at-risk and what fraction of that is recoverable via Samsung / TSMC-Arizona / Intel Foundry? Quantify **substitutability half-life.**
45. Intel 18A ramp: per-quarter wafer-out plan, current yield estimates (public + channel checks), and first external tape-outs. What wafer volume must IFS hit for utilization-driven GM recovery?
46. High-NA EUV (ASML EXE:5000) deliveries and allocation. INTC received first tools — what real production advantage does this confer on Intel 14A vs TSMC A16?
47. Samsung Foundry as AMD plan-B: node readiness, yield, and whether AMD has any active tape-outs there. Rapidus (Japan) timeline credibility.

### 3.2 Packaging and advanced assembly
48. CoWoS-L and CoWoS-S capacity quarterly (TSMC + OSAT expansion): who gets wafers (NVDA first, then AMD, then everyone else). AMD MI350/MI400 allocation and delivery risk.
49. Substrate bottleneck (ABF substrates — Ibiden, Unimicron, Shinko, Semco): capacity for 2026 AI SKUs. Does this favor whoever has on-shore packaging (Intel Foveros / EMIB)?
50. HBM3E / HBM4 supply split (SK Hynix, Samsung, Micron) and customer allocation. What is AMD's HBM secured for 2026?
51. Intel's packaging-as-a-service pitch — any external customers committed (Broadcom MEDIA, MSFT, AWS)? Credible revenue line or vaporware?

### 3.3 Server channel and demand signal
52. ODM order books (Supermicro, Quanta, Wistron, Inventec, Foxconn Industrial Internet, Wiwynn): AMD EPYC vs Intel Xeon SKU counts in quoted build plans for 1H2026.
53. OEM platform coverage: Dell PowerEdge, HPE ProLiant, Lenovo ThinkSystem, Cisco UCS — next-gen platform split between Turin and Granite Rapids. How does this compare to prior-gen launch month at matched age?
54. Hyperscaler capex and its **CPU vs GPU vs custom-ASIC split** (MSFT, META, GOOGL, AMZN, ORCL). CPU socket count trajectory across the top 5 — *is the CPU trend actually strong* or is the money going to accelerators?
55. Enterprise refresh cycle indicators: Windows-Server 2016 EOL, VMware by Broadcom repricing refugees, legacy Xeon replacements. Net CPU-socket TAM delta 2025-2027.
56. Edge / telco (open-RAN, 5G core) CPU socket demand and INTC's incumbent position vs AMD / ARM inroads.

### 3.4 Geopolitics, exports, and policy
57. China revenue exposure for each (last 4 Qs), net of export-control restrictions. What fraction is low-end / not restricted, and what fraction is at risk under further BIS rules?
58. CHIPS Act: grant milestones, clawback conditions, and how much cash INTC has actually drawn vs announced. Delta in the FY capex schedule.
59. EU Chips Act and Magdeburg fab cancellation: recovered cash, lost strategic optionality, and what this implies about INTC's real foundry ambition.
60. Japan Rapidus and India Micron/Tata fabs — do they meaningfully diversify the supply chain by 2028, and which design wins are landing there?
61. Mobileye and Altera monetization: progress, net proceeds, and impact on balance sheet and discounted-foundry-capex math.

### 3.5 Second-order: talent, patents, ecosystem
62. LinkedIn flow analysis: net eng talent migration between INTC ↔ AMD ↔ NVDA ↔ hyperscalers over last 24 months. Weight by seniority (director+, distinguished engineer). Is the INTC bench actually hollowing out as hypothesized?
63. Patent issuance & citation velocity in CPU / packaging / foundry domains, INTC vs AMD, last 5 years.
64. Software ecosystem: oneAPI adoption vs ROCm vs open-source CPU backends (llama.cpp CPU path, vLLM CPU). Does Intel's software lead translate to CPU-AI inference share in 2026?

---

## 4. Subagent dispatch plan (how to parallelize)

Proposed allocation — each bullet = one subagent brief. Each brief must return (a) a data table, (b) a 5-bullet take, (c) the single *highest-information* follow-up question.

- **Agent A — Fundamentals (P&L and product):** questions 1-10, 13-14.
- **Agent B — Roadmap & technology:** questions 11-12, 15-16, 62-64.
- **Agent C — Trade-design priors:** questions 17-20, 35-37.
- **Agent D — 13F / activist flows:** questions 21-25, 41-42.
- **Agent E — Insider & congressional flows:** questions 26-33.
- **Agent F — Derivatives & credit:** questions 34, 38-40.
- **Agent G — Foundry & wafer supply chain:** questions 43-47.
- **Agent H — Packaging, HBM, substrates:** questions 48-51.
- **Agent I — Server channel & demand:** questions 52-56.
- **Agent J — Geopolitics & policy:** questions 57-61.

Synthesizer pass after all 10 agents return: build the **INTC-vs-AMD comparative dashboard** and a **weekly update template** that can be re-run automatically.

---

## 5. Output contract for every subagent

Each agent must produce:
1. **Primary-source citations only** (10-Q/K, 8-K, S-1, 13F, Form 4, STOCK Act PTR, FR notices, TSMC/ASML transcripts, Mercury Research, company investor-day decks). No sell-side summaries as the load-bearing source.
2. **Date-stamped** data points; flag any figure older than 90 days as stale.
3. **Disagreement log** — where agent's read diverges from the thesis anchors at the top of this doc.
4. A **machine-readable JSON block** at the bottom with the key numeric deltas (for downstream dashboarding).

---

## 6. Open meta-question

Is the binding constraint on alpha here **data access** (primary-source, timely) or **framing** (asking the right pairwise question)? Prior says framing — most retail INTC/AMD takes compare two separate monographs rather than *pairwise* deltas. The question list above is built to force pairwise comparison at every step.
