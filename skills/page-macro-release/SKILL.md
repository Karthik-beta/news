---
name: page-macro-release
description: "Rules for writing a macro release page (inflation, GDP, jobs, trade, retail sales and other official statistics) from a research brief built from statistics-agency and central-bank data. Load with news-brand when the brief's page type is macro release."
---

# Macro release pages

Draft, to be finalised in Phase 4. Use with `news-brand`.

## What the page answers
What the latest figure is, how it compares with the previous period and a year earlier, which components moved it, and how it fits the last few years.

## Sources
The statistics agency or central bank that publishes the series (BLS, BEA, ONS, Eurostat, StatCan, MOSPI, NBS, ABS, e-Stat…) and its data API. FRED, ECB Data Portal and similar aggregators are fine as the data route, but credit the original publisher.

## Rules specific to macro releases
- **Name the measure exactly** as the agency does ("CPI-U, 12-month change, not seasonally adjusted"). State seasonal adjustment and base year in the subtitle.
- **Annual vs monthly change**: say which. Never mix them in one chart.
- **Revisions**: if the release revised earlier figures, use the revised values and say so in a short line.
- **Units and scale as published** (index points, %, persons in thousands, local currency).
- **No forecasts of our own.** Central-bank or agency projections may appear only as published and credited.
- **Country names in fly titles** ("America's inflation", "China's economy"); the agency's name in the source line.

## Typical layout
KeyFigures (headline rate, core or the second measure, monthly change) → Line of the headline over 3–5 years with the latest point highlighted → Bars of component contributions or changes → Prose → Table of components as published.

Regional comparison pages: ranked bars (map only when geography is the point), always with the table.

## Headline patterns
- "Annual consumer-price inflation fell to 2.4% in September"
- "China's GDP grew 4.6% in the third quarter from a year earlier"
