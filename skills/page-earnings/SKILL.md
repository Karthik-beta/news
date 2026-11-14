---
name: page-earnings
description: "Rules for writing an earnings page (quarterly or annual company results) from a research brief built from filings and XBRL data. Load with news-brand when the brief's page type is earnings."
---

# Earnings pages

Draft, to be finalised in Phase 4 (content system design). Use with `news-brand`.

## What the page answers
How much did the company sell and earn in the period, what changed from a year earlier, which segments and regions drove it, and anything the filing itself flags (one-off items, guidance, restatements).

## Sources
Primary only for figures: the filing (10-K, 10-Q, 20-F, annual report, ESEF, EDINET, DART, BSE/HKEX filings) and the company's results press release. Earnings-call webcasts are linked, not embedded, unless the company embeds them on a public page.

## Rules specific to earnings
- **Periods as the company labels them**: "fiscal 2024", "fourth quarter of fiscal 2024", "Q2 FY25". If the fiscal year isn't the calendar year, say once when it ends.
- **Compare like with like**: year-on-year against the same fiscal period. Quarter-on-quarter only when the company reports it that way.
- **Currency and scale as reported**; "as reported" tables keep the filing's scale (US$ millions).
- **Non-GAAP figures** only if the filing or release states them, labelled as the company labels them ("adjusted operating profit"), and never in place of the reported figure.
- **Guidance** is the company's, quoted or tabled as published, credited ("Company guidance"). Never our own expectation; never "beat" or "missed" unless comparing to the company's own guidance.
- **No market prices, share-price moves, analyst estimates or ratings.**
- **Restatements**: show the restated prior figure and say it was restated.

## Typical layout (Tier 1)
KeyFigures (revenue, the segment of note, margin, net income) → Prose → Waterfall "Of every $100 of sales…" (Sankey on Tier 1 if segments matter) → Prose → Bars by segment with change → Line of margins over 5 years → Table by region (as reported) → Note on one-off items, as the filing explains them → Video only if the company embedded one.

Tier 2: KeyFigures → one chart (segments or the trend that matters) → short Prose → Table as reported.

## Headline patterns
- "Services reached a quarter of Apple's sales in fiscal 2024"
- "TSMC's third-quarter revenue rose 39% from a year earlier"
Avoid: "crushes", "beats", "disappoints", "investors".
