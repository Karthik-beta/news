# News — Technical Design

Nov 14, 2026 · @Karthik

## Summary

All work happens on the author's laptop: discovery, fetching, parsing, AI generation and rendering. The cloud only stores and serves finished pages. Each run produces 100+ pages from a single Claude Code prompt, 2–3 runs a day, growing to 250k+ pages in about a year. Everything stays on free tiers and token use is kept to a minimum.

This document covers the technical side. Product scope lives in the separate PRD.

### Design principles

1. **Code before tokens.** Fetching, parsing, numbers, checks, rendering and uploads are done by code. The AI only decides what to say and how to lay it out.
2. **The model never writes numbers.** Pages reference data by ID; code fills in the values. This saves output tokens and prevents made-up figures.
3. **AI lives only on the laptop.** No model calls in the cloud, at build time or at request time.
4. **No site rebuilds.** Each page is rendered and uploaded on its own; only affected hubs are refreshed. The home page's curation is data the SSR Worker reads at request time, so curating it never rebuilds or redeploys anything.
5. **Publishing is separate from indexing.** Every page is public, but only a daily budget of pages is offered to search engines.
6. **A designed content system, not random generation.** Every page can be traced to why it was chosen and what it was built from.
7. **Everything is measured.** Every event goes to ClickHouse so the pipeline can be tuned.
8. **Room to grow.** Categories, sources, components and storage back-ends can be added without redesign.

## System overview

&#91;embedded content: System overview · laptop, cloud, readers\]

The `news` CLI fetches and parses public sources into PostgreSQL and hands Claude a compact brief. Claude writes OpenUI Lang, the renderer turns it into a page bundle, and the CLI publishes through the write API into R2, DynamoDB and D1. Readers get static pages from the CDN, dynamic hubs from the SSR Worker, and search and MCP from their own Workers. Every step on the laptop writes events to ClickHouse.

## Local environment

The laptop is the whole production system: a Windows 11 machine with an Intel i5-8250U (4 cores, 8 threads, integrated GPU), 24 GB RAM and about 300 GB free disk. The design favours low CPU use and is generous with RAM and disk.

### Languages and runtimes

| Layer | Choice | Why |
| --- | --- | --- |
| CPU-heavy tools (fetch, parse, XBRL, compression, upload, indexing) | Rust, one compiled `news` binary | Lowest CPU per item; single file to run |
| Rendering, Astro site, Workers code, glue | Bun (runtime and package manager) | Fast JavaScript runtime for the React/OpenUI renderer |
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

PostgreSQL was chosen over a second Neo4j instance: the workload is mostly relational with shallow graph lookups (1–3 hops) and similarity search, which PostgreSQL with pgvector handles in one engine with less RAM. A lean Neo4j Community instance can be added later if graph algorithms are needed. The existing Neo4j container is not used.

### Local folders

| Folder | Holds | Pruned after publish? |
| --- | --- | --- |
| `work/raw/` | Downloaded source files (PDF, HTML, XBRL, JSON) | Yes |
| `work/parsed/` | Normalized data per item | Yes |
| `work/briefs/` | Compact briefs given to the AI | Yes |
| `work/pages/` | Generated OpenUI Lang, page bundles, rendered output | Yes, once uploaded |
| `site/` | Astro project, component library, brand tokens | No (source code) |
| PostgreSQL and ClickHouse data | Graph, ledger, taxonomy, telemetry | No |

## Content system

Pages come from a designed system, not random generation: every page records why it was selected and what it was built from. This section is an outline; the full design is researched and built with Opus 5.5 during development (build plan step 5).

&#91;embedded content: Content system · 10 stages with a feedback loop\]

Code runs every stage except synthesis. Haiku assists in scoring and research packs only where code is unsure, and telemetry feeds back into the scoring weights.

