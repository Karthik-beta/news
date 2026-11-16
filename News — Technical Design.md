# News — Technical Design

Nov 14, 2026 · @Karthik

## Summary

All work happens on the author's laptop: discovery, fetching, parsing, AI generation and rendering. The cloud only stores and serves finished pages. Each run produces 100+ pages, 2–3 runs a day, growing to 250k+ pages in about a year. Runs start on schedules Karthik sets, or when he presses Start in **Board**, the desktop app of the always-on Bureau service ([ADR 0007](docs/adr/0007-bureau-service-and-board.md)). Everything stays on free tiers and token use is kept to a minimum.

This document covers the technical side. Product scope lives in the separate PRD. The laptop side is called **Bureau** and is designed in full in the [Bureau design](News%20—%20Bureau%20Design.md); where the two overlap, this document gives the summary and links to the Bureau section that specifies it.

### Design principles

1. **Code before tokens.** Fetching, parsing, numbers, checks, rendering and uploads are done by code. The AI only decides what to say and how to lay it out.
2. **The model never writes numbers.** Pages reference data by ID; code fills in the values. This saves output tokens and prevents made-up figures.
3. **AI lives only on the laptop.** No model calls in the cloud, at build time or at request time.
4. **No site rebuilds.** Each page is rendered and uploaded on its own; only affected hubs are refreshed. The home page's curation is data the SSR Worker reads at request time, so curating it never rebuilds or redeploys anything.
5. **Publishing is separate from indexing.** Every page is public, but only a daily budget of pages is offered to search engines.
6. **A designed content system, not random generation.** Every page can be traced to why it was chosen and what it was built from.
7. **Everything is measured.** Every event goes to ClickHouse so the pipeline can be tuned.
8. **Room to grow.** Categories, sources, components and storage back-ends can be added without redesign.
9. **Always on, one control surface.** Bureau runs as a Windows service on the laptop; Board, a Rust desktop app, is where Karthik starts, approves, schedules and watches everything. Scheduled runs publish only pages that pass every check, and gated workflows wait for him in Board.

## System overview

&#91;embedded content: System overview · laptop, cloud, readers\]

The `news` CLI fetches and parses public sources into PostgreSQL and hands Claude a compact brief. Claude writes OpenUI Lang, the renderer turns it into a page bundle, and the CLI publishes through the write API into R2, DynamoDB and D1. Readers get static pages from the CDN, dynamic hubs from the SSR Worker, and search and MCP from their own Workers. Every step on the laptop writes events to ClickHouse.

## Local environment

The laptop is the whole production system: a Windows 11 machine with an Intel i5-8250U (4 cores, 8 threads, integrated GPU), 24 GB RAM and about 300 GB free disk. The design favours low CPU use and is generous with RAM and disk.

### Languages and runtimes

| Layer | Choice | Why |
| --- | --- | --- |
| CPU-heavy tools (fetch, parse, XBRL, compression, upload, indexing), orchestration and the AI runner | Rust, one compiled `news` binary that is also the Bureau service and Board | Lowest CPU per item; single file to run |
| Control surface (Board) | Rust, egui/eframe and egui_plot, inside the same `news` binary (`news board`) | Rust only and lean; themes generated from the design tokens |
| Rendering, Astro site, Workers code, glue | Bun (runtime and package manager); the page renderer runs as a warm local server the service calls | Fast JavaScript runtime for the React/OpenUI renderer |
| Hard PDF tables | Docling (Python), local | Best table quality; used only when needed, one at a time |
| Scanned PDFs | Tesseract OCR | Free, local |
| Embeddings for similarity and de-duplication | Small local embedding model (ONNX) on CPU | AI stays on the laptop; cheap at a few hundred items a day |
| AI | Claude Code CLI on the author's Teams Premium plan: Sonnet 5.5 and Haiku 5.5 (Opus 5.5 for development work) | Subscription only, no paid API |

### Containers (Docker)

| Container | Status | Use in this project | Isolation |
| --- | --- | --- | --- |
| PostgreSQL + pgvector (Apache AGE optional) | **New**, dedicated to this project | Working database, queue, ledger, taxonomy, knowledge graph, embeddings | Own container, own volume, own port |
| ClickHouse | **Shared** with another project | Telemetry and events | Own `news` database, own `news` user, settings profile limiting threads and memory, quota |
| OpenTelemetry Collector | New, small | Receives Claude Code telemetry and writes it to ClickHouse | Own container |

Board shows the health of every container. It can start and stop the News containers (PostgreSQL and the OTel Collector); the shared ClickHouse is only health-checked, never stopped.

PostgreSQL was chosen over a second Neo4j instance: the workload is mostly relational with shallow graph lookups (1–3 hops) and similarity search, which PostgreSQL with pgvector handles in one engine with less RAM. A lean Neo4j Community instance can be added later if graph algorithms are needed. The existing Neo4j container is not used.

### Local folders

