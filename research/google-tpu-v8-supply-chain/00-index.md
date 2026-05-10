# Google TPU v8 ("Ironwood"-era) supply chain — financials & TPU/GPU exposure

Source: TechNews 科技新報 supply-chain map (image, May 2026), 22 tickers across 6 functional buckets. Cross-checked context: this set names component / interconnect / power / thermal / packaging / EMS / PCB suppliers credibly tied to Google's 8th-gen TPU rack-scale build-out.

## Required deliverable per ticker

For each ticker pull, from primary sources where possible (10-K/Q, 8-K, IR transcripts, press releases) as of the latest available period (target Q1 FY2026 prints, late-Apr / early-May 2026):

1. **TTM revenue** and **last-Q revenue** + YoY growth.
2. **Forward revenue** — company guidance for next-Q and FY, and consensus FY+1 if available.
3. **Earnings guidance** — most recent EPS / op-margin / FCF guidance ranges as published.
4. **EPS** — last-Q GAAP and non-GAAP, TTM.
5. **Gross margin** — last-Q GAAP and non-GAAP, trend over last 4 Qs.
6. **Prior-TPU supply relationship** — did this company supply TPU v4/v5e/v5p/v6 (Trillium)? Cite product / press-release / channel-check evidence.
7. **AI revenue split: TPU vs NVDA-GPU vs other-ASIC** — last reported quarter, with confidence band. Most companies don't disclose; mark estimates and method clearly.
8. **Disclosed customer concentration** — Google as % of revenue if disclosed; same for NVDA, MSFT, META, AMZN, AMD, etc.

## Subagent assignment

| Agent | Bucket | Tickers |
|-------|--------|---------|
| A | Optical & high-speed interconnect chips | COHR, AAOI, LITE, MRVL, CRDO |
| B | Server power supply | MPWR, AEIS, VICR |
| C | Liquid cooling & thermal | VRT, MOD, NVT |
| D | Design / foundry / advanced packaging | AVGO, TSM, AMKR |
| E | EMS, test & probe | CLS, JBL, FLEX, FORM |
| F | High-density interconnect & PCB | APH, TEL, GLW, TTMI |

Each agent writes `research/google-tpu-v8-supply-chain/<bucket>-<letter>.md` with one section per ticker following the deliverable template.

## Output rules
- Cite a source URL or filing for every number.
- Date-stamp every figure; flag any >120 days old as STALE.
- Distinguish DISCLOSED vs ESTIMATED for the TPU/GPU split.
- Do not pad with sell-side narrative summaries; if a primary number is unavailable, say so.
