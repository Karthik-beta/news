# News — Development Plan

Nov 14, 2026 · @Karthik

How News gets built: the phases, the work in each, which Claude sub-agent (Opus 5.5 or Sonnet 5.5) does it, how work is reviewed and tested, and when each phase is done.

Read with the [PRD](News%20—%20Product%20Requirements%20Document.md), the [Technical Design](News%20—%20Technical%20Design.md) and the [design system](design/README.md). Where this plan and those documents disagree, those documents win on *what* to build; this plan decides *how* and *by whom*.

## Summary

- Development runs as a **main orchestrator session** (Opus 5.5) that plans, delegates and integrates, plus **sub-agents** that each do one bounded piece of work in their own git worktree.
- **Opus 5.5** does the work that needs judgement: architecture, the OpenUI rendering spike, the component library, all component testing, the content system design, security-sensitive code, bug fixes, skills and prompts, and every review.
- **Sonnet 5.5** does well-specified feature work at volume: source connectors, CLI commands, Workers, page templates, scripts, the telemetry dashboard.
- Every change lands through a pull request on GitHub, reviewed by an Opus review agent and by CodeRabbit (light profile), with CI green before merge.
- Phase 2 (design) is complete. The next phases are Foundations and the OpenUI rendering spike, then the component library.

## Constraints that shape the plan

| Constraint | Effect on development |
|---|---|
| One person, part-time | The orchestrator does the coordinating; Karthik approves phase exits and merges, not every line |
| Laptop: i5-8250U, 4 cores / 8 threads, 24 GB RAM | At most **3 coding sub-agents at once**; heavy jobs (Playwright screenshot runs, Rust release builds, Docling) run **one at a time** |
| Claude Teams Premium plan, no paid API | Sub-agents are briefed tightly to save tokens; Opus is used where it pays for itself; runs pause rather than overrun limits |
| Free tiers only | CI on GitHub Actions free minutes (private repo: 2,000 min/month); heavy visual tests run locally, CI runs the fast suite |
| No AI in the cloud | Nothing built here calls a model at request time |
| Windows 11 host | Scripts must run under Git Bash and PowerShell; Docker for PostgreSQL, ClickHouse, OTel Collector |

## Repository

GitHub: `Karthik-beta/news` (private). Default branch `main`, protected: no direct pushes, PR required, CI must pass, CodeRabbit review must have run.

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
├─ cli/                       Rust `news` CLI (Cargo workspace: core, connectors, commands)
├─ workers/                   Cloudflare Workers: api, search, mcp (site SSR lives in site/)
├─ pipeline/                  bun runner for `claude -p`, orchestration scripts
├─ skills/                    page-type skills and the brand skill (from design/brand-skill.md)
├─ dashboard/                 local telemetry dashboard (bun web app, localhost only)
├─ infra/                     docker-compose, ClickHouse schema, OTel config, DynamoDB/D1 setup
├─ .claude/agents/            sub-agent definitions (created in Phase 0)
└─ .github/workflows/         CI
```

## The sub-agent team

### Roles

| Agent | Model | Effort | Used for |
|---|---|---|---|
| **Orchestrator** (the main session) | Opus 5.5 | high | Breaks phases into tasks, writes briefs, starts sub-agents, integrates results, keeps this plan's checklist current. Never holds large file dumps. |
| **Architect** | Opus 5.5 | high | Module boundaries, data models, interfaces, ADRs. Runs before any multi-agent build in a phase. |
| **Spike agent** | Opus 5.5 | high | Unknowns: OpenUI server rendering, prompt caching behaviour, free-tier limits. Produces a finding plus a working proof. |
| **Component builder** | Opus 5.5 | high | Builds design-system components and chart components (Technical Design: library built with Opus). |
| **Component tester** | **Opus 5.5 only** | high | Tests every component in every area and on every screen (see [Phase 3](#phase-3--component-library)). Never Sonnet. |
| **Feature builder** | Sonnet 5.5 | high | Well-specified features: connectors, CLI commands, Workers, templates, scripts, dashboard. |
| **Test writer** | Sonnet 5.5 | high | Unit and integration tests for non-component code (CLI, Workers, pipeline). |
| **Bug fixer** | Opus 5.5 | high | Any failing test, CI failure or reported defect that isn't a one-line fix. |
| **Reviewer** | Opus 5.5 | high | Reviews every PR before CodeRabbit: correctness, design-system compliance, security, tests. Not the author's agent. |
| **Security reviewer** | Opus 5.5 | high | Write API, secrets handling, MCP server, anything public-facing. |
| **Docs writer** | Sonnet 5.5 | high | READMEs, command help, methodology page drafts (Opus reviews prose that readers will see). |

This matches the existing pstack model configuration: features and refactoring on Sonnet; bug fixes, judgement, prose and the hardest tasks on Opus.

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

## Quality gates (apply to every phase)

**Definition of done for any task**
- Meets the brief and the relevant design or technical document.
- Tests written and passing locally; CI green.
- Opus review approved; CodeRabbit comments addressed or answered.
- No secrets, no literal design values outside tokens, no TODOs without an issue.
- Docs updated where behaviour changed.

**CI (GitHub Actions, fast suite on every PR)**
- `site`: typecheck, lint (Biome or ESLint), unit tests, accessibility tests (axe) on component fixtures, SSR render tests, a token-only lint (no hex colours, font names or px font sizes outside `tokens.css`), and a small visual-test smoke set (2 widths × 2 themes).
- `cli`: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`.
- `workers`: typecheck, unit tests with Miniflare.
- Secret scanning (gitleaks) and actionlint.

