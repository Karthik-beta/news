# News

Visual, fully sourced pages for major company, economic and science reports. One-person project; Claude agents do most of the building.

- What to build: `News — Product Requirements Document.md`, `News — Technical Design.md`. How and by whom: `News — Development Plan.md`.
- Design system: `design/` (tokens in `design/tokens/tokens.json` are the source of truth). Load the `news-design-system` skill for any visual work.
- Orchestrating work through sub-agents: load `news-subagent-workflow`. Components: built by Opus (`news-component-builder`), tested by a different Opus agent (`news-component-testing`).
- Home page: dynamic and agent-curated every 2–3 days by the `curate-home` workflow (built in Phase 7; Technical Design, "Home page curation"). Curation is data in DynamoDB read by the SSR Worker; it never rebuilds or redeploys the site.

## Layout
`site/` Astro + React (Bun) · `cli/` Rust `news` CLI · `workers/` Cloudflare Workers · `pipeline/` page runner · `dashboard/` local telemetry · `infra/` Docker, ClickHouse, OTel · `skills/` runtime skills for page writers (not dev skills).

## Commands (repo root)
`bun run setup` (first time, safe to re-run) · `bun run doctor` (`--e2e` for the telemetry round trip) · `bun run dev` · `bun run typecheck` · `bun run test` · `bun run test:integration` (workspaces, then the CLI's Postgres tests) / `test:e2e` (need `infra:up`; see `site/TESTING.md`) · `bun run validate` (config, skill and agent files) · `bun run lint` · `bun run infra:up` / `infra:down` · `bun run tokens`. Rust: `cargo build|test|clippy` in `cli/`.

## Rules
- Bun is the runtime; Astro runs under Bun (`bun --bun astro …`) because the system Node is older than Astro needs.
- Never commit secrets; `.env` is local. Cloud accounts and keys are Karthik's to handle.
- Numbers on pages come from source data by code, never typed by a model; never convert currencies, units or fiscal periods.
- Work on a branch and open a PR; `main` gets changes only through reviewed PRs.
