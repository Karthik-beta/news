---
name: news-review
description: "The Opus 5.5 review checklist for any branch or pull request in the News repo, covering correctness, data and number fidelity, design-system compliance, accessibility, security, free-tier cost and tests, with a fixed findings format. Use this whenever you review a sub-agent's branch, a PR, a diff, or are asked to check work before merge, even if the request just says \"look this over\"."
---

# Reviewing News work

You are the reviewer between a builder and the PR. CodeRabbit reviews afterwards with a light profile; you are the one who catches what matters for this product: wrong numbers, broken trust signals, off-brand UI, unsafe endpoints and missing tests. Be specific and brief. Don't rewrite the work; report.

Review against the brief the builder was given, the Development Plan item, and the relevant documents in `design/` or the Technical Design. Run the tests yourself rather than trusting the builder's summary.

## Checklist (apply the sections that the diff touches)

**Correctness**
- Does it do what the brief says, including edge cases and error paths?
- Idempotent where the CLI requires it; resumable where runs can stop.

**Numbers and sources (the product's trust)**
- No currency, unit or period conversion; values exactly as the source states them.
- Every displayed figure resolves to a data ID and a source record; nothing typed in by a model.
- Formatting follows `design/content-style.md` (true minus, bn/m/trn, dates, fiscal labels).
- Licences and attribution flow from the source registry; outlet text and images never copied.

**Design system** (load `news-design-system`)
- Tokens only; no literal colours, font names or px font sizes.
- Crimson only for brand/emphasis; never for decreases; no red/green for up/down.
- One highlight per chart; chart anatomy complete (rule and tag, title, subtitle, source, data table).
- Works at 360px and in dark mode (look at the screenshots).

**Accessibility**
- Semantic markup, names, keyboard, focus, no colour-only meaning, charts with accessible summaries.

**Security and privacy**
- No secrets in code, fixtures or logs. Write API checks its key and validates input. Public endpoints read-only.
- No tracking, no cookies, YouTube only via click-to-load `youtube-nocookie.com`, user input stored only as typed.

**Cost and limits**
- No heavy CPU in Workers; caching set; DynamoDB writes paced; nothing calls an AI service from the cloud.

**Tests**
- New behaviour has tests that would fail without it. Components: an Opus tester's `TESTING.md` sign-off exists and covers all areas and all screens.

## Findings format

Return exactly this, most severe first:

```
Verdict: approve | changes needed | blocked
Findings:
1. [blocker|major|minor] path:line — what is wrong — why it matters — the fix
2. …
Tests run: <commands and results>
Not checked: <anything you couldn't verify, and why>
```

No praise, no restating the diff. If there are no findings, say "Verdict: approve" and list the tests you ran.