**Local (heavy, before a component or template PR)**
- The full visual matrix (every component × every screen width × both themes × browsers). See Phase 3.

## Phases

The phases follow the Technical Design's build plan. Each starts when the previous phase's exit criteria are met, except where noted as parallel.

### Phase 0 — Foundations

**Goal:** a working local environment and the development scaffolding.

| Work | Agent | Notes |
|---|---|---|
| Git repo, `.gitignore`, `.gitattributes`, CodeRabbit config, GitHub remote, branch protection | Orchestrator | **Done** (14 Nov 2026) except branch protection, which needs the first CI workflow |
| `.claude/agents/` role definitions and a short project `CLAUDE.md` | Opus architect | Keep `CLAUDE.md` tiny (it loads into every session) |
| Monorepo skeleton: `site/` (Bun + Astro + React), `cli/` (Cargo workspace), `workers/`, `pipeline/`, `infra/` | Sonnet feature builder | Empty packages that build and test |
| `infra/docker-compose.yml`: PostgreSQL + pgvector, OTel Collector | Sonnet feature builder | Own volumes and ports; nothing touches the existing Neo4j container |
| ClickHouse: `news` database, `news` user, settings profile and quota on the shared container | Sonnet feature builder, Opus review | Must not affect the other project's database |
| CI workflow (fast suite skeleton), then branch protection on `main` | Sonnet feature builder | |
| Cloudflare and AWS accounts, budget alarms, least-privilege keys | **Karthik** (manual) | Agents never create accounts or handle credentials; Karthik puts keys in the local secrets file |
| Secrets handling: `.env.example`, local secrets file loader, gitleaks pre-commit hook | Sonnet feature builder, Opus security review | |

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
10. **Lists and navigation**: LeadStory, StoryItem, SectionHead, PageContents, RelatedPages, Pagination.
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

Screenshot baselines live in the repo; any diff fails the run and must be approved by the Opus tester with a reason. The full matrix runs locally (it is CPU-heavy: one run at a time); CI runs a smoke subset (360 and 1440, light and dark, Chromium).

**Tester sign-off.** For each component the Opus tester records in its folder: areas covered, screens covered, issues found and fixed, and any accepted exceptions. Defects go back to the builder (or the Opus bug fixer after two rounds).

#### 3.5 Integration