| Stage | What it does |
| --- | --- |
| Connectors | One per source, run by code; record licence and attribution; extract embeds and images from the source |
| Candidates | Every discovered item normalized into PostgreSQL with entities extracted |
| Scoring | Importance, novelty, data richness, freshness and coverage gap |
| Clustering | Groups items that are the same story, using embeddings and shared entities; removes duplicates |
| Selection | Breadth-first quotas by category, region and type against `NEWS_TARGET_PAGES`; assigns tier and indexing priority |
| Research pack | Combines and cross-checks several sources into one compact brief with data IDs |
| Synthesis | Sonnet decides what the page says and which components show it, writing OpenUI Lang and text |
| Render, check | Code renders the bundle and runs the quality checks |
| Publish | Uploads, updates hubs and search, applies the index budget |
| Telemetry | Usage, quality and traffic signals flow back into scoring |

**Updates:** when a source revises data (restatements, GDP revisions), the ledger detects the change and queues an update of the existing page.

## Agent orchestration and token optimisation

One prompt in Claude Code starts a run. The main session stays small: it plans and hands out work, and never holds page content.

### How a run works

1. The author prompts the main session (for example, "run today's batch").
2. The main session calls `news plan`, which returns a short list of item IDs grouped into batches. The target count comes from `NEWS_TARGET_PAGES` and is a suggestion, not a hard limit.
3. Code runs fetch, parse and brief for every item (zero tokens).
4. A bun runner starts worker calls with `claude -p`, one per item, 3–4 in parallel (the laptop mostly waits on the network).
5. Each worker returns only `{item_id, status, page_id}` to the runner; the main session sees a one-line summary per batch.
6. Code renders, checks, publishes and prunes.

### Model roles

| Model | Used for |
| --- | --- |
| Haiku 5.5 | Classifying and tagging candidates; summarising long parsed text into the brief; the sample quality check; small updates |
| Sonnet 5.5 | Selection judgement where code scoring is unsure; synthesis; page layout in OpenUI Lang; narrative text; home page curation (`NEWS_MODEL_CURATOR`) |
| Opus 5.5 | Development time only: building the component library, designing the content system, writing skills |

### Keeping each `claude -p` call lean

- **No MCP servers loaded** (`--strict-mcp-config` with an empty config), so no tool schemas fill the context.
- **Minimal tools** via `--allowedTools`: only the `news` commands the step needs.
- **Low `--max-turns`** and an explicit `--model` per call.
- **Tiny project `CLAUDE.md`** and one short skill per step; no auto-memory.
- **Identical prompt prefix** (instructions, brand rules, component index) on every call, run back to back so prompt caching stays warm.
- **Small input:** a compact brief (about 1–3k tokens) with data IDs, never raw documents.
- **Small output:** OpenUI Lang plus text, with numbers as data references.
- **Component catalog on demand:** the prompt holds a one-line-per-component index; full details come from `news component <type>` only for the components the model picks.
- **`--output-format json`** so token usage for every call is logged to ClickHouse.

### Rough token budget per page

| Part | Tokens (approx.) |
| --- | --- |
| Cached prefix (instructions, brand rules, component index) | 5–8k, mostly cache reads |
| Brief | 1–3k |
| Full specs for chosen components | 2–4k |
| Output (OpenUI Lang and text) | 2–3k |

These are planning estimates; ClickHouse telemetry will show the real numbers per page type.

## Agent toolbox: the `news` CLI

One Rust binary gives agents everything they need, with short help text and compact JSON output. A CLI costs far fewer tokens than a large MCP tool catalog loaded into every session.

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

Every command writes events to ClickHouse. Commands are idempotent, so a failed run can resume.

## Data model

### Local PostgreSQL (working and knowledge store)

| Table / area | Holds | Kept after prune? |
| --- | --- | --- |
| `sources` | Source registry: connector, base URL, licence, attribution, image policy | Yes |
| `candidates` | Every discovered item, normalized, with scores and status | Yes (compact row) |
| `clusters` | Groups of candidates that are the same story | Yes |
| `queue` | Selected items: tier, priority, status, attempts | Yes |
| `ledger` | Source URL, content hash, page URL, publish date, version | Yes (prevents reprocessing) |
| `entities` + `edges` | Knowledge graph: companies, people, countries, institutions, topics, sources, pages and their links | Yes |
| `embeddings` (pgvector) | Vectors for candidates and pages, for similarity and de-duplication | Yes |
| `taxonomy` | Category tree (parent, slug, name, order); pages link to many categories | Yes |
| `data_points` | Normalized figures with IDs used by page references | Until published |
| `home_curations` | Draft and published home curations: slots, chosen figures, the curator's internal rationale, check results, approval and publish times | Yes |

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

