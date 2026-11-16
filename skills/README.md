# Runtime skills (page writers)

These skills are for the **production pipeline**: the `claude -p` calls (Sonnet 5.5, Haiku 5.5) that write and check pages. They are deliberately **not** in `.claude/skills/`, so they don't load into development sessions, and development skills don't load into page calls.

| Skill | Loaded when | Status |
|---|---|---|
| [news-brand](news-brand/SKILL.md) | Every page call (cached prompt prefix) | Agreed (from the design system) |
| [page-earnings](page-earnings/SKILL.md) | Earnings pages | Draft, finalised in Phase 4 |
| [page-macro-release](page-macro-release/SKILL.md) | Macro releases | Draft, finalised in Phase 4 |
| [page-major-report](page-major-report/SKILL.md) | Institutional reports | Draft, finalised in Phase 4 |
| [page-policy-decision](page-policy-decision/SKILL.md) | Central-bank and policy decisions | Draft, finalised in Phase 4 |
| [page-news-story](page-news-story/SKILL.md) | News stories | Draft, finalised in Phase 4 |
| [page-science](page-science/SKILL.md) | Science pages | Draft, finalised in Phase 4 |
| [page-explainer](page-explainer/SKILL.md) | Explainers | Draft, finalised in Phase 4 |
| news-home-curation | The home page curator in the Frontpage preset (`/bureau curate home`, every 2–3 days); loaded by the `/bureau` session, not by a `claude -p` call | Planned: written in Phase 7 (Development Plan) |
| A3: classify an ambiguous item (name to be chosen) | Haiku batches for free-text items the rules table cannot classify | Planned: Phase 7 |
| A4: same story? (name to be chosen) | Haiku batches for grey-zone story pairs | Planned: Phase 7 |
| A5: condense long source text (name to be chosen) | Haiku, when a brief's source text is too long; extractive and digit-free | Planned: Phase 7 |
| A7: sample check (name to be chosen) | Haiku comparison of a page with its brief, on a sample of pages | Planned: Phase 7 |
| A9: triage a reader error report (name to be chosen) | Haiku batches in the Errata workflow | Planned: Phase 7 |
| A6 entity shortlist, A10 alt text (names to be chosen) | Only if switched on; off at launch (Bureau design, decision D12) | Not planned yet |

Each AI step's contract (input, output schema, validator, fallback, budget) is in the [Bureau design §7](../News%20—%20Bureau%20Design.md#7-ai-usage-ledger).

## How the pipeline uses them

Ghost, the Bun AI runner (Phase 7), builds each page call's prompt prefix from: instructions, `news-brand`, the one page-type skill for the item, the component index and that page type's pre-selected component specs; the brief is the input. Keep the prefix identical across calls so prompt caching stays warm, and keep each skill short: every word is paid for on every page.

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
