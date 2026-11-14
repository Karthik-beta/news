---
name: news-brand
description: "Brand and layout rules for writing News pages in OpenUI Lang. Load before choosing components or writing any page text."
---

# News brand rules (for page-writing agents)

Keep this file short: it sits in the cached prompt prefix of every page call. The full design documents are in `design/`; this is the part an agent needs while writing a page. Page-type skills in `skills/page-*` add the rules for each kind of page.

## Voice
- Neutral, precise, plain. Report the source; never predict, recommend, rate or judge.
- Plain verbs: rose, fell, grew, held, cut. No hype words (soared, plunged, massive, stunning).
- British spelling; official names keep their own spelling.
- Never write a number yourself. Reference data IDs; code fills in values.

## Every page has
1. **Fly title**: 2–4 words naming the subject, sentence case ("Apple's annual results").
2. **Headline**: one sentence stating the main finding, ≤ 80 characters, no full stop, no question, no pun.
3. **Standfirst**: 1–2 sentences, ≤ 200 characters, adding the 2–3 numbers behind the headline. Doesn't repeat the headline.
4. The fixed base is added by code: byline, "Data as of", categories, sources panel, disclaimers, downloads. Don't write them.

## Choosing components
- Start with what the data is doing; see the form table below. The answer is sometimes **key figures, not a chart**.
- Lead with `KeyFigures` (2–4) when the page has headline numbers.
- Then alternate: a short `Prose` passage (1–3 paragraphs), a `ChartFigure`, prose, a chart… Balanced density: no more than two charts in a row without text between them.
- Use a `Note` only for context in your own words (why something happened, as the source explains it). At most two per page. Facts never go in notes; explanations never go in prose.
- Use `Quote` only for short primary-source quotes with attribution.
- Images only when they carry information and come from the allow-list. Video only when the source itself has it.
- Typical lengths: Tier 1, 4–7 visual components; Tier 2, 2–4.

| Data's job | Component / form |
|---|---|
| A few headline numbers | `KeyFigures` |
| Change over time | Line (continuous) or columns (periods) |
| Ranking | Sorted horizontal bars |
| Revenue to profit | Waterfall (Sankey for Tier 1 on wide screens) |
| Part of a whole (≤ 6 parts) | Segmented or stacked bar. Never a pie |
| Before vs after, guidance vs actual | Dot plot or `ComparisonTable` |
| Rate decisions | `PolicyDecision` + step line |
| Geography | Ranked bars; a map only when geography is the point |
| Dated events | `Timeline` |
| Published forecasts | Table, as published, credited to the forecaster |
| Science results | `Finding` + `MethodSummary` |

## Every chart has
- **Title = the takeaway**, as a sentence, true from the chart alone ("Services was the only large category to grow by double digits").
- **Subtitle = what, when, unit** ("Net sales by category, fiscal 2024, US$ billions").
- **One highlight at most.** Set `highlight` to the series, bar or value the title is about; everything else is context. Use the categorical palette only when every series matters equally.
- Bars start at zero. One y-axis. No pies, donuts, 3D, dual axes or gauges.
- Direct labels for up to 4 series. Value labels only where they matter.

## Colour rules (you choose roles, never colours)
- Crimson = brand and highlight only. It **never** means down, loss or bad.
- Never encode up/down or good/bad with colour. Write changes in words ("Up 2.0%", "Down 3.4%", "Unchanged") or signed figures (+12.9%, −7.1%).

## Numbers, units, dates
- Exactly as the source: original currency, units and fiscal-period labels. No conversion.
- Formats: 167,045; $391.0bn; 2.4%; "up 2.1 points"; "cut by 25 basis points"; −5.7% with a true minus.
- Dates: 1 November 2024. Fiscal periods as the company says them ("fourth quarter of fiscal 2024").
- Precision: never more than the source gives.

## Don't
- Don't invent context, motives or predictions.
- Don't copy outlet text; link outlets as context only.
- Don't use ALL CAPS labels, emoji, exclamation marks or rhetorical questions.
- Don't add decorative images, icons or extra headings.

## Example outlines (not templates)

**Earnings, Tier 1**: KeyFigures (net sales, segment of note, margin, net income) → Prose → Waterfall or Sankey "Of every $100 of sales…" → Prose → Bars by segment → Line of margins → Table by region (as reported) → Note (one-off items explained by the filing) → Video if the company embedded one.

**Macro release (CPI)**: KeyFigures (headline rate, core rate, monthly change) → Line of the 12-month rate over 3–5 years, latest highlighted → Bars of components' contributions → Prose → Table of components as published.

**Policy decision**: PolicyDecision → Quote from the statement → Step line of the rate path → Prose → Timeline of recent decisions → Video of the press conference (click-to-load) if official.

**Major report (IMF WEO)**: KeyFigures (global growth projections as published) → Table of projections by region, credited → Bars of the largest revisions → Prose per chapter (short) → Note on what changed since the last edition, as the report states it.

**Science**: Finding(s) → MethodSummary → Chart from the paper's data → Image from the agency or paper, credited → Prose on context as the authors state it → Badge for preprint vs peer-reviewed.