| Folder | Holds | Pruned after publish? |
| --- | --- | --- |
| `work/raw/sha256/…` | Downloaded source files (PDF, HTML, XBRL, JSON) exactly as received, content-addressed by SHA-256 and zstd-compressed (Morgue, [Bureau design §3.2](News%20—%20Bureau%20Design.md#32-wire-stringers-runner-morgue)) | No: structured data (JSON, XBRL, CSV, SDMX) kept 400 days, HTML and PDF 90 days, the hash, URL and locator for ever, so every figure stays provable (Bureau design, decision D7) |
| `work/parsed/` | Normalized data per item | Yes |
| `work/briefs/` | Compact briefs given to the AI | Yes |
| `work/pages/` | Generated OpenUI Lang, page bundles, rendered output | Yes, once uploaded |
| `site/` | Astro project, component library, brand tokens | No (source code) |
| PostgreSQL and ClickHouse data | Graph, ledger, taxonomy, telemetry | No |

### The Bureau service and Board

Full design: [Bureau design §8.1](News%20—%20Bureau%20Design.md#81-board-and-the-bureau-service) and [ADR 0007](docs/adr/0007-bureau-service-and-board.md).

- **The Bureau service** (`news service run`) is a Windows service that logs on as Karthik's account, because `claude -p` uses his Claude login and Docker runs under his user. It hosts Chief (runs, gates, the queue for heavy runs), Rota, the Dispatch workers for every pool and Ghost, the AI runner. Runs continue with Board closed. `news service install|uninstall`, run by Karthik in his terminal, installs it (Windows asks for his password) or removes it. Updates are started from Board only and wait for a safe point; runs resume from PostgreSQL.
- **Board** (`news board`) is a pure-Rust egui desktop app in the same binary. It opens minimised at log-on, follows the Windows light or dark setting with a manual override, and talks only to the service. If the service is down it shows an offline banner, a "Start service" button and the last error. Notifications are native Windows toasts; clicking one opens Board on that item.
- **Control** goes over the **Hotline**, a named pipe locked to Karthik's user and to local clients. Starts, approvals and every other high-tier (H) action are accepted only from Board; his terminal (the `news` CLI) can read status and run the low-risk tiers only, and the service refuses everything but reads from any process it spawned for `claude -p`. Nothing is exposed to the network.
- **Names** (Karthik, 16 Nov 2026): **Rota** is the scheduler, **Hotline** the control pipe, **Plate** the warm Bun render server, **Lineup** the heavy runs waiting for the slot, **Standing** the versioned settings store (`news standing`).

## Content system

Pages come from a designed system, not random generation: every page records why it was selected and what it was built from. This section is a summary; the full design is [Bureau design §3 Subsystems](News%20—%20Bureau%20Design.md#3-subsystems) and [§4 Scoring and ranking](News%20—%20Bureau%20Design.md#4-scoring-and-ranking-in-full).

&#91;embedded content: Content system · 10 stages with a feedback loop\]

Code runs every stage except page writing (Reporter). Scoring, ranking and selection are fully code. Haiku is used only for ambiguous classification (A3), grey-zone story matching (A4) and extractive, digit-free condensation of long source text into the brief (A5). Telemetry and Karthik's weekly pairwise labels (the Huddle workflow) feed back into the scoring weights.

| Stage (Bureau name) | What it does |
| --- | --- |
| Contacts | Source registry: licence, attribution, rate limits, source tier |
| Wire (Stringers, Runner, Morgue) | One Stringer (connector) per source discovers new items; the shared Runner fetches politely (conditional GET, rate limits, `Retry-After`, robots.txt); Morgue stores the exact bytes received |
| Diary | Release calendars from official ICS feeds, APIs and pages, so releases are pre-warmed and captured on schedule by code |
| Clerk, Ledger, Roster | Parse snapshots into data points with values exactly as published, a vintage and a locator (Ledger); resolve entities by identifier and alias (Roster); emit candidates with event keys |
| Taster | Story detection: event keys first, then SimHash and local embeddings for unstructured items; removes duplicates; recognises updates to existing pages |
| Nose | Scores each story 0–100 from importance, magnitude or surprise, freshness, data richness, novelty and coverage gap, with a Pitch explaining every component |
| Desk | Fills the run breadth-first with diminishing returns per category, region, page type and entity against `NEWS_TARGET_PAGES`; assigns tier (the Budget) |
| Fixer | Builds the compact brief (1–3k tokens) with data references, code-computed context lines and fenced source text |
| Reporter | Sonnet writes the headline, summary and OpenUI Lang with data references only |
| Proof | Renders and runs code checks C1–C14 on every page, plus a Haiku sample check |
| Press | Publishes through an outbox with idempotency keys; applies the index budget |
| Ticker | Events, spans and AI-call records to ClickHouse; signals flow back into scoring |

**Updates:** when a source revises data (restatements, GDP revisions), Rewrite finds the affected pages; value-only changes are re-rendered by code and only direction flips need the AI (see [Updating a page](#updating-a-page)).

## Agent orchestration and token optimisation

Every run is driven by **Chief**, a state machine inside the Bureau service, so runs are durable, resumable, and their approval stops are enforced by code rather than by a prompt. A run starts when Karthik presses Start in Board, or when a schedule he set in Board fires; a schedule counts as his approval to start. The main run is the Edition workflow, 2–3 times a day. No chat session drives anything ([ADR 0007](docs/adr/0007-bureau-service-and-board.md); Bureau design, decisions D17–D21). Full design: [Bureau design §5 Orchestration engine](News%20—%20Bureau%20Design.md#5-orchestration-engine) and [§8 Board, the Bureau service and the preset workflows](News%20—%20Bureau%20Design.md#8-board-the-bureau-service-and-the-preset-workflows).

### How a run works

1. An Edition starts from a schedule Karthik set, or from Start in Board with its arguments (`target`, `categories`, `review_plan`). Starts and approvals reach the service only over the Hotline and only from Board; nothing else can start a run.
2. Chief runs the code steps at zero tokens: discover, normalise, detect stories, score and select (`news plan` is one of these steps; the target comes from the `NEWS_TARGET_PAGES` setting and is a suggestion, not a hard limit), fetch, parse and brief.
3. At an AI step, **Ghost**, the AI runner inside the service (Rust), claims the AI jobs, runs one `claude -p` per item, `NEWS_PARALLEL` (3) at a time (the laptop mostly waits on the network), and stores each result through `news ai accept`, which validates it. Board shows the progress.
4. Code renders through Plate, the warm local render server, checks (Proof), publishes through the outbox (Press), applies the index budget and prunes. A scheduled Edition publishes the pages that pass Proof without stopping, unless its schedule switches the plan review on.
5. Where a workflow has an approval stop, Chief stores a code-rendered summary, Board raises a toast, and the run waits; Karthik answers in Board with Approve, Request changes (with a note) or Reject, bound to the hash of exactly what he saw. The model never records a decision.
6. If the Claude plan's usage limit is reached, the run pauses, Board raises a toast, and the run resumes on its own when the limit resets.
7. Every run ends with a Wrap (run summary) written by code.

Jobs, runs, steps and approvals live in PostgreSQL in **Dispatch**, a small custom job engine on `FOR UPDATE SKIP LOCKED` with River-style retries, snoozes and leases (Bureau design, decision D1). Only one heavy run (one that writes or re-renders pages in bulk) runs at a time. Another heavy run queues for the slot: a scheduled one waits, and is skipped and noted if it would start more than about 2 hours late; a manual Start offers "Queue it" or "Pause the running one and start this" (Bureau design, decision D20). Quiet hours and a Bureau token cap with a reserve, both Karthik's settings, keep runs off the laptop and the plan when he needs them ([Bureau design §5.6](News%20—%20Bureau%20Design.md#56-scheduling), [§7.4](News%20—%20Bureau%20Design.md#74-quota-guard)).

### Model roles

Every AI step is a contract: typed input built by code, a JSON Schema for the output, a code validator, a fallback that needs no AI, and a budget. The full list (A1–A12) is the [Bureau design §7 AI usage ledger](News%20—%20Bureau%20Design.md#7-ai-usage-ledger).

| Model | Used for |
| --- | --- |
| Haiku 5.5 | Ambiguous classification only (A3); grey-zone story matching (A4); extractive, digit-free condensation of long source text with verifiable quote spans (A5); the sample check (A7); triage of reader error reports (A9). Optional entity shortlist (A6) and alt text (A10) are off at launch (Bureau design, decision D12) |
| Sonnet 5.5 | Writing and repairing pages, including updates after a direction flip (A1, A2): what the page says, layout in OpenUI Lang, narrative text; home page curation drafts (A8, `NEWS_MODEL_CURATOR`); Rollcall's proposals (A11); eval judges (A12). Models are per-step settings in Board, starting at these values |
| Opus 5.5 | Development time only: building the component library, designing the content system, writing skills |

Selection is fully code: there is no model call for "selection judgement". Disagreements with the scorer are handled by Karthik's weekly pairwise labels.

### Keeping each `claude -p` call lean

Ghost spawns every page call with the flags below; the exact call and the reason for each flag are in [Bureau design §3.9 Reporter](News%20—%20Bureau%20Design.md#39-reporter). The combination is confirmed in the Phase 7 lean-call spike. Every call also runs in a Windows Job Object the service owns (the spawn marker), so it and anything it starts can never send a control command.

- **No tools:** `--tools ""` disables every built-in tool. (`--allowedTools` only pre-approves tools so they run without a permission prompt; it does not restrict which tools exist, so it is not used for this.) Reporter needs no tools because code pre-selects the component specs (Bureau design, decision D4).
- **No MCP servers, hooks or plugins:** `--strict-mcp-config` with an empty config, so no tool schemas fill the context and no project MCP servers load; an explicit `--settings` file and `--setting-sources` keep project hooks and plugins out of page calls; `--disable-slash-commands`; `--no-session-persistence`.
- **Subscription only:** Ghost unsets `ANTHROPIC_API_KEY` in the child environment (if it is set, `-p` uses it instead of the subscription); never `--bare`, which does not read subscription credentials; `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1`; an allow-listed environment.
- **Bounded output:** `--json-schema` with the page-draft schema, so the harness validates and returns structured output; `--max-turns 2`; `--max-budget-usd` as a runaway guard (a client-side estimate, not a bill); an explicit `--model` per call.
- **Identical prompt prefix** (instructions, brand rules, one page-type skill, the component index and that page type's pre-selected component specs) in one file named by its hash; Ghost orders each batch by page type so calls with the same prefix run back to back and prompt caching stays warm.
- **Small input:** a compact brief (about 1–3k tokens, hard cap 4k) with data references, never raw documents.
- **Small output:** headline, summary and OpenUI Lang with numbers only as data references.
- **Component catalogue in the prefix:** code chooses the specs from the page-type skill's typical layout (`news component <type>`) and puts them in the cached prefix; a repair round supplies any missing spec.
- **`--output-format json`** so token usage for every call is logged to ClickHouse.

### Rough token budget per page

| Part | Tokens (approx.) |
| --- | --- |
| Cached prefix (instructions, brand rules, page-type skill, component index, pre-selected component specs) | 6–10k, mostly cache reads |
| Brief | 1–3k |
| Output (headline, summary, OpenUI Lang) | 2–4k |

These are planning estimates; ClickHouse telemetry will show the real numbers per page type. The per-run ceiling is `NEWS_AI_RUN_TOKENS`.

## Agent toolbox: the `news` CLI

One Rust binary gives agents and code everything they need, with short help text and compact JSON output. The same binary is the Bureau service (`news service`) and Board (`news board`). Inside `claude -p` calls it is the AI's toolbox (read and run-scoped commands only); in Karthik's terminal it reads status and runs the low-risk tiers for debugging; it has no high-tier (H) form. A CLI costs far fewer tokens than a large MCP tool catalog loaded into every session; each command can print its input and output JSON Schemas (`news schema <command>`), so a read-only MCP adapter could be generated later without a second implementation (Bureau design, decision D3). Page-writing calls use no tools at all.

Every command follows the same conventions ([Bureau design §6](News%20—%20Bureau%20Design.md#6-agent-tool-catalogue)): stdout carries exactly one JSON envelope (`{"ok": true, "data": …}` or `{"ok": false, "error": {"code", "message", "retryable", "hint"}}`); anything shown to Karthik also comes back as code-rendered `text`, printed verbatim; exit codes are 0 ok, 2 usage, 3 not found, 4 conflict or precondition, 5 retryable, 6 validation failed, 7 gated, 10 internal bug. Each command has a permission tier: R (read-only), W (run-scoped writes), G (gated: runs only as an authorised step of a live run, for example `news publish` and `news home publish`) and H (Karthik only, and only from Board over the Hotline: starting runs, answering gates, pausing, cancelling, home curation rollback and the other maintenance actions; never from the terminal, a script or a process the service spawned for `claude -p`).

| Command | Does (code, no tokens) | Returns to the AI |
| --- | --- | --- |
| `news discover` | Runs source connectors; stores new candidates | Count of new candidates |
| `news plan` | Scores, clusters and selects items breadth-first against `NEWS_TARGET_PAGES` | Batches of item IDs |
| `news fetch <id>` | Downloads source files and API data | Status only |
| `news parse <id>` | XBRL, JSON, CSV, PDF text and tables, embeds and images | Status only |
| `news brief <id>` | Builds the compact research pack | The brief (about 1–3k tokens) |
| `news components` | Lists the component index | One line per component |
| `news component <type>` | Full spec of one component (props, data shape, example) | Spec |
| `news related <id>` | Related entities and pages from the graph | Short list |
| `news save <id> <file>` | Stores the AI's OpenUI Lang and text for an item | Status |
| `news render <id>` | Merges data, renders HTML, Markdown and JSON | File paths |
| `news check <id>` | Code checks (see Quality checks) | Pass or errors |
| `news publish <id>` | Uploads to R2 and DynamoDB, updates search, purges hub caches | Page URL |
| `news update <page_id>` | Loads an existing page for revision | Current page and new data |
| `news prune` | Deletes working files for published items | Count |
| `news stats` | Run and coverage summary | Short table |
| `news home candidates` | Lists pages eligible for the home page (published since the last curation, plus still-relevant earlier Tier 1 pages) with tier, categories, region, page type, freshness, key figures and recent traffic | Compact candidate list |
| `news home save <file>` | Stores a draft curation from the curator agent | Draft ID |
| `news home check <draft>` | Code checks on a draft curation (see Home page curation) | Pass or errors |
| `news home preview <draft>` | Renders the home page locally with the draft, using the same SSR code as production | Local URL |
| `news home publish <draft>` | Sends the approved curation to the write API; purges home and region-view caches | Version number |
| `news home rollback <version>` | Makes an earlier curation live again | Version number |
| `news home show` | The live curation and its age | Short table |

Command groups added by the Bureau design, each a tool family named after the part of Bureau that owns it (full inputs, outputs, errors and tiers in [Bureau design §6.4](News%20—%20Bureau%20Design.md#64-catalogue)):

| Group | Owner | Commands |
| --- | --- | --- |
| `news run` | Chief | `presets`, `status` (the status board), `lineup` (the Lineup), `list`. Start, gate answers, pause, resume and cancel are Board actions with no CLI form |
| `news jobs` | Dispatch | `list`, `dead`, `claim`, `heartbeat`, `complete`, `fail`, `retry`, `pause`, `unpause` |
| `news service` | Bureau | `run` (the service itself), `install`, `uninstall`, `update` |
| `news board` | Board | Opens the desktop app, optionally on one run, gate or source |
| `news standing` | Bureau | `show`, `history`: Standing, the versioned settings, schedules included. Changing them is a Board action |
| `news ai` | Ghost | `input`, `accept` (schema and semantic validators), `schema`, `fallback` |
| `news sources` | Contacts | `list`, `show`, `health`, `probe`, `enable`, `disable` |
| `news calendar` | Diary | `upcoming`, `sync`, `late` |
| `news stories` | Taster | `show`, `members`, `similar`, `merge`, `split` |
| `news scores` | Nose | `explain` (the Pitch), `weights`, `propose`, `label`, `adopt` |
| `news entities` | Roster | `resolve`, `show`, `unresolved`, `alias` |
| `news eval` | Critic | `run`, `report`, `label`, `promote` |
| `news review` | Huddle | `weekly` (the Debrief) |
| `news corrections` | Errata | `pull`, `show`, `resolve` |
| `news schema <command>` | All | Input and output JSON Schemas of any command |

Every command writes events to ClickHouse. Commands are idempotent (each step is keyed on its inputs), so a failed run can resume.

## Data model

### Local PostgreSQL (working and knowledge store)

| Table / area | Holds | Kept after prune? |
| --- | --- | --- |
| `sources` | Source registry (Contacts): connector, base URL, licence, attribution, image policy; extended with publisher, source tier, redistribution flag, rate limit, concurrency, daily quota, free-key env var name, calendar kind and health | Yes |
| `candidates` | Every discovered item, normalized, with status; extended with event key, story, documents, entities, page type and who classified it (rule, AI or human) | Yes (compact row) |
| `clusters` | Groups of candidates that are the same story | Yes |
| `queue` | Selected items: tier, priority, status, attempts | Yes |
| `ledger` | Source URL, content hash, page URL, publish date, version | Yes (prevents reprocessing) |
| `entities` + `edges` | Knowledge graph: companies, people, countries, institutions, topics, sources, pages and their links | Yes |
| `embeddings` (pgvector) | Vectors for candidates and pages, for similarity and de-duplication | Yes |
| `taxonomy` | Category tree (parent, slug, name, order); pages link to many categories | Yes |
| `data_points` | Figures with IDs used by page references (Ledger); extended with the verbatim published value, vintage, supersedes link, snapshot hash and locator inside the snapshot | Yes: pages reference them and revisions need their history |
| `home_curations` | Draft and published home curations: slots, chosen figures, the curator's internal rationale, check results, approval and publish times | Yes |

Tables added by the Bureau design (sketches in [Bureau design §3](News%20—%20Bureau%20Design.md#3-subsystems) and [§5](News%20—%20Bureau%20Design.md#5-orchestration-engine); all in one schema, created by numbered migrations in `cli/` and owned by the Rust CLI):

| Area | Tables |
| --- | --- |
| Wire and Diary | `snapshots`, `fetch_state`, `calendar_events` |
| Clerk and Ledger | `documents`, `text_blocks`, `derived_points` |
| Roster | `entity_ids`, `entity_aliases` |
| Taster, Nose, Desk | `stories`, `scores`, `run_plans` (the Budget), `event_weights` |
| Dispatch and Chief | `jobs`, `pools`, `schedules`, `runs`, `run_steps`, `approvals` (the first design's `start_tickets` is dropped, ADR 0007) |
| The Bureau service | Standing, the versioned settings table: every change with who (always `board`), when, and old and new value; preset schedules are settings. Each run records the settings version it started with |
| Press | `publish_outbox`, `pages`, `page_refs` |
| Analytics | `traffic_daily` |

The TD's `clusters`, `queue` and `ledger` tables overlap with Bureau's `stories`, `jobs` and `run_plans`, and `snapshots`, `fetch_state` and `pages`; which of them survive is settled by the Phase 4 ADRs. "Ledger" in the Bureau design names the data point store (`data_points`, `derived_points`), not the TD's `ledger` table.

### Taxonomy that can grow

- Categories are rows, not code: adding, splitting or renaming one needs no deploy of the renderer.
- A page can belong to several categories and sub-categories.
- **Page URLs carry no category:** `/2026/11/apple-fy26-q4-results`. Only hub URLs contain category slugs, and renamed hubs get redirects.

### Page bundle (the unit of publishing)

| File | Content |
| --- | --- |
| `page.oui` | OpenUI Lang written by the AI, with data references |
| `data.json` | All values referenced by the page, with source IDs |
| `sources.json` | Source records: URL, document, page or XBRL tag, licence, retrieval date |
| `index.html` | Rendered page |
| `index.md` | Markdown version for LLMs |
| `meta.json` | ID, title, summary, categories, entities, tags, region, dates, version, index status, renderer version |

### DynamoDB (cloud database of record)

Single-table design, because the free 25 read and 25 write capacity units are shared by every table and index in the account.

| Item | Partition key | Sort key |
| --- | --- | --- |
| Page record | `PAGE#<id>` | `V#<version>` and `LATEST` |
| Hub listing entry | `HUB#<category or entity>` | `<date>#<page id>` |
| Entity | `ENT#<id>` | `META` |
| Taxonomy node | `TAX#<id>` | `META` |
| Redirect | `URL#<old path>` | `REDIRECT` |
| Home curation | `HOME#CURATION` | `V#<version>` and `LATEST` |

- Items stay small (metadata, `.oui`, summary). Large data goes to R2 with a pointer, because DynamoDB items are capped at 400 KB.
- At most one secondary index, planned carefully, because it shares the free capacity.
- Writes are paced at publish time to stay within write capacity.

## OpenUI and the component library

Pages are written in OpenUI Lang, the open-source (MIT) format from Thesys, rendered with its React renderer. Its makers report 50–67% fewer output tokens than JSON UI formats. Saved OpenUI Lang text renders without any model call.

**OpenUI Dashboards is not used.** It is a managed Thesys product that issues an API key, which conflicts with "no paid services" and "no AI in the cloud". The dashboard capability is built instead.

### Component library

- **Base:** the open-source `@openuidev/react-ui` library (about 44 components: layout, tables, several chart types, cards, images).
- **Extension:** hundreds of custom components built with Opus 5.5 using `createLibrary` and `defineComponent`, covering at least what OpenUI Dashboards offers: KPI grids, Sankey, waterfall, segment breakdowns, time series with annotations, maps, timelines, comparison tables, quote and source cards, image galleries, video embeds, finding cards for science, and more.
- **Every component accepts data references,** not inline numbers, and records its source IDs for citations.
- **Every component renders on the server** to HTML, and only interactive parts load JavaScript in the browser.

### Staying on-brand with free layouts

- Brand lives in **design tokens** (type, colour, spacing, dark mode) and in the components themselves. Any layout the model chooses still looks like the site.
- A short **brand skill** gives principles and a few example pages, not templates.
- **Sonnet decides the layout freely** for each page, according to the content and its type.
- The design direction must be discussed with the author before any design work, using the `frontend-design` skill first.

### Component index and details

- `news components` returns one line per component (name, purpose, data shape), kept short enough for the cached prefix.
- `news component <type>` returns full props, data shape and one example, only for the components chosen.

## Rendering, publishing and caching

The laptop is the build machine, one page at a time. There is no site-wide rebuild.

### Static article pages

1. The Astro project is built only when the design changes. That build produces the shell: CSS, JavaScript for interactive components, layouts and fixed pages (about, methodology).
2. On publish, a bun script renders only the new or updated page with the same components (Astro's Container API or React server rendering) into the page bundle.
3. The bundle is compressed and uploaded to R2. Article pages are served as static files from R2 on `news.karthik.nz` through the Cloudflare CDN, so a page view uses no Worker or database.

### Dynamic pages (SSR)

- **Home, category and sub-category hubs, entity hubs, region views, "latest" and search results** are rendered on request by an Astro SSR Worker reading DynamoDB.
- Responses are **cached at the edge for hours or days.**
- On each publish, only the affected URLs are purged: the page's hubs, its entity hubs, "latest" and home. Purges are batched to stay within Cloudflare's purge-rate limits.
- **The home page is curated data, not a build.** The SSR Worker reads the live curation (`HOME#CURATION` / `LATEST`), batch-reads the page records for its slots and the newest entries of `HUB#latest`, and renders the home page from those. Publishing a curation writes a new version and purges the home page and its region views; nothing is rebuilt or redeployed.

### Home page curation

The home page's lead story, side stories and section rows are chosen by a curation agent every 2–3 days, when **Frontpage**, the home curation preset, runs on Karthik's schedule or from Start in Board ([Bureau design §8.6](News%20—%20Bureau%20Design.md#86-frontpage-home-curation)). The agent runs on the laptop like every other AI step; the cloud only stores and serves the result.

**The workflow (Frontpage)**

1. **Gather.** `news home candidates` returns a compact list of at most 40 eligible pages, pre-selected by code with the same bucket-diversity rule as run selection ([Bureau design §4.5](News%20—%20Bureau%20Design.md#45-index-priority-and-home-candidates)): published since the last curation, plus still-relevant earlier Tier 1 pages, with tier, categories, region, page type, age, key figures (by data ID), mini-chart options and recent traffic from analytics.
2. **Curate.** The curator (A8: a `claude -p` call through Ghost with no tools, its input built by code from the candidates; Sonnet 5.5 by default, `NEWS_MODEL_CURATOR`), using the `news-home-curation` skill, returns a draft curation as JSON: one lead, two side stories, 3–5 ordered sections with 3–4 stories each, the figure or mini chart each slot shows (chosen from the page's existing data IDs), and a one-line internal rationale per pick (stored, never shown). It writes no new reader-facing text.
3. **Save and check.** `news home save` stores the draft in PostgreSQL; `news home check` validates it in code:
   - every page exists, is published and is not withdrawn; no page appears twice;
   - every chosen figure or chart resolves to the page's own data;
   - breadth: at least three categories and three regions across the curated slots, no more than two stories about the same entity, at least one non-finance section;
   - freshness: the lead is no older than 7 days unless it is a Tier 1 page the curator explicitly marks as still leading;
   - every slot filled; the draft fits the home template's slot counts.
   Failures go back to the curator with the errors (code calls it again; at most two rounds).
4. **Preview.** `news home preview` renders the home page locally from the draft with the same SSR code as production, light and dark, at phone and desktop widths; Board shows the previews.
5. **Approve.** Frontpage stops at its `publish` gate, and Board raises a toast. In Board, Karthik sees the preview and the code-rendered summary and answers Approve, Request changes (with a note) or Reject. Changes go back to the curator with the note; nothing is published without approval, and the approval is bound to the hash of the exact draft shown. A scheduled Frontpage waits here too.
6. **Publish.** `news home publish` (a G-tier step that runs only under that approval) sends the curation to the write API (`PUT /v1/home/curation`), which validates it again, writes `HOME#CURATION / V#<n>` and moves `LATEST`, then purges the home page and region views. Karthik can make an earlier version live from Board's published-content actions (H tier, Board only).

**Between curations**

- Curated slots stay as published. The automatic "Latest" list (newest 8–10 pages from `HUB#latest`) updates with every page publish.
- A curated page that is withdrawn or deleted is skipped at render time; its slot is filled from the best recent page of the same section by tier and freshness.
- A curated page that is updated shows its updated title, summary and figures, because slots point at page IDs, not copies.
- Region views show the curated stories for that region, topped up with that region's latest pages.
- Board flags a curation older than `NEWS_HOME_STALE_DAYS` (default 4).

**Curation record** (DynamoDB item, a few KB): version, created and approved times, curator model, the slots (`lead`, `side[2]`, `sections[{category, pages[{page_id, figure_id?, chart_id?}]}]`), and the check results. Rationale stays in PostgreSQL only.

### Updating a page

When a source revises or restates data, Rewrite finds the affected pages through their data references and code classifies the change ([Bureau design §3.11](News%20—%20Bureau%20Design.md#311-rewrite)):

- **Value only:** code re-renders the page with the new value. The prose holds no numbers, so it does not change and no AI is needed.
- **Direction or threshold flip** (a change crosses zero, a "lowest since …" line stops being true): Reporter rewrites the page in update mode with the old page, the diff and the brief.
- **Restatement of a prior period:** code re-renders and adds the "restated" note.

The page is re-rendered under the same URL, checked, the previous version is kept in DynamoDB, and the page shows "Updated on …". Corrections from reader reports go through the Errata workflow, started from Board.

### Renderer versions

Each page records the renderer version that produced it. When components improve, pages can be re-rendered in batches from their stored `.oui` and data without a site rebuild (the Reprint workflow, started from Board).

## Cloud services

The cloud stores and serves; it runs no AI and no content generation.

| Service | Role |
| --- | --- |
| Cloudflare R2 | Static article pages (HTML, Markdown, JSON, CSV), large `data.json` files, site assets |
| Cloudflare CDN | Serves R2 files and caches SSR responses at the edge |
| Cloudflare Workers | Astro SSR (home, hubs, latest, categories); write API; search API; MCP v2 server |
| Cloudflare D1 | Full-text search index (sharded); small lookups |
| AWS DynamoDB | Database of record: page records and versions, hub listings, entities, taxonomy, redirects, home page curations |
| Cloudflare Web Analytics | Page analytics, cookieless (beacon behaviour checked at build; server-side zone analytics as fallback) |

### Workers

| Worker | Purpose | Notes |
| --- | --- | --- |
| `site` | Astro SSR for dynamic pages, including the curated home page | Reads DynamoDB (pages, hubs, live curation); edge-cached for hours or days |
| `api` | Write API used by `news publish` and `news home publish` | Single secret key held on the laptop; schema-checked input; writes R2, DynamoDB, D1; triggers cache purges |
| `search` | Public search API | Queries D1; popular queries cached at the edge |
| `mcp` | MCP v2 server (stateless, spec revision 2026-07-28) | Read-only |

Workers on the free plan get very little CPU per request, so heavy work never runs in a Worker: search runs inside D1 and rendering happens on the laptop. DynamoDB is called from Workers over HTTPS with signed requests.

## Search

Phase 1 uses Cloudflare D1 full-text search (SQLite FTS5), behind the project's own `/search` API so the engine can be swapped without changing anything readers see.

- **What is indexed:** title, summary, entities, tags, categories, region and date (about 2–4 KB per page), not full body text.
- **Sharding:** one D1 database per year or category group, because each free database is capped at 500 MB (up to 10 databases per account).
- **Writes:** `news publish` adds or updates the page's search rows.
- **Reads:** keyword search with filters by category, region, type and date; popular queries cached at the edge.
- **Limit:** the free plan allows 5 million rows read and 100,000 rows written per day, and queries fail once the daily cap is hit. The search page shows a friendly message if that happens.

**Upgrade path:** if search traffic outgrows D1, move to AWS Lambda with a Tantivy index built locally in Rust and stored in R2, behind the same API. That allows full-text search and better ranking. Semantic (AI) search is out of scope, because AI stays on the laptop.

## Open access

### Formats

Every page is available as HTML, `index.md`, `data.json` and per-chart CSV at the same path. Site-wide: `llms.txt`, XML sitemaps (indexed pages only), RSS/Atom feeds per category.

### MCP v2 server

A read-only, stateless MCP server on a Worker, following the 2026-07-28 spec revision. Proposed tools:

| Tool | Returns |
| --- | --- |
| `search_pages` | Matching pages with title, summary and URL |
| `get_page` | A page as Markdown or JSON |
| `list_categories` | The category tree |
| `get_entity` | An entity and its pages |
| `get_data` | The data behind a page or chart |

### Bulk dumps

Still to decide. Leading options: a Hugging Face dataset (free public hosting that AI users already use) with an Internet Archive mirror, or GitHub Releases (2 GB per file). Dumps are built on the laptop from DynamoDB and R2 and uploaded on a schedule (for example monthly).

### Rate limits

Per-client limits start within free-tier capacity and are raised later. Static files on R2 carry no Worker cost; only search, MCP and SSR hubs count against the daily Worker quota, which edge caching protects. Heavy users are pointed to the bulk dumps.

## Indexing control and domain reputation

Publishing and indexing are separate decisions, to protect the reputation of `karthik.nz`. Google publishes no safe page count for large-scale content; its scaled-content policy judges whether each page serves a distinct purpose.

- **Every page is published** and open to people, AI tools and MCP clients.
- **Only an indexing budget is offered to search engines** each day: `NEWS_INDEX_PER_DAY`, starting low (for example 20).
- Pages outside the budget get `noindex` and stay out of the sitemap. They are promoted later, best first.
- **Selection for indexing** favours Tier 1 pages and pages with the most unique data, visuals and sources.
- **Ramp-up:** the budget is raised in steps only while Search Console shows pages being indexed, not piling up as "Crawled – currently not indexed".
- Index status is stored on each page record and logged to ClickHouse.

## Quality checks

There is no per-page manual approval: Karthik's Start in Board, or a schedule he set, is the approval for an Edition and authorises publishing; a scheduled Edition publishes only the pages that pass every check. Quality is protected by cheap automatic checks (Proof; full list in [Bureau design §3.10](News%20—%20Bureau%20Design.md#310-proof)).

1. **Code checks on every page (zero tokens):**
   - The page parses and renders (C1).
   - Every data reference resolves, and every figure has a source (C2).
   - Values match the saved source data; derived values are recomputed from their inputs (C3).
   - **Numeric-literal lint** (C4): after the data references are removed, model-written prose contains no digits and no written-out quantities, apart from a short allow-list of names such as G20.
   - **Direction words** (C5): "rose", "fell", "cut" and similar agree with the sign of the referenced change.
   - **Quotes verbatim** (C6): every quote is an exact substring of its source text block.
   - **Period and unit labels** (C7) exactly as the source writes them.
   - **Licence** (C8): no data files for sources that forbid redistribution; attribution for every source.
   - Required fields (title, summary, alt text, categories) and disclaimers are present (C9).
   - Links, image URLs and video embeds respond (C10).
   - Banned words and tone, size and structure, and accessibility (C11–C13).
   - **No duplicate page** (C14): a page with the same event key or slug must be an update, not a new page.
2. **Haiku sample check** (A7; `SAMPLE_RATE`, starting at 1 in 20, plus every Tier 1 page in the first month): it compares the page's Markdown with its brief and returns typed issues (unsupported claim, wrong direction, missing context, tone, attribution).
3. **Failures** are logged to ClickHouse. A Copy that fails validation or a check in its own text gets one repair call (A2); a second failure sends the item back to the queue for the next run. A Tier 2 item that fails twice may be published as a code-only templated page, kept `noindex` until a later run upgrades it (Bureau design, decision D10).
4. Pages failing code checks are never published. A high-severity sample-check flag blocks an unpublished page; on a published page it queues an update.
5. **Approval stops** exist only where a workflow declares one: Frontpage's `publish` gate (`news home check` runs the code checks, then Karthik approves the local preview before `news home publish`), and the gates the other presets declare, such as Catchup's `plan` and Audit's `promote` (full list in [Bureau design §8.1](News%20—%20Bureau%20Design.md#81-board-and-the-bureau-service)). Karthik answers every gate in Board.

## Telemetry and local analytics

Every event is recorded in ClickHouse (database `news`, on the shared container) so the pipeline can be analysed and tuned later.

### Event sources

- **`news` CLI:** every stage (discover, plan, fetch, parse, brief, save, render, check, publish, prune, update, and the home curation stages: candidates, curate, check, preview, approve, publish, rollback) with duration, status, item, run and error.
- **Claude Code:** OpenTelemetry export (tokens, cache reads, cost estimates, tool calls) through an OTel Collector into ClickHouse.
- **`claude -p` results:** per-call usage from `--output-format json`, tagged with run and item IDs.
- **Cloud:** Cloudflare Web Analytics and Worker/R2/D1/DynamoDB usage pulled through their APIs.

### Core table: `events`

| Column | Example |
| --- | --- |
| `ts` | Event time |
| `run_id`, `item_id`, `page_id` | Links events across stages |
| `stage`, `event` | `generate`, `completed` |
| `model` | `sonnet-5.5` |
| `tokens_in`, `tokens_out`, `cache_read`, `cache_write` | Token counts |
| `duration_ms`, `status`, `error` | Timing and outcome |
| `attrs` | JSON for anything else (page type, tier, category, region) |
| `job_id`, `attempt` | The Dispatch job and its attempt (added by the Bureau design) |
| `preset`, `step` | The workflow (preset) and its step, for example `edition`, `write` |
| `source_id`, `error_class` | The source and the error class (`transient`, `rate_limited`, `ai_invalid_output` …) |
| `prompt_version` | The prompt version of an AI step |
| `trace_id`, `span_id` | Links events to OpenTelemetry traces (one trace per run, spans per step and job) |

The added columns are fields that are filtered and grouped on; until now they were reachable only in `attrs`. New tables, ordered for their main query: `ai_calls` (one row per `claude -p` call: step, model, prompt version, tokens, cache reads and writes, turns, validation result), `fetches` (one per HTTP request), `scores` (every score computed, selected or not), `check_results` (one per check per page) and `runs` (one per run). Ghost tags each `claude -p` with `OTEL_RESOURCE_ATTRIBUTES` (run, job, step) so Claude Code's own telemetry joins to the jobs. Full design: Ticker, [Bureau design §9.2](News%20—%20Bureau%20Design.md#92-observability).

### Board

Board, the desktop app of the Bureau service ([Bureau design §8.1](News%20—%20Bureau%20Design.md#81-board-and-the-bureau-service)), is also the local analytics view; nothing is published and nothing listens on the network. Its Analytics screen reads ClickHouse and draws simple egui_plot charts styled by the design tokens, showing: pages by category, region, type and tier; coverage gaps; queue state; tokens per page by type; failures by stage; index budget use; free-tier usage against limits; site traffic; and the live home curation with its age (flagged after `NEWS_HOME_STALE_DAYS`). Its other screens show the service log and one record per `claude -p` call, content and sources, and whole-project actions. The planned Bun web dashboard is dropped (ADR 0007).

## Configuration and security

### Settings

These are settings in **Standing**, a versioned PostgreSQL table, changed only in Board (the terminal can read them with `news standing show|history`): every change records who (always `board`), when, and the old and new value, and each run snapshots the settings version it started with. The keys keep the names below. Preset schedules, quiet hours, the Bureau token cap and reserve, and models per step are settings too; their values are Karthik's to set in Board. `.env` keeps only bootstrap values: how to reach PostgreSQL, ClickHouse and the OTel Collector ([ADR 0007](docs/adr/0007-bureau-service-and-board.md)).

| Setting | Purpose | Starting value |
| --- | --- | --- |
| `NEWS_TARGET_PAGES` | Suggested pages per run (not a hard limit) | 100 |
| `NEWS_INDEX_PER_DAY` | Pages offered to search engines per day | 20 |
| `SAMPLE_RATE` | Share of pages getting the Haiku spot check | 0.05 |
| `NEWS_PARALLEL` | Parallel `claude -p` workers | 3 |
| `NEWS_MODEL_MAIN` / `NEWS_MODEL_LIGHT` | Model per role | Sonnet 5.5 / Haiku 5.5 |
| `NEWS_MODEL_CURATOR` | Model for the home page curator | Sonnet 5.5 |
| `NEWS_HOME_STALE_DAYS` | Age at which Board flags the live home curation | 4 |
| `BACKFILL_FROM` | Earliest date for backfill | Start of the current year |
| `NEWS_AI_RUN_TOKENS` | Token ceiling per run (input-equivalent); new AI jobs stop when it is reached and the run finishes its code work | 2,500,000 |
| `NEWS_CONTACT_EMAIL` | Contact e-mail sent in the User-Agent only to sources that require one (SEC EDGAR); a dedicated address (Bureau design, decision D14) | — |
| `queue_late_limit` | How late a scheduled heavy run may start before it is skipped and noted (Bureau design §5.5) | 2 hours |
| `missed_slot_limit` | A missed schedule slot is not run if the next slot is closer than this (Bureau design §5.6) | 2 hours |
| `NEWS_RAW_RETAIN_DAYS_DATA` | Days to keep raw structured snapshots (JSON, XBRL, CSV, SDMX) (decision D7, pending) | 400 |
| `NEWS_RAW_RETAIN_DAYS_DOCS` | Days to keep raw HTML and PDF snapshots (decision D7, pending) | 90 |

### Security

- **One way in: the Hotline.** Starts, gate answers, pauses, resumes, cancels, Lineup and Standing changes and every other H-tier action reach the Bureau service only over the Hotline, a Windows named pipe whose access list allows Karthik's user alone and which rejects remote clients, and only from Board: the service checks that the client is the installed `news.exe` running as `news board`. His terminal (the `news` CLI) may read status and run the low-risk tiers only, so a script or a development Claude Code session cannot start or approve anything. There is no `CLAUDECODE` guard. A schedule is a setting changed only from Board, so a scheduled start carries the same protection ([ADR 0007](docs/adr/0007-bureau-service-and-board.md); [Bureau design §5.5](News%20—%20Bureau%20Design.md#55-preset-runs-chief) and [§9.3](News%20—%20Bureau%20Design.md#93-security-and-safety)).
- **The spawn marker.** Every `claude -p` the service starts runs in a Windows Job Object it owns, without breakaway, so the call and everything it starts can never send a control command; the pipe refuses them.
- **G-tier rule.** Publishing and other gated commands (`news publish`, `news home publish`, `news eval promote` …) run only as the current step of a live run, for an item in its plan, authorised by that step's approval (bound to the hash of exactly what was shown, `subject_sha`) or, for an ungated step, by the run's start from Board or Karthik's schedule; otherwise they return `gated`.
- **The service runs as Karthik.** It has his rights; this protects against a model acting on its own or on injected text, not against other software running as him. He types his own password at `news service install`.
- **Page and drafting calls** get an allow-listed environment (no `ANTHROPIC_API_KEY`, cloud keys or write-API key) and no tools; source text is fenced as untrusted data ([Bureau design §9.3](News%20—%20Bureau%20Design.md#93-security-and-safety)).
- **Downloaded files** are untrusted: parsed by memory-safe Rust with size limits; Docling runs in a container without network access (proposed).
- **Secrets** live in Windows Credential Manager under Karthik's account; he enters them in Board. Nothing secret crosses the pipe or sits in PostgreSQL or `.env`.
- The write API accepts one secret key, rotated occasionally. Only Press (`news publish`, `news home publish`) reads it, and only inside an authorised run step; the service's background pools never do.
- Cloud credentials are least-privilege: one key for publishing only, plus tokens Karthik creates for Board's cloud and development views: Cloudflare analytics (read-only), AWS read-only for DynamoDB and CloudWatch metrics, and a GitHub fine-grained token for this repository with read access to pull requests, issues and checks and Actions read and write. GitHub requires Actions write to dispatch a workflow, and Karthik approved Board running the deploy workflow on `main`; that permission also allows other Actions operations, such as re-running or cancelling runs, which Board does not use.
- Public endpoints (site, search, MCP) are read-only.
- Nothing on the laptop listens on the network: the Hotline is local only, and Plate binds 127.0.0.1.
- AWS budget alarm at $0.01 from day one; Cloudflare and AWS usage watched in Board.

## Free-tier budget and limits

Planning figures, checked during setup and monitored in Board's cloud usage view (Cloudflare Workers, R2 and D1; DynamoDB), which reads them with the read-only Cloudflare and AWS tokens and raises local alarms at a share of each limit. Over-limit behaviour differs by service: R2 bills, D1 queries fail, DynamoDB throttles.

| Service | Free allowance | Pressure point | Mitigation |
| --- | --- | --- | --- |
| Cloudflare R2 | 10 GB storage; 1M writes, 10M reads a month; free egress | Storage passes 10 GB around 250k+ pages | Compress bundles; keep large data lean; move overflow later |
| Cloudflare D1 | 500 MB per database, up to 10; 5M rows read and 100k rows written a day | Search reads on busy days | Shard; index summaries only; cache popular queries |
| Cloudflare Workers | Daily request quota; small CPU time per request | SSR hubs, search, MCP | Edge caching for hours or days; static articles bypass Workers |
| AWS DynamoDB | 25 GB; 25 read and 25 write capacity units (provisioned, shared by all tables and indexes) | Publish bursts; hub reads | Paced writes; single table; hubs cached at the edge |

A rough size estimate is 20–40 KB per page across all formats when compressed, so 250k pages need about 5–10 GB in R2.

## Build plan

1. **Foundations.** Repositories, Docker PostgreSQL and the OTel Collector, the ClickHouse `news` database and user, Cloudflare and AWS accounts with budget alarms, secrets handling.
   - Exit: local services running, events reaching ClickHouse.
2. **Spike: server rendering of OpenUI.** Confirm the OpenUI React renderer produces HTML on the server inside Astro and bun.
   - Exit: one page rendered to static HTML with working interactive charts.
3. **Design direction.** Discuss the design with the author using the `frontend-design` skill: type, colour, layout, dark mode, tone.
   - Exit: agreed design tokens and principles.
4. **Component library (Opus 5.5).** Hundreds of components on top of `@openuidev/react-ui`, plus the component index and per-component specs.
5. **Content system design (Opus 5.5).** Designed in the [Bureau design](News%20—%20Bureau%20Design.md); this step turns it into ADRs, runs the embedding spike and labels the golden sets ([Bureau design §10](News%20—%20Bureau%20Design.md#10-build-order)).
6. **Rust `news` CLI and Bureau's code subsystems.** In order: core crates and Dispatch; then, right after that first wave and before the connectors, Chief inside the Bureau service (Hotline, spawn marker, scheduler, Lineup, the `claude -p`-from-service spike) and the Board shell (Runs & approvals, Settings & schedules, Docker health; Standing and Credential Manager; `news service install|update`); then Runner and Morgue; Contacts rows and the first Stringers (SEC EDGAR, FRED, BLS, ECB, arXiv, central-bank statements, then IMF, World Bank, Eurostat, ONS, StatCan); Clerk and Ledger; Diary; Roster's resolver; Taster; Nose and Desk; Fixer; Proof; Press (dry-run).
7. **Cloud.** R2 bucket and domain, DynamoDB table, D1 search shards, Workers (site SSR including the curated home page, API including the curation endpoint, `Idempotency-Key` support and the corrections endpoint, search, MCP v2), caching and purge.
8. **Orchestration.** The lean-call spike; Ghost (Rust, in the service); runtime skills and prompts; Critic (golden sets and judges); Chief's presets Edition, Flash, Frontpage (candidates, curator draft, checks, preview, approval in Board, publish; rollback from Board), Salvage and Audit; Board's service log and AI-call records; first full run end to end, first scheduled Edition and first home curation.
9. **Telemetry and dashboard.** Ticker (ClickHouse columns, tables and traces) and Board's Content & sources and Analytics screens.
10. **Launch and ramp.** The remaining presets (Huddle with its Board form, Rollcall, Errata, Catchup, Followup, Reprint); Board's whole-project screen (published-content actions, cloud usage, Deploy, development status); start with a low indexing budget, then tune using telemetry, Search Console and the weekly Huddle.

## Open items to research during development

- **OpenUI server rendering:** the docs don't describe it; the spike in step 2 settles it.
- **Content system:** designed in the [Bureau design](News%20—%20Bureau%20Design.md); its decisions D1–D21 are in [§11](News%20—%20Bureau%20Design.md#11-decisions).
- **Bulk dumps:** Hugging Face Datasets with an Internet Archive mirror, or GitHub Releases; check limits and terms.
- **Search scaling:** watch D1 daily reads; the trigger and design for moving to Lambda with Tantivy.
- **Cloudflare Web Analytics:** confirm the current beacon sets no cookies; otherwise use server-side zone analytics.
- **Exact free-tier figures:** Workers CPU per request and daily requests, cache purge rates, AWS always-free terms under the current account plans.
- **Storage growth:** when R2 nears 10 GB, choose between compression gains, moving large data elsewhere, or a paid plan.
- **Docling throughput** on the laptop's CPU, and how often PDFs need it.
- **Local embedding model:** bge-small-en-v1.5 on ONNX Runtime (Bureau design, decision D6); CPU cost measured in the Phase 4 spike.
- **Image licensing allow-list:** which sources and licences are accepted automatically.
- **Prompt-cache behaviour** across back-to-back `claude -p` calls, to confirm savings in telemetry; measured in the Phase 7 lean-call spike with the exact flag set and the usage-limit error shape.
- **Home curation workflow form:** settled. Home curation is the Frontpage preset, run by Chief in the Bureau service on Karthik's schedule or from Board, with an approval stop in Board before publish ([Bureau design §8.6](News%20—%20Bureau%20Design.md#86-frontpage-home-curation); decision D17).
- **`claude -p` from the service:** that it uses Karthik's subscription login when started by a Windows service under his account, the usage-limit error's shape and reset time, that Job Object membership reaches every process a call starts, and that the shared ClickHouse and Docker are reachable from the service; confirmed in the spike in the service step, before the connectors ([ADR 0007](docs/adr/0007-bureau-service-and-board.md)).
- **Settings Karthik sets later:** the Bureau daily and weekly token cap and reserve, quiet-hour times and whether they hold heavy runs or reduce parallelism, and the default schedule times.
- **Development sessions and the Hotline:** settled (Karthik, 16 Nov 2026). H-tier actions are accepted only from Board; the CLI reads status and runs the low-risk tiers; no `CLAUDECODE` guard.
- **Note for the PRD:** the PRD now matches this design (category-free URLs, D1 search, cookieless analytics, automated checks instead of per-page review).
