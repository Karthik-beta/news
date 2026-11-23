---
name: page-macro-release
description: "Rules for a macro release page (inflation, GDP, jobs, trade, retail sales and other official statistics) from a brief built from statistics-agency and central-bank data. Loaded with news-brand and news-reporter when the brief's page type is macro release."
---

# Macro release pages

## What the page answers
What the latest figure is, how it compares with the previous period and a year earlier, which components moved it, and how it fits the last few years.

## Rules
- **Name the measure as the agency does**, in words: "consumer prices, change from a year earlier, not seasonally adjusted". Seasonal adjustment and base go in the subtitle.
- **Annual or monthly change**: say which. Never mix them in one chart.
- **Revisions**: when the brief marks earlier figures as revised, say so in one short line.
- **Units and scale as published** (index points, %, persons in thousands, local currency).
- **No forecasts of our own.** Agency or central-bank projections only as the brief gives them, credited.
- **Credit the publisher**, not the data route: BLS, not FRED.
- **Country in the fly title** ("America's inflation", "China's economy").

## Typical layout
`KeyFigures` (headline rate, the second measure, monthly change) → `LineChart` of the headline rate over several years, latest period highlighted → `BarChart` of component changes or contributions → `Prose` → `DataTable` of components as published.

Regional comparisons: `BarChart`, `ChoroplethMap` only when geography is the point, always with a `DataTable`.

## Headline patterns
- "Annual consumer-price inflation fell to {dp_us_cpi_yoy} in {pd_us_cpi_latest}"
- "China's GDP grew {dp_cn_gdp_yoy} from a year earlier in {pd_cn_gdp_q3}"
