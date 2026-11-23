---
name: page-science
description: "Rules for a science page from papers, preprints, agency releases and open datasets (arXiv, OpenAlex, PubMed, bioRxiv, NASA, ESA, NOAA, CERN and similar). Loaded with news-brand and news-reporter when the brief's page type is science."
---

# Science pages

## What the page answers
What was found, how it was found, how strong the evidence is as the authors describe it, and where to read the source.

## Rules
- **Source type is visible**: peer-reviewed paper, preprint, agency release or dataset, as the brief gives it. A preprint says "not yet peer-reviewed" in the summary too.
- **Findings as the authors state them**, with their stated limitations. No overstating and no extrapolation beyond the study.
- **Method**: design, sample, period and limitations only as the brief gives them; leave out what the source does not say.
- **Units as published**, with uncertainty ranges where the brief has them.
- **No advice to readers.** Code adds "Not medical advice." where it applies.
- **Images**: `ImageFigure` only from the brief's `media`, never decorative.

## Typical layout
`Finding` (one per main finding) → `MethodSummary` → a chart from the paper's or dataset's data → `ImageFigure` from the agency or paper → `Prose` on context as the authors describe it.

## Headline patterns
- "Carbon dioxide in the atmosphere reached {dp_co2_annual} in {pd_co2_year}, {cx_co2_record}"
- For preprints: "A preprint, not yet peer-reviewed, reports that <finding> in <sample>"
