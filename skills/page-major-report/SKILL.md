---
name: page-major-report
description: "Rules for a major report page (IMF, World Bank, OECD, BIS, WTO, WHO and similar flagship institutional reports) from a brief. Loaded with news-brand and news-reporter when the brief's page type is major report."
---

# Major report pages

## What the page answers
The report's headline figures, what changed since the previous edition, and what each main chapter says, in minutes instead of a long PDF.

## Rules
- **Forecasts belong to the institution.** Credit them in titles and subtitles ("The IMF projects …", "IMF projection"). Forecast grids go in a `DataTable`.
- **Edition** in the subtitle: the report's name with a `{pd_…}` ref ("World Economic Outlook, {pd_weo_edition}").
- **Revisions since the last edition** are often the news: use the brief's `dv_` revisions as the report states them, credited.
- **Chapter summaries are short**: two to four sentences each in `Prose`, under a `## ` heading with the chapter's title, in the report's own terms.
- **Quotes** only from the report or official statements, as `Quote` refs.

## Typical layout
`KeyFigures` (headline projections) → `DataTable` of projections by region or economy, as published → `BarChart` or `DotPlot` of the largest revisions → `Prose`, one short section per main chapter → `Note` on what changed since the previous edition, as the report states it → `VideoEmbed` of the official launch if the brief has it.

## Headline patterns
- "The IMF projects global growth of {dp_weo_world_growth} in {pd_weo_year}"
- "The World Bank cut its growth forecast for low-income countries in {pd_gep_edition}"
