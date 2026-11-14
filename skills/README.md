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
| news-home-curation | The home page curator in the `curate-home` workflow (every 2–3 days) | Planned: written in Phase 7 (Development Plan) |

## How the pipeline uses them

The bun runner (Phase 7) builds each page call's prompt from: instructions, `news-brand`, the one page-type skill for the item, the component index, then the brief. Keep the prefix identical across calls so prompt caching stays warm, and keep each skill short: every word is paid for on every page.

Page calls must not load the project's development plugins and skills. Run them with an explicit settings file that enables no plugins, an empty strict MCP config (`--strict-mcp-config`), and only the `news` CLI tools allowed (see the Technical Design, "Keeping each claude -p call lean").

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
