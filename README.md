# News

Major company, economic and science reports, turned into visual, fully sourced pages. Planned home: `news.karthik.nz`.

| Read | For |
|---|---|
| [News — Product Requirements Document](News%20—%20Product%20Requirements%20Document.md) | What the product is |
| [News — Technical Design](News%20—%20Technical%20Design.md) | How it works |
| [News — Development Plan](News%20—%20Development%20Plan.md) | How it gets built, by which agents |
| [design/](design/README.md) | The design system, tokens and mock-ups |

## Local development

### Prerequisites (Windows 11, already on this laptop)

| Tool | Version used | Notes |
|---|---|---|
| Bun | 1.4.x | Runtime and package manager; Astro runs under Bun |
| Rust | stable (rustup) | `cli/rust-toolchain.toml` adds rustfmt, clippy, rust-analyzer |
| Docker Desktop | 29.x | PostgreSQL + pgvector, the OTel Collector and DynamoDB Local |
| Shared ClickHouse | `shorts-clickhouse-1` | Another project's container; News gets its own database and user |
| Git, GitHub CLI, Claude Code | | |
| Optional | gitleaks, Tesseract | Local secret scan; scanned PDFs (Phase 5) |

### First run

```bash
bun run setup
```

This is safe to re-run. It:
1. creates `.env` from `.env.example` with generated local passwords (git-ignored);
2. starts `news-postgres` (PostgreSQL 17 + pgvector on **127.0.0.1:5433**, because 5432 is the native PostgreSQL), `news-otel` (OTLP on 4317/4318, health on 13133) and `news-dynamodb` (AWS's DynamoDB Local, in memory, on **127.0.0.1:8000**, for the Worker tests);
3. creates the isolated `news` database, user, settings profile (2 threads, 2 GB) and quota in the shared ClickHouse, plus the `news.events` table;
4. installs dependencies, syncs design tokens into the site and builds the Rust CLI;
5. runs the doctor, including an end-to-end check that a telemetry event reaches ClickHouse through the collector.

### Everyday commands

| Command | Does |
|---|---|
| `bun run doctor` | Checks tools, `.env`, containers and services (`-- --e2e` adds the telemetry round trip) |
| `bun run dev` | Site at http://127.0.0.1:4321 |
| `bun run typecheck` · `bun run test` · `bun run lint` | Checks across all workspaces (`bun run test` is the unit level) |
| `bun run test:integration` | Integration level, needs `bun run infra:up`: the site build checks, the Workers against D1/R2 in Miniflare and DynamoDB Local, then the CLI's PostgreSQL tests |
| `bun run test:e2e` | E2E level: site smoke tests (Playwright; first time: `bunx playwright install chromium` in `site/`), each Worker under the Wrangler harness, the dashboard process |
| `bun test scripts/tests` · `bun run validate` | The setup scripts' tests; config, skill and agent file checks |
| `bun run infra:up` · `infra:down` · `infra:logs` | Local containers |
| `bun run clickhouse:setup` | Re-apply the ClickHouse database, user and tables |
| `bun run tokens` | Regenerate `design/tokens/tokens.css` from `tokens.json` |
| `cargo build` · `cargo test` · `cargo clippy` in `cli/` | Rust CLI (`cli/target/debug/news version`) |

### Layout

```
site/       Astro + React site, component library (Phase 3), renderer
cli/        Rust `news` CLI (Phase 5)
workers/    Cloudflare Workers: api, search, mcp (Phase 6)
pipeline/   bun runner for page-writing agents (Phase 7)
dashboard/  local telemetry dashboard, localhost only (Phase 8)
infra/      docker-compose, ClickHouse SQL, OTel config, PostgreSQL init
skills/     runtime skills for the page writers
design/     design system (done)
.claude/    project skills, sub-agent definitions, project plugins
```

### Notes for this machine

- **Bun installs use the hoisted layout** (`bunfig.toml`). With the isolated layout, module resolution climbed to `C:\Users\Admin\node_modules` and picked up stale packages.
- **TypeScript is pinned to 6.x** until `astro check` supports TypeScript 7.
- **Astro runs under Bun** (`bun --bun astro …`): Astro 7 needs Node ≥ 22.23 and the system Node is 22.16.
- **Cloud credentials** (Cloudflare, AWS, the write-API key) are added to `.env` by Karthik only.
