---
name: page-earnings
description: "Rules for an earnings page (quarterly or annual company results) from a brief built from filings and XBRL data. Loaded with news-brand and news-reporter when the brief's page type is earnings."
---

# Earnings pages

## What the page answers
How much the company sold and earned in the period, what changed from a year earlier, which segments and regions drove it, and anything the filing itself flags (one-off items, guidance, restatements).

## Rules
- **Periods as the company labels them**, always through `{pd_…}` refs. Don't explain when the fiscal year ends; code adds the brief's fiscal note.
- **Like with like**: year-on-year against the same fiscal period, through the brief's `dv_` changes. Quarter-on-quarter only when the brief has that change.
- **Currency and scale as reported.** Subtitles name the filing's unit ("US$ millions, as reported").
- **Non-GAAP figures** only when the brief has them, labelled as the company labels them ("adjusted operating profit"), never in place of the reported figure.
- **Guidance** is the company's, credited ("Company guidance"). Compare with it only when the brief has both guidance and actual, in words ("above the company's guidance"). Never our own expectation.
- **No share prices, market reactions, analyst estimates or ratings.**
- **Restatements**: when the brief marks a prior figure as restated, use it and say it was restated.

## Typical layout
- Tier 1: `KeyFigures` (revenue, the segment of note, margin, net income) → `Prose` → `Waterfall` of revenue to profit (`Sankey` if segments matter) → `Prose` → `BarChart` by segment → `LineChart` of margins → `DataTable` by region, as reported → `Note` on one-off items as the filing explains them → `VideoEmbed` only if the brief has the company's own video.
- Tier 2: `KeyFigures` → one chart (segments, or the trend that matters) → short `Prose` → `DataTable`, as reported.

## Headline patterns
- "Services brought in {dv_aapl_services_share_fy2024} of Apple's sales in {pd_fy2024}"
- "TSMC's revenue rose {dv_tsmc_revenue_yoy_q3} from a year earlier in {pd_tsmc_q3}"
