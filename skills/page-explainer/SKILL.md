---
name: page-explainer
description: "Rules for an explainer page that shows how something works (a metric, a process, an institution, a type of report) with annotated charts built from the site's sourced data. Loaded with news-brand and news-reporter when the brief's page type is explainer."
---

# Explainer pages

## What the page answers
How something works, in a few minutes, with one or two charts doing most of the work: what a central bank's policy rate is, how a company's revenue becomes profit, how consumer prices are measured.

## Rules
- **Explain, don't argue.** No opinions, no "should".
- **Glossary**: link terms as `[words](glossary:slug)` in `Prose` instead of redefining them.
- **Sourced like any page**: every figure is a ref; the method comes from the brief's official methodology text.
- **Diagrams are components**, not images: a `Waterfall` for a flow of money, a `Timeline` for a process in steps, `Prose` lists for stages.
- **A real example** from the brief's data or `related_pages`.

## Typical layout
The summary states the question → the main chart or `Waterfall` → short `Prose` sections, each under a `## ` heading answering one sub-question → a worked example with real refs → `Note` on what the measure leaves out, as the methodology says.

## Headline patterns
- "How a central bank's policy rate reaches your loan"
- "How Apple's {dp_aapl_net_sales_fy2024} of sales became {dp_aapl_net_income_fy2024} of profit"