The home page's lead story, side stories and section rows are chosen by a curation agent every 2–3 days, when the author runs the preset `curate-home` workflow in Claude Code. The agent runs on the laptop like every other AI step; the cloud only stores and serves the result.

**The workflow**

1. **Gather.** `news home candidates` returns a compact list of eligible pages: published since the last curation, plus still-relevant earlier Tier 1 pages, with tier, categories, region, page type, age, key figures (by data ID), mini-chart options and recent traffic from analytics.
2. **Curate.** A curator agent (Sonnet 5.5 by default, `NEWS_MODEL_CURATOR`), using the `news-home-curation` skill, returns a curation as JSON: one lead, two side stories, 3–5 ordered sections with 3–4 stories each, the figure or mini chart each slot shows (chosen from the page's existing data IDs), and a one-line internal rationale per pick (stored, never shown). It writes no new reader-facing text.
3. **Save and check.** `news home save` stores the draft in PostgreSQL; `news home check` validates it in code:
   - every page exists, is published and is not withdrawn; no page appears twice;
   - every chosen figure or chart resolves to the page's own data;
   - breadth: at least three categories and three regions across the curated slots, no more than two stories about the same entity, at least one non-finance section;
   - freshness: the lead is no older than 7 days unless it is a Tier 1 page the curator explicitly marks as still leading;
   - every slot filled; the draft fits the home template's slot counts.
   Failures go back to the curator with the errors (at most two rounds).
4. **Preview.** `news home preview` renders the home page locally from the draft with the same SSR code as production, light and dark, at phone and desktop widths, and prints the URL.
5. **Approve.** The workflow stops and asks the author to approve, change or reject the draft in the Claude Code session. Changes go back to the curator; nothing is published without approval.
6. **Publish.** `news home publish` sends the curation to the write API (`PUT /v1/home/curation`), which validates it again, writes `HOME#CURATION / V#<n>` and moves `LATEST`, then purges the home page and region views. `news home rollback <version>` makes an earlier version live.

**Between curations**

- Curated slots stay as published. The automatic "Latest" list (newest 8–10 pages from `HUB#latest`) updates with every page publish.
- A curated page that is withdrawn or deleted is skipped at render time; its slot is filled from the best recent page of the same section by tier and freshness.
- A curated page that is updated shows its updated title, summary and figures, because slots point at page IDs, not copies.
- Region views show the curated stories for that region, topped up with that region's latest pages.
- The local dashboard flags a curation older than `NEWS_HOME_STALE_DAYS` (default 4).

**Curation record** (DynamoDB item, a few KB): version, created and approved times, curator model, the slots (`lead`, `side[2]`, `sections[{category, pages[{page_id, figure_id?, chart_id?}]}]`), and the check results. Rationale stays in PostgreSQL only.

### Updating a page

An agent revises the page (new data, a restatement, a correction). It is re-rendered under the same URL, the previous version is kept in DynamoDB, and the page shows "Updated on …".

### Renderer versions

Each page records the renderer version that produced it. When components improve, pages can be re-rendered in batches from their stored `.oui` and data without a site rebuild.

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

There is no manual approval step; the author's prompt is the approval. Quality is protected by cheap automatic checks.

1. **Code checks on every page (zero tokens):**
   - The page parses and renders.
   - Every data reference resolves, and every figure has a source.
   - Values match the saved source data.
   - Links, image URLs and video embeds respond.
   - Required fields (title, summary, alt text, categories) are present.
2. **Haiku spot check on a sample** (`SAMPLE_RATE`, starting at 1 in 20): it compares the page's Markdown with its brief and flags wrong claims, missing context or tone problems.
3. **Failures** are logged to ClickHouse and the item returns to the queue for an agent to fix.
4. Pages failing code checks are never published. Spot-check failures on published pages trigger an update.
5. **Home curations** are the one place with a human approval step: `news home check` runs code checks (see Home page curation), then the author approves the local preview before `news home publish`.

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

### Local analytics dashboard

A small local bun web app, not published, showing: pages by category, region, type and tier; coverage gaps; queue state; tokens per page by type; failures by stage; index budget use; free-tier usage against limits; site traffic; and the live home curation with its age (flagged after `NEWS_HOME_STALE_DAYS`).

## Configuration and security

### Environment variables

| Variable | Purpose | Starting value |
| --- | --- | --- |
| `NEWS_TARGET_PAGES` | Suggested pages per run (not a hard limit) | 100 |
| `NEWS_INDEX_PER_DAY` | Pages offered to search engines per day | 20 |
| `SAMPLE_RATE` | Share of pages getting the Haiku spot check | 0.05 |
| `NEWS_PARALLEL` | Parallel `claude -p` workers | 3 |
| `NEWS_MODEL_MAIN` / `NEWS_MODEL_LIGHT` | Model per role | Sonnet 5.5 / Haiku 5.5 |
| `NEWS_MODEL_CURATOR` | Model for the home page curator | Sonnet 5.5 |
| `NEWS_HOME_STALE_DAYS` | Age at which the dashboard flags the live home curation | 4 |
| `BACKFILL_FROM` | Earliest date for backfill | Start of the current year |

### Security

- The write API accepts one secret key stored only on the laptop, rotated occasionally.
- Cloud credentials (Cloudflare, AWS) live in a local secrets file excluded from version control, with least-privilege keys: one for publishing only.
- Public endpoints (site, search, MCP) are read-only.
- The local dashboard listens only on localhost.
- AWS budget alarm at $0.01 from day one; Cloudflare usage watched in the dashboard.

## Free-tier budget and limits

Planning figures, checked during setup and monitored in the dashboard. Over-limit behaviour differs by service: R2 bills, D1 queries fail, DynamoDB throttles.

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
5. **Content system design (Opus 5.5).** Research and design of the source registry, connectors, scoring, clustering, selection, research packs and synthesis.
6. **Rust `news` CLI and connectors.** Commands in the toolbox table, the first connectors (SEC EDGAR, FRED, IMF, World Bank, arXiv, central banks), parsing and briefs.
7. **Cloud.** R2 bucket and domain, DynamoDB table, D1 search shards, Workers (site SSR including the curated home page, API including the curation endpoint, search, MCP v2), caching and purge.
8. **Orchestration.** Skills, prompts and the bun runner for `claude -p`; the `curate-home` workflow (candidates, curator skill, checks, preview, approval, publish, rollback); first full run end to end and first home curation.
9. **Telemetry and dashboard.** ClickHouse tables and the local analytics app.
10. **Launch and ramp.** Start with a low indexing budget, then tune using telemetry and Search Console.

## Open items to research during development

- **OpenUI server rendering:** the docs don't describe it; the spike in step 2 settles it.
- **Content system:** full design of discovery, scoring, clustering, selection and synthesis, led by Opus.
- **Bulk dumps:** Hugging Face Datasets with an Internet Archive mirror, or GitHub Releases; check limits and terms.
- **Search scaling:** watch D1 daily reads; the trigger and design for moving to Lambda with Tantivy.
- **Cloudflare Web Analytics:** confirm the current beacon sets no cookies; otherwise use server-side zone analytics.
- **Exact free-tier figures:** Workers CPU per request and daily requests, cache purge rates, AWS always-free terms under the current account plans.
- **Storage growth:** when R2 nears 10 GB, choose between compression gains, moving large data elsewhere, or a paid plan.
- **Docling throughput** on the laptop's CPU, and how often PDFs need it.
- **Local embedding model:** choice and CPU cost.
- **Image licensing allow-list:** which sources and licences are accepted automatically.
- **Prompt-cache behaviour** across back-to-back `claude -p` calls, to confirm savings in telemetry.
- **Home curation workflow form:** a saved Claude Code workflow in `.claude/workflows/` (run by name) versus a skill-driven slash command; settle when building it, keeping one command for the author and an approval stop before publish.
- **Note for the PRD:** the PRD now matches this design (category-free URLs, D1 search, cookieless analytics, automated checks instead of per-page review).
