---
name: page-policy-decision
description: "Rules for a policy decision page (central-bank rate decisions and major government economic policy decisions) from a brief built from official statements, data and press-conference material. Loaded with news-brand and news-reporter when the brief's page type is policy decision."
---

# Policy decision pages

## What the page answers
What was decided, by how much, by what vote, how it compares with recent decisions, and how the institution explained it in its own words.

## Rules
- **The decision in words and refs**: "held at {dp_…}", "cut by {dv_…} to {dp_…}". Name the rate as the institution does (policy repo rate, deposit facility rate, federal funds target range, cash rate).
- **Vote and stance** only as the brief states them.
- **Quotes** from the statement or press conference only, as `Quote` refs.
- **No predictions** of the next move, however soft. No market expectations.
- **Rate paths** as a `StepLineChart` with decision markers, the latest decision highlighted.
- **Video**: `VideoEmbed` only for the official press conference in the brief's `media`.

## Typical layout
`PolicyDecision` (rate, change, vote, next meeting, path) → `Quote` from the statement → `StepLineChart` of the rate path → `Prose` explaining the decision as the institution did → `Timeline` of recent decisions → `VideoEmbed` of the press conference.

## Headline patterns
- "The RBI held its repo rate at {dp_rbi_repo} and changed its stance to neutral"
- "The ECB cut its deposit rate by {dv_ecb_dfr_change} to {dp_ecb_dfr}"