- Fixture pages: one per page type (earnings, macro release, major report, policy decision, news story, science, explainer), built from real public data, rendered through the full renderer. The Opus tester runs the full matrix on these pages too.
- Karthik reviews the seven fixture pages side by side before the phase exits (the design system's "real-content test").

**Exit:** every component built, specified and signed off by an Opus tester across all areas and all screens; component index and specs available through the CLI stub; the seven fixture pages approved by Karthik.

### Phase 4 — Content system design

**Goal:** the researched design of discovery, scoring, clustering, selection, research packs and synthesis (Technical Design: led by Opus).

| Work | Agent |
|---|---|
| Source registry schema (licence, attribution, image policy) and the first 20 sources' entries | Opus architect, Sonnet docs writer for entries |
| Scoring model (importance, novelty, data richness, freshness, coverage gap) and its weights | Opus architect |
| Clustering and de-duplication (embeddings plus shared entities); choice of local ONNX embedding model, benchmarked on the laptop CPU | Opus spike agent |
| Breadth-first selection with quotas; tier and indexing priority rules | Opus architect |
| Research-pack format (the brief): fields, size budget (1–3k tokens), data IDs | Opus architect |
| Page-type skills (earnings, macro, report, policy, news, science, explainer) and the brand skill from `design/brand-skill.md` | Opus (judgement and prose) |

Can start in parallel with late Phase 3 (no shared files).

**Exit:** content-system design document and ADRs merged; skills drafted; brief format fixed.

### Phase 5 — Rust `news` CLI and connectors

**Goal:** every command in the Technical Design's toolbox, and the first connectors.

| Work | Agent |
|---|---|
| Cargo workspace architecture: `core` (models, data points, ledger), `store` (PostgreSQL), `telemetry` (ClickHouse events), `connectors`, `commands` | Opus architect |
| Core crates and the command framework (compact JSON output, idempotency, event logging) | Opus feature builder (one agent; everything depends on it) |
| Commands: discover, plan, fetch, parse, brief, components, component, related, save, render (calls the Bun renderer), check, publish, update, prune, stats | Sonnet feature builders (up to 3 in parallel, grouped so no two touch the same files) |
| Connectors, one per source, starting with SEC EDGAR (XBRL), FRED, IMF, World Bank, arXiv, and the central banks (Fed, ECB, RBI, BoE, RBA, BoJ) | Sonnet feature builders, one connector per task, in parallel |
| XBRL and unit/currency/fiscal-period parsing | Opus (correctness-critical) |
| PDF tables via Docling, scanned PDFs via Tesseract | Sonnet feature builder; Opus review |
| `news check`: render check, data references, value matching, links, required fields, axe-core run, design-system checks | Opus feature builder |
| Tests for commands and connectors (recorded fixtures, no live network in CI) | Sonnet test writer; Opus reviewer |

**Exit:** `news discover → plan → fetch → parse → brief` works for the first connectors; `render`, `check` and a dry-run `publish` work on fixture items; CI green.

### Phase 6 — Cloud

| Work | Agent |
|---|---|
| R2 bucket, custom domain `news.karthik.nz`, caching rules | Sonnet feature builder (scripts in `infra/`), Karthik applies DNS |
| DynamoDB single-table design and access patterns | Opus architect |
| D1 search shards and FTS5 schema | Sonnet feature builder, Opus review |
| `api` Worker (write API: secret key, schema validation, R2/DynamoDB/D1 writes, cache purges) | Sonnet feature builder, **Opus security reviewer** |
| `site` SSR (home, hubs, entity hubs, latest, search results) from the Phase 3 templates | Sonnet feature builder; Opus tester for template visuals |
| `search` Worker | Sonnet feature builder |
| `mcp` Worker (stateless MCP v2, spec revision 2026-07-28, read-only) | Opus feature builder (new spec), Opus security review |
| Rate limiting, `robots.txt`, `llms.txt`, sitemaps, RSS/Atom | Sonnet feature builder |
| Free-tier usage checks and alarms | Sonnet feature builder |

**Exit:** a fixture page published end to end and served from `news.karthik.nz`; hubs, search and MCP answering; security review passed.

### Phase 7 — Orchestration

| Work | Agent |
|---|---|
| Bun runner for `claude -p` workers (parallelism, retries, resume, JSON results) | Sonnet feature builder |
| Lean-call configuration: strict empty MCP config, allowed tools, max turns, model per call | Sonnet feature builder, Opus review |
| Prompts and cached prefix (instructions, brand rules, component index): measure cache hits | Opus (judgement) |
| Haiku spot-check flow and failure re-queue | Sonnet feature builder |
| First full run of 10 pages, then 100 | Orchestrator with Karthik |

**Exit:** one prompt produces 100+ pages that pass checks; token use per page logged.

### Phase 8 — Telemetry and dashboard

| Work | Agent |
|---|---|
| ClickHouse tables and materialised views | Sonnet feature builder, Opus review |
| OTel Collector pipeline for Claude Code telemetry | Sonnet feature builder |
| Local dashboard (bun web app, localhost only) using the design system's chart components | Sonnet feature builder; Opus tester for its charts |

**Exit:** dashboard shows pages, coverage, queue, tokens per page, failures, index budget, free-tier usage and traffic.

### Phase 9 — Launch and ramp

| Work | Agent |
|---|---|
| Launch checklist: legal pages, privacy note, methodology page, disclaimers, error-report form | Opus (prose readers see), Karthik approves |
| Indexing budget switch, Search Console, IndexNow | Sonnet feature builder |
| Launch-day review of 20 random pages against the design system and sources | Opus reviewer |
| Ramp: tune scoring, budget and prompts from telemetry | Opus with Karthik |

**Exit (PRD v1):** site live on `news.karthik.nz`; runs produce 100+ pages each.

## Risks in the development process

| Risk | Mitigation |
|---|---|
| OpenUI can't render on the server | Phase 1 spike before any component work; fallback renderer in the ADR |
| Parallel agents conflict | Worktrees per agent; file ownership per task; foundations merged before fan-out |
| Agents drift from the design system | Tokens-only lint; Opus reviewer checks against design docs; visual baselines; CodeRabbit path instructions |
| Visual test runs overload the laptop | One heavy run at a time; CI runs only a smoke subset |
| Usage limits stall a phase | Finish in-flight work, then pause; Opus only where it pays; tight briefs |
| Flaky screenshot tests | Fixed fonts (self-hosted), fixed viewport and device scale, animations off, fixed dates in fixtures |
| Secrets leak | Agents never handle credentials; gitleaks in pre-commit, CI and CodeRabbit |
| Hidden costs from free tiers | Budget alarms; usage tracked in the dashboard; no paid services added without Karthik's approval |

## Progress checklist

- [x] Phase 2: design system agreed and documented
- [x] Git repository, CodeRabbit config (light review), GitHub remote
- [ ] Phase 0: agent definitions, monorepo skeleton, local services, CI, branch protection, accounts (Karthik)
- [ ] Phase 1: OpenUI server-rendering spike and ADR
- [ ] Phase 3.1: foundations
- [ ] Phase 3.2–3.3: component families and specs
- [ ] Phase 3.4: Opus component testing sign-off, all areas and all screens
- [ ] Phase 3.5: seven fixture pages approved
- [ ] Phase 4: content system design and skills
- [ ] Phase 5: CLI and first connectors
- [ ] Phase 6: cloud services live
- [ ] Phase 7: first 100-page run
- [ ] Phase 8: telemetry dashboard
- [ ] Phase 9: launch

## Open questions

- Public or private repository long-term. It starts private; the open-data goal may favour making it public once secrets handling is proven.
- CI minutes: if the private repo's free minutes run short, move more checks to the local pre-push hook or make the repo public (unlimited Actions minutes).
- Whether routine PRs may auto-merge when CI and both reviews are green, or Karthik merges everything.
