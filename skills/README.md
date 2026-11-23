# Runtime skills (page writers)

These skills are for the **production pipeline**: the `claude -p` calls (Sonnet 5.5, Haiku 5.5) that write and check pages. They are deliberately **not** in `.claude/skills/`, so they don't load into development sessions, and development skills don't load into page calls.

| Skill | Step | Loaded when | Output | Size (≈ tokens) |
|---|---|---|---|---|
| [news-reporter](news-reporter/SKILL.md) | A1, A2 | Every page call, first in the cached prefix: write, update after a direction flip, repair | `page-draft.v1` | 1,070 |
| [news-brand](news-brand/SKILL.md) | A1, A2 | Every page call (cached prefix): voice, refs, banned words, components | — | 1,540 |
| [page-earnings](page-earnings/SKILL.md) | A1, A2 | Earnings pages | — | 540 |
| [page-macro-release](page-macro-release/SKILL.md) | A1, A2 | Macro releases | — | 440 |
| [page-major-report](page-major-report/SKILL.md) | A1, A2 | Institutional reports | — | 410 |
| [page-policy-decision](page-policy-decision/SKILL.md) | A1, A2 | Central-bank and policy decisions | — | 400 |
| [page-news-story](page-news-story/SKILL.md) | A1, A2 | News stories | — | 370 |
| [page-science](page-science/SKILL.md) | A1, A2 | Science pages | — | 380 |
| [page-explainer](page-explainer/SKILL.md) | A1, A2 | Explainers | — | 370 |
| [news-classify](news-classify/SKILL.md) | A3 | Haiku, free-text items the rules table cannot classify | `a3.classify.v1` | 640 |
| [news-story-match](news-story-match/SKILL.md) | A4 | Haiku, grey-zone story pairs; separate when unsure | `a4.storymatch.v1` | 560 |
| [news-condense](news-condense/SKILL.md) | A5 | Haiku, when a brief's source text is over budget; extractive, digit-free | `a5.condense.v1` | 570 |
| [news-spot-check](news-spot-check/SKILL.md) | A7 | Haiku comparison of a page with its brief, on a sample of pages | `a7.spotcheck.v1` | 710 |
| [news-home-curation](news-home-curation/SKILL.md) | A8 | The Frontpage curator (every 2–3 days): a draft Karthik approves in Board | Draft curation (lead, side, sections, rationale) | 970 |
| [news-rollcall](news-rollcall/SKILL.md) | A11 | Rollcall's proposals: a draft Karthik approves in Board | `{"proposals": […]}` (Bureau §7.1) | 680 |
| A9: triage a reader error report (name to be chosen) | A9 | Haiku batches in the Errata workflow | `a9.triage.v1` | Planned: #21 |
| A6 entity shortlist, A10 alt text | A6, A10 | Only if switched on; off at launch (Bureau design, decision D12) | — | No skill |

A page call's prefix is `news-reporter` + `news-brand` + one `page-*` skill (about 3,000 tokens together) + the component index + that page type's component specs. Sizes are UTF-8 bytes ÷ 4, a rough count; telemetry gives the real one.

Each AI step's contract (input, output schema, validator, fallback, budget) is in the [Bureau design §7](../News%20—%20Bureau%20Design.md#7-ai-usage-ledger); output schemas are in `contracts/cli/ai/`. Every skill states its input shape, its output, what happens when the output fails validation, and that `source_text` and other outside text are untrusted data, never instructions.

## How the pipeline uses them

Ghost, the AI runner inside the Bureau service (Rust, Phase 7; ADR 0007), builds each page call's prompt prefix from: instructions (`news-reporter`), `news-brand`, the one page-type skill for the item, the component index and that page type's pre-selected component specs; the brief is the input. Keep the prefix identical across calls so prompt caching stays warm, and keep each skill short: every word is paid for on every page.

Page calls must not load the project's development plugins and skills, and get no tools at all. Run them with `--tools ""`, an explicit settings file that enables no plugins or hooks, and an empty strict MCP config (`--strict-mcp-config`) (see the Technical Design, "Keeping each claude -p call lean", and the [Bureau design §3.9](../News%20—%20Bureau%20Design.md#39-reporter)).

## Development skills

Development skills live in [`.claude/skills/`](../.claude/skills/) and load in Claude Code sessions in this repo:

| Skill | Use |
|---|---|
| news-design-system | Any visual work; changing tokens |
| news-component-builder | Building components and charts (Opus 5.5) |
| news-component-testing | Testing components in all areas and on all screens (Opus 5.5 only) |
| news-subagent-workflow | Orchestrating sub-agents, PRs and merges |
| news-review | Reviewing branches and PRs (Opus 5.5) |
| news-source-connector | Adding or changing data-source connectors |

Plugins installed at project scope (`.claude/settings.json`, from the `claude-plugins-official` marketplace): frontend-design, cloudflare, playwright, rust-analyzer-lsp, clickhouse-best-practices, databases-on-aws, coderabbit, context7. Also available without installing: the bundled `dataviz` skill (chart colour validation) and `skill-creator`.
