# News

Visual, fully sourced pages for major company, economic and science reports. One-person project; Claude agents do most of the building.

- What to build: `News — Product Requirements Document.md`, `News — Technical Design.md`. How and by whom: `News — Development Plan.md`.
- Design system: `design/` (tokens in `design/tokens/tokens.json` are the source of truth). Load the `news-design-system` skill for any visual work.
- Orchestrating work through sub-agents: load `news-subagent-workflow`. Components: built by Opus (`news-component-builder`), tested by a different Opus agent (`news-component-testing`).
- Bureau, the laptop system (`News — Bureau Design.md`, `docs/adr/0007-bureau-service-and-board.md`): an always-on Windows service (`news service`; Karthik installs it with `news service install`) with Chief behind it; Karthik will control everything from Board, its egui desktop app (`news board`); high-tier actions (starts, approvals, cancels, settings) are accepted only from Board over the Hotline, and the `news` CLI in a terminal reads status and runs low-risk commands only. Runs start from Board or schedules he sets (Rota); the home page is curated every 2–3 days by the Frontpage preset and approved in Board, as data in DynamoDB, never a rebuild.

## Layout
`site/` Astro + React (Bun) · `cli/` Rust `news` binary: CLI, Bureau service, Board · `workers/` Cloudflare Workers · `pipeline/` page-call settings and golden sets · `infra/` containers on Kiri (https://runkiri.com, docker's default context; not Docker Desktop), ClickHouse, OTel · `skills/` runtime skills for page writers (not dev skills).

## Commands (repo root)
`bun run setup` (first time, safe to re-run) · `bun run doctor` (`--e2e` for the telemetry round trip) · `bun run dev` · `bun run typecheck` · `bun run test` · `bun run test:integration` (workspaces, then the CLI's Postgres tests) / `test:e2e` (need `infra:up`; see `site/TESTING.md`) · `bun run validate` (config, skill and agent files) · `bun run lint` · `bun run infra:up` / `infra:down` · `bun run tokens`. Rust: `cargo build|test|clippy` in `cli/`.

## Rules
- Bun is the runtime; Astro runs under Bun (`bun --bun astro …`) because the system Node is older than Astro needs.
- Never commit secrets; `.env` is local. Cloud accounts and keys are Karthik's to handle.
- Numbers on pages come from source data by code, never typed by a model; never convert currencies, units or fiscal periods.
- Work on a branch and open a PR; `main` gets changes only through reviewed PRs.
