# News — Development Plan

Nov 14, 2026 · @Karthik

How News gets built: the phases, the work in each, which Claude sub-agent (Opus 5.5 or Sonnet 5.5) does it, how work is reviewed and tested, and when each phase is done.

Read with the [PRD](News%20—%20Product%20Requirements%20Document.md), the [Technical Design](News%20—%20Technical%20Design.md), the [Bureau design](News%20—%20Bureau%20Design.md) (the laptop system: ingestion, ranking, writing, checking, orchestration, the Bureau service, Board and the preset workflows), [ADR 0007](docs/adr/0007-bureau-service-and-board.md) (the service and Board) and the [design system](design/README.md). Where this plan and those documents disagree, those documents win on *what* to build; this plan decides *how* and *by whom*.

## Summary

- Development runs as a **main orchestrator session** (Opus 5.5) that plans, delegates and integrates, plus **sub-agents** that each do one bounded piece of work in their own git worktree.
- **Opus 5.5** does the work that needs judgement: architecture, the OpenUI rendering spike, the component library, all component testing, the content system design, security-sensitive code, bug fixes, skills and prompts, and every review.
- **Sonnet 5.5** does well-specified feature work at volume: source connectors, CLI commands, Workers, page templates, scripts, Board's screens.
- Every change lands through a pull request on GitHub, reviewed by an Opus review agent and by CodeRabbit (light profile), with CI green before merge.
- Phase 2 (design) is complete. The next phases are Foundations and the OpenUI rendering spike, then the component library.
- Karthik runs Bureau from **Board**, a desktop app in the `news` binary, backed by the always-on **Bureau service** (Chief behind it; [ADR 0007](docs/adr/0007-bureau-service-and-board.md)). Runs start from Board, his terminal or schedules he sets in Board; the main run is the Edition. The home page is **dynamic and agent-curated** every 2–3 days by the Frontpage preset, on schedule or from Board, and Karthik approves each curation in Board. The curation is server-side data; it never rebuilds the site. Building Bureau, Board and the workflows is part of this plan (Phases 4–9, in the order of [Bureau design §10](News%20—%20Bureau%20Design.md#10-build-order); the service and the Board shell come right after the first wave of PRs and before the connectors; for curation see [Home page curation](#home-page-curation-across-the-phases)).

## Constraints that shape the plan

| Constraint | Effect on development |
|---|---|
| One person, part-time | The orchestrator does the coordinating; Karthik approves phase exits and merges, not every line |
| Laptop: i5-8250U, 4 cores / 8 threads, 24 GB RAM | At most **3 coding sub-agents at once**; heavy jobs (Playwright screenshot runs, Rust release builds, Docling) run **one at a time** |
| Claude Teams Premium plan, no paid API | Sub-agents are briefed tightly to save tokens; Opus is used where it pays for itself; runs pause rather than overrun limits |
| Free tiers only | CI on GitHub Actions, free on standard runners because the repo is public; the fast suite runs on every PR and the full browser matrix nightly; heavy visual tests also run locally before a component PR |
| No AI in the cloud | Nothing built here calls a model at request time |
| Windows 11 host | Scripts must run under Git Bash and PowerShell; Docker for PostgreSQL, ClickHouse, OTel Collector |

## Repository

GitHub: `Karthik-beta/news` (public). Default branch `main`, protected: no direct pushes, PR required, CI must pass, CodeRabbit review must have run.

```
news/
├─ News — *.md                PRD, Technical Design, this plan
├─ design/                    design system (done): docs, tokens, mock-ups, brand board
├─ site/                      Bun + Astro: component library, renderer, page shell, SSR Worker app
│  ├─ src/components/         OpenUI components (one folder per component)
│  ├─ src/charts/             chart components
│  ├─ src/styles/             tokens.css (copied from design/tokens), base.css, fonts
│  ├─ src/pages/              static and SSR templates
│  └─ tests/                  unit, a11y, visual, SSR tests
├─ cli/                       Rust `news` binary (Cargo workspace: core, connectors, commands, the Bureau service with
│                             Chief, the scheduler, Dispatch workers and Ghost, and Board, the egui desktop app)
├─ contracts/                 JSON Schemas: cli/ (commands, AI step outputs), control/ (the control pipe, ADR 0007)
├─ workers/                   Cloudflare Workers: api, search, mcp (site SSR lives in site/)
├─ pipeline/                  `claude -p` call settings, prompt prefixes and golden sets in eval/ (Ghost itself is Rust, in cli/)
├─ skills/                    runtime skills for the page writers: news-brand, page-type skills, news-home-curation, AI-step skills
├─ infra/                     docker-compose, ClickHouse schema, OTel config, DynamoDB/D1 setup
├─ .claude/agents/            sub-agent definitions (created in Phase 0)
└─ .github/workflows/         CI
```

Every Bureau workflow (preset) is declared in Chief, in the Rust code, and started from Board, Karthik's terminal or one of his schedules; there are no per-workflow commands, skills or workflow files (Bureau design, decision D17). The `dashboard/` skeleton from Phase 0 is removed by the Board shell PR.

## The sub-agent team

### Roles

| Agent | Model | Effort | Used for |
|---|---|---|---|
| **Orchestrator** (the main session) | Opus 5.5 | high | Breaks phases into tasks, writes briefs, starts sub-agents, integrates results, keeps this plan's checklist current. Never holds large file dumps. |
| **Architect** | Opus 5.5 | high | Module boundaries, data models, interfaces, ADRs. Runs before any multi-agent build in a phase. |
| **Spike agent** | Opus 5.5 | high | Unknowns: OpenUI server rendering, prompt caching behaviour, free-tier limits. Produces a finding plus a working proof. |
| **Component builder** | Opus 5.5 | high | Builds design-system components and chart components (Technical Design: library built with Opus). |
| **Component tester** | **Opus 5.5 only** | high | Tests every component in every area and on every screen (see [Phase 3](#phase-3--component-library)). Never Sonnet. |
| **Feature builder** | Sonnet 5.5 | high | Well-specified features: connectors, CLI commands, Workers, templates, scripts, Board's screens. |
| **Test writer** | Sonnet 5.5 | high | Unit and integration tests for non-component code (CLI, Workers, pipeline). |
| **Bug fixer** | Opus 5.5 | high | Any failing test, CI failure or reported defect that isn't a one-line fix. |
| **Reviewer** | Opus 5.5 | high | Reviews every PR before CodeRabbit: correctness, design-system compliance, security, tests. Not the author's agent. |
| **Security reviewer** | Opus 5.5 | high | Write API, secrets handling, MCP server, anything public-facing. |
| **Docs writer** | Sonnet 5.5 | high | READMEs, command help, methodology page drafts (Opus reviews prose that readers will see). |

This matches the existing pstack model configuration: features and refactoring on Sonnet; bug fixes, judgement, prose and the hardest tasks on Opus.

**The Bureau service and Board** (ADR 0007). Service work (Chief, the control pipe and its security, the scheduler and queue, Ghost in Rust, install and update) is built by Opus and gets an Opus security review. Board's screens are built by Sonnet and reviewed by Opus, and a different Opus agent checks that they use the design tokens correctly.

### Agent definitions

Phase 0 creates one file per role in `.claude/agents/` with:
- `model:` `opus` or `sonnet`, and effort `high`;
- the tools the role needs (testers get Playwright and the browser; builders don't get `gh`; reviewers are read-only plus `gh pr review`);
- a short system prompt: the role, the documents to read first (for component work: `design/README.md`, `components.md`, `charts.md`, `tokens.json`), the definition of done, and "return a summary, not file contents".

### How a task runs

```
Orchestrator ──brief──▶ Builder (own worktree, own branch)
                          │ implement → self-test → commit
                          ▼
                       Opus Reviewer ──changes?──▶ Builder fixes (max 2 rounds, then Opus bug fixer)
                          │ approve
                          ▼
                    PR on GitHub ──▶ CI + CodeRabbit (light) ──▶ fixes ──▶ Karthik merges (or auto-merge when green, by phase rule)
```

1. **Brief.** The orchestrator writes a brief: goal, files and documents to read, interfaces to honour, what's out of scope, definition of done, and what to return. Briefs are short and point at files rather than pasting them.
2. **Isolation.** Each builder works in its own git worktree on a branch named `phase-N/<area>/<short-name>` (for example `phase-3/charts/waterfall`). Two agents never edit the same files at once.
3. **Self-test.** The builder runs the relevant tests before committing. It returns: branch, a 5-line summary, test results, open questions.
4. **Review.** An Opus reviewer (a fresh agent, not the builder) reviews the diff against the brief and the design docs. At most two fix rounds; a third failure goes to the Opus bug fixer with the reviewer's notes.
5. **PR.** The orchestrator opens the PR with `gh pr create`. CI runs; CodeRabbit reviews with the light profile. The builder or bug fixer addresses CodeRabbit comments that are real; the orchestrator resolves the rest with a reply.
6. **Merge.** Squash-merge into `main`. Karthik merges phase-exit PRs and anything touching secrets, the write API or the brand; routine PRs can merge when CI and both reviews are green.

### Parallelism rules

- Up to **3 coding sub-agents** at once; up to **2 read-only** agents (reviewers, explorers) alongside them.
- **One heavy job at a time**: full Playwright visual runs, Rust release builds, Docling runs.
- Parallel tasks must have no shared files and no dependency on each other's unmerged work. Shared foundations (tokens, base primitives, core crates) are built first, by one agent, and merged before fan-out.
- The orchestrator keeps a task board in this document's [checklist](#progress-checklist) and never starts work that depends on an unmerged branch.

### Token discipline for sub-agents

- Briefs point at files; agents read only what they need.
- Agents return summaries (≤ 200 words), never full files or logs.
- Exploration of a large area goes to one read-only agent that reports conclusions.
- Opus for judgement, Sonnet for volume. Don't use Opus to write the 40th connector; don't use Sonnet to decide an architecture.
- When a plan usage limit is near, finish in-flight tasks, then pause; never start new fan-outs.

### Skills and plugins

Installed in the repo so every session and sub-agent gets them (see [skills/README.md](skills/README.md)).

| Skill or plugin | Where | Loaded by |
|---|---|---|
| `news-design-system` | `.claude/skills/` | Every agent doing visual work |
| `news-component-builder` | `.claude/skills/` | Opus component builders |
| `news-component-testing` | `.claude/skills/` | Opus component testers only |
| `news-subagent-workflow` | `.claude/skills/` | The orchestrator |
| `news-review` | `.claude/skills/` | Opus reviewers |
| `news-source-connector` | `.claude/skills/` | Connector builders (Sonnet) and their Opus reviewers |
| `news-brand` and seven `page-*` skills | `skills/` | Production page calls only, not development sessions |
| Skills for the Haiku AI steps A3, A4, A5, A7, A9 and Rollcall's proposals A11 (planned, Phase 7) | `skills/` | Ghost's `claude -p` calls only |
| `news-home-curation` (planned, Phase 7) | `skills/` | The curator's `claude -p` call in the Frontpage preset |
| frontend-design, cloudflare, playwright, rust-analyzer-lsp, typescript-lsp, clickhouse-best-practices, databases-on-aws, coderabbit, context7 | Plugins, project scope (`.claude/settings.json`) | All development sessions |
| dataviz (chart colour validation), skill-creator | Bundled with Claude Code | On demand |

## Quality gates (apply to every phase)

**Definition of done for any task**
- Meets the brief and the relevant design or technical document.
- Tests written and passing locally; CI green.
- Opus review approved; CodeRabbit comments addressed or answered.
- No secrets, no literal design values outside tokens, no TODOs without an issue.
- Docs updated where behaviour changed.

**CI (GitHub Actions, fast suite on every PR)**
- `site`: typecheck, lint (Biome or ESLint), unit tests, accessibility tests (axe) on component fixtures, SSR render tests, a token-only lint (no hex colours, font names or px font sizes outside `tokens.css`), and a small visual-test smoke set (2 widths × 2 themes).
- `cli`: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, the PostgreSQL tests against a pgvector service container, and `cargo deny` (bans, licences, sources).
- `workers`: typecheck, unit tests with Miniflare, integration tests against D1/R2 in Miniflare and DynamoDB Local, and E2E against each Worker under the Wrangler harness.
- Secret scanning (gitleaks), actionlint, and config, skill and agent file checks.

**Nightly (GitHub Actions, daily and on every push to `main`)**
- The site's full Playwright matrix (Chromium, Firefox and WebKit; every screen, both themes) in four shards, and the screenshot tests in the Playwright Docker image.

**Test levels and commands.** `bun run test` (unit), `bun run test:integration` (needs `bun run infra:up`: PostgreSQL and DynamoDB Local), `bun run test:e2e`. See `site/TESTING.md`.

**Local (heavy, before a component or template PR)**
- The full visual matrix (every component × every screen width × both themes × browsers). See Phase 3.

## Phases

The phases follow the Technical Design's build plan; the Bureau work inside Phases 4–9 follows the build order in [Bureau design §10](News%20—%20Bureau%20Design.md#10-build-order). Each phase starts when the previous phase's exit criteria are met, except where noted as parallel.

### Phase 0 — Foundations

**Goal:** a working local environment and the development scaffolding.

| Work | Agent | Notes |
|---|---|---|
| Git repo, `.gitignore`, `.gitattributes`, CodeRabbit config, GitHub remote, branch protection | Orchestrator | **Done** (14 Nov 2026), including branch protection on `main` (PRs only, CI checks required) |
| `.claude/agents/` role definitions and a short project `CLAUDE.md` | Opus architect | **Done** (10 roles) |
| Monorepo skeleton: `site/` (Bun + Astro + React), `cli/` (Cargo workspace), `workers/`, `pipeline/`, `dashboard/`, `infra/` | Sonnet feature builder | **Done**: builds, typechecks and tests pass. The `dashboard/` skeleton is removed by the Board shell PR (ADR 0007) |
| `infra/docker-compose.yml`: PostgreSQL + pgvector, OTel Collector | Sonnet feature builder | **Done**: PostgreSQL on 5433, collector on 4317/4318 |
| ClickHouse: `news` database, `news` user, settings profile and quota on the shared container | Sonnet feature builder, Opus review | **Done**; `bun run doctor -- --e2e` proves events reach ClickHouse |
| CI workflow (fast suite skeleton), then branch protection on `main` | Sonnet feature builder | **Done** (14 Nov 2026): first run green on `main`; protection on. Full test workflow in progress (`phase-0/testing`) |
| Cloudflare and AWS accounts, budget alarms, least-privilege keys | **Karthik** (manual) | Agents never create accounts or handle credentials; Karthik puts keys in the local secrets file |
| Secrets handling: `.env.example`, local secrets file loader, gitleaks pre-commit hook | Sonnet feature builder, Opus security review | `.env.example` and generated local `.env` done; gitleaks runs in CI, local hook pending |

**Exit:** local services running; a test event reaches ClickHouse; CI green on `main`; branch protection on.

### Phase 1 — Spike: server rendering of OpenUI

**Goal:** prove the riskiest assumption before building hundreds of components on it.

| Work | Agent |
|---|---|
| Render one OpenUI Lang page to static HTML with the React renderer inside Astro and Bun, with one interactive chart hydrating in the browser | Opus spike agent |
| Measure: HTML size, JS shipped, render time per page on the laptop | Opus spike agent |
| Decide the rendering approach (Astro Container API vs React server rendering) and write an ADR in `docs/adr/` | Opus architect |
| Fallback plan if server rendering fails (for example our own server renderer for the OpenUI Lang AST) | Opus architect |

**Exit:** one page rendered end to end to static HTML with a working interactive chart; ADR merged.

### Phase 2 — Design direction

**Done** (14 Nov 2026). Direction E agreed; design system, tokens, mock-ups and brand board in [`design/`](design/README.md).

### Phase 3 — Component library

**Goal:** the full on-brand component library on `@openuidev/react-ui`, every component built and tested to the design system, plus the component index and per-component specs the page agents use.

All building and all testing in this phase is done by **Opus 5.5** agents (Technical Design: the library is built with Opus). Sonnet is not used for components.

#### 3.1 Foundations (one Opus builder, merged before fan-out)

- Copy `design/tokens/tokens.css` into `site/src/styles/` via a build step (never by hand); base CSS (reset, body, links, focus ring, selection, print).
- Self-hosted fonts: Literata variable (roman, italic) and Fira Sans 400–700 + 400 italic, subset as in [typography.md](design/typography.md#loading-the-fonts).
- Primitives every component uses: `FigureFrame` (rule and tag, title, subtitle, source, actions), `SourceRef` (data reference + citation popover), `Text` styles, `Rule`, `VisuallyHidden`, number and date formatters following [content-style.md](design/content-style.md).
- Data-reference contract: how a component receives `data.json` IDs and source IDs, and how missing data renders.
- The test harness (see 3.4) and the fixture format.

#### 3.2 Component families (up to 3 Opus builders in parallel, one family each)

Order of work, from the families most pages need:

1. **Article head and chrome**: Masthead, FlyTitle, Headline, Standfirst, PageMeta, Disclaimer, Footer, SkipLink, DateLine, RegionFilter.
2. **Data**: KeyFigures (with sparkline variant), SourcePopover, DataTable, ComparisonTable, DataLinks.
3. **Charts I**: ChartFigure, column, horizontal bar, line, step line, sparkline.
4. **Charts II**: waterfall, Sankey (with phone fallback), stacked/segmented bar, dot plot/dumbbell, slope.
5. **Charts III**: small multiples, heatmap, choropleth map (Equal Earth), histogram.
6. **Text**: Prose, GlossaryTerm, Quote, Note, MethodSummary.
7. **Page-type components**: PolicyDecision, Finding, Timeline.
8. **Media**: ImageFigure (with failure fallback), ImageGallery, VideoEmbed (click-to-load, `youtube-nocookie.com`).
9. **Sources**: SourcesPanel, Badge.
10. **Lists and navigation**: LeadStory, StoryItem, SectionHead, LatestList, PageContents, RelatedPages, Pagination. The home-page components take a page ID and an optional figure or chart ID from the curation, and render only the page's own text.
11. **Controls and states**: Button, TextField, TextArea, Select, Checkbox, SearchField, Tooltip, empty, error, 404 and loading states.
12. **Extension set**: the remaining components to reach "at least what OpenUI Dashboards offers" (Technical Design), each proposed by the Opus architect with a one-line purpose and approved by Karthik before building.

Each component is built to its spec in [components.md](design/components.md) or [charts.md](design/charts.md), uses tokens only, renders on the server, and loads JavaScript only for its interactive part.

#### 3.3 Component specs and index

- For every component: `news component <type>` output (props, data shape, one example) and a one-line entry for `news components`.
- Opus builder writes them with the component; the Opus reviewer checks that the one-line index stays short enough for the cached prompt prefix.

#### 3.4 Component testing (Opus 5.5 agents only)

**Every component should be well tested in all areas and also be well tested with different screens (Opus 5.5 agent only).**

Testing is done by a dedicated **Opus 5.5 component tester** that did not build the component. Sonnet agents never write or run component tests. A component is not done until its tester signs it off.

**All areas**

| Area | What is tested | How |
|---|---|---|
| Rendering | Renders from fixtures; every prop and variant; default props | Unit tests (Vitest + Testing Library) |
| Data | Data references resolve; every figure carries its source ID; missing data, nulls, zero, negatives, very large and very small numbers, long series, single-point series | Unit tests with edge-case fixtures |
| Content edge cases | Very long and very short headlines and labels, long words and URLs, many categories, empty lists, one item, the maximum number of items | Fixtures + visual tests |
| Formatting | Currencies ($, €, £, ¥, ₹, ₩, NT$, HK$, A$), units, fiscal periods, minus signs, thousands separators, dates exactly as [content-style.md](design/content-style.md) | Unit tests |
| Server rendering | Renders to static HTML with no JavaScript; content readable without JS; hydration matches the server output with no warnings | SSR tests in Bun |
| Design-system compliance | Tokens only (no literal colours, fonts or px font sizes); crimson never used for decreases; spacing from the scale; no shadows or radius outside the allowed cases | Token lint + review against the design docs |
| Accessibility | axe-core: zero serious or critical issues; correct roles, names and headings; keyboard operation (tab order, arrow keys in charts, Escape closes popovers); visible focus; screen-reader output for charts (accessible name, data table); colour never the only signal | axe + Playwright keyboard tests + manual screen-reader pass (NVDA, VoiceOver, TalkBack) on representative components |
| Interaction | Tooltips and popovers on hover, focus and tap; data-table toggle; click-to-load video; gallery overlay; form validation | Playwright interaction tests |
| Themes | Light, dark (system setting and explicit `data-theme`), forced colours (Windows high contrast), print | Visual tests per theme |
| Motion | No animation on load; transitions ≤ 200 ms; nothing moves with `prefers-reduced-motion: reduce` | Playwright emulation |
| Zoom and text size | 200% browser zoom and 200% text size without loss of content or sideways page scroll | Playwright |
| Images and failures | Image load failure shows the alt-text placeholder at the same size; video placeholder without network | Playwright with blocked requests |
| Performance | JS shipped per component within budget (static components: 0 KB); render time per page within budget; no layout shift (CLS 0) | Bundle analysis + Lighthouse CI on a fixture page |
| Cross-browser | Chromium, Firefox and WebKit (Safari engine) | Playwright projects |

**Different screens**

Every component is screenshot-tested at each of these viewport widths, in light and dark, in all three browser engines:

| Width (px) | Represents |
|---|---|
| 320 | Smallest supported phones, 400% zoom reflow |
| 360 | Common Android phones |
| 390 | Current iPhones |
| 430 | Large phones |
| 600 | Tablet breakpoint edge |
| 768 | Tablet portrait |
| 980 | Desktop breakpoint edge (rail appears) |
| 1120 | Container maximum |
| 1440 | Laptops |
| 1920 | Large desktop monitors |

Plus:
- **Touch emulation** on phone and tablet widths (tap targets ≥ 24px, tooltips on tap).
- **Landscape phone** (844 × 390) for charts and video.
- **Device scale factors** 1× and 2×/3× for chart line crispness.
- Each component checked **alone and in context**: inside a fixture article page and a fixture home page, next to other components, to catch spacing and rhythm problems.

Screenshot baselines live in the repo; any diff fails the run and must be approved by the Opus tester with a reason. The full matrix runs locally (it is CPU-heavy: one run at a time) and nightly in CI; every PR runs a smoke subset (360 and 1440, light and dark, Chromium).

**Tester sign-off.** For each component the Opus tester records in its folder: areas covered, screens covered, issues found and fixed, and any accepted exceptions. Defects go back to the builder (or the Opus bug fixer after two rounds).

#### 3.5 Integration

- Fixture pages: one per page type (earnings, macro release, major report, policy decision, news story, science, explainer), built from real public data, rendered through the full renderer. The Opus tester runs the full matrix on these pages too.
- Karthik reviews the seven fixture pages side by side before the phase exits (the design system's "real-content test").
- A fixture **home page** rendered from fixture curations: a normal curation, one with a withdrawn page (slot back-filled), one filtered by region, and an empty "Latest" edge case. The Opus tester runs the full matrix on it.

**Exit:** every component built, specified and signed off by an Opus tester across all areas and all screens; component index and specs available through the CLI stub; the seven fixture pages and the fixture home page approved by Karthik.

### Phase 4 — Content system design

**Goal:** turn the [Bureau design](News%20—%20Bureau%20Design.md) (discovery, story detection, scoring, selection, briefs, page writing, checks and orchestration) into ADRs, settle its measured unknowns, and label the golden sets the later phases test against.

| Work | Agent |
|---|---|
| ADRs from the Bureau design: Dispatch and Chief (decisions D1, D17–D21; ADR 0007 records the service and Board), tool surface (D3), Ledger and the provenance model, Nose v1 and Desk, Taster, AI step contracts and the Reporter call (D4), derived figures (D5) | Opus architect |
| Embedding spike: bge-small-en-v1.5 vs nomic-embed-text-v1.5 vs EmbeddingGemma vs MiniLM on the i5-8250U (docs per second, memory, G3 precision); numbers and choice in an ADR (D6) | Opus spike agent |
| Golden-set labelling: G2 (classification), G3 (story pairs), G7 (entities); event weights, prominence lists and coverage targets as seed data, in `pipeline/eval/` | Opus with Karthik |
| Contacts: the first 20 sources' entries (licence, attribution, rate limits, source tier, calendar kind) | Opus architect, Sonnet docs writer for entries |
| Page-type skills (earnings, macro, report, policy, news, science, explainer) (drafts exist in `skills/`; finalise them) and the `news-brand` skill | Opus (judgement and prose) |
| Home curation design: slot model, candidate signals (tier, freshness, breadth, traffic), the code-check rules, fallback and region-view rules, and the curation record schema, written as an ADR | Opus architect |

Can start in parallel with late Phase 3 (no shared files).

**Exit:** Bureau ADRs merged (including the home curation ADR); embedding model chosen from measurements; golden sets G2, G3 and G7 in `pipeline/eval/`; skills drafted; brief format fixed ([Bureau design §3.8](News%20—%20Bureau%20Design.md#38-fixer)).

### Phase 5 — Rust `news` CLI and connectors

**Goal:** every command in the Technical Design's toolbox and Bureau's code subsystems, built in the order of [Bureau design §10](News%20—%20Bureau%20Design.md#10-build-order), with the first Stringers (connectors). Steps 2 and 3, the Bureau service and the Board shell, come right after the first wave of PRs (#4, #5, #6) and before the connectors (#9), so every later feature lands in Board as it is built (ADR 0007).

| # | Work | Agent | Exit evidence |
|---|---|---|---|
| — | Cargo workspace architecture: `core` (models, data points, Ledger), `store` (PostgreSQL, sqlx migrations), `telemetry` (ClickHouse events), `connectors`, `commands` | Opus architect | ADR |
| 1 | Core crates and the command framework: migrations, the JSON envelope and exit codes, error codes, idempotency, event logging; Dispatch (job claim, leases and the rescuer, retry classes and snooze) | Opus feature builder (one agent; everything depends on it) | Golden CLI tests; concurrency and crash tests |
| 2 | **Chief and the Bureau service:** runs and steps, approvals bound to `subject_sha`, the G-tier rule, the heavy-slot queue; the scheduler (preset schedules, missed slots, quiet hours, quota guard); the Dispatch workers hosted in the service; the control pipe and the spawn marker; `news` control commands over the pipe. First, the `claude -p`-from-service spike: Karthik's login under the service account, the usage-limit error shape and reset time, Job Object inheritance, ClickHouse and Docker reachability | Opus (correctness-critical); Opus security review | Pipe, marker, queue and crash tests; spike findings in an ADR |
| 3 | **The Windows service and the Board shell:** `news service install\|uninstall\|update`; the settings table and Credential Manager; `news board` with Runs & approvals, Settings & schedules and Docker health (screens step 1), the offline banner, themes from the tokens, toasts; remove `dashboard/` | Opus for the service parts; Sonnet for Board; Opus review; a different Opus agent checks the token styling | Karthik installs the service, then starts, approves and cancels a fixture run from Board |
| 4 | Runner (polite HTTP: conditional GET, rate limits, `Retry-After`, robots.txt) and Morgue, with `fetch_state` | Sonnet feature builder; Opus review | Fake-server tests |
| 5 | Contacts rows and Stringers in parallel: SEC EDGAR, FRED, BLS, ECB, arXiv, central-bank statements (Fed, ECB, RBI, BoE, RBA, BoJ), then IMF, World Bank, Eurostat, ONS, StatCan | Sonnet feature builders, one Stringer per task, up to 3 in parallel; Opus review for XBRL, units and periods | Fixture tests |
| 6 | Clerk and Ledger: data points with vintages and locators, derived points with guards; XBRL and unit, currency and fiscal-period parsing | Opus (correctness-critical) | Property tests |
| 7 | Diary: release calendars from ICS, APIs and pages; pre-warm and capture jobs | Sonnet feature builder | ICS and API fixture tests |
| 8 | Roster's resolver and its seed data | Sonnet feature builder; Opus review | G7 precision ≥ 0.99 |
| 9 | Taster and local embeddings (after the Phase 4 spike) | Sonnet feature builder | G3 precision |
| 10 | Nose, Desk and the Nose tools (`news scores`, `news plan`) | Opus | The worked example in [Bureau design §4.6](News%20—%20Bureau%20Design.md#46-worked-example) reproduced as a test |
| 11 | Fixer (`news brief`) | Opus | Size and fencing tests |
| 12 | Proof: `news check` C1–C14 (render, references, values, numeric-literal lint, direction words, quotes, labels, licence, required fields, links, banned words, size, axe-core, duplicates) and the Ghost tools' validators (`news ai accept`) | Opus | Contract tests |
| 13 | Press: publish outbox with idempotency keys and the index budget (dry-run) | Sonnet feature builder; Opus security review | Idempotency tests |
| — | Remaining toolbox commands: components, component, related, save, render (calls the warm Bun render server), update, prune, stats | Sonnet feature builders (up to 3 in parallel, grouped so no two touch the same files) | Golden CLI tests |
| — | PDF tables via Docling, scanned PDFs via Tesseract | Sonnet feature builder; Opus review | Fixture tests |
| — | `news home` commands: candidates, save, check, preview, publish, rollback, show; the `home_curations` PostgreSQL table | Sonnet feature builder; Opus review of the check rules | Fixture tests |
| — | Tests for commands and Stringers (recorded fixtures, no live network in CI) | Sonnet test writer; Opus reviewer | CI |

**Exit:** the Bureau service is installed and Karthik controls fixture runs from Board; `news discover → plan → fetch → parse → brief` works for the first Stringers; `render`, `check` and a dry-run `publish` work on fixture items; `news home candidates → save → check → preview` works on fixture pages; every item's exit evidence above passes; CI green.

### Phase 6 — Cloud

| Work | Agent |
|---|---|
| R2 bucket, custom domain `news.karthik.nz`, caching rules | Sonnet feature builder (scripts in `infra/`), Karthik applies DNS |
| DynamoDB single-table design and access patterns, including `HOME#CURATION` versions and `LATEST` | Opus architect |
| D1 search shards and FTS5 schema | Sonnet feature builder, Opus review |
| `api` Worker (write API: secret key, schema validation, R2/DynamoDB/D1 writes, cache purges), including `PUT /v1/home/curation` (validates the curation again, writes a new version, moves `LATEST`, purges home and region views) and rollback | Sonnet feature builder, **Opus security reviewer** |
| Write API honours `Idempotency-Key` (a replay returns the first result); corrections endpoint for `news corrections pull` (reader error reports) | Sonnet feature builder, **Opus security reviewer** |
| `site` SSR (home, hubs, entity hubs, latest, search results) from the Phase 3 templates. The home page reads the live curation and the page records for its slots, adds the automatic "Latest" list, back-fills withdrawn pages and serves region views | Sonnet feature builder; Opus tester for template visuals |
| `search` Worker | Sonnet feature builder |
| `mcp` Worker (stateless MCP v2, spec revision 2026-07-28, read-only) | Opus feature builder (new spec), Opus security review |
| Rate limiting, `robots.txt`, `llms.txt`, sitemaps, RSS/Atom | Sonnet feature builder |
| Free-tier usage checks and alarms | Sonnet feature builder |

**Exit:** a fixture page published end to end and served from `news.karthik.nz`; replaying the same publish with its `Idempotency-Key` returns the first result; hubs, search and MCP answering; publishing a test curation changes the live home page with no build or deploy (verified by checking the deployed Worker version is unchanged); security review passed.

### Phase 7 — Orchestration

| Work | Agent | Exit evidence |
|---|---|---|
| Lean-call spike: confirm the `claude -p` flags ([Bureau design §3.9](News%20—%20Bureau%20Design.md#39-reporter)), settings isolation, cache TTL and hit ratio across back-to-back calls. (Login under the service account and the usage-limit error shape are confirmed earlier, in the Phase 5 service spike) | Opus spike agent | Findings in an ADR |
| Ghost (Rust, inside the Bureau service): claim AI jobs, spawn `claude -p` in the service's Job Object with `--tools ""`, a strict empty MCP config, an explicit settings file, `--json-schema`, max turns and model per call; environment allow-list; validation through `news ai accept`; heartbeats; pause on usage limits and resume at the reset; per-call accounting for the quota guard | Opus feature builder; Opus security review | Fake-`claude` tests |
| Runtime skills and prefixes: Reporter (A1 and A2 prompts) and the cached prefix with pre-selected component specs (measure cache hits); skills for A3, A4, A5, A7 (sample check and failure re-queue), A9 and Rollcall's proposals (A11) | Opus (judgement and prose) | G1, G2, G6 pass |
| Critic: `news eval`, golden sets G1–G7 from `pipeline/eval/`, A12 judges checked against Karthik's labels | Opus (judgement); Sonnet for glue | Audit started from Board produces the G1 and G6 reports |
| `news-home-curation` skill: how to pick the lead, side stories and sections; breadth and freshness; using only the pages' own text; choosing figures and charts | Opus (judgement and prose) | Curator evaluation (below) |
| Preset step bodies for Edition, Flash, Frontpage (candidates → curator draft (`claude -p`) → save → check, up to two fix rounds → local preview → **approval in Board** → publish; rollback from Board or the terminal), Salvage and Audit ([Bureau design §8](News%20—%20Bureau%20Design.md#8-board-the-bureau-service-and-the-preset-workflows)) | Opus (orchestration); Sonnet for glue | Karthik runs each of the five once from Board |
| Board screens step 2: the live service log and one record per `claude -p` call (step, model, tokens, time, result, error; brief and output viewable) | Sonnet feature builder; Opus review; a different Opus agent checks token styling | Records of a real run shown |
| Curator evaluation (golden set G5): fixture candidate sets (busy week, quiet week, one region dominating, a withdrawn lead) with expected properties; check pass rate and breadth measured | Opus | G5 report |
| First full run of 10 pages, then 100, started from Board; then a scheduled Edition; then the first real home curation | Karthik, from Board | 100 pages pass checks |

**Exit:** an Edition started from Board, and one started by a schedule, each produce 100+ pages that pass checks; token use per page logged; Karthik approves a Frontpage curation in Board once end to end, and it goes live without a rebuild.

### Phase 8 — Telemetry and dashboard

The dashboard is now Board's Analytics screen (ADR 0007); the phase keeps its name to match the milestone.

| Work | Agent |
|---|---|
| Ticker: the new `events` columns, the `ai_calls`, `fetches`, `scores`, `check_results` and `runs` tables, materialised views and traces ([Bureau design §9.2](News%20—%20Bureau%20Design.md#92-observability)) | Sonnet feature builder, Opus review |
| OTel Collector pipeline for Claude Code telemetry, joined to runs and jobs | Sonnet feature builder |
| Board screens step 3, Content & sources: stories and scores, pages, source health, the Diary, dead jobs and retry | Sonnet feature builder; Opus review; a different Opus agent checks token styling |
| Board screens step 4, Analytics: simple egui_plot charts styled by the tokens, with the Bureau panels: runs, queue, ingestion, stories and ranking, AI (including Bureau usage against the cap and the total), quality, publishing, laptop | Sonnet feature builder; Opus review; a different Opus agent checks token styling |

**Exit:** Board's Analytics screen shows a real run, and pages, coverage, queue, tokens per page, failures, index budget, free-tier usage, traffic, and the live home curation with its age (flagged after `NEWS_HOME_STALE_DAYS`).

### Phase 9 — Launch and ramp

| Work | Agent |
|---|---|
| Launch checklist: legal pages, privacy note, methodology page, disclaimers, error-report form | Opus (prose readers see), Karthik approves |
| Indexing budget switch, Search Console, IndexNow | Sonnet feature builder |
| Launch-day review of 20 random pages against the design system and sources | Opus reviewer |
| The remaining presets, each a declaration in Chief, started from Board or a schedule: Huddle (weekly, with its Board form for ranking calibration), Rollcall (weekly, waiting at its `actions` gate), Errata, Catchup, Followup, Reprint | Sonnet feature builders; Opus review |
| Board screens step 5, the whole project: published-content actions (withdraw or correct a page, home curation rollback, Errata reports); cloud free-tier usage and alarms; Deploy buttons that trigger the GitHub Actions deploy workflow for `main` and show its history; development status (PRs, CI, issues; read-only) | Sonnet feature builder; Opus review; Opus security review for the tokens and Deploy |
| Ramp: tune scoring, budget and prompts from telemetry and the weekly Huddle | Opus with Karthik |

**Exit (PRD v1):** site live on `news.karthik.nz`; runs produce 100+ pages each; the home page is curated through Frontpage and approved in Board; the first weekly review (Huddle) completed.

### Home page curation across the phases

The home page is dynamic and curated by an agent every 2–3 days, when the Frontpage preset runs on Karthik's schedule or from Board. The curation is data stored in DynamoDB and read by the SSR Worker, so it never rebuilds or redeploys the site. (Requirement: PRD "Home page curation"; design: Technical Design "Home page curation", [Bureau design §8.6](News%20—%20Bureau%20Design.md#86-frontpage-home-curation) and design/layout.md.)

| Phase | Curation work | Agent |
|---|---|---|
| 3 | Home components (LeadStory, StoryItem, SectionHead, LatestList) take page IDs from a curation; fixture home page tested in all areas and on all screens | Opus builder, Opus tester |
| 4 | Curation ADR: slots, signals, check rules, fallbacks, record schema | Opus architect |
| 5 | `news home` commands and the `home_curations` table | Sonnet feature builder, Opus review |
| 6 | `HOME#CURATION` in DynamoDB; `PUT /v1/home/curation` and rollback; SSR home reading the live curation; purges | Sonnet feature builders, Opus security review, Opus tester |
| 7 | `news-home-curation` skill; the Frontpage preset with its curator draft and its `publish` approval stop in Board; curator evaluation (G5); first real curation | Opus, Sonnet for glue, Karthik approves |
| 8 | Board: live curation and its age | Sonnet feature builder |
| 9 | Board: home curation rollback; launch with a curated home page | Sonnet feature builder; Karthik |

**Done means:** Frontpage runs on its schedule (or Karthik presses Start); it gathers candidates, drafts, checks, previews and waits; Board raises a toast; after Karthik approves in Board, the home page changes within a minute, with no build or deploy, and a rollback from Board (or `news home rollback <version>` in the terminal) restores the previous version.

## Risks in the development process

| Risk | Mitigation |
|---|---|
| OpenUI can't render on the server | Phase 1 spike before any component work; fallback renderer in the ADR |
| Parallel agents conflict | Worktrees per agent; file ownership per task; foundations merged before fan-out |
| Agents drift from the design system | Tokens-only lint; Opus reviewer checks against design docs; visual baselines; CodeRabbit path instructions |
| Visual test runs overload the laptop | One heavy run at a time; PRs run only a smoke subset, and the full matrix runs nightly in CI |
| Usage limits stall a phase | Finish in-flight work, then pause; Opus only where it pays; tight briefs |
| Flaky screenshot tests | Fixed fonts (self-hosted), fixed viewport and device scale, animations off, fixed dates in fixtures |
| Secrets leak | Agents never handle credentials; gitleaks in pre-commit, CI and CodeRabbit |
| Hidden costs from free tiers | Budget alarms; usage tracked in Board; no paid services added without Karthik's approval |
| `claude -p` cannot use Karthik's login from a Windows service | Nothing AI runs from the service | The service spike is the first thing in Phase 5 step 2, before anything is built on it; if it fails, Karthik decides the fallback (ADR 0007) |
| The service step delays the connectors | Phase 5 slips | Only steps 2 and 3 sit between the first wave and #9; component work (#7, #8) carries on in parallel |
| Board drifts from the design system | Inconsistent look | Themes generated from `tokens.json`; a different Opus agent checks the token styling of every Board screen |
| Curation quietly needs a rebuild after all (for example the home template caches slot data at build time) | Phase 6 exit test publishes a curation and checks the deployed Worker version is unchanged; slots store page IDs only |
| Curator picks narrowly (one region or category) | Code checks for breadth; fixture evaluations in Phase 7; Karthik approves every curation |

## Progress checklist

- [x] Phase 2: design system agreed and documented
- [x] Git repository, CodeRabbit config (light review), GitHub remote
- [x] Phase 0: agent definitions, monorepo skeleton, local services, ClickHouse `news` database, `.env` handling
- [x] Phase 0: first CI run and branch protection
- [ ] Phase 0: Cloudflare and AWS accounts (Karthik); local gitleaks hook
- [ ] Phase 1: OpenUI server-rendering spike and ADR
- [ ] Phase 3.1: foundations
- [ ] Phase 3.2–3.3: component families and specs (including the home-page components)
- [ ] Phase 3.4: Opus component testing sign-off, all areas and all screens
- [ ] Phase 3.5: seven fixture pages and the fixture home page approved
- [ ] Phase 4: Bureau ADRs (D1–D5, Ledger and provenance, Nose and Desk, Taster) and skills
- [x] ADR 0007: the Bureau service and Board (decided 16 Nov 2026)
- [ ] Phase 4: embedding spike; golden sets G2, G3, G7 labelled
- [ ] Phase 4: home curation ADR
- [ ] Phase 5: core crates and Dispatch
- [ ] Phase 5: the `claude -p`-from-service spike; Chief in the Bureau service, the control pipe and spawn marker, the scheduler and the heavy-slot queue
- [ ] Phase 5: the Windows service install and update, settings and Credential Manager, and the Board shell (Runs & approvals, Settings & schedules, Docker health); `dashboard/` removed
- [ ] Phase 5: Runner, Morgue, Contacts rows and the first Stringers; Clerk and Ledger; Diary; Roster's resolver
- [ ] Phase 5: Taster, Nose and Desk, Fixer, Proof, Press (dry-run)
- [ ] Phase 5: `news home` commands
- [ ] Phase 6: cloud services live; write API `Idempotency-Key` and corrections endpoint
- [ ] Phase 6: curated home page served from DynamoDB, no rebuild
- [ ] Phase 7: lean-call spike; Ghost in Rust; runtime skills; Critic
- [ ] Phase 7: Chief's presets Edition, Flash, Frontpage, Salvage, Audit; Board's service log and AI-call records
- [ ] Phase 7: first 100-page run started from Board, then a scheduled Edition
- [ ] Phase 7: Frontpage: first curation approved in Board and live
- [ ] Phase 8: Ticker; Board's Content & sources and Analytics screens
- [ ] Phase 9: remaining presets (Huddle, Rollcall, Errata, Catchup, Followup, Reprint); Board's whole-project screen
- [ ] Phase 9: launch

## Open questions

- Public or private repository: settled, the repo is public. Actions minutes on standard runners are free, so CI minutes are no longer a limit.
- Whether routine PRs may auto-merge when CI and both reviews are green, or Karthik merges everything.
- Whether the CLI should refuse control commands inside a Claude Code development session (ADR 0007, open question).
- Settings Karthik sets later in Board: the Bureau token cap and reserve, quiet hours, and the default schedule times.
- Names for the new parts without one yet (the scheduler, the control pipe, the heavy-slot queue, the render server, the settings store), and the `news settings` command name: to be chosen with Karthik.

**Only Karthik can do** (for the service and Board): type his Windows password at `news service install`; create the read-only tokens for Board (Cloudflare analytics and usage; AWS IAM read-only for DynamoDB and CloudWatch metrics; a GitHub fine-grained token that reads PRs, issues and checks and runs the deploy workflow) and enter them in Board, which stores them in Credential Manager; approve any paid service (none is planned).
