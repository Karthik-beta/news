# News — Bureau Design

Nov 14, 2026 · @Karthik · Draft for review

**Bureau** is the part of News that runs on Karthik's laptop: where the reporting happens, away from head office, while the cloud side only serves readers. This document designs all of Bureau: ingestion, normalisation, story detection, scoring and ranking, content generation, checking, orchestration, and the preset workflows Karthik starts from Claude Code through one command, `/bureau`. It builds on the [PRD](News%20—%20Product%20Requirements%20Document.md), the [Technical Design](News%20—%20Technical%20Design.md) (TD) and the [Development Plan](News%20—%20Development%20Plan.md). Where it departs from them, the change is listed in [Changes to existing documents](#14-changes-to-existing-documents); the existing documents have not been edited.

**Names.** Every part of Bureau has an approved one-word newsroom name (Wire, Stringers, Morgue, Diary, Clerk, Ledger, Roster, Taster, Nose, Desk, Fixer, Reporter, Proof, Press, Chief, Dispatch, Ghost …). The full reference table is in [§15 Names](#15-names). Commands are verb-based: one Claude Code entry point, `/bureau <verb> <object>`, and the `news` CLI underneath it (`news run start`, `news jobs retry`). Existing names (`news` CLI commands, `news-home-curation`, the `page-*` and `news-brand` skills, PostgreSQL tables named in the TD) stay as they are; the TD's `curate-home` workflow becomes Frontpage, started with `/bureau curate home`.

**Research.** External facts were checked on 14 Nov 2026 against primary documentation and are cited by number; the list is in [References](#16-references). Anything not confirmed in a primary source, or that is my estimate, is marked *(speculative)* or *(unverified)*.

---

## Contents

1. [Purpose, scope and principles](#1-purpose-scope-and-principles)
2. [System map](#2-system-map)
3. [Subsystems](#3-subsystems)
4. [Scoring and ranking in full](#4-scoring-and-ranking-in-full)
5. [Orchestration engine](#5-orchestration-engine)
6. [Agent tool catalogue](#6-agent-tool-catalogue)
7. [AI usage ledger](#7-ai-usage-ledger)
8. [One entry point and the preset workflows](#8-one-entry-point-and-the-preset-workflows)
9. [Quality, observability, security, performance](#9-quality-observability-security-performance)
10. [Build order](#10-build-order)
11. [Open decisions for Karthik](#11-open-decisions-for-karthik)
12. [Risks](#12-risks)
13. [Glossary of terms used](#13-glossary-of-terms-used)
14. [Changes to existing documents](#14-changes-to-existing-documents)
15. [Names](#15-names)
16. [References](#16-references)

---

## 1. Purpose, scope and principles

### Purpose

Turn a stream of public, primary-source releases (filings, statistics, central-bank decisions, institutional reports, papers) into 100+ checked pages per run, 2–3 runs a day, on one Windows laptop, with Claude Code as the only AI runtime, and with every figure traceable to the byte of source data it came from.

### Scope

| In scope (Bureau) | Out of scope |
| --- | --- |
| Contacts, Wire (Stringers, Runner), Diary, Morgue | Cloud serving: R2, DynamoDB, D1, Workers (TD "Cloud services") |
| Clerk, Ledger, Roster | The write API's internals; only its contract (the publish interface) is used here |
| Taster, Nose, Desk | Component library and renderer internals (Development Plan Phases 1 and 3) |
| Fixer, Reporter, Proof, Rewrite | Site design (design system) |
| Press (laptop side of publishing, index budget) | Search and MCP servers in the cloud |
| Chief, Dispatch, Ghost | Development workflow (sub-agents, PRs) |
| `/bureau` and every preset workflow, including Frontpage (home curation) | |
| Ticker events and Board panels for this system | |
| Critic | |

### Principles

| # | Principle | What it means in practice |
| --- | --- | --- |
| P1 | **Code first** | Fetching, parsing, normalising, entity resolution, de-duplication, clustering, scoring, ranking, selection, scheduling, validation, number handling, rendering and publishing are deterministic code. A model is called only where the job needs language or judgement. |
| P2 | **Every AI step is a contract** | Typed input built by code, a JSON Schema for the output, a code validator, a fallback that needs no AI, and a budget (turns, tokens, cost ceiling). No AI output reaches a page or a database row without passing its validator. |
| P3 | **Numbers never come from a model** | Models see values so they can understand them, but write only data references. A code lint rejects any digit in model prose that is not an allowed pattern (see [Proof](#310-proof)). Derived figures (changes, shares) are computed by code with a recorded formula. |
| P4 | **Provenance for every data point** | Each value records the raw snapshot it was parsed from (SHA-256), a locator inside it (XBRL tag and context, JSON path, PDF page and table cell, series ID and observation date), the vintage and the parser version. |
| P5 | **Structured identity before similarity** | Most items on this site have a natural key (company + fiscal period, series + reference period, central bank + meeting date, report + edition, DOI or arXiv ID). Story identity comes from those keys; embeddings are used only for the unstructured remainder. |
| P6 | **Agents get purpose-built tools** | Compact JSON briefs in, compact JSON out, stable error codes, least privilege, every tool testable without AI. |
| P7 | **Resumable by default** | All state lives in PostgreSQL; every step is idempotent and keyed; a crash, a usage-limit stop or a closed laptop lid resumes where it stopped. |
| P8 | **Unattended means code only** | Background jobs (calendar sync, release-time fetching, source probes) run without Karthik. Anything that calls a model runs only inside a preset Karthik started, because AI usage is on his plan and his prompt is the approval (PRD "Content production"). |
| P9 | **Light on a 4-core laptop** | Small fixed worker pools, one heavy run at a time, network-bound work overlapped with AI waits, no extra servers beyond the PostgreSQL, ClickHouse and OTel containers already planned. |
| P10 | **Everything measured** | Every step emits events and spans to ClickHouse, joined by `run_id` and `job_id`, including each `claude -p` call. |
| P11 | **One entry point** | Karthik starts and controls everything with `/bureau`; Chief is the only way a preset runs, and nothing bypasses its gates (§8.1). |

---

## 2. System map

### 2.1 Subsystems and data flow

```
                       ┌──────────────────────────── BUREAU (laptop) ──────────────────────────────────────┐
 Karthik ──/bureau──▶  │  Chief (preset runs, gates, Wrap) ──enqueues/advances──▶ Dispatch (queue + workers)│
                       │        ▲ status board                       ▲  ▲ schedules        │ claim/complete │
                       │        │                                    │  └──── Diary ◀──────┤ (ICS/API/HTML) │
                       │        │                                    │                     ▼                │
 Public sources        │  ┌───────────────────┐   raw bytes   ┌─────────────┐                              │
 (EDGAR, FRED, BLS,    │  │ Wire              │──────────────▶│ Morgue      │ content-addressed files      │
  Eurostat, ECB, IMF,──┼─▶│ Stringers (1 per  │               │             │ + snapshots table            │
  arXiv, OpenAlex …)   │  │ source) + Runner  │               └──────┬──────┘                              │
   HTTP, polite        │  └─────────┬─────────┘                      │                                     │
                       │            │ candidate refs                 ▼                                     │
                       │            │                         ┌─────────────┐──▶ Ledger (values, vintages,  │
                       │            └────────────────────────▶│ Clerk       │      locators)                │
                       │                                      │ parse, units│──▶ text blocks                │
                       │                                      │ periods     │──▶ Roster (IDs, aliases)      │
                       │                                      └──────┬──────┘                               │
                       │                                             ▼ items with entities + event keys    │
                       │                                      ┌─────────────┐                               │
                       │                                      │ Taster      │ event keys, SimHash,          │
                       │                                      │             │ embeddings (pgvector)         │
                       │                                      └──────┬──────┘                               │
                       │                                             ▼                                     │
                       │                                      ┌─────────────┐                               │
                       │                                      │ Nose + Desk │──▶ Budget (tiers, buckets,    │
                       │                                      └──────┬──────┘     reasons, Pitch per story) │
                       │                                             ▼                                     │
                       │                                      ┌─────────────┐                               │
                       │                                      │ Fixer       │──▶ brief (1–3k tokens,        │
                       │                                      └──────┬──────┘     data IDs, quotes)         │
                       │                                             ▼                                     │
                       │   Ghost (Bun) ──claude -p──▶ Reporter ──▶ Copy                                     │
                       │                                             ▼                                     │
                       │   render (Bun) ──▶ Proof (code + sample) ──Markup──▶ Press (outbox, index) ──HTTPS──┼──▶ write API
                       │                                                         ▲                         │    (cloud)
                       │   Rewrite ◀── new vintages / restatements ──────────────┘                         │
                       │                                                                                   │
                       │   Every box ──events/spans──▶ Ticker (OTel → ClickHouse `news`) ──▶ Board          │
                       └───────────────────────────────────────────────────────────────────────────────────┘
```

### 2.2 Who drives what

```
 Karthik ──types──▶ /bureau                 → status board (code): running, waiting, due, problems, next action
                    /bureau run daily        (Claude Code interactive session, small context)
                         │
                         │ 0. news run intent "run daily"         → {preset: "edition", args, writes: true}
                         │ 1. news run start edition              → {run_id, next}   (Chief refuses or queues a 2nd heavy run)
                         │ 2. loop: news run next <run>           → runs code steps inline until a stop:
                         │        stop = ai_batch  → bun pipeline/run-ai.ts --run <run>   (Ghost; claude -p ×3)
                         │        stop = approval  → session asks Karthik, then news run approve <run> <gate>
                         │        stop = done      → prints the Wrap
                         ▼
                 PostgreSQL (runs, run_steps, jobs, approvals …) = single source of truth
                         ▲
 Windows Task Scheduler ─┘  news work --pools net,cpu   (Dispatch background workers, optional, code only:
                                                         calendar sync, release-time fetch, source probes)
```

The interactive session never holds page content. It sees one compact JSON object per `next` call and one line per AI batch.

### 2.3 Life of one item (US CPI, October release)

| Time (ET) | Step | Code or AI | Output |
| --- | --- | --- | --- |
| Day −1 | Diary has `macro:us.cpi:2026-10` expected 08:30 from the BLS ICS feed [22] | Code | `calendar_events` row |
| Day −1 | Pre-warm: history of the CPI series and components fetched, entity and template context cached | Code | data points (prior vintages) |
| 08:32 | A Dispatch worker runs the BLS Stringer (conditional GET; retries every 5 min until new data) | Code | snapshot in Morgue + new data points |
| 08:33 | Clerk stores values exactly as published in Ledger; event key `macro:us.cpi:2026-10` | Code | `candidates` row |
| 08:33 | Taster: event key already has a story → join; no embedding needed | Code | `stories` member |
| 08:33 | Nose: score 75.2 (worked example in §4.6) | Code | `scores` row, Pitch |
| 08:40 | Karthik types `/bureau`; the status board shows the CPI story captured and suggests `run daily` | Code | Status board |
| 11:00 | Karthik runs `/bureau run daily` (Edition); Desk puts the story in Tier 1, Economy/North America bucket | Code | Budget row |
| 11:02 | Fixer: 41 data points (incl. 3 derived changes), 6 quote spans from the BLS release text | Code (+ Haiku condensation only if the release text is long) | brief JSON |
| 11:05 | Reporter: Sonnet writes OpenUI Lang with data references only | AI | Copy |
| 11:06 | Render, Proof (numeric-literal lint, refs, direction words, links, licence) | Code | Markup |
| 11:07 | Press uploads; index budget gives it `index` (Tier 1) | Code | page URL |
| 11:40 | Edition ends | Code | Wrap |
| Day +30 | BLS revises seasonal factors; new vintage → Rewrite re-renders, no AI needed unless a direction word flips | Code | page v2, "Updated on …" |

---
## 3. Subsystems

Each subsystem below follows the same pattern: responsibility, inputs and outputs, data model, algorithms, AI use, agent tools, failure modes, tests, metrics. Table sketches use PostgreSQL 17 types. New table names are plain working names (implementation detail, not product names); existing TD tables keep their names.

All new tables live in one schema (`news`), are created by numbered migrations in `cli/` (sqlx), and are owned by the Rust CLI. Bun code reads and writes them only through `news` commands, so every rule has one implementation.

### 3.1 Contacts

**Responsibility.** One row per source, holding everything code needs to fetch politely and credit correctly. Extends the TD `sources` table.

```sql
create table sources (
  source_id        text primary key,            -- 'sec-edgar', 'fred', 'bls', 'eurostat', 'arxiv' …
  name             text not null,
  publisher        text not null,               -- who to credit ("U.S. Bureau of Labor Statistics")
  connector        text not null,               -- Rust connector id
  base_url         text not null,
  source_tier      smallint not null,           -- 1 official primary, 2 official aggregator (FRED, ECB portal),
                                                -- 3 preprint/repository, 4 news outlet (link-only)
  licence          text not null,               -- 'public-domain' | 'cc-by-4.0' | 'custom' | 'unclear'
  attribution      text not null,
  redistribution   boolean not null,            -- false → link and summarise, no data files (PRD rule 4)
  image_policy     text not null,
  auth_env         text,                        -- NAME of the env var holding a free key, never the key
  rate_per_sec     numeric(6,3) not null,       -- documented limit, then halved for safety
  max_concurrency  smallint not null default 1,
  daily_quota      integer,                     -- e.g. BLS v2: 500 queries/day [22]
  robots_respect   boolean not null default true,
  regions          text[] not null,
  categories       text[] not null,
  calendar_kind    text,                        -- 'ics' | 'api' | 'html' | 'derived' | null
  enabled          boolean not null default true,
  health           text not null default 'unknown', -- 'green' | 'amber' | 'red' (computed)
  notes            text
);
```

**Documented limits that set the defaults** (checked 14 Nov 2026):

| Source | Limit or rule | Default in registry |
| --- | --- | --- |
| SEC EDGAR | Max 10 requests/s; declared User-Agent with organisation and contact e-mail [18] | 4 req/s; UA contact from `NEWS_CONTACT_EMAIL` (local `.env`, never committed) |
| FRED | Free key; 120 req/min *(unverified: official pages give no figure)* [21] | 1 req/s |
| BLS API v2 | Free key; 500 queries/day, 50 series/query; 50 requests per 10 s [22] | 0.5 req/s, daily quota 400 |
| BEA | Free key; 100 req/min *(from the user guide via a search snippet)* [23] | 0.5 req/s |
| StatCan WDS | 25 req/s per IP [27] | 2 req/s |
| OECD SDMX | 60 downloads/hour in the official explainer; a mirror says 20/min, so budget for the lower [31] | 1 per 60 s |
| arXiv (API, OAI-PMH, RSS) | 1 request every 3 s, one connection, shared across all your machines [33] | 0.3 req/s, concurrency 1 |
| OpenAlex | 10 req/s without key, 30 with a free key; daily budget [34] | 3 req/s with free key |
| Crossref | Polite pool with `mailto`: 10 req/s, 3 concurrent [35] | 3 req/s, concurrency 2 |
| PubMed E-utilities | 3 req/s without key, 10 with [36] | 2 req/s |
| OpenFIGI | Mapping: 25 req/min without key, 250 with [41] | 20 req/min |

**AI use.** None. Registry rows are written by developers (Sonnet docs writer, Opus review), never by a runtime model.

**Tools.** `news sources list|show|health|probe` (agents); `enable|disable` (Karthik only).

**Failure modes.** Missing licence → the connector refuses to emit candidates (`licence_missing`). Free key absent → source `amber`, skipped, reported by the Rollcall workflow.

**Tests.** Schema test that every enabled source has licence, attribution, rate and tier; a lint fails CI if `rate_per_sec` is above half the documented limit recorded in `notes`.

### 3.2 Wire: connectors, Runner, Morgue

**Responsibility.** Discover new items per source, download them politely and store the exact bytes received with their provenance. No interpretation beyond what is needed to list items.

**Connector contract** (Rust trait in `cli/crates/connectors`):

```rust
pub trait Connector: Send + Sync {
    fn id(&self) -> &'static str;
    /// Cheap listing since a cursor (feed, index, API "updatedAfter"). Returns refs, not content.
    async fn discover(&self, cx: &Cx, since: Cursor) -> Result<Discovery, ConnError>;
    /// Download one ref through the shared Runner. One ref may need several files.
    async fn fetch(&self, cx: &Cx, r: &ItemRef) -> Result<Vec<SnapshotId>, ConnError>;
    /// Pure function of snapshot bytes → normalised output. No network. Versioned.
    fn parse(&self, snaps: &[Snapshot]) -> Result<Parsed, ParseError>;
    const PARSER_VERSION: u32;
}
pub struct ItemRef { pub source_id: String, pub url: String, pub event_key_hint: Option<String>,
                     pub published_at: Option<DateTime<Utc>>, pub title: Option<String> }
```

`parse` is pure and network-free, so every parser is tested from recorded fixtures (as the `news-source-connector` skill already requires) and can be re-run on old snapshots when it improves.

**Runner** (one shared polite HTTP client):

| Concern | Rule | Basis |
| --- | --- | --- |
| Conditional requests | Store `ETag` and `Last-Modified` per URL; send `If-None-Match` / `If-Modified-Since`; a 304 costs no parse | RFC 9110 §13 [39] |
| Rate limits | Token bucket per host from the registry; global network pool of 6 | Source terms (table above) |
| `Retry-After` | Honour seconds or HTTP-date on 429/503; the job is *snoozed* (not counted as a failed attempt) | RFC 9110 [39]; River's snooze [5] |
| robots.txt | Cached ≤ 24 h; 4xx on robots.txt = allowed, 5xx = disallow all; case-insensitive UA match falling back to `*` | RFC 9309 [40] |
| User-Agent | `News/<version> (+https://news.karthik.nz/data-access)`, plus the contact e-mail from `.env` only for sources that require one (SEC) | SEC fair access [18] |
| Compression and size | `Accept-Encoding: gzip, br`; stream to disk, hash while streaming; content-type allow-list and max size per source (default 50 MB, PDFs 200 MB) | Low CPU, safety |
| Timeouts | Connect 10 s, total 120 s (bulk files 30 min) | |

**Morgue.** Content-addressed files plus a table.

```
work/raw/sha256/ab/cd/abcd…ef.zst        -- bytes exactly as received, zstd level 3
```

```sql
create table snapshots (
  sha256 bytea primary key, source_id text not null references sources, url text not null,
  fetched_at timestamptz not null, http_status smallint not null, content_type text, bytes bigint not null,
  etag text, last_modified text, storage_path text not null, retain_until timestamptz
);
create table fetch_state (                      -- one row per polled URL
  url_hash bytea primary key, url text not null, source_id text not null,
  etag text, last_modified text, last_status smallint, last_sha256 bytea,
  last_fetched timestamptz, next_due timestamptz, consecutive_errors smallint default 0
);
```

**Retention** (proposed change: the TD prunes `work/raw/` after publish). Structured data (JSON, XBRL, CSV, SDMX) is kept 400 days, because it is small and is the proof behind every figure; HTML and PDF 90 days; the SHA-256, URL and locator for ever. A figure can be re-fetched and re-verified against the source only while the source still returns the same bytes; after a snapshot expires, a source that has since revised or reformatted its data cannot reproduce the original bytes, so the proof is lost even though the verbatim value stays in the Ledger. *(Speculative sizing: 1–3 GB a month compressed for structured data, 5–15 GB a month for PDFs and HTML; ~300 GB is free. Measured in Phase 5.)*

**Discovery routes per source family** [18]–[38]:

| Family | Discovery | Change detection |
| --- | --- | --- |
| SEC filings | Atom "latest filings" feed by form type and the nightly daily index [20]; `submissions` JSON per tracked CIK [19] | `accn` + `filed`; `companyfacts` keeps every filed fact, so restatements appear as new facts [19] |
| FRED / ALFRED | `releases/dates`; `series/observations` with `realtime_start` [21] | `series/vintagedates` lists the dates data changed [21] |
| BLS, BEA | ICS calendars [22][23]; API pull at release time | No vintages: compare with the previous snapshot; store new values as a new vintage |
| Eurostat, ECB, OECD, IMF, BIS, ABS | SDMX REST; `updatedAfter` where supported (ECB, OECD) [25][31] | `updatedAfter`, otherwise hash comparison |
| ONS | `api.beta.ons.gov.uk/v1`; legacy endpoints are being retired [26] | Hash comparison; connector flagged "beta API" |
| StatCan | WDS `getChangedSeriesList` / `getChangedCubeList` [27] | Native change lists |
| arXiv | Category RSS/Atom (updated daily at midnight US Eastern) and OAI-PMH with `from=` [33] | OAI datestamp; version suffix |
| OpenAlex, Crossref, Europe PMC, PubMed, bioRxiv | Date-windowed queries; bioRxiv `pubs` links preprints to published versions [37] | DOI and version |
| Central banks | Press-release pages and RSS where available; meeting calendars are HTML only [38] | Hash of statement text |
| Institutional reports (IMF WEO, OECD Outlook, BIS, World Bank) | Publication pages plus data API for the tables [29][30][31][32] | Edition key; hash |

**AI use.** None. Even "is this filing an earnings release?" is answered by code from structured signals (8-K Item 2.02 in the submissions JSON `items` field [19]).

**Failure modes.**

| Failure | Detection | Handling |
| --- | --- | --- |
| Network error, 5xx | Status, timeout | Retry with full-jitter backoff [8], max 6 attempts over ~2 h |
| 429 / `Retry-After` | Status and header | Snooze until the time given; halve the host bucket for 1 h |
| robots disallow | RFC 9309 parse | `licence_blocked`; source amber |
| Feed shape changed | 0 refs on a day the calendar expected ≥ 1, or a deserialisation error | `upstream_changed`; source red; dev task raised by the Rollcall workflow |
| Daily quota nearly spent | Per-source counter | Defer non-urgent pulls to tomorrow |
| Wrong content type or oversize | Headers, streamed size | Abort; `invalid_data` |

**Tests.** Recorded fixtures per connector (no network in CI); a fake HTTP server for the 304 path, both `Retry-After` forms, robots 4xx/5xx semantics and token-bucket pacing on a virtual clock.

**Metrics.** Requests, 304 ratio, bytes, latency p50/p95 per host; errors by class; quota use; discovery lag (source publish time → discovered).

### 3.3 Diary

**Responsibility.** Know what is due and when, so work is scheduled ahead: pre-warm history the day before, fetch at release time, and show the Edition workflow what is about to land.

```sql
create table calendar_events (
  cal_id bigserial primary key, source_id text not null references sources,
  event_key text not null unique,     -- 'macro:us.cpi:2026-10', 'policy:fed:2026-12-09', 'earnings:0000320193:2027Q1'
  title text not null,
  expected_at timestamptz,            -- null when only the day is known
  expected_day date not null,
  precision text not null,            -- 'exact' | 'day' | 'window'
  window_days smallint default 0,
  origin text not null,               -- 'ics' | 'api' | 'html' | 'derived'
  status text not null default 'expected',  -- 'expected' | 'captured' | 'late' | 'cancelled'
  captured_at timestamptz
);
```

| Origin | Sources | How |
| --- | --- | --- |
| ICS | BLS [22], BEA [23], Eurostat (theme-filtered) [24] | Parse daily; map event summaries to event keys with a per-source rule table |
| API | FRED `releases/dates` with `include_release_dates_with_no_data=true` for future dates [21]; BIS [32] | Daily sync |
| HTML | FOMC, ECB, BoE, BoJ, RBA, RBI meeting calendars [38] | Parsed by connector, plus a hand-checked yearly seed file, because these matter most and change rarely |
| Derived | Company earnings: no reliable free earnings-date source exists (Nasdaq's JSON is unofficial) | Expected window = same fiscal period last year ±7 days from the company's own filing history; inside the window the SEC 8-K Atom feed is polled every 10 minutes for that CIK |

**Algorithm.** For each event: a *pre-warm* job at `expected_day − 1` (fetch history, resolve entities, build the static parts of the brief) and a *capture* job at `expected_at + 2 min` that retries every 5 minutes for 2 hours, then hourly for a day, then marks `late`.

**AI use.** None. **Failure modes.** A wrong calendar date only delays capture: feed discovery still finds the release, so the calendar is an accelerator, never the only route. **Metrics.** Release → capture and release → publish latency; `late` count by source.

### 3.4 Clerk and Ledger

**Responsibility.** Turn snapshots into (a) data points with values exactly as published and full provenance, (b) text blocks for quotes and condensation, (c) entity mentions, (d) candidates with event keys.

**Data points** (replaces the TD sketch; kept after publish, because pages reference them and revisions need history):

```sql
create table data_points (
  dp_id        text primary key,     -- 'dp_' || base32(sha256(series_key|context_key|period|vintage))[0:16]
  context_key  text not null,        -- canonical identity context: sorted XBRL dimension=member pairs,
                                     -- unit, entity scheme, plus any source-specific identity fields
                                     -- (SEC: the filing's accession number; SDMX: key dimensions);
                                     -- '' when the fact has none. Two SEC filings filed the same day
                                     -- therefore never share a dp_id; a restatement is a new dp_id
                                     -- linked through supersedes. Two facts with
                                     -- the same series, period and filing but different contexts get different IDs.
  series_key   text not null,        -- 'xbrl:0000320193:us-gaap:Revenues', 'fred:CPIAUCSL', 'bls:CUUR0000SA0'
  entity_id    bigint references entities,
  concept      text not null,        -- source's own concept or measure name
  label        text not null,        -- source's label, verbatim
  value_text   text not null,        -- exactly as published ("1,234.5", "-0.3")
  value        numeric,              -- exact decimal parsed from value_text; never float
  unit         text not null,        -- verbatim ('USD', 'Index 1982-84=100', 'Percent', 'Thousands of Persons')
  currency     char(3),              -- ISO 4217 when the unit is money; never converted
  scale        text,                 -- as stated ('millions'); value is NOT multiplied
  decimals     smallint,             -- XBRL @decimals when present
  period_label text not null,        -- verbatim ('Q4 FY26', 'October 2026')
  period_start date, period_end date,
  period_type  text not null,        -- 'instant' | 'duration'
  fiscal_year  smallint, fiscal_period text,   -- the company's own (XBRL dei elements)
  seasonal_adj text,                 -- 'SA' | 'NSA' | 'SAAR' | null, as stated
  vintage      timestamptz not null, -- FRED realtime_start, SEC 'filed', or first-seen time
  supersedes   text references data_points,
  snapshot_sha bytea not null references snapshots,
  locator      jsonb not null,
  parser       text not null,        -- connector id + PARSER_VERSION
  retrieved_at timestamptz not null
);
create index on data_points (series_key, period_end, vintage desc);
```

Locators make every figure traceable to a position inside the stored bytes:

```json
{"kind":"xbrl","accn":"0000320193-26-000123","tag":"us-gaap:Revenues","context":"FY2026Q4","unit":"USD","decimals":-6}
{"kind":"json","path":"$.observations[812].value","series":"CPIAUCSL","realtime_start":"2026-11-13"}
{"kind":"pdf","page":17,"table":2,"row":4,"col":3,"bbox":[72,410,540,428]}
{"kind":"sdmx","flow":"ECB:ICP(1.0)","key":"M.U2.N.000000.4.ANR","obs":"2026-10"}
```

**Derived figures.** Changes and shares are computed by code into their own table with the formula recorded, and cited on the page as "Calculated by News from …" (decision D5):

```sql
create table derived_points (
  dp_id    text primary key,      -- 'dv_…'
  formula  text not null,         -- 'pct_change(a,b)' | 'pp_change(a,b)' | 'diff(a,b)' | 'share(a,total)'
  inputs   text[] not null,       -- dp_ids
  value    numeric not null, unit text not null, rounding smallint not null,
  guard    jsonb not null         -- preconditions that held: same unit, currency, scale, seasonal adjustment,
);                                 -- like-for-like periods
```

Guards keep the no-conversion rule: a change is computed only between points with identical unit, currency, scale and seasonal adjustment and comparable periods (the same fiscal period a year apart, or as the page-type skill says). If a guard fails, no derived point exists and the writer cannot mention that change.

**Text blocks** (for quotes, condensation and spot checks):

```sql
create table documents (
  doc_id bigserial primary key, snapshot_sha bytea not null references snapshots,
  kind text not null,            -- 'press_release' | 'statement' | 'filing_section' | 'report_chapter' | 'abstract'
  title text, lang text not null default 'en', published_at timestamptz,
  text_len int not null, simhash bigint not null, content_sha bytea not null
);
create table text_blocks (
  doc_id bigint references documents, block_no int, kind text, page_no int, text text not null,
  primary key (doc_id, block_no)
);
```

**Parsers.** XBRL from the SEC JSON APIs first (inline XBRL only when needed); JSON, CSV, SDMX and JSON-stat in Rust; PDF text with a Rust extractor; hard tables with Docling and scanned pages with Tesseract (TD), each in a single-slot pool.

**Event keys** (story identity for structured items):

| Page type | Event key pattern |
| --- | --- |
| Earnings | `earnings:<cik or lei>:<fiscal_year><fiscal_period>` |
| Macro release | `macro:<country>.<series_family>:<reference_period>` |
| Policy decision | `policy:<central_bank>:<decision_date>` |
| Major report | `report:<institution>.<series>:<edition>` |
| Science | `paper:<doi>` or `paper:arxiv:<id>` (version dropped) |
| News story | none (assigned by clustering) |

**Candidates** (extends the TD table):

```sql
create table candidates (
  cand_id bigserial primary key, source_id text not null, event_key text, story_id bigint,
  doc_ids bigint[] not null default '{}', dp_count int not null default 0,
  entity_ids bigint[] not null default '{}',
  page_type text, page_type_by text not null,    -- 'rule' | 'ai' | 'human'
  categories text[], region text,
  published_at timestamptz, first_seen timestamptz not null default now(),
  status text not null    -- 'new' | 'grouped' | 'scored' | 'selected' | 'briefed' | 'written' | 'published' | 'skipped'
);
```

**Classification by rules first.** Page type, categories and region come from a rules table keyed on source, concept and entity kind (SEC 10-Q → Earnings → Finance/Earnings; BLS CPI → Macro release → Economy/Inflation; arXiv `astro-ph.*` → Science/Space). Rules cover the structured sources completely. Only free-text items (news stories, some agency press releases, science sub-categories) can be ambiguous; those go to A3 with a closed set of allowed labels.

**AI use.** A3 only. No AI touches values, units, periods or locators.

**Failure modes.** Parse error → typed `ParseError` with the locator where it failed; the job goes to the dead list (no blind retries: a parser fix plus the Salvage workflow re-runs it). Unknown unit string → stored verbatim, item `needs_human` for the unit vocabulary, never guessed.

**Tests.** Golden parse fixtures per connector with at least one awkward case (restatement, non-calendar fiscal year, footnoted value, missing observation); property tests that derived-point guards never allow a change across different units, currencies or scales; a round-trip test that `value` formats back to `value_text`.

**Metrics.** Data points per snapshot, parse failures by connector and class, unknown units, derived points refused by guards.

### 3.5 Roster and Roster's resolver

**Responsibility.** One identity per company, country, central bank, institution, official, topic and source; mentions resolved to it by identifiers and aliases.

```sql
create table entities (
  entity_id bigserial primary key,
  kind text not null,            -- 'company' | 'country' | 'central_bank' | 'institution' | 'person' | 'topic' | 'agency'
  name text not null, prominence smallint not null default 3,   -- 1 flagship, 2 major, 3 other (§4.2)
  region text, slug text unique, attrs jsonb not null default '{}'
);
create table entity_ids (
  scheme text not null,          -- 'cik' | 'lei' | 'ticker@MIC' | 'iso3166' | 'iso4217' | 'ror' | 'wikidata' | 'figi'
  value text not null, entity_id bigint not null references entities,
  valid_from date not null default '-infinity', valid_to date,
  primary key (scheme, value, valid_from)
);
create table entity_aliases (
  alias_norm text not null, entity_id bigint not null references entities,
  source text not null, weight real not null default 1, primary key (alias_norm, entity_id)
);
create index on entity_aliases using gin (alias_norm gin_trgm_ops);
-- TD 'edges' table unchanged: (src, rel, dst, weight, since)
```

**Seed data, all free.** SEC `company_tickers.json` for CIK ↔ ticker ↔ name (the SEC does not guarantee its accuracy, so it is a seed, not truth) [18]; GLEIF LEI files (Golden Copy updated three times a day) [42]; OpenFIGI where a ticker needs a FIGI [41]; the ROR dump for research institutions (CC0) [43]; Wikidata (CC0) for labels, aliases and ISO 3166 codes (property P297) [44]; an open ISO 4217 list *(source to be chosen; unverified)*.

**Resolution, deterministic, in order.**

1. **Identifier**: CIK, LEI, ISO code, series ID or DOI in the source → exact entity. Covers almost all structured items.
2. **Alias exact match** after normalisation (Unicode NFKC, case-fold, strip legal suffixes such as Inc., plc, Ltd, AG, S.A., and punctuation).
3. **Gazetteer scan** of titles and first paragraphs with an Aho–Corasick automaton over all aliases, constrained by the expected kind (a central-bank item expects one central bank).
4. **Fuzzy fallback**: `pg_trgm` similarity ≥ 0.6, same kind, unique best match with a margin ≥ 0.1 over the second; otherwise unresolved.
5. **Unresolved** mentions go to a queue shown in the Huddle workflow. Optional A6 picks from a code-made shortlist of ≤ 5 entities or answers "none"; it can never create an entity.

**Tools.** `news entities resolve|show` (agents, read-only); `alias add|merge` (Karthik only).

**Failure modes.** Ticker reuse after delisting → validity dates; similarly named companies → the margin rule refuses ambiguous matches.

**Tests.** 300 labelled mentions (suffix variants, "Bank of …" collisions, country adjectives such as "Chinese") with precision ≥ 0.99 required on resolved mentions; recall reported.

**Metrics.** Resolution rate per step, unresolved queue size, labelled-set precision per resolver release.

### 3.6 Taster

**Responsibility.** Group candidates that describe the same story, drop duplicates, and recognise updates to existing pages.

```
candidate ─▶ (1) exact duplicate?  content_sha seen in 30 days → drop (keep provenance link)
          ─▶ (2) near duplicate?   SimHash-64 Hamming ≤ 3 within 7 days → same document group
          ─▶ (3) event key?        yes → story = that key's story (create if new); go to (5)
          ─▶ (4) no key (news, unstructured):
                 embed title + first ~400 tokens → nearest story centroids in a 72 h window (pgvector HNSW)
                 cos ≥ 0.82 and ≥ 1 shared resolved entity              → join
                 0.72 ≤ cos < 0.82, or ≥ 0.82 with no shared entity      → grey zone → A4 (Haiku, batched) or stay separate
                 cos < 0.72                                              → new story
          ─▶ (5) existing page?   (every candidate that reaches a story runs this check, keyed or not)
                                     same event key, or cos ≥ 0.85 to a page from the last 30 days with a shared entity
                                   → UPDATE candidate, not a new page
```

The thresholds (0.72, 0.82, 0.85) are starting values *(speculative)*, calibrated in Phase 4 on 200 labelled same/different pairs (golden set G3, §9.1).

**Why this shape.** 64-bit SimHash with Hamming distance 3 was shown to find near-duplicate web pages at the scale of billions [45] and is very cheap; it catches syndicated copies of one press release. Rust has MinHash and SimHash implementations (for example `gaoya`) [46], though a 64-bit SimHash is ~50 lines and may be simpler to own. Most items here have event keys, so similarity search runs only on the small unstructured remainder. Batch clustering (HDBSCAN) is not needed for a stream; nearest-centroid assignment within a time window follows the online approach used for streaming news clustering [47] *(details of [47] beyond its abstract not verified)*.

**Embedding model.** Recommended default **BAAI bge-small-en-v1.5** (33M parameters, 384 dimensions, 512-token context, MIT, MTEB average 62.2) [49] through `fastembed-rs` on ONNX Runtime (`ort`) [50], int8 if the Phase 4 spike shows no material loss. Benchmarked alongside it:

| Model | Dim | Licence | Notes |
| --- | --- | --- | --- |
| bge-small-en-v1.5 | 384 | MIT | Small, fast, in fastembed-rs [49][50] |
| nomic-embed-text-v1.5 | 768 (Matryoshka to 256) | Apache-2.0 | 8k context; needs task prefixes [49] |
| EmbeddingGemma-300M | 768 (MRL 512/256/128) | Gemma licence (gated) | MTEB English v2 69.7, but licence terms need reading [49] |
| Qwen3-Embedding-0.6B | ≤ 1024 | Apache-2.0 | Best scores; 0.6B parameters is heavy for this CPU; candle backend [49][50] |
| all-MiniLM-L6-v2 | 384 | Apache-2.0 | Baseline; 256-token limit [49] |

No published throughput exists for this CPU. A server benchmark of MiniLM-L12 on ONNX Runtime reached ~140–230 docs/s on 16 cores, ~10 docs/s on candle, with disabling intra-op spinning the biggest single win [51]; Intel's int8 gains for bge rely on AMX/VNNI, which the i5-8250U lacks [52]. *Estimate (speculative): 20–60 docs/s for bge-small on 2 threads, so 2,000 items a day embed in about a minute.* The spike measures it.

```sql
create table embeddings (
  owner_kind text not null,        -- 'candidate' | 'story' | 'page'
  owner_id bigint not null, model text not null,   -- 'bge-small-en-v1.5@int8'
  v halfvec(384) not null, created_at timestamptz not null default now(),
  primary key (owner_kind, owner_id, model)
);
create index on embeddings using hnsw (v halfvec_cosine_ops);
create table stories (
  story_id bigserial primary key, event_key text unique, title text,
  first_seen timestamptz not null, last_seen timestamptz not null, member_count int not null default 1,
  entity_ids bigint[] not null default '{}', page_id text,
  status text not null default 'open'          -- 'open' | 'covered' | 'closed'
);
```

pgvector 0.8.x HNSW defaults (m 16, ef_construction 64, ef_search 40) are kept; `hnsw.iterative_scan = relaxed_order` is set for the time-window query so the filter does not starve results; `halfvec` halves storage [48]. *(1M rows × 384 dims as halfvec ≈ 0.8 GB plus index.)*

**AI use.** A4 grey-zone adjudication only. Fallback: keep separate. Two pages are better than a wrong merge, and step (5) later catches it as an update.

**Tools.** `news stories show|members|similar` (read-only); `merge|split` are Karthik-only.

**Tests.** G3 pairs: precision ≥ 0.97 on "join" decisions, recall reported; SimHash tests on syndicated releases; idempotency (re-running changes nothing).

**Metrics.** Items per story, grey-zone rate, A4 calls a day, manual merges or splits.

### 3.7 Nose and Desk (summary)

Code scores each story 0–100 from importance, magnitude or surprise, freshness, data richness, novelty and coverage gap. Desk fills the run breadth-first with diminishing returns per bucket and assigns tier and indexing priority. Full design in [§4](#4-scoring-and-ranking-in-full).

```sql
create table scores (
  story_id bigint not null, computed_at timestamptz not null, scorer_version text not null,
  components jsonb not null,      -- {"I":0.95,"M":0.528,"F":0.841,"D":0.945,"N":0.9,"G":0.1}
  total numeric(5,2) not null, explain text not null,     -- the Pitch
  primary key (story_id, computed_at)
);
create table run_plans (           -- the Budget
  run_id uuid not null, story_id bigint not null, rank int not null, tier smallint not null,
  bucket text not null, adjusted numeric(5,2) not null, reason text not null,
  action text not null,           -- 'new' | 'update'
  primary key (run_id, story_id)
);
```

### 3.8 Fixer

**Responsibility.** Build the brief (the TD's research pack) for one story: everything the writer needs and nothing else. Code only, except optional condensation of long source text (A5).

**Brief format** (JSON, target 1–3k tokens, hard cap 4k):

```json
{
  "brief_version": 1,
  "item": {"story_id": 81234, "event_key": "macro:us.cpi:2026-10", "page_type": "macro-release", "tier": 1,
           "action": "new", "slug_hint": "us-cpi-october-2026", "data_as_of": "2026-11-13"},
  "entities": [{"id": "ent_us", "name": "United States", "kind": "country"},
               {"id": "ent_bls", "name": "U.S. Bureau of Labor Statistics", "kind": "agency"}],
  "facts": [
    {"ref": "dp_7kq2", "label": "CPI-U, all items, 12-month change, NSA", "value": "2.4", "unit": "Percent", "period": "October 2026"},
    {"ref": "dp_9mz1", "label": "CPI-U, all items, 12-month change, NSA", "value": "2.7", "unit": "Percent", "period": "September 2026"},
    {"ref": "dv_3ab8", "label": "Change in the 12-month rate", "value": "-0.3", "unit": "pp", "derived": "pp_change"}
  ],
  "series": [{"ref": "sr_cpi_yoy_5y", "label": "CPI-U 12-month change, monthly, 2021–2026", "points": 60}],
  "periods": [{"ref": "pd_2026_10", "label": "October 2026"}, {"ref": "pd_2021_03", "label": "March 2021"}],
  "context": [{"id": "cx_1", "text": "Lowest 12-month rate since March 2021", "rule": "series_min_since"}],
  "quotes": [{"id": "q_1", "doc": 5521, "block": 3, "text": "The index for shelter rose 0.2 percent in October…"}],
  "source_text": {"untrusted": true, "points": ["…"]},
  "media": {"images": [], "video": null},
  "related_pages": [{"page_id": "2026/10/us-cpi-september-2026", "title": "…"}],
  "components": {"suggested": ["KeyFigures", "LineChart", "BarChart", "DataTable", "Prose"]},
  "rules": {"disclaimer": "not-investment-advice", "fiscal_note": null}
}
```

- **Facts carry values** so the writer understands direction and size, but it may output only `ref` tokens. The numeric-literal lint enforces this (C4).
- **Reference formats.** Fixer issues exactly six kinds of reference, all lower-case `prefix_body` with `_`-separated parts: `dp_…` (data point), `dv_…` (derived value), `sr_…` (series, e.g. `sr_cpi_yoy_5y`), `pd_…` (period label, e.g. `pd_2026_10` for "October 2026"), `q_…` (quote) and `cx_…` (context line). The draft schema (§3.9) accepts these and nothing else, and C2 checks each one exists in this brief.
- **Periods are references too.** The writer never types a date or year: it writes `pd_…` and code renders the label from the brief, so C4 can forbid digits outright.
- **Context lines are computed by code** ("lowest since …", "first cut since …", "record in the series"), each with the rule that produced it, so it can be re-checked. They are the news hooks the writer can use without inventing anything.
- **Untrusted text is fenced**: source text sits under `source_text` with `"untrusted": true`, JSON-encoded; the writer's instructions say it is data, not instructions (§9.3).
- **Component specs are pre-selected** by code from the page-type skill's typical layout and placed in the cached prefix, so the writer needs no tool calls (D4).
- **Size budget**: above 3k tokens (local tokenizer estimate), code drops the lowest-priority facts and series first, then condenses text (A5).

**Tests.** Snapshot tests per page type; size budget; no untrusted text outside `source_text`. **Metrics.** Brief tokens by page type, facts per brief, condensation calls.

### 3.9 Reporter

**Responsibility.** Turn a brief into a Copy: headline, fly title, summary and OpenUI Lang with data references. The main AI step (A1) and the only place reader-facing prose is written.

**Call shape** (Ghost spawns one `claude -p` per item, `NEWS_PARALLEL` = 3):

```bash
claude -p \
  --model "$NEWS_MODEL_MAIN" \
  --tools "" --strict-mcp-config --mcp-config '{"mcpServers":{}}' \
  --settings pipeline/claude/page-call.settings.json \
  --disable-slash-commands --no-session-persistence \
  --append-system-prompt-file work/prefix/macro-release.3f9a1c.md \
  --json-schema "$(cat pipeline/schemas/page-draft.v1.json)" \
  --max-turns 2 --max-budget-usd 0.60 \
  --output-format json \
  < work/briefs/81234.json
# child env: ANTHROPIC_API_KEY unset; CLAUDE_CODE_DISABLE_AUTO_MEMORY=1;
#            OTEL_RESOURCE_ATTRIBUTES=news.run_id=<run>,news.job_id=<job>,news.step=A1
```

Why each flag [62][63][68]: `--tools ""` disables all built-in tools (`--allowedTools` only pre-approves, it does not restrict); `--strict-mcp-config` with an empty config stops project MCP servers loading, and without it `-p` also runs project hooks and `.mcp.json` servers with no trust prompt; `--json-schema` makes the harness validate the result and re-prompt, returning `structured_output`, or the subtype `error_max_structured_output_retries`; `--max-budget-usd` is a client-side estimate that can overshoot, used as a runaway guard, not a bill; if `ANTHROPIC_API_KEY` is set, `-p` uses it instead of the subscription, so the runner removes it; `--bare` must **not** be used, because it never reads subscription credentials. The settings file and `--setting-sources` keep project hooks and plugins out of page calls *(the exact combination is confirmed in the Phase 7 lean-call spike; unverified)*.

**Prompt layout for caching.** The prefix (instructions + `news-brand` + one `page-*` skill + component index + that page type's pre-selected component specs) is one file named by its hash, identical for every call of that page type. The runner orders each batch by page type so calls with the same prefix run back to back. On a subscription, `-p` main-conversation requests get a 1-hour cache TTL while sub-agents and workflow agents get 5 minutes [68], one reason page writing runs through Ghost rather than workflow agents. Cache reads are checked per call from `usage` in the JSON result.

**Output schema** (`page-draft.v1.json`, abridged):

```json
{
  "type": "object", "additionalProperties": false,
  "required": ["headline", "fly_title", "summary", "oui", "refs_used", "components_used", "angle"],
  "properties": {
    "headline":  {"type": "string", "minLength": 20, "maxLength": 110},
    "fly_title": {"type": "string", "maxLength": 40},
    "summary":   {"type": "string", "minLength": 60, "maxLength": 240},
    "oui":       {"type": "string", "maxLength": 24000},
    "refs_used": {"type": "array", "items": {"type": "string", "pattern": "^(dp|dv|sr|pd|q|cx)_[a-z0-9]+(_[a-z0-9]+)*$"}},
    "components_used": {"type": "array", "items": {"type": "string"}},
    "angle": {"type": "string", "enum": ["latest-figure", "change", "composition", "trend", "decision", "finding", "comparison", "timeline"]}
  }
}
```

Headlines and summaries carry numbers only as `{ref}` tokens that the renderer fills, for example `"Annual inflation in America fell to {dp_7kq2} in October"`.

**Validation (code, `news ai accept`).** JSON schema → OpenUI Lang parses → every ref exists in the brief → numeric-literal lint → banned words (brand rules) → component props valid against specs → length rules. On failure: one repair call (A2) with the error list and the previous draft; a second failure sends the item back to the queue as `ai_invalid_output` for the next run, and the run carries on.

**Fallback.** For Tier 2 items whose writer fails twice, a **templated page** built only by code (KeyFigures + main chart + data table + sources, headline from a page-type pattern with refs, no prose) may be published, flagged `templated` so a later run upgrades it (D10).

**Budget.** ≤ 2 turns; brief ≤ 3k tokens; prefix 6–10k tokens (mostly cache reads); output ≤ 4k tokens; per-run token ceiling `NEWS_AI_RUN_TOKENS` (new).

**Tests.** Without AI: validator tests with hand-made good and bad drafts (digits in prose, unknown ref, wrong props, banned words). With AI: golden set G1 (§9.1). **Metrics.** Tokens in/out, cache-read ratio, turns, validation failures by rule, repair rate, wall time, cost estimate.

### 3.10 Proof

**Responsibility.** Decide whether a rendered page may be published: code checks on every page (zero tokens), a Haiku sample check on a share (A7). Extends TD "Quality checks".

| # | Check | Rule | Severity |
| --- | --- | --- | --- |
| C1 | Parse and render | OpenUI Lang parses; renderer returns HTML, Markdown, JSON | block |
| C2 | References resolve | Every ref in `.oui`, headline and summary exists in `data.json` with a source | block |
| C3 | Values match source | Each rendered value equals the data point's `value` through the shared formatter; derived values recomputed from inputs | block |
| C4 | **Numeric-literal lint** | Model text contains no digit sequence at all, except an entity name containing digits that Roster resolved for this brief and a short allow-list (G7, G20, COVID-19, 5G …). Periods, years and dates are written only as `pd_…` references and rendered by code (§3.8), so there is no year exemption to misuse: "revenue was 2024 USD" is blocked whether or not 2024 is a period in the brief | block |
| C5 | **Direction words** | A sentence pairing a change ref with a direction word ("rose", "fell", "cut", "raised", "unchanged" …, ~60 verbs) must agree with the sign of the value | block |
| C6 | Quotes verbatim | Every quote is an exact substring of its text block | block |
| C7 | Period and unit labels | Shown labels equal the source's period label and unit; fiscal labels as the company writes them | block |
| C8 | Licence | `redistribution=false` sources produce no CSV/JSON of their data; attribution present for every source | block |
| C9 | Required fields | Title, summary, categories, alt text; disclaimers by category | block |
| C10 | Links and media | Source URLs, image URLs and embeds respond (cached 24 h); images only from the allow-list | block (sources) / warn (images: placeholder exists) |
| C11 | Banned words and tone | Brand list; no "beat/miss" unless against the company's own guidance | block |
| C12 | Size and structure | Bundle size budget; component count within the page type's range | warn |
| C13 | Accessibility | axe-core: zero serious or critical issues; every Tier 1 page and 1 in 10 Tier 2 pages (browser cost) | block when run |
| C14 | Duplicate page | No published page with the same event key or slug (else it must be an update) | block |

**Sample check (A7).** `SAMPLE_RATE` (0.05) of pages, plus every Tier 1 page in the first month, get a Haiku comparison of the page Markdown with its brief, returning typed issues (§7). A high-severity flag blocks an unpublished page; on a published page it queues an update. The checker's own recall is measured with seeded faults (G6).

**Tools.** `news check <id>` → `{pass, failures:[{check, where, message}]}`. **Metrics.** Pass rate per check and page type, block reasons, sample flag rate, confirmed-flag rate.

### 3.11 Rewrite

**Responsibility.** Keep published pages correct when sources revise or restate data.

1. A new vintage arrives (`supersedes` set), or an SEC fact with a later `filed` date for the same tag and period.
2. Code finds affected pages through `page_refs`.
3. Code classifies the change:
   - **Value only**: re-render with the new value. Prose is unchanged because it contains no numbers. No AI.
   - **Direction or threshold flip**: a referenced change crosses zero, a context line ("lowest since …") stops being true, or C5 would now fail → A1 in update mode with the old page, the diff and the brief.
   - **Restatement of a prior period** (earnings): re-render and add the "restated" note component automatically.
4. Proof run; Press publishes version n+1 with "Updated on …".

**Metrics.** Revisions detected, pages updated, share needing AI, revision → update latency.

### 3.12 Press

**Responsibility.** The laptop side of publishing: send bundles to the write API once, safely, at a pace the free tiers allow; decide indexing.

**Outbox** (transactionally staged drain [7]): the transaction that marks a page `ready` also inserts an outbox row; one publisher job drains it in order, so a crash never half-publishes or forgets a page.

```sql
create table publish_outbox (
  outbox_id bigserial primary key, page_id text not null, version int not null, bundle_sha bytea not null,
  idempotency_key text not null unique,    -- page_id:version:bundle_sha
  op text not null,                        -- 'put_page' | 'withdraw' | 'index_promote' | 'home_curation'
  created_at timestamptz not null default now(), sent_at timestamptz, response jsonb, attempts int default 0
);
create table pages (
  page_id text primary key, slug text unique not null, story_id bigint, page_type text, tier smallint,
  version int not null, status text not null,    -- 'draft' | 'ready' | 'published' | 'withdrawn'
  published_at timestamptz, updated_at timestamptz, index_status text not null default 'noindex',
  renderer_version text, bundle_sha bytea, oui_sha bytea, flags text[] default '{}'
);
create table page_refs (page_id text, version int, ref text, primary key (page_id, version, ref));
```

**Publish interface** (contract with the cloud; same substance as the TD): `PUT /v1/pages/{page_id}` with an `Idempotency-Key` header (Stripe-style keys [6]); body = bundle manifest and files; response `{version, url, purged:[…]}`; `PUT /v1/home/curation` for curations. A token bucket below DynamoDB's 25 write units paces writes (default 5 pages/s, burst 10), and purges are batched.

**Index budget.** Daily: take published `noindex` pages, excluding any flagged `templated` (they stay `noindex` until a writer upgrades them, D10), rank them by `index_priority = score + 10·[Tier 1] + uniqueness` (uniqueness = distinct data series and charts no indexed page has) and promote up to `NEWS_INDEX_PER_DAY`. Code only.

**Failure modes.** 5xx → retry with backoff (safe because of the idempotency key); 4xx validation → `bug` class, dead list; throttling → snooze the publish pool.

**Metrics.** Publish latency, retries, throttles, pages per index status.

### 3.13 Ticker (summary)

Every subsystem emits rows to the existing `news.events` table and OTel spans; new columns, tables and dashboard panels are in [§9.2](#92-observability).

---

## 4. Scoring and ranking in full

### 4.1 Goals

1. Rank stories by how much they deserve a page, using only signals code can compute from source data and our own records.
2. Fill each run **breadth-first** across categories, regions and page types (PRD "Content scope"), not just top-N.
3. Explain every decision: each score has a Pitch (one line per component) that Karthik and agents can read.
4. Stay simple enough to tune by hand now, with a clear path to learning from feedback later.

Signals deliberately **not** used: market prices or market capitalisation (PRD non-goal), analyst consensus (paid data, and it invites "beat/miss" framing the brand rules forbid), social-media virality, and any model's opinion of "importance". There is no free consensus feed, so "surprise" here means surprise against the series' own history, not against forecasts.

The literature on news values (Galtung and Ruge; Harcup and O'Neill's 2017 update [53]) and Google's published news-ranking factors (relevance, prominence, authoritativeness, freshness and others, without weights [54]) inform the choice of signals; the weights below are ours.

### 4.2 Signals

All signals are in [0, 1]. "Story" means a Taster cluster; signals use the story's best member.

| Signal | Symbol | Formula | Notes |
| --- | --- | --- | --- |
| Importance | I | `0.5·E + 0.3·P + 0.2·T` | Event base, prominence, source tier |
| Event base | E | From the table below, by event type | Data, editable |
| Prominence | P | max over linked entities: prominence 1 → 1.0, 2 → 0.6, 3 → 0.3 | See prominence rules |
| Source tier | T | tier 1 → 1.0, 2 → 0.9, 3 → 0.6, 4 → 0.3 | From Contacts |
| Magnitude / surprise | M | By page type, below | Never from a model |
| Freshness | F | `2^(−age_h / h)` | Half-life h by page type |
| Data richness | D | `min(1, ln(1+n) / ln 51) · (0.7 + 0.3·s)` | n = resolvable data points; s = 1 if a series of ≥ 12 points exists |
| Novelty | N | 1.0 new event key; 0.9 regular series with a page last period; 0.3 follow-up of a covered story; for unstructured items `1 − max cos` to pages of the last 30 days sharing an entity | Updates are planned separately |
| Coverage gap | G | mean over the story's category, region and page type of `max(0, (target − actual₇d) / target)` | Shares over the last 7 days of published pages |
| Quality multiplier | Q | product of: 0.9 if `redistribution=false`; 0.9 if parse warnings; 0.8 if the main entity is unresolved | |

**Event base weights (E)**, stored as data (`event_weights` table), starting values:

| Event type | E |
| --- | --- |
| Policy decision: Fed, ECB, BoE, BoJ, RBI, PBoC, RBA, BoC | 1.0 |
| Flagship institutional report (IMF WEO / GFSR / Fiscal Monitor, OECD Economic Outlook, World Bank GEP, BIS Annual Report) | 0.95 |
| Headline macro in target regions: CPI, GDP, employment (US, euro area, UK, China, India, Japan, Australia, Canada) | 0.9 |
| Other central-bank decisions | 0.8 |
| Company results (10-K, 10-Q, 20-F, ESEF annual report, results 8-K Item 2.02) | 0.7 |
| Other macro releases; other institutional reports | 0.6 |
| Agency science release (NASA, ESA, NOAA …); peer-reviewed paper | 0.6 / 0.5 |
| News story (primary-source-backed) | 0.5 |
| Preprint | 0.3 |
| Other 8-K items | 0.3 |

**Prominence rules (P)**, stored on `entities.prominence`, reviewed quarterly:

- Companies: 1 = a hand-kept flagship list (about 200, seeded from the largest companies by reported annual revenue *within each reporting currency*, so no conversion is needed, plus additions); 2 = constituents of major regional indices (public lists); 3 = others.
- Countries: 1 = United States, euro area, United Kingdom, China, India, Japan, Australia, Canada; 2 = other G20; 3 = others.
- Institutions: 1 = IMF, World Bank, OECD, BIS, WHO and the central banks above; 2 = other multilaterals and national agencies; 3 = others. Research institutions: 2 if in a hand-kept list, else 3.

**Magnitude / surprise (M)** by page type:

| Page type | Formula |
| --- | --- |
| Macro release | `z = |x_t − x_{t−1}| / σ(Δx)` over the last 60 observations (20 for quarterly); `M = 1 − e^(−z/2)`; if the value is a 10-year high or low, `M = max(M, 0.8)` |
| Policy decision | \|Δrate\| ≥ 50 bp → 1.0; 25 bp → 0.8; hold with a code-detected statement change (text diff > 20 %) → 0.4; plain hold → 0.3; +0.2 (cap 1) for the first move after ≥ 6 holds or a change of direction |
| Earnings | `z` of year-on-year revenue growth against the company's own last 12 quarters of year-on-year growth; `M = 1 − e^(−z/2)`; loss↔profit flip or restatement → `max(M, 0.7)` |
| Major report | `z` of the headline forecast revision against the edition before, over the standard deviation of past revisions; 0.5 if not computable |
| Science | 0.3, + 0.2 if the paper links an open dataset, + 0.2 if it is the peer-reviewed version of a tracked preprint |
| News story | breadth: `min(1, log₂(1 + distinct primary sources) / log₂ 9)` |

**Freshness half-lives (h, hours):** news 12, macro 24, policy 24, earnings 48, science 72, major report 96. Backfill ignores F.

**Coverage targets** (shares of a 7-day window; data, editable): categories Tech/Business/Finance/Economy/Science 0.20 each; regions North America 0.25, Europe 0.20, India 0.15, China 0.10, other Asia 0.10, Australia 0.05, Global 0.15; page types earnings 0.25, macro 0.20, policy 0.10, major report 0.10, news 0.15, science 0.15, explainer 0.05.

### 4.3 Score

```
S = 100 · Q · (0.30·I + 0.20·M + 0.15·F + 0.15·D + 0.10·N + 0.10·G)
```

Weights live in a versioned config row (`scorer_version`), so every stored score says which weights produced it.

### 4.4 Selection (Budget)

**Greedy with diminishing returns per bucket** (a diversity rule in the spirit of maximal marginal relevance [56]):

```
adjusted(s) = S(s) · 0.92^n_cat(s) · 0.92^n_region(s) · 0.95^n_type(s) · 0.5^n_entity(s)
```

where each `n` counts stories already picked this run in the same category, region, page type or main entity. A story can carry up to three categories (and several regions): its `n_cat(s)` is the **largest** count among its categories (likewise `n_region`), so it is penalised by its most-covered bucket, and picking it increments the count of every category and region it carries. Loop: pick the highest `adjusted`, update counts, recompute (a lazy max-heap keeps this fast).

**Stopping.** `adjusted` only orders the candidates; it never stops the run. Stop when `NEWS_TARGET_PAGES` new pages are planned, or when no unpicked story with `S ≥ 40` (the Tier 2 floor) remains that the hard limits allow. *(An earlier draft stopped when the best `adjusted` fell below 35. With five categories that caps a run near 65 pages, since even a perfect story's 14th pick in one category is worth 100 × 0.92¹³ ≈ 33.8, so it could never reach the 100-page default.)*

Hard limits: **category cap** = `max(2, ⌈0.35 · NEWS_TARGET_PAGES⌉)` stories per category (35 % of a run, at least 2), counted on each story's primary category; one entity ≤ 2 stories per run; **Tier 1 quota** = `max(1, ⌊0.20 · NEWS_TARGET_PAGES⌋)` pages (Tier 1 pages cost more tokens). A Tier 1 story picked after the quota is used is still planned, but written with Tier 2 treatment (planned tier 2); it is not dropped. A story the category or entity cap blocks is skipped this run and stays queued.

**Tiers:** Tier 1 if `S ≥ 70`, or the main entity has prominence 1 and `S ≥ 60`; Tier 2 if `S ≥ 40`; otherwise stays *queued* (re-scored next run; freshness decays, so stale items fall away, and the Catchup workflow can pick them with F ignored).

**Updates** are planned separately: value-only revisions never enter the plan (code re-renders them); AI updates take at most 10 % of the run.

### 4.5 Index priority and home candidates

- **Index priority** (daily, Press): `S + 10·[Tier 1] + uniqueness`.
- **Home candidates** (`news home candidates`): `home_score = 0.45·S_now + 20·[Tier 1] + 0.20·traffic_pct + 0.15·visual_richness` where `S_now` re-applies freshness at the current time, `traffic_pct` is the page's view percentile among candidates (0 until analytics exist) and `visual_richness` (0–100) counts charts and key figures. Code pre-selects the top 40 with the same bucket-diversity rule, so the curator sees a broad, short list; `news home check` keeps the TD's breadth rules.

### 4.6 Worked example

Five stories in one run; coverage over the last 7 days: Economy 18 %, Finance 26 %, Science 10 %; North America 30 %, Global 10 %, India 9 %; macro 20 %, earnings 22 %, report 8 %, policy 10 %, science 8 %.

| | US CPI, October | IMF WEO, October | RBI policy (hold) | Apple FY26 Q4 results | arXiv preprint (cs.LG) |
| --- | --- | --- | --- | --- | --- |
| E, P, T | 0.9, 1.0, 1.0 | 0.95, 1.0, 1.0 | 1.0, 1.0, 1.0 | 0.7, 1.0, 1.0 | 0.3, 0.6, 0.6 |
| **I** | 0.950 | 0.975 | 1.000 | 0.850 | 0.450 |
| **M** | z = 1.5 → 0.528 | z = 0.67 → 0.283 | hold → 0.300 | z = 0.5 → 0.221 | 0.300 |
| **F** | 6 h / 24 → 0.841 | 50 h / 96 → 0.697 | 3 h / 24 → 0.917 | 20 h / 48 → 0.749 | 30 h / 72 → 0.749 |
| **D** | n = 40, series → 0.945 | n > 50 → 1.000 | n = 12, series → 0.652 | n = 120 → 1.000 | n = 6, no series → 0.346 |
| **N** | 0.9 | 1.0 | 1.0 | 1.0 | 1.0 |
| **G** | (0.10 + 0 + 0)/3 = 0.033 | (0.10 + 0.33 + 0.20)/3 = 0.211 | (0.10 + 0.40 + 0)/3 = 0.167 | (0 + 0 + 0.12)/3 = 0.040 | (0.50 + 0.33 + 0.47)/3 = 0.433 |
| **S** | **75.2** | **72.5** | **71.2** | **66.6** | **50.3** |
| Tier | 1 | 1 | 1 | 1 (prominence 1, S ≥ 60) | 2 |

Arithmetic for US CPI: `0.30·0.950 + 0.20·0.528 + 0.15·0.841 + 0.15·0.945 + 0.10·0.9 + 0.10·0.033 = 0.285 + 0.106 + 0.126 + 0.142 + 0.090 + 0.003 = 0.752`.

**Selection with a target of 4**, adding a sixth story, US PPI (S = 70.0; Economy, North America, macro). Primary categories: CPI, IMF WEO, RBI and PPI are Economy; Apple is Finance; the preprint is Science. With a target of 4 the category cap is `max(2, ⌈0.35 · 4⌉)` = 2 and the Tier 1 quota is `max(1, ⌊0.20 · 4⌋)` = 1.

| Pick | Chosen (adjusted) | Planned tier | Why the others moved |
| --- | --- | --- | --- |
| 1 | US CPI (75.2) | 1 (uses the Tier 1 quota) | — |
| 2 | IMF WEO (72.5 × 0.92 = 66.7) | 2 (quota used) | PPI now 70 × 0.92 × 0.92 × 0.95 = 56.3 (same category, region and type as CPI) |
| 3 | Apple (66.6 × 0.92 = 61.3, shares North America with CPI) | 2 (quota used) | Economy now has 2 = the cap, so RBI (would be 71.2 × 0.92² = 60.3) and PPI are blocked |
| 4 | arXiv preprint (50.3 × 0.92 = 46.3, shares Global with IMF WEO) | 2 | Only unblocked story left |

The run stops at the target of 4, with every limit satisfied: Economy 2 of 4 (the cap), one Tier 1 page, no entity twice. RBI and US PPI each score higher than the preprint on their own, but the category cap keeps the run broad; both stay queued and are re-scored next run (Economy's coverage share rises, so its G falls). This table is the Phase 5 test fixture for Desk; the fixture also adds a target-10 case so the entity limit and a Tier 1 quota of 2 are exercised.

### 4.7 Calibration and tuning

| What | How | When |
| --- | --- | --- |
| Log everything | Every score (selected or not) with components to ClickHouse `scores` | Always |
| Sanity views | Score histograms by page type; top 20 selected vs top 20 rejected, with Pitchs | Huddle |
| Human labels | Karthik answers ~30 pairwise questions a week ("which deserved a page first?"), 5 minutes | Huddle |
| Fit | Logistic regression on component differences, `P(a ≻ b) = σ(w·(x_a − x_b))` (a Bradley–Terry model), weights ≥ 0, summing to 1, L2-shrunk towards the current weights; each weight moves at most ±0.05 a week | Weekly, proposed as a diff |
| Agreement metric | Kendall's τ between the scorer's order and Karthik's labels on a held-out 20 % | Weekly trend |
| Adopt | Only after Karthik approves the diff; new `scorer_version` | Approval stop |

### 4.8 How it can evolve (not built now)

1. **Reader interest prior** once 8+ weeks of analytics exist: `R = (C·v̄ + Σviews) / (C + n_pages)` per category and entity (a Bayesian average, C ≈ 20), weight 0.05–0.10, taken from G and N.
2. **Learning to rank** (LightGBM `lambdarank` [57]) only when there are ~2,000+ labelled pairs or a stable engagement label *(threshold speculative)*. Features are the same components, so the switch is contained.
3. **Contextual bandits** [58] are not planned: one author, low traffic, and no randomised exposure logs for offline evaluation.

---

## 5. Orchestration engine

### 5.1 What it must do

- Run hundreds of small jobs per run across pools with different limits (network, CPU, Docling, AI, publish).
- Survive crashes, sleep and usage-limit stops; resume without repeating finished work.
- Hold multi-step preset runs with approval stops that may wait hours.
- Work from both Rust (most jobs) and Bun (Ghost, the renderer).
- Add no new server process if possible; PostgreSQL 17 is already there.

### 5.2 Options compared

| Option | Language | Needs | Licence | Weight on this laptop | Fit |
| --- | --- | --- | --- | --- | --- |
| **Custom queue on `FOR UPDATE SKIP LOCKED`**, River-style semantics | Rust (SQL) | PostgreSQL | ours | None | Full control; one implementation of retry, snooze, pools; Bun calls it through the CLI [1][2] |
| apalis + apalis-postgres 1.x | Rust | PostgreSQL | *(unverified)* | Library | Retries, timeouts, heartbeat and orphan re-enqueue, LISTEN/NOTIFY [9]; less control over cross-language claim and snooze semantics |
| graphile_worker (Rust port) 0.14 | Rust | PostgreSQL | *(unverified)* | Library | Compatible with Node Graphile Worker [17]; young |
| pg-boss 12 | TypeScript (Node or Bun) | PostgreSQL 13+ | MIT | Library | Retries, backoff, dead letters, cron [10]; would put orchestration in Bun while Rust owns the data |
| pgmq | SQL extension, Rust client | PostgreSQL 14–18 | PostgreSQL | Tiny | A visibility-timeout queue primitive only [11]; no runs, steps or snooze |
| DBOS Transact | TypeScript (Rust SDK pre-production) | PostgreSQL only | MIT | Library | True durable execution with checkpointed steps [12]; Rust SDK not production-ready, so it would make Bun the orchestrator |
| Absurd | SQL + TS/Python SDK | PostgreSQL only | Apache-2.0 | One SQL file | Checkpointed steps, sleeps, events [15]; self-described experiment |
| Restate | Rust server, many SDKs | Single binary + local disk | BSL 1.1 | Moderate; Windows binary not listed (Docker) | Strong durable execution [13]; a new server to run and learn |
| Temporal | Go server, TS SDK | Server + its DB | MIT | Heavy | Dev server not for production use [14]; overkill for one user |
| Inngest / Hatchet | Go servers | SQLite or PostgreSQL; Docker | SSPL server / MIT | Moderate to heavy | Built for teams and fleets [16] |
| Claude Code workflows as the orchestrator | JavaScript in Claude Code | Claude Code | — | None | Workflows cannot take user input mid-run, `-p` runs do not pause for usage limits, and agents get a 5-minute cache TTL [64][68]; good for fan-out inside one AI step, wrong as the system's backbone |

### 5.3 Recommendation

**A small custom PostgreSQL job engine in Rust (Dispatch), copying River's proven semantics** (unique jobs, states, attempt-based backoff, snooze without spending an attempt, a rescuer for stuck jobs) [2]–[5], plus a run/step table that makes preset runs durable (Chief).

Why:

- **No new moving parts.** PostgreSQL is already there; there is one machine and one user, so leader election, multi-node fairness and dashboards for fleets are unnecessary.
- **One owner of the rules.** Rust owns the data model; Bun reaches jobs only through `news jobs`, so retry, snooze and idempotency are written once.
- **Durable execution where it matters, without a framework.** Every step is idempotent and keyed on its inputs (§5.8), so "replay from the last checkpoint" is simply "re-run the step; finished keys are skipped". That is the core benefit of DBOS, Absurd or Restate [12][13][15] at a fraction of the weight.
- **Size.** About 1,000–1,500 lines of Rust and SQL *(estimate)*, testable without AI or network.

Fallback (D1): if building the engine takes longer than a week of agent time, adopt **apalis-postgres** for the worker loop and keep the run/step tables on top.

### 5.4 Job model

```sql
create table jobs (
  job_id      bigserial primary key,
  kind        text not null,            -- 'source.discover', 'doc.fetch', 'doc.parse', 'story.detect', 'story.score',
                                        -- 'run.select', 'item.brief', 'ai.write', 'ai.repair', 'item.render', 'item.check',
                                        -- 'ai.spotcheck', 'page.publish', 'page.revise', 'calendar.sync', 'calendar.capture',
                                        -- 'index.promote', 'work.prune', 'ai.classify', 'ai.storymatch', 'ai.condense'
  pool        text not null,            -- 'net' | 'cpu' | 'docling' | 'render' | 'ai' | 'publish'
  run_id      uuid, item_ref text,      -- story id, snapshot sha, page id …
  args        jsonb not null default '{}',
  unique_key  text,                     -- e.g. 'doc.parse:<sha>:<parser>' ; see §5.8
  priority    smallint not null default 2,   -- 0 urgent (release capture) … 4 backfill
  state       text not null default 'available',
              -- 'available' | 'scheduled' | 'running' | 'retryable' | 'snoozed' | 'completed' | 'discarded' | 'cancelled'
  attempt     smallint not null default 0, max_attempts smallint not null default 6,
  snoozes     smallint not null default 0,
  run_after   timestamptz not null default now(),
  lease_until timestamptz, locked_by text,
  error_class text, errors jsonb not null default '[]',   -- last 5 errors with class, message, at
  result      jsonb, created_at timestamptz not null default now(), finished_at timestamptz
);
create unique index jobs_unique_live on jobs (unique_key)
  where unique_key is not null and state not in ('completed','discarded','cancelled');
create index jobs_ready on jobs (pool, priority, run_after)
  where state in ('available','scheduled','retryable','snoozed');
create table pools (
  pool text primary key, max_running smallint not null, paused_until timestamptz, note text
);
```

**Claim** (one short transaction, safe with concurrent workers [1]). `SKIP LOCKED` alone stops two workers taking the same job, but not two workers both seeing one free slot and each taking a different job, which would exceed `max_running`. So the claim first locks the pool's row, which serialises claimers per pool for the few milliseconds the claim takes; the capacity count is then exact:

```sql
begin;
-- 1. One claimer per pool at a time; others wait here briefly (they do not skip).
select max_running, paused_until from pools where pool = $1 for update;
-- 2. With the pool locked, the running count cannot change under us.
with c as (
  select job_id from jobs
  where pool = $1 and state in ('available','scheduled','retryable','snoozed') and run_after <= now()
    and coalesce((select paused_until from pools where pool = $1), '-infinity') < now()
  order by priority, run_after, job_id
  for update skip locked
  limit greatest(0, least($2, (select max_running from pools where pool = $1)
                             - (select count(*) from jobs where pool = $1 and state = 'running')))
)
update jobs j set state = 'running', attempt = j.attempt + 1,
                  lease_until = now() + $3::interval, locked_by = $4
from c where j.job_id = c.job_id
returning j.*;
commit;
```

**Releasing a slot.** Capacity is counted from jobs in state `running`, so a slot frees itself the moment a job leaves that state: completion, failure, snooze or cancel each update the job's state in their own transaction. The lease reaper, which moves `running` jobs whose `lease_until` has passed back to `retryable`, takes the same pool lock first, so an expired job's slot and a new claim never race. No separate counter exists, so there is nothing to drift.

**State machine.**

```
 available/scheduled ──claim──▶ running ──ok──────────────▶ completed
        ▲    ▲                    │  ├──transient error────▶ retryable (run_after = backoff) ──┐
        │    └────────snooze──────┤  ├──rate limited───────▶ snoozed (run_after = Retry-After; attempt − 1, snoozes + 1)
        │                         │  ├──permanent error────▶ discarded (dead list, error_class kept)
        │                         │  └──attempts exhausted─▶ discarded
        │                         └──lease expired (rescuer)──▶ retryable
        └──────────────── Salvage re-enqueues discarded jobs ───────────┘
```

**Leases and heartbeats.** Leases default to 2 minutes for code jobs and 10 minutes for AI jobs; workers heartbeat every lease/3. A rescuer (run by whoever holds `pg_try_advisory_lock(42)`) moves expired `running` jobs to `retryable`. This is the visibility-timeout pattern of pgmq and apalis-postgres [9][11].

**Retry policy by error class.**

| Class | Examples | Policy |
| --- | --- | --- |
| `transient` | timeouts, 5xx, connection reset, renderer crash | Full-jitter backoff `sleep = random(0, min(cap, 30 s · 2^attempt))`, cap 1 h, max 6 attempts [8] |
| `rate_limited` | 429, `Retry-After`, DynamoDB throttle | Snooze to the given time (no attempt spent) [5] |
| `ai_usage_limit` | Claude plan limit reached | Pause pool `ai` (`paused_until`); run → `paused`; resume later |
| `ai_invalid_output` | schema or validator failure after repair | Discard for this run; re-plan next run (max 2 runs) |
| `check_failed` | Proof block | One AI repair if the failure is in model text; else discard with reason |
| `upstream_changed` | feed or schema drift | Discard; source health red |
| `invalid_data` | unparsable value, unknown unit | Discard; `needs_human` |
| `licence_blocked` | robots disallow, licence missing | Discard; never retried automatically |
| `bug` | panics, 4xx from our own API, invariant broken | Discard; event with stack; dev task |

**Unique keys** stop duplicate work: enqueueing a job whose `unique_key` is live is a no-op that returns the existing `job_id` (River's unique jobs [3]).

### 5.5 Preset runs (Chief)

```sql
create table runs (
  run_id uuid primary key, preset text not null, args jsonb not null,
  status text not null,     -- 'running' | 'waiting_approval' | 'waiting_ai' | 'paused' | 'succeeded' | 'failed' | 'cancelled'
  current_step text, config jsonb not null,      -- snapshot of env and scorer/prompt versions at start
  budget jsonb not null,                         -- {"ai_tokens": 2500000, "pages": 100}
  started_at timestamptz not null, finished_at timestamptz, summary jsonb
);
create table run_steps (
  run_id uuid references runs, seq smallint, step text not null,
  kind text not null,       -- 'code' | 'ai_batch' | 'approval'
  status text not null,     -- 'pending' | 'running' | 'done' | 'skipped' | 'failed'
  input_hash bytea, output jsonb, started_at timestamptz, finished_at timestamptz,
  primary key (run_id, seq)
);
create table approvals (
  run_id uuid, gate text, decision text not null,   -- 'approved' | 'rejected' | 'changes'
  note text, decided_at timestamptz not null, token_hash bytea not null,
  primary key (run_id, gate)
);
```

Each preset is declared in Rust as an ordered list of steps (code, AI batch or approval). `news run next <run>`:

1. Runs **code steps inline**: enqueues the step's jobs and drains them in-process with the pool limits (so no background worker is needed), records the step output, moves on.
2. At an **AI batch** step, enqueues the AI jobs and returns `{"stop":"ai_batch","command":"bun pipeline/run-ai.ts --run <id>"}`. The session runs that command; the runner claims `ai` jobs, calls `claude -p`, and posts outputs through `news ai accept`.
3. At an **approval** step, returns `{"stop":"approval","gate":"…","summary":{…}}`. Only `news run approve` (human-confirmed in the session) records the decision and a one-time token. Commands behind the gate (for example the publish step of a curation) refuse to run without a matching approval row, so the gate is enforced by code, not by the prompt.
4. Returns `{"stop":"done","summary":…}` at the end.

Every call is idempotent: calling `next` twice after a crash finishes the current step and carries on.

### 5.6 Scheduling

| What | Mechanism |
| --- | --- |
| Preset runs (AI) | Only when Karthik starts them (P8). |
| Background code jobs | `news work` long-running process, started by Windows Task Scheduler at log-on (optional). Runs pools `net`, `cpu`, `docling`, `publish` only. |
| Recurring jobs | `schedules` table (`kind`, cron expression in UTC, `next_at`), evaluated every minute by the Dispatch worker under an advisory lock: `calendar.sync` 06:00 UTC daily, source discovery every 30–120 min per source, `index.promote` daily, `work.prune` daily. |
| Calendar-driven jobs | Pre-warm and capture jobs created by `calendar.sync` with `run_after` set (§3.3). |
| Laptop asleep | Missed schedules run once at wake (no catch-up storms: each schedule keeps only its latest due run). |

### 5.7 Concurrency limits on a 4-core laptop

| Pool | `max_running` | Why |
| --- | --- | --- |
| `net` | 6 (plus per-host buckets) | Network-bound; cheap on CPU |
| `cpu` (parse, XBRL, embed, score, brief) | 2 | Leaves cores for Claude Code processes, Docker and the renderer |
| `docling` | 1 | Heavy; one at a time (Development Plan) |
| `render` | 2 | A warm Bun render server on localhost serves both slots, avoiding start-up cost per page |
| `ai` | `NEWS_PARALLEL` (3) | Plan limits and laptop memory; the laptop mostly waits on the network |
| `publish` | 1 | Paced by the write-API token bucket |

Heavy development jobs (Playwright matrices, release builds) should not overlap a run; the Edition workflow preflight warns if one is running.

### 5.8 Idempotency keys per step

| Step | Key | Effect of re-running |
| --- | --- | --- |
| fetch | URL + validators (ETag/Last-Modified) → snapshot SHA | 304 or same SHA: nothing new |
| parse | snapshot SHA + parser version | Same data points (dp_ids are content hashes) |
| detect | candidate id + detector version | Same story |
| score | story id + scorer version + hour | Same score row |
| brief | story id + hash of referenced data points + brief version | Same brief file |
| ai.write | brief hash + prompt version + model | Reuses the stored Copy unless `--force` |
| render | `.oui` hash + `data.json` hash + renderer version | Same bundle SHA |
| publish | page id + version + bundle SHA (sent as `Idempotency-Key`) | Write API returns the earlier result |

### 5.9 Recovery and replay

| Event | What happens |
| --- | --- |
| CLI or runner crash | Leases expire; rescuer makes jobs `retryable`; `news run resume <run>` continues |
| Laptop sleeps mid-run | Same as a crash; leases expire on wake |
| PostgreSQL restart | Transactions roll back; jobs stay in their last committed state |
| Plan usage limit | `-p` calls fail rather than wait [64]; runner marks pool `ai` paused and the run `paused`, returns `{"stop":"paused_usage","resume_hint":"…"}` |
| Bad parser shipped | Fix, then the Salvage workflow re-parses affected snapshots (`parse` is pure) and lets revisions flow through Rewrite |
| Bad prompt shipped | Audit catches it before promotion; if it slipped through, re-run `ai.write` for affected pages with `--force` under the new prompt version |

Replay modes: **re-validate** (run today's validators on stored AI outputs; no tokens), **re-render** (stored `.oui` + data; no tokens), **re-execute** (new AI call; tokens; needs approval when more than 20 items).

---

## 6. Agent tool catalogue

### 6.1 CLI or MCP?

| Criterion | `news` CLI (JSON in/out) | Local MCP server (stdio) |
| --- | --- | --- |
| Context cost | Zero until called; help text on demand | Tool search now defers MCP tool definitions, loading names only [67], so cost is smaller than it was |
| Least privilege | Claude Code permission rules per command pattern, e.g. `Bash(news brief *)` [66] | Per tool name (`mcp__server__tool`) [66] |
| Use from code | Same binary used by Bun, tests and Task Scheduler | Needs an MCP client |
| Testable without AI | Yes: golden JSON tests per command | Yes, but needs a harness |
| Page calls | Not needed at all (writer has no tools) | Not needed |
| Fits TD | Yes (TD "Agent toolbox") | New component |

**Recommendation (D3): CLI first, for everything.** Each command can print its input and output JSON Schemas (`news schema <command>`), so a thin read-only MCP adapter (`news mcp`, stdio) can be generated later if interactive sessions would benefit, without a second implementation. Page-writing calls use no tools at all; agents in preset sessions use only the commands their preset needs. Note that Claude Code permission rules match command text, so a deny rule is a guardrail, not a security boundary [66]; the real boundaries are the approval tokens and the secrets split (§9.3).

### 6.2 Conventions for every command

- **Envelope.** stdout carries exactly one JSON object; logs go to stderr and to telemetry.

  ```json
  {"ok": true, "data": {}, "warnings": [], "truncated": false}
  {"ok": false, "error": {"code": "not_found", "message": "story 81234 not found", "retryable": false, "hint": "news stories similar --text …"}}
  ```

- **Exit codes:** 0 ok · 2 usage · 3 not found · 4 conflict or precondition failed · 5 retryable (network, lock, busy) · 6 validation failed · 7 gated (approval required) · 10 internal bug.
- **Error codes** (stable): `usage`, `not_found`, `conflict`, `precondition`, `gated`, `validation_failed`, `rate_limited`, `upstream_unavailable`, `licence_blocked`, `budget_exceeded`, `internal`.
- **Size discipline.** Default output ≤ ~2k tokens; `--limit`, `--fields`, `--cursor` for more; `"truncated": true` when cut. Long text never returned inline unless asked for.
- **Determinism.** Same input and database state → same output (stable ordering), so commands have golden-output tests.
- **Every call** emits an event with `run_id` and `job_id` taken from `NEWS_RUN_ID` / `NEWS_JOB_ID` env vars.

### 6.3 Permission tiers

| Tier | Meaning | Who |
| --- | --- | --- |
| R | Read-only | Any agent in a preset session |
| W | Writes run-scoped state (drafts, AI outputs, job outcomes) | Preset session agent; Ghost |
| G | Gated: needs an approval token for the run | Code steps after an approval; never called directly by agents |
| H | Human only | Karthik in a terminal; denied to agents in `.claude/settings.json` |

### 6.4 Catalogue

Existing TD commands keep their names; new command groups carry working labels.

**Existing commands (TD), with shapes made concrete**

| Command | Input | Output (data) | Errors | Tier |
| --- | --- | --- | --- | --- |
| `news discover [--source s]` | — | `{new_candidates, by_source:{}}` | `upstream_unavailable` | W (code step) |
| `news plan --run <id>` | target pages from env | `{run_id, planned, by_tier:{}, by_bucket:{}, top:[{story_id, S, tier, reason}]}` (top 10 only) | `precondition` (nothing scored) | W |
| `news fetch <story>` / `news parse <story>` | — | `{status, snapshots, data_points}` | `rate_limited`, `invalid_data` | W |
| `news brief <story>` | — | the brief (§3.8) | `not_found`, `budget_exceeded` (too big after trimming) | R |
| `news components` | — | one line per component | — | R |
| `news component <type>` | — | props, data shape, example | `not_found` | R |
| `news related <story>` | — | ≤ 10 related pages | — | R |
| `news save <story> <file>` | Copy JSON | `{draft_id}` | `validation_failed` | W |
| `news render <story>` / `news check <story>` | — | paths / `{pass, failures}` | `validation_failed` | W |
| `news publish <story>` | — | `{url, version}` | `gated` outside a run step, `rate_limited` | G |
| `news update <page>` | — | `{page, changed_refs}` | `not_found` | R |
| `news prune` / `news stats` | — | counts / short table | — | W / R |
| `news home candidates` | — | ≤ 40 candidates (§4.5) | — | R |
| `news home save <file>` / `check <draft>` / `preview <draft>` | curation JSON | `{draft_id}` / `{pass, failures}` / `{url}` | `validation_failed` | W |
| `news home publish <draft>` / `rollback <v>` | — | `{version}` | `gated` without approval | G |
| `news home show` | — | live curation and age | — | R |

**Chief tools** (`news run …`, Chief)

| Command | Input | Output | Errors | Tier |
| --- | --- | --- | --- | --- |
| `news run start <preset> [--arg k=v]` | preset key, args | `{run_id, steps:[…], next}` | `conflict` (a run of this preset is already active) | W |
| `news run next <run>` | — | `{stop: "ai_batch" \| "approval" \| "paused_usage" \| "done" \| "failed", …}` | `precondition` | W |
| `news run status <run>` | — | step table, counts, budget used | `not_found` | R |
| `news run approve <run> <gate> [--note]` | — | `{approved, token_expires}` | `precondition` (gate not waiting) | H (Karthik confirms in session) |
| `news run reject <run> <gate> --note` | — | `{rejected}` | | H |
| `news run resume <run>` / `cancel <run>` | — | as `next` / `{cancelled}` | | W / H |
| `news run list [--active]` | — | recent runs | | R |

**Dispatch tools** (`news jobs …`, the queue)

| Command | Output | Tier |
| --- | --- | --- |
| `news jobs list [--state s] [--pool p] [--run r]` | counts and ≤ 20 jobs | R |
| `news jobs dead [--class c]` | discarded jobs grouped by `error_class` and message fingerprint | R |
| `news jobs claim --pool ai --limit n --worker w` | jobs with args and input file paths | W (Ghost only) |
| `news jobs heartbeat <job>` / `complete <job> --result f` / `fail <job> --class c --message m` | `{ok}` | W (Ghost only) |
| `news jobs retry <job \| --class c>` | `{requeued}` | W (in Salvage, ≤ 50 at once, else H) |
| `news jobs pause <pool> --until t` / `unpause <pool>` | `{paused_until}` | H |

**Ghost tools** (`news ai …`, contracts for every AI step)

| Command | Input | Output | Tier |
| --- | --- | --- | --- |
| `news ai input <job>` | — | path of the step's input file (brief, pairs, text) | W |
| `news ai accept <job> <output.json>` | model output | `{valid, errors:[{rule, where, message}], stored}` — runs schema + semantic validators | W |
| `news ai schema <step>` | — | JSON Schema of the step's output | R |
| `news ai fallback <job>` | — | applies the step's code fallback | W |

**Contacts tools** (`news sources …`): `list`, `show <source>`, `health [--since 7d]` (green/amber/red with reasons, 304 ratio, error classes, last success, quota), `probe <source>` (one live discovery call with no writes; R), `enable|disable <source>` (H).

**Diary tools** (`news calendar …`): `upcoming [--days 7] [--region r]` (R), `sync` (W, code step), `late` (R).

**Taster tools** (`news stories …`): `show <story>`, `members <story>`, `similar --text t | --cand c` (R); `merge <a> <b>`, `split <story> <cands…>` (H).

**Nose tools** (`news scores …`): `explain <story>` (the Pitch; R), `weights` (current config; R), `propose --from-labels` (fits new weights, prints a diff; W), `adopt <version>` (H).

**Roster tools** (`news entities …`): `resolve <text> [--kind k]` (R), `show <entity>` (R), `unresolved` (R), `alias add|merge` (H).

**Critic tools** (`news eval …`): `run <suite> [--prompt-version v]` (W, spends tokens only for AI suites, under the run budget), `report <eval_run>` (R), `label <set>` (interactive labelling helper; H), `promote <step> <prompt_version>` (G).

**Huddle tools** (`news review …`): `weekly` (a compact report: coverage vs targets, top rejected stories, failures, tokens per page, index budget, curation age; R).

**Errata tools** (`news corrections …`): `pull` (fetches new reader error reports from the cloud; W), `show <report>` (R), `resolve <report> --action update|dismiss` (G for update, W for dismiss).

**`news schema <command>`** prints input and output schemas for any command (R).

---

## 7. AI usage ledger

Every place a model is called. Each is a contract (P2): code builds the input, a JSON Schema bounds the output, a code validator decides acceptance, a fallback needs no AI, and a budget caps it. All calls go through Ghost with `--json-schema`, `--tools ""`, an empty strict MCP config and `--output-format json` (§3.9), except A8 and A11, which run in Karthik's interactive session.

Models: `NEWS_MODEL_MAIN` = Sonnet 5.5, `NEWS_MODEL_LIGHT` = Haiku 5.5, `NEWS_MODEL_CURATOR` = Sonnet 5.5. Opus 5.5 is never called at runtime (development only, as the TD says).

### 7.1 The ledger

| ID | Step | Model | Input (built by code) | Output schema (sketch) | Validation (code) | Fallback (no AI) | Budget | Volume per run *(speculative)* |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **A1** | Write page (new, or update after a direction flip) | Sonnet | Brief (§3.8); for updates also the old page and the data diff | `page-draft.v1` (§3.9) | Schema, OpenUI parse, refs, C4 numeric lint, C5 direction, banned words, props, lengths | Tier 2: templated page (D10); Tier 1: re-queue | ≤ 2 turns; ≤ 4k output; `--max-budget-usd 0.60` | ~100 |
| **A2** | Repair a draft | Sonnet | Previous draft + validator or check errors + brief | `page-draft.v1` | Same as A1 | Re-queue for next run | 1 call per item; ≤ 2 turns | ~5–15 |
| **A3** | Classify an ambiguous item (page type, categories, region) | Haiku | Title, first 300 words (fenced), source, rule candidates, allowed label sets | `{"page_type": enum, "categories": [enum ≤ 3], "region": enum, "confidence": "high"\|"medium"\|"low"}` | Enums from the taxonomy table; must include the rule's top candidate unless `confidence = high` | Rule's best guess, flagged `page_type_by = rule-unsure` | Batched 20 items/call; 1 turn | 0–3 calls |
| **A4** | Same story? (grey zone) | Haiku | Up to 20 pairs: titles, first 150 words, shared entities, dates | `{"pairs": [{"a": id, "b": id, "same": bool, "basis": enum["same_event","same_entity_different_event","different"]}]}` | IDs must match input; one answer per pair | Keep separate | Batched; 1 turn | 0–5 calls |
| **A5** | Condense long source text for a brief | Haiku | Text blocks with IDs (fenced, untrusted) + page type + what the brief needs | `{"points": [{"text": str ≤ 200, "support": [block_id]}], "quotes": [{"block": id, "start": int, "end": int}]}` | No digits in `text`; every `support` and quote offset exists; quotes are spans of real blocks; ≤ 12 points | Code extract: first paragraph + headings + sentences with entity mentions | 1 turn; ≤ 1.5k output | ~10–20 |
| **A6** | Pick an entity from a shortlist (optional, off by default) | Haiku | Mention + context sentence + ≤ 5 candidate entities from code | `{"choice": entity_id \| "none"}` | Choice ∈ shortlist ∪ {none} | Leave unresolved | Batched 30/call | 0–2 calls |
| **A7** | Sample spot check | Haiku | Page Markdown + brief | `{"verdict": "pass"\|"flag", "issues": [{"kind": enum["unsupported_claim","wrong_direction","missing_context","tone","attribution","other"], "where": str ≤ 120, "evidence": [ref or block_id], "severity": "low"\|"high"}]}` | Kinds from enum; `where` must be a substring of the page; evidence IDs exist | Skip sample (logged) | 1 turn | ~5 (+ all Tier 1 in month 1) |
| **A8** | Home curation (Frontpage) | Sonnet (curator) | ≤ 40 candidates from `news home candidates` | TD curation JSON (lead, sides, sections, figure IDs, rationale) | `news home check` (TD rules) | Keep the live curation | ≤ 2 fix rounds | Every 2–3 days |
| **A9** | Triage a reader error report | Haiku | Report text (fenced, untrusted) + page refs and labels | `{"kind": enum["figure","wording","link","image","other","spam"], "ref": ref\|null, "claim": str ≤ 200}` | `ref` must belong to the page | Manual triage in the workflow | Batched 20/call | Weekly |
| **A10** | Alt text when the image source gives none (optional) | Haiku | Image caption, title, credit, page context (text only; no image download) | `{"alt": str 20–150}` | No digits unless in the caption; length | Neutral alt from caption or title | Batched | Rare |
| **A11** | Preset session (the interactive orchestrator) | Session model (Sonnet recommended) | Compact JSON from `news run next` | Free text to Karthik; tool calls limited to the preset's commands | Approval tokens enforced by code | Karthik runs the CLI by hand | Small: no page content ever enters it | Per preset |
| **A12** | Eval judge (only in the Audit workflow) | Sonnet judging Haiku steps; Haiku + Sonnet cross-check for A1 prose | Output + reference + one rubric question | `{"pass": bool, "reason_code": enum}` (one judge per failure mode) | Judges themselves validated against Karthik's labels (TPR/TNR) [61] | Deterministic metrics only | Per eval budget | Weekly or per prompt change |

**Removed from the TD's model roles:** "Sonnet: selection judgement where code scoring is unsure". Selection is fully code (§4); disagreements are handled by Karthik's weekly pairwise labels, not by a per-run model call. **Narrowed:** Haiku "summarising long parsed text" becomes A5, extractive and digit-free with verifiable quote spans.

### 7.2 Token budget per run *(speculative; replaced by telemetry after the first runs)*

| Step | Calls | Input per call (mostly cached prefix) | Output per call | Total input | Total output |
| --- | --- | --- | --- | --- | --- |
| A1 | 100 | ~11k (8k cached + 3k brief) | ~3k | ~1.1M (≈ 0.8M cache reads) | ~300k |
| A2 | 10 | ~14k | ~3k | ~140k | ~30k |
| A3–A6 | ~25 | ~4k | ~1k | ~100k | ~25k |
| A7 | 5 | ~8k | ~0.5k | ~40k | ~3k |
| A11 | 1 session | ~30k | ~5k | ~30k | ~5k |

`NEWS_AI_RUN_TOKENS` (new; default 2.5M input-equivalent) stops new AI jobs when reached; the run finishes code work and reports what was left.

### 7.3 Deliberately not done with AI

| Task | Done by | Why not AI |
| --- | --- | --- |
| Finding new items, fetching, change detection | Connectors, Runner, calendars | Deterministic, cheap, auditable |
| Parsing XBRL, JSON, SDMX, CSV, PDF tables | Rust parsers, Docling, Tesseract | Numbers must be exact; models misread tables |
| Units, currencies, scales, fiscal periods | Stored verbatim; formatted by shared code | PRD: never convert; never guess |
| Derived figures (changes, shares) | `derived_points` with guards | Arithmetic must be exact and reproducible |
| "Record high", "lowest since", "first cut since" | Code context rules (§3.8) | Facts about a series are computable |
| Entity resolution | IDs, aliases, gazetteer, trigram (A6 only for leftovers, off by default) | Identifiers exist for almost everything |
| De-duplication and story identity | Hashes, SimHash, event keys, embeddings | Embeddings are a local model, not a generative call |
| Category, region, page type for structured sources | Rules table | Fully determined by source and concept |
| Scoring, ranking, selection, tiering | Formulas (§4) | Must be explainable and tunable |
| Index budget | Formula | Domain-reputation policy, not taste |
| Choosing images | Allow-list and licence metadata (PRD) | Licensing is a rule |
| Headline numbers | Refs filled by the renderer | P3 |
| Quotes | Verbatim spans checked by code | Misquotes are unacceptable |
| Glossary links, sources panel, citations, Markdown/JSON versions | Renderer | Mechanical |
| Checks C1–C14 | Code | Zero tokens on every page |
| Scheduling, retries, approvals | Dispatch, Chief | Durable state belongs in PostgreSQL |
| Weekly statistics | SQL | A model only presents what code computed (in A11) |

---

## 8. Preset workflow catalogue

### 8.1 How every preset works

**Entry.** There is exactly one user-invoked project skill, `.claude/skills/bureau/SKILL.md`, with `disable-model-invocation: true`, so Karthik starts everything with `/bureau` and it never loads into other sessions [65]. There are no per-preset skills or commands: presets are rows in Chief (code), and `/bureau curate home`, `/bureau run daily` and so on are arguments to the one skill. The skill is short: with no arguments it prints Chief's status board (`news run status`); otherwise it maps `<verb> <object>` (or plain English, confirmed before anything that writes) to a preset id, starts the run, loops on `next`, runs Ghost when told, asks Karthik at approval stops and prints the Wrap. Preset-specific guidance sits in `.claude/skills/bureau/references/<preset>.md` and is read only when that preset runs. All logic lives in the `news` CLI (Chief), so every preset is testable without AI (D2).

```
/bureau run daily target=120
  └─ news run start daily --arg target=120          → {"run_id":"r_7f3c", "next":"preflight"}
  └─ loop:
       news run next r_7f3c
         → {"stop":"ai_batch","step":"write","jobs":112,"command":"bun pipeline/run-ai.ts --run r_7f3c"}
       bun pipeline/run-ai.ts --run r_7f3c                   → one line per 10 jobs: "write 40/112 ok=38 repair=2"
       news run next r_7f3c
         → {"stop":"done","summary":{"published":104,"tier1":17,"failed":5,"queued_next":3,"tokens_in":1210000,...}}
```

**Why not `.claude/workflows/` for presets.** Claude Code's dynamic workflows are JavaScript modules run by name, good at fan-out, but they cannot take user input mid-run (each approval gate would need a separate run), `-p`-launched runs do not pause for usage limits, and their agents get a 5-minute cache TTL [64][68]. A skill-driven session over a CLI state machine gives one command, real approval stops in the same session, and durable state in PostgreSQL. A workflow file can still be used *inside* a step later (for example to fan out curator evaluations), started by the session (D2).

**Session permissions** (`.claude/settings.json`, project): allow `Bash(news *)` read and run-scoped commands and `Bash(bun pipeline/* --run *)`; deny `Bash(news publish *)`, `Bash(news home publish *)`, `Bash(news home rollback *)`, `Bash(news run approve *)` and H-tier commands for agents, so Karthik types those or confirms them at the prompt. (Command-text rules are guardrails; approval tokens are the enforcement.)

**Common rules.** Only one active run per preset (`conflict` otherwise). Every preset starts with a preflight: services up (`bun run doctor`), no heavy dev job running, Sitrep not red for the sources it needs, AI budget headroom. Every preset ends with a Wrap written to `runs.summary` and ClickHouse.

### 8.2 Edition

| | |
| --- | --- |
| **Trigger** | `/bureau run daily [target=N] [categories=…] [review_plan=yes]`, 2–3 times a day |
| **Purpose** | The main production run (PRD "How a run works") |

| # | Step | Kind | Output |
| --- | --- | --- | --- |
| 1 | Preflight | Code | Pass or blockers |
| 2 | Discover all due sources; capture any calendar events now due | Code | New candidates |
| 3 | Normalise, resolve entities, detect stories | Code (+ A3, A4 batches if needed) | Stories |
| 4 | Score, select, tier (Budget) | Code | Plan: counts by bucket and tier, top 10 with Pitchs |
| 5 | *Optional approval* of the plan (only if `review_plan=yes`) | Approval | — |
| 6 | Fetch, parse, brief for planned stories | Code (+ A5 for long texts) | Briefs |
| 7 | Write pages | AI batch (A1) | Copys |
| 8 | Render, check; repair failures once | Code + AI batch (A2) | Ready pages |
| 9 | Sample spot check | AI batch (A7) | Flags |
| 10 | Publish via outbox; apply revisions due (value-only re-renders) | Code | URLs |
| 11 | Index promotion; prune working files | Code | — |
| 12 | Summary | Code | Wrap |

**Approval stops.** None by default: Karthik's prompt is the approval (PRD). `review_plan=yes` adds one stop after step 4.
**Failure handling.** Per-item failures never stop the run; they go to the dead list with a class. The run stops early only for: preflight blockers, `ai_usage_limit` (paused), or token budget reached (finishes code work, publishes what passed).
**Resumability.** `/bureau run daily resume` or `news run resume <run>`; finished steps and items are skipped by their keys.

### 8.3 Flash

| | |
| --- | --- |
| **Trigger** | `/bureau run quick <event_key or story_id …>` (e.g. right after an FOMC decision) |
| **Steps** | Capture now → normalise → brief → write (A1) → render, check, repair → publish. Same as the daily run for 1–10 named items, skipping discovery and selection; tier from the score. |
| **Approval** | None |
| **Failure / resume** | As the daily run |

### 8.4 Catchup

| | |
| --- | --- |
| **Trigger** | `/bureau run backfill from=2026-01-01 [to=…] [page_types=…] [sources=…] [chunk=50]` (`BACKFILL_FROM` by default; current year only, per PRD) |
| **Steps** | 1 Enumerate historical events from sources and calendars (code) → 2 Score with F ignored, select (code) → 3 **Approval**: shows item count, chunk plan, estimated tokens and hours → 4 For each chunk: fetch, parse, brief, write, render, check, publish (same as daily steps 6–10) → 5 Summary |
| **Approval** | One, at step 3 |
| **Failure** | Chunk-level progress; per-item failures to the dead list; stops cleanly at the token budget |
| **Resume** | Re-run the command with `resume`; the next unfinished chunk continues. Backfill jobs use priority 4 so a daily run can interleave |

### 8.5 Frontpage (home curation; was the TD's `curate-home`)

| | |
| --- | --- |
| **Trigger** | `/bureau curate home`, every 2–3 days (TD "Home page curation") |
| **Steps** | 1 `news home candidates` (code, §4.5) → 2 Curator (A8, `news-home-curation` skill) → 3 `news home save` + `check` (code; up to two fix rounds with the curator) → 4 `news home preview` (code; light and dark, phone and desktop) → 5 **Approval** → 6 `news home publish` (gated) → 7 Summary |
| **Approval** | Step 5: approve, request changes (back to step 2 with Karthik's note), or reject |
| **Failure** | Check failures after two rounds → stop and show errors; the live curation stays |
| **Resume** | Draft and check results are stored; `resume` returns to the approval stop |
| **Rollback** | `news home rollback <version>` (H) |

### 8.6 Rollcall

| | |
| --- | --- |
| **Trigger** | `/bureau check sources [source=…]`, weekly or when the dashboard shows red |
| **Steps** | 1 `news sources health` (code: last success, error classes, 304 ratio, quota, robots changes, calendar `late` events, parse-failure rate vs 30-day baseline) → 2 `probe` each red or amber source (code, no writes) → 3 Session summarises and proposes actions: disable, retry later, or a development task with the failing fixture attached → 4 **Approval** for disabling a source or for drafting development tasks |
| **AI** | Only the session's summary (A11). Connector fixes are development work (Sonnet builder, Opus review), not runtime |
| **Output** | Sitrep in ClickHouse and the session |

### 8.7 Salvage

| | |
| --- | --- |
| **Trigger** | `/bureau retry [class=…] [since=…]` |
| **Steps** | 1 `news jobs dead` grouped by class and fingerprint (code) → 2 Auto-retry `transient` and `rate_limited` groups (code) → 3 For `ai_invalid_output` and `check_failed`: re-validate stored outputs with current validators (no tokens) → 4 **Approval** to re-execute AI for the rest (shows count and estimated tokens) → 5 AI batch (A1/A2) → 6 Render, check, publish → 7 Summary of anything still failing, with suggested owner (parser bug, prompt, source) |
| **Approval** | Step 4, and any requeue of more than 50 jobs |
| **Resume** | As any run |

### 8.8 Followup

| | |
| --- | --- |
| **Trigger** | Runs as step 10 of every daily run; standalone `/bureau apply revisions [source=…]` after a big revision day (e.g. annual GDP benchmark) |
| **Steps** | 1 Find superseded data points referenced by pages (code) → 2 Classify (value-only, direction flip, restatement) → 3 Re-render value-only pages (code) → 4 AI update for direction flips (A1 update mode) → 5 Check, publish with "Updated on …" |
| **Approval** | Only if more than 100 pages would change at once |

### 8.9 Errata

| | |
| --- | --- |
| **Trigger** | `/bureau review corrections`, weekly or when the dashboard shows new reports |
| **Steps** | 1 `news corrections pull` (code) → 2 Triage (A9) → 3 For `figure` reports: code re-fetches the source and compares the value at the cited locator → 4 **Approval** per report with code's evidence (confirmed / not reproduced) → 5 Confirmed: fix (parser fix as a dev task, or re-render, or A1 update) → 6 Publish with "Updated on …"; resolve the report |
| **Output** | Corrections log (feeds the methodology page's corrections count, if Karthik wants one) |

### 8.10 Audit

| | |
| --- | --- |
| **Trigger** | `/bureau run evals suite=<writer\|classify\|storymatch\|spotcheck\|curator\|all> [prompt_version=…]`; required before promoting any prompt, skill or model change |
| **Steps** | 1 Load golden set (§9.1) → 2 Deterministic metrics on stored or fresh outputs (code) → 3 AI calls for the candidate version (A1–A9 as relevant) → 4 Judges on a sample (A12) → 5 Comparison report: candidate vs live, per metric, with regressions highlighted → 6 **Approval** to promote → 7 `news eval promote` (gated) |
| **Budget** | Its own token budget, shown before step 3 |

### 8.11 Huddle

| | |
| --- | --- |
| **Trigger** | `/bureau review week`, weekly |
| **Steps** | 1 `news review weekly` (code: coverage vs targets, pages by tier/type/region, failures by class, tokens per page, cache-read ratio, index budget use and Search Console status entered by Karthik, curation age, unresolved entities) → 2 Ranking calibration: 30 pairwise questions to Karthik in the session → 3 `news scores propose --from-labels` (code fit, §4.7) → 4 Session presents the report and proposed changes (weights, coverage targets, `NEWS_INDEX_PER_DAY` step, `SAMPLE_RATE`) → 5 **Approval** per change → 6 Apply (config rows, new versions) |
| **Output** | Debrief; versioned config changes |

### 8.12 Reprint

| | |
| --- | --- |
| **Trigger** | `/bureau rerender pages renderer=<version> [page_types=…] [limit=…]` after component improvements (TD "Renderer versions") |
| **Steps** | 1 Select pages on older renderer versions (code) → 2 Re-render from stored `.oui` and data (code, no tokens) → 3 Checks → 4 **Approval** with a sample of 10 before/after previews → 5 Paced publish (code) |

### 8.13 Background jobs (code only, not presets)

| Job | Schedule | Notes |
| --- | --- | --- |
| `calendar.sync` | Daily 06:00 UTC | Creates pre-warm and capture jobs |
| `calendar.capture` | At release times | Fetch, parse, normalise, detect, score; no AI. Captured items wait for the next daily or quick run |
| `source.discover` | Every 30–120 min per source | Feeds candidates |
| `index.promote` | Daily | Index budget |
| `work.prune` | Daily | Working files for published items; snapshot retention |
| Analytics pull | Daily | Cloudflare Web Analytics into `traffic_daily` for later ranking (§4.8) |

Run by the Dispatch worker if Karthik enables it (D8); otherwise the same jobs run inline at the start of each preset.

---

## 9. Quality, observability, security, performance

### 9.1 Quality and evaluation

**Layers, cheapest first** (deterministic checks before any model judge, as Anthropic's and Hamel Husain's guidance both recommend [60][61]):

| Layer | What | AI? | Where it runs |
| --- | --- | --- | --- |
| Unit and property tests | Parsers, guards, formulas, validators, claim SQL, state machines | No | CI |
| Golden CLI outputs | Every `news` command on a fixture database | No | CI |
| Validator contract tests | Hand-made good and bad AI outputs per step | No | CI |
| Code checks on every page | C1–C14 | No | Every run |
| Golden sets with AI | G1–G6 below | Yes | Audit |
| Seeded faults | Pages with known injected errors, to measure the checkers | Partly | Audit |
| Production sampling | A7 on 5 %; Karthik reads 5 random pages a week *(recommended)* | Partly | Runs, Huddle |

**Golden sets** (stored under `pipeline/eval/`, versioned; labels by Karthik with Opus help in Phase 4):

| Set | Contents | Key metrics and starting targets *(speculative)* |
| --- | --- | --- |
| G1 Writer | 35 frozen briefs (5 per page type), including adversarial cases: injection text in the source, missing data, restatement, non-calendar fiscal year, a long statement, negative values | Valid on first try ≥ 90 %, after repair ≥ 98 %; C4/C5 violations after repair 0; tokens per page; judge pass rates per failure mode |
| G2 Classification | 200 ambiguous items | Page type accuracy ≥ 0.95; category F1 |
| G3 Story pairs | 200 same/different pairs | "Join" precision ≥ 0.97; recall reported |
| G4 Ranking pairs | Karthik's weekly pairwise labels | Kendall's τ trend |
| G5 Curator | Development Plan fixtures (busy week, quiet week, one region dominating, withdrawn lead) | `news home check` pass rate; breadth |
| G6 Seeded faults | 50 pages with one injected fault each (wrong direction word, unsupported claim, misattributed quote, missing context) | A7 recall ≥ 0.7; false flags ≤ 10 % |
| G7 Entities | 300 labelled mentions | Precision ≥ 0.99 |

**Judges (A12).** One binary judge per failure mode, never a 1–10 score; order randomised and a reference given, because LLM judges show position, verbosity and self-preference biases [59]; a different model from the generator where possible [60]; each judge checked against ~100 of Karthik's labels for true-positive and true-negative rates before it is trusted [61].

**Prompt and model changes** go through the Audit workflow and are promoted only by approval; each AI output stores `prompt_version` and model, so regressions are traceable.

### 9.2 Observability

**Events.** Keep the existing `news.events` table, and add columns for fields that are filtered and grouped on (they are currently only reachable in `attrs`):

```sql
ALTER TABLE news.events
  ADD COLUMN job_id      String DEFAULT '',
  ADD COLUMN attempt     UInt8 DEFAULT 0,
  ADD COLUMN preset      LowCardinality(String) DEFAULT '',
  ADD COLUMN step        LowCardinality(String) DEFAULT '',
  ADD COLUMN source_id   LowCardinality(String) DEFAULT '',
  ADD COLUMN error_class LowCardinality(String) DEFAULT '',
  ADD COLUMN prompt_version LowCardinality(String) DEFAULT '',
  ADD COLUMN trace_id    String DEFAULT '',
  ADD COLUMN span_id     String DEFAULT '';
```

**New ClickHouse tables** (MergeTree, partitioned by month, ordered for their main query):

| Table | One row per | Key columns |
| --- | --- | --- |
| `ai_calls` | `claude -p` call | run, job, step (A1…), model, prompt_version, tokens in/out, cache read/write (5 m and 1 h), turns, duration, subtype, validation result, repair round, est. cost |
| `fetches` | HTTP request | source, host, status, 304 flag, bytes, duration, retry-after |
| `scores` | score computation | story, scorer version, components, total, selected, tier |
| `check_results` | check per page | page, check id, pass, message |
| `runs` (ReplacingMergeTree) | run | preset, status, counts, budget used, duration |

Daily materialised views roll these up for the dashboard.

**Traces.** Rust uses `tracing` with an OpenTelemetry OTLP exporter to the existing collector, which already writes `otel_traces` to ClickHouse. One trace per run; spans per step and job. Ghost sets `OTEL_RESOURCE_ATTRIBUTES=news.run_id=…,news.job_id=…,news.step=…` on each `claude -p`, so Claude Code's own `claude_code.api_request` events and `claude_code.token.usage` metrics [69] join to our jobs.

**Dashboard panels for this system** (adds to the TD list):

| Area | Panels |
| --- | --- |
| Runs | Run timeline (steps as bars), status, pages per hour, time per stage |
| Queue | Jobs by pool and state; dead list by error class; oldest waiting job |
| Ingestion | Source health matrix (green/amber/red, last success, 304 ratio, errors, quota); release → capture → publish latency for calendar events; `late` events |
| Stories and ranking | Score distribution by page type; selected vs rejected; coverage vs targets (category, region, page type) over 7 and 30 days |
| AI | Tokens per page by type and tier; cache-read ratio; validation failures by rule; repair rate; calls per step A1–A12; run budget used; usage-limit pauses |
| Quality | Check failures by check; A7 flag rate; corrections; revision updates |
| Publishing | Index status counts; promotions per day; write-API retries and throttles |
| Laptop | CPU and memory during runs (OTel host-metrics receiver), disk used by `work/raw` |

### 9.3 Security and safety

**Prompt injection from source text.** Press releases, papers, news pages and reader reports are untrusted. Anthropic's guidance is to keep untrusted content in clearly marked data, tell the model it is data, constrain outputs, and apply least privilege [71]; Claude Code's docs warn against piping untrusted content straight into an agent with tools [71]. Defences here:

1. **No tools in content calls.** A1–A10 run with `--tools ""` and an empty strict MCP config, and no project hooks or plugins load. Injected text has nothing to act with.
2. **Fenced data.** Source text sits in JSON fields marked `"untrusted": true`; the prefix says such fields are material to describe, never instructions.
3. **Bounded outputs.** JSON Schema, enums and validators: numbers cannot appear (C4), links must come from the brief's source list, quotes must be verbatim (C6), refs must exist (C2).
4. **Consequences live in code.** No AI output can publish, approve, delete or fetch; only code steps can, and the risky ones are gated.
5. **Tested.** G1 includes injection cases; A7 flags off-topic or promotional text.

**Secrets.**

- `claude -p` children get an allow-listed environment (`PATH`, profile and app-data folders, OTel variables, `NEWS_RUN_ID`, `NEWS_JOB_ID`); never `ANTHROPIC_API_KEY`, AWS or Cloudflare keys or `NEWS_WRITE_API_KEY`.
- Only `news publish` and `news home publish` read the write-API key, and only inside a gated code step.
- Keys stay in the local `.env` (CLAUDE.md rule); agents never edit it.

**Least privilege.** Tiers R/W/G/H (§6.3); approval tokens checked in code; deny rules in the project settings for H-tier commands.

**Downloaded files.** Treated as untrusted: parsed by memory-safe Rust with size limits and no `unwrap` on external data; Docling runs in a container with `--network none` and a read-only mount *(proposed)*; nothing downloaded is executed or opened by Claude.

**Licensing.** Enforced by C8 and the registry's `redistribution` flag.

**Personal data.** Reader error reports may include an optional e-mail; the Errata workflow stores only the report text and page locally, not the e-mail.

**Terms of use.** The pipeline runs the unmodified `claude` binary under Karthik's own subscription login. Anthropic's docs restrict offering claude.ai login inside third-party products built on the Agent SDK and say developers building products should use API keys [70]. A personal pipeline using the stock CLI fits the end-user case better than an SDK-based tool, which is one more reason to stay on `claude -p`. *(My reading, not legal advice; Karthik may want to confirm with Anthropic.)*

### 9.4 Performance budget on the laptop *(speculative; measured in Phases 5 and 7)*

| Stage | Per item | Pool | Notes |
| --- | --- | --- | --- |
| Discover | — | net | 2–5 min per run across sources, mostly waiting |
| Fetch | 1–10 s | net (6) | Conditional GET makes repeats cheap |
| Parse (JSON/XBRL/SDMX) | 50–500 ms | cpu (2) | |
| Parse (Docling PDF table) | 30–120 s | docling (1) | Rare; only hard tables |
| Embed (unstructured only) | 20–50 ms | cpu | |
| Score, select | < 5 s per run | cpu | |
| Brief | 50–200 ms | cpu | |
| Write (A1) | 60–150 s | ai (3) | Network-bound; dominates the run |
| Render | 0.3–1 s | render (2) | Warm Bun server |
| Checks | 1–3 s (+ 2–4 s when axe runs) | cpu/render | Link checks cached 24 h |
| Publish | 0.5–2 s | publish (1) | Paced |

**A 100-page run:** about 60–80 minutes, of which ~50 minutes is the AI batch (100 × ~90 s ÷ 3); code steps overlap with AI waits.

**Memory:** PostgreSQL 2–3 GB (`shared_buffers` 2 GB), ClickHouse capped by its settings profile, OTel collector ~100 MB, three `claude` processes, the Bun render server ~0.5 GB, the embedding model ~150 MB. Comfortable within 24 GB.

**Disk:** snapshot retention as §3.2; PostgreSQL grows roughly with data points (a few GB a year).

---

## 10. Build order

Mapped onto the Development Plan. Agent choices follow its rules (Opus for judgement and correctness-critical code, Sonnet for well-specified volume work).

| Phase | Work from this document | Agent | Exit evidence |
| --- | --- | --- | --- |
| 4 Content system design | ADRs from this document: Dispatch and Chief (D1, D2), tool surface (D3), data point and provenance model, scoring v1 and selection, story detection, AI step contracts and writer call (D4), derived figures (D5) | Opus architect | ADRs merged |
| 4 | Embedding spike: bge-small vs nomic vs EmbeddingGemma vs MiniLM on the i5-8250U (docs/s, memory, G3 precision) | Opus spike | Numbers and choice in an ADR |
| 4 | Golden-set labelling: G2, G3, G7 (Karthik with Opus); event weights, prominence lists, coverage targets as seed data | Opus + Karthik | Sets in `pipeline/eval/` |
| 5 Rust CLI | 1 Core crates, migrations, JSON envelope, error codes, telemetry | Opus feature builder (one agent) | Golden CLI tests |
| 5 | 2 Dispatch + Chief (claim, leases, rescuer, retry classes, runs, approvals) | Opus (correctness-critical) | Concurrency and crash tests |
| 5 | 3 Runner + Morgue + `fetch_state` | Sonnet; Opus review | Fake-server tests |
| 5 | 4 Connectors in parallel: SEC EDGAR, FRED, BLS, ECB, arXiv, central-bank statements, then IMF, World Bank, Eurostat, ONS, StatCan | Sonnet ×3; Opus review for XBRL/units/periods | Fixture tests |
| 5 | 5 Clerk, data points, derived points with guards | Opus | Property tests |
| 5 | 6 Diary | Sonnet | ICS/API fixture tests |
| 5 | 7 Roster's resolver and seeds | Sonnet; Opus review | G7 ≥ 0.99 precision |
| 5 | 8 Taster and embeddings | Sonnet after the spike | G3 precision |
| 5 | 9 Nose, Desk, Nose tools | Opus | Worked-example test reproduces §4.6 |
| 5 | 10 Fixer | Opus | Size and fencing tests |
| 5 | 11 Proof C1–C14, Ghost tools validators | Opus | Contract tests |
| 5 | 12 Press outbox and index budget (dry-run) | Sonnet; Opus security review | Idempotency tests |
| 6 Cloud | Write API honours `Idempotency-Key`; corrections endpoint for `news corrections pull` | Sonnet; Opus security review | Replay returns the first result |
| 7 Orchestration | Lean-call spike: confirm flags (§3.9), cache TTL and hit ratio, usage-limit error shape | Opus spike | Findings in an ADR |
| 7 | Ghost (Bun): claim, spawn, env allow-list, validation, heartbeats, pause on limits | Sonnet; Opus review | Fake-`claude` tests |
| 7 | Runtime skills and prefixes: A1/A2 prompts, A3–A7, A9 skills | Opus (judgement and prose) | G1, G2, G6 pass |
| 7 | Preset skills: Edition, Flash, Frontpage, Salvage, Audit | Opus (orchestration), Sonnet glue | Karthik runs each once |
| 7 | First 10-page run, then 100 | Orchestrator with Karthik | 100 pages pass checks |
| 8 Telemetry | ClickHouse columns and tables, traces, dashboard panels (§9.2) | Sonnet; Opus review | Panels show a real run |
| 9 Launch and ramp | Huddle, calibration, Rollcall, Errata, Catchup, Followup, Reprint | Sonnet builders, Opus review | First weekly review completed |

---

## 11. Open decisions for Karthik

| # | Decision | Options | Recommendation |
| --- | --- | --- | --- |
| D1 | Orchestration engine | (a) custom Rust queue on PostgreSQL with River-style semantics; (b) apalis-postgres; (c) DBOS TypeScript in Bun; (d) Restate or Temporal | **(a)**, with (b) as the fallback if it takes more than a week of agent time. No new servers; one owner of the rules |
| D2 | Form of preset workflows | (a) user-invoked skills (`disable-model-invocation`) driving a CLI state machine; (b) `.claude/workflows/` files | **(a)**. Workflows can't take input mid-run and get short cache TTLs; keep them for fan-out inside a step if needed |
| D3 | Agent tool surface | (a) `news` CLI only, schemas exportable, MCP adapter later if wanted; (b) local MCP server now | **(a)** |
| D4 | Writer call shape | (a) no tools; code pre-selects component specs into the cached prefix; (b) writer calls `news component` on demand | **(a)**: fewer turns, warmer cache, nothing for injected text to use; repair round supplies any missing spec |
| D5 | Derived figures | (a) code computes changes and shares with guards, labelled "Calculated by News from …"; (b) only published figures | **(a)**. Headlines like "rose 39 % from a year earlier" need them; guards keep the no-conversion rule |
| D6 | Embedding model | bge-small-en-v1.5, nomic v1.5, EmbeddingGemma (gated licence), MiniLM | **bge-small-en-v1.5**, confirmed by the Phase 4 spike |
| D7 | Raw snapshot retention | (a) prune after publish (TD); (b) structured data 400 days, documents 90 days, hashes for ever | **(b)**, so every figure stays provable |
| D8 | Background code-only worker | (a) Dispatch worker via Task Scheduler for discovery, calendar capture, index promotion; (b) everything only inside presets | **(a)**: releases are captured on time with no AI and no tokens |
| D9 | Earnings dates | (a) derived windows from filing history + SEC 8-K polling; (b) unofficial feeds | **(a)**; no reliable free official source exists |
| D10 | Templated fallback pages | (a) publish Tier 2 code-only pages when the writer fails twice, kept `noindex` until upgraded; (b) re-queue only | **(a)**, flagged and never indexed until a writer upgrades them |
| D11 | Model for the preset session | Sonnet or Opus | **Sonnet**: the session only reads compact JSON and asks questions |
| D12 | Optional AI steps A6 (entity shortlist) and A10 (alt text) | On or off at launch | **Off**; switch on if the unresolved queue or missing alt text proves it necessary |
| D13 | Plan review stop in the daily run | On or off by default | **Off**; `review_plan=yes` when wanted |
| D14 | SEC contact e-mail in the User-Agent | Personal address or a dedicated one | **A dedicated address** in `.env` (SEC requires a contact [18]) |
| D15 | Human reading sample | None, or 5 random pages a week | **5 a week** in the Huddle workflow; it calibrates A7 |

---

## 12. Risks

| Risk | Effect | Mitigation |
| --- | --- | --- |
| Lean-call flags behave differently from the docs (settings sources, hooks) | Page calls load project context or tools | Phase 7 spike before building the runner; env and config isolation as a second layer |
| Plan usage limits hit mid-run | Partial runs | Run token budget, pause and resume, back-to-back cached prefixes |
| Hand-set weights pick a narrow mix | Repetitive coverage | Bucket diversity, coverage gap, weekly pairwise calibration |
| Grey-zone thresholds wrong | Merged or split stories | G3 calibration; keep-separate fallback; update detection |
| Source APIs change (ONS and IMF are mid-migration [26][29]) | Gaps | Source health workflow; hash-based change detection; fixtures |
| Numeric lint too strict | Good drafts rejected | Allow-list maintained from G1 failures; repair round |
| Engine bugs (custom code) | Stuck or duplicated jobs | Unique keys, crash tests, idempotent steps; apalis fallback |
| Disk growth from snapshots | Disk pressure | Retention policy; dashboard panel |

---

## 13. Glossary of terms used

| Term | Meaning here |
| --- | --- |
| Event key | A deterministic identity for a structured story, such as `macro:us.cpi:2026-10` |
| Story | A group of candidates that describe the same event |
| Brief | The compact JSON research pack given to the writer (TD term) |
| Ref | An ID (`dp_`, `dv_`, `sr_`, `q_`, `cx_`) the model writes instead of a number, series, quote or context line |
| Vintage | The time at which a published value was valid (a release or filing date) |
| Locator | Where inside a stored snapshot a value was read |
| Lease | A time-limited claim on a job; expires if the worker dies |
| Snooze | Postpone a job without counting a failed attempt |
| Gate | An approval stop whose decision is recorded and checked by code |

---

## 14. Changes to existing documents

Proposed only; nothing has been edited.

| Document | Section | Now | Proposed change | Why |
| --- | --- | --- | --- | --- |
| TD | Keeping each `claude -p` call lean | "Minimal tools via `--allowedTools`" | Use `--tools ""` (or a short `--tools` list) to restrict; `--allowedTools` only pre-approves [62] | Correctness |
| TD | Keeping each `claude -p` call lean | — | Add: `--json-schema`, `--max-budget-usd`, `--no-session-persistence`, `--disable-slash-commands`, explicit `--settings`/`--setting-sources`; unset `ANTHROPIC_API_KEY`; never `--bare` on a subscription; `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1` [62][63] | Subscription auth, lean context, no project hooks in page calls |
| TD | Keeping each `claude -p` call lean | "Component catalog on demand" via `news component` | Code pre-selects component specs into the cached prefix; writer has no tools (D4) | Fewer turns, better caching, injection defence |
| TD | Agent orchestration, How a run works | Main session calls `news plan`, then a Bun runner | Main session drives a preset through `news run start/next`; `news plan` becomes a step inside it | Durable, resumable runs with approval gates |
| TD | Model roles | Sonnet "selection judgement where code scoring is unsure"; Haiku "summarising long parsed text" | Remove the Sonnet selection role; Haiku condensation becomes extractive, digit-free, with quote spans (A5); add A3, A4, A7, A9 contracts | Code-first selection; verifiable condensation |
| TD | Content system | "Haiku assists in scoring … where code is unsure" | Haiku only for ambiguous classification (A3) and grey-zone story matching (A4) | Scoring stays fully code |
| TD | Agent toolbox | Command table | Add command groups `news run`, `jobs`, `ai`, `sources`, `calendar`, `stories`, `scores`, `entities`, `eval`, `review`, `corrections`, `work` and `news schema`; JSON envelope and exit codes (§6) | Purpose-built tools |
| TD | Data model, PostgreSQL | Table list | Add `snapshots`, `fetch_state`, `calendar_events`, `documents`, `text_blocks`, `derived_points`, `entity_ids`, `entity_aliases`, `stories`, `scores`, `run_plans`, `jobs`, `pools`, `runs`, `run_steps`, `approvals`, `publish_outbox`, `pages`, `page_refs`, `event_weights`; extend `sources`, `candidates`, `data_points` (vintage, locator, verbatim value) | Provenance, orchestration |
| TD | Local folders | `work/raw/` pruned after publish | Content-addressed `work/raw/sha256/…`, retention 400 days for structured data, 90 days for documents (D7) | Provable figures |
| TD | Telemetry | `events` columns | Add `job_id`, `attempt`, `preset`, `step`, `source_id`, `error_class`, `prompt_version`, `trace_id`, `span_id`; new tables `ai_calls`, `fetches`, `scores`, `check_results`, `runs` | Joins and dashboards |
| TD | Quality checks | Code checks list | Add numeric-literal lint, direction-word check, verbatim quotes, licence, duplicate-page checks (C4–C8, C14) | Number fidelity |
| TD | Environment variables | List | Add `NEWS_AI_RUN_TOKENS`, `NEWS_CONTACT_EMAIL`, `NEWS_RAW_RETAIN_DAYS_DATA` (400), `NEWS_RAW_RETAIN_DAYS_DOCS` (90) | Budgets, SEC UA, retention |
| TD | Open items: "Home curation workflow form" | Workflow file vs slash command | Settled: user-invoked skill + CLI state machine (D2) | Approval stops in-session |
| TD | Updating a page | Agent revises the page | Value-only revisions are code re-renders; AI only for direction flips (§3.11) | Fewer tokens, fewer errors |
| TD | Security | — | Approval tokens for gated commands; env allow-list for `claude -p`; Docling without network | Least privilege |
| PRD | Content production, step 2 | "scores, clusters and selects" | No change in substance; add that releases are captured on schedule by code from official calendars | Clarity |
| PRD | Trust and editorial; methodology | "Numbers come from data" | Add that some figures (changes, shares) are calculated by News from source figures and labelled as such (D5) | Honesty about derived figures |
| Development Plan | Repository tree; Phase 7 | `.claude/workflows/` for presets, starting with `curate-home` | One user-invoked skill, `/bureau`, in `.claude/skills/` driving `news run` (no per-preset commands; `curate-home` becomes `/bureau curate home`); `.claude/workflows/` optional for fan-out | D2 |
| Development Plan | Phase 4 | Work list | Add ADRs listed in §10, the embedding spike's measurements, golden-set labelling (G2, G3, G7) | Build order |
| Development Plan | Phase 5 | Work list | Add Dispatch, Chief, Runner, Morgue, Diary, Fixer, Ghost tools validators | Build order |
| Development Plan | Phase 7 | Work list | Add the lean-call spike, Ghost env allow-list and pause-on-limit, golden sets G1/G6, the Audit workflow | Build order |
| `skills/README.md` | Table | Page skills and curation | Add runtime skills for A3, A4, A5, A7, A9 (and A6, A10 if enabled) | Every AI step has a skill |
| `news-source-connector` skill | What a connector delivers | Discover, fetch, parse | Add: `parse` must be pure; locators per data point; conditional GET via the shared Runner; calendar entries; event-key rule | Provenance and politeness |

---

## 15. Name inventory

The system as a whole is **Bureau** (named). Below is every working label used in this document. Kinds: subsystem, component, workflow, tool family, artefact, data store. The last column offers one-word newsroom-role suggestions for Karthik to accept, change or ignore; the body text keeps the working labels so a find-and-replace can apply whatever he picks. Existing names may stay as they are.

| Label | What it is | Kind | Where it appears | Suggested newsroom name |
| --- | --- | --- | --- | --- |
| Bureau | Everything in this document: the laptop side of News | System (**named**) | Title, §1, §2 | — |
| Contacts | The `sources` table and its rules for licence, rate and tier | Subsystem / data store | §2, §3.1, §4.2 | Rolodex |
| Wire | Connectors, polite fetching, snapshot storage | Subsystem | §1, §2, §3.2 | Wire |
| Runner | Shared polite HTTP client (conditional GET, rate limits, robots) | Component | §2, §3.2, §10 | Runner |
| Morgue | Content-addressed raw files plus the `snapshots` table | Data store | §1, §2, §3.2, §10 | Morgue |
| Diary | Expected releases and meetings; drives pre-warm and capture | Subsystem / data store | §2, §3.3, §10 | Diary |
| Clerk | Parsing snapshots into data points, text blocks, candidates | Subsystem | §2, §3.4, §10 | Clerk |
| Ledger | `data_points` and `derived_points` with vintages and locators | Data store | §1, §2, §3.4 | Almanac |
| Roster | Entities, identifiers, aliases, edges | Data store | §1, §2, §3.5, §3.10 | Contacts |
| Roster's resolver | Deterministic mention → entity resolution | Component | §3.5, §10 | Librarian |
| Taster | Event keys, de-duplication, clustering, update detection | Subsystem | §1, §2, §3.6 | Monitor |
| Nose | Score formula and its calibration | Subsystem | §1, §2, §3.7, §4 | Nose |
| Desk | Breadth-first run filling and tiering | Component | §1, §2, §3.7, §4.4 | Slot |
| Budget | The stories chosen for a run, with tier, bucket, reason | Artefact | §2, §3.7, §4.4, §8.2 | Budget |
| Pitch | One-line-per-component explanation of a score | Artefact | §3.7, §4.1, §4.7, §6.4 | Tally |
| Fixer | Builds the brief (research pack) | Subsystem | §1, §2, §3.8 | Fixer |
| Reporter | AI writing of pages (A1, A2) | Subsystem | §1, §2, §3.9 | Reporter |
| Copy | Writer output: headline, fly title, summary, OpenUI Lang with refs | Artefact | §2, §3.9, §5.8 | Copy |
| Proof | Code checks C1–C14 and the sample check | Subsystem | §1, §2, §3.10 | Proof |
| Markup | Result of the checks for one page | Artefact | §2.3 | Markup |
| Rewrite | Updating pages when sources revise data | Subsystem | §1, §2, §3.11 | Rewrite |
| Press | Outbox, paced publishing, index budget | Subsystem | §1, §2, §3.12 | Press |
| Ticker | Events, spans and AI call records into ClickHouse | Subsystem | §1, §3.13, §9.2 | Ticker |
| Board | The local dashboard (TD "Local analytics dashboard") | Subsystem (existing concept) | §1, §2, §9.2 | Board |
| Dispatch | PostgreSQL job queue: jobs, pools, leases, retries | Subsystem | §1, §2, §5 | Desk |
| Chief | Run/step state machine with approval gates behind presets | Subsystem | §1, §2, §5.5, §8 | Producer |
| Dispatch worker | Optional background process running code-only pools (`news work`) | Component | §1, §2, §5.6, §8.13 | Nightdesk |
| Ghost | Bun program that claims AI jobs and runs `claude -p` (TD "bun runner"; `pipeline/run-ai.ts`) | Component | §1, §2, §3.9, §5.5, §8.1 | Stringers |
| Critic | Golden sets, judges, comparisons | Subsystem | §1, §9.1 | Ombudsman |
| Wrap | End-of-run report | Artefact | §2.2, §8.1 | Wrap |
| Sitrep | Output of the source health workflow | Artefact | §8.6 | Sitrep |
| Debrief | Output of the weekly review | Artefact | §8.11 | Critique |
| Edition | Main production preset (`/bureau run daily`) | Workflow | §2, §8.2 and throughout | Edition |
| Flash | Preset for a few named items now (`/bureau run quick`) | Workflow | §8.3 | Flash |
| Catchup | Preset for current-year backfill (`/bureau run backfill`) | Workflow | §4.4, §8.4 | Catchup |
| Frontpage | Home curation preset, started with `/bureau curate home` (was the TD's `curate-home`) | Workflow | §8.5 | Frontpage |
| Rollcall | Preset for source checks and fixes (`/bureau check sources`) | Workflow | §3.1, §3.2, §8.6 | Beatcheck |
| Salvage | Preset for dead jobs and replays (`/bureau retry`) | Workflow | §3.4, §5.4, §5.9, §8.7 | Salvage |
| Followup | Preset (and daily step) for source revisions (`/bureau apply revisions`) | Workflow | §8.8 | Update |
| Errata | Preset for reader error reports (`/bureau review corrections`) | Workflow | §8.9, §9.3 | Errata |
| Audit | Preset for golden-set evaluation and prompt promotion (`/bureau run evals`) | Workflow | §5.9, §8.10, §9.1 | Audit |
| Huddle | Preset for weekly stats and ranking calibration (`/bureau review week`) | Workflow | §3.5, §4.7, §8.11 | Conference |
| Reprint | Preset to re-render pages on a new renderer (`/bureau rerender pages`) | Workflow | §8.12 | Reprint |
| Chief tools | `news run`: start, next, status, approve, reject, resume, cancel, list | Tool family | §2.2, §5.5, §6.4, §8 | Rundown |
| Dispatch tools | `news jobs`: list, dead, claim, heartbeat, complete, fail, retry, pause | Tool family | §5, §6.4 | Docket |
| Ghost tools | `news ai`: input, accept, schema, fallback | Tool family | §3.9, §6.4 | Intake |
| Contacts tools | `news sources`: list, show, health, probe, enable, disable | Tool family | §3.1, §6.4, §8.6 | Beats |
| Diary tools | `news calendar`: upcoming, sync, late | Tool family | §6.4 | Datebook |
| Taster tools | `news stories`: show, members, similar, merge, split | Tool family | §3.6, §6.4 | Threads |
| Nose tools | `news scores`: explain, weights, propose, adopt | Tool family | §6.4, §8.11 | Scales |
| Roster tools | `news entities`: resolve, show, unresolved, alias | Tool family | §3.5, §6.4 | Index |
| Critic tools | `news eval`: run, report, label, promote | Tool family | §6.4, §8.10 | Review |
| Huddle tools | `news review`: weekly | Tool family | §6.4, §8.11 | Huddle |
| Errata tools | `news corrections`: pull, show, resolve | Tool family | §6.4, §8.9 | Corrigenda |
| `news` CLI and its TD commands (`discover`, `plan`, `fetch`, `parse`, `brief`, `components`, `component`, `related`, `save`, `render`, `check`, `publish`, `update`, `prune`, `stats`, `home …`) | The agent toolbox | Tool family (existing) | §3, §6.4 | keep |
| `curate-home` | Former name of the home curation workflow; replaced by Frontpage and `/bureau curate home` | Retired name | §8.5 | retired |
| `news-home-curation` | Curator skill | Runtime skill (existing, planned) | §7.1, §8.5 | keep |
| `news-brand`, `page-earnings`, `page-macro-release`, `page-major-report`, `page-policy-decision`, `page-news-story`, `page-science`, `page-explainer` | Page-writer skills | Runtime skills (existing) | §3.9 | keep |
| Brief | The research pack | Artefact (existing TD term) | §3.8 | keep |

**Commands (verb-based, concrete; not part of the naming).**

| Command | Starts or does |
| --- | --- |
| `/bureau run daily`, `/bureau run quick`, `/bureau run backfill`, `/bureau curate home`, `/bureau check sources`, `/bureau retry`, `/bureau apply revisions`, `/bureau review corrections`, `/bureau run evals`, `/bureau review week`, `/bureau rerender pages` | The preset workflows (§8), as user-invoked skills |
| `news run …`, `news jobs …`, `news ai …`, `news sources …`, `news calendar …`, `news stories …`, `news scores …`, `news entities …`, `news eval …`, `news review …`, `news corrections …`, `news work`, `news schema` | New `news` CLI command groups (§6) |
| `bun pipeline/run-ai.ts --run <id>` | Ghost for one run's AI batch |

---

## 16. References

Checked 14 Nov 2026. *(unverified)* in the text marks facts not confirmed in these sources.

**Orchestration**

1. PostgreSQL 17, `SELECT … FOR UPDATE SKIP LOCKED`: https://www.postgresql.org/docs/17/sql-select.html
2. Brandur Leach, "River: a fast, robust job queue for Go + Postgres": https://brandur.org/river
3. River, unique jobs: https://riverqueue.com/docs/unique-jobs
4. River, job retries: https://riverqueue.com/docs/job-retries
5. River, snoozing jobs: https://riverqueue.com/docs/snoozing-jobs
6. Brandur Leach, "Implementing Stripe-like idempotency keys in Postgres": https://brandur.org/idempotency-keys
7. Brandur Leach, "Transactionally staged job drains in Postgres": https://brandur.org/job-drain
8. AWS Architecture Blog, "Exponential backoff and jitter": https://aws.amazon.com/blogs/architecture/exponential-backoff-and-jitter/
9. apalis: https://docs.rs/crate/apalis/latest ; apalis-postgres: https://docs.rs/crate/apalis-postgres/latest
10. pg-boss: https://github.com/timgit/pg-boss
11. pgmq: https://github.com/pgmq/pgmq
12. DBOS docs: https://docs.dbos.dev/ ; DBOS Rust guide (pre-production): https://docs.dbos.dev/rust/programming-guide
13. Restate server overview: https://docs.restate.dev/server/overview.md ; licence: https://raw.githubusercontent.com/restatedev/restate/main/LICENSE
14. Temporal development server: https://docs.temporal.io/cli/server
15. Armin Ronacher, "Absurd workflows": https://lucumr.pocoo.org/2025/11/3/absurd-workflows/ ; https://github.com/earendil-works/absurd
16. Hatchet: https://github.com/hatchet-dev/hatchet ; Inngest self-hosting: https://www.inngest.com/docs/self-hosting
17. graphile_worker (Rust): https://crates.io/crates/graphile_worker

**Sources and fetching**

18. SEC, Accessing EDGAR data (fair access, User-Agent, `company_tickers.json`): https://www.sec.gov/search-filings/edgar-search-assistance/accessing-edgar-data
19. SEC, EDGAR APIs (submissions, XBRL): https://www.sec.gov/search-filings/edgar-application-programming-interfaces
20. SEC, RSS feeds: https://www.sec.gov/about/rss-feeds
21. FRED API, `releases/dates`: https://fred.stlouisfed.org/docs/api/fred/releases_dates.html ; `series/vintagedates`: https://fred.stlouisfed.org/docs/api/fred/series_vintagedates.html
22. BLS API FAQ: https://www.bls.gov/developers/api_faqs.htm ; release schedule and ICS: https://www.bls.gov/schedule/news_release/
23. BEA API: https://apps.bea.gov/API/docs/index.htm ; release schedule: https://www.bea.gov/news/schedule
24. Eurostat API: https://ec.europa.eu/eurostat/web/user-guides/data-browser/api-data-access/api-getting-started ; release calendar: https://ec.europa.eu/eurostat/news/euro-indicators/release-calendar
25. ECB Data Portal API: https://data.ecb.europa.eu/help/api/overview
26. ONS developer hub, API retirement: https://developer.ons.gov.uk/retirement/
27. Statistics Canada WDS user guide: https://www.statcan.gc.ca/en/developers/wds/user-guide
28. ABS Data API: https://www.abs.gov.au/about/data-services/application-programming-interfaces-apis/data-api-user-guide
29. IMF API: https://data.imf.org/en/Resource-Pages/IMF-API
30. World Bank Indicators API: https://datahelpdesk.worldbank.org/knowledgebase/articles/889392-about-the-indicators-api-documentation
31. OECD, API best practices: https://www.oecd.org/en/data/insights/data-explainers/2024/11/Api-best-practices-and-recommendations.html
32. BIS data tools: https://data.bis.org/help/tools
33. arXiv API terms of use: https://info.arxiv.org/help/api/tou.html ; RSS: https://info.arxiv.org/help/rss.html ; OAI-PMH: https://info.arxiv.org/help/oa/index.html
34. OpenAlex rate limits and authentication: https://help.openalex.org/how-to-use-the-api/rate-limits-and-authentication
35. Crossref REST API access: https://www.crossref.org/documentation/retrieve-metadata/rest-api/access-and-authentication/
36. NCBI E-utilities: https://www.ncbi.nlm.nih.gov/books/NBK25497/
37. bioRxiv/medRxiv API: https://api.biorxiv.org/
38. Meeting calendars: Fed https://www.federalreserve.gov/monetarypolicy/fomccalendars.htm ; ECB https://www.ecb.europa.eu/press/calendars/mgcgc/html/index.en.html ; Bank of England https://www.bankofengland.co.uk/monetary-policy/upcoming-mpc-dates ; Bank of Japan https://www.boj.or.jp/en/mopo/mpmsche_minu/index.htm
39. RFC 9110 HTTP Semantics (conditional requests, `Retry-After`): https://www.rfc-editor.org/rfc/rfc9110 ; RFC 9111 HTTP Caching: https://www.rfc-editor.org/rfc/rfc9111
40. RFC 9309 Robots Exclusion Protocol: https://www.rfc-editor.org/rfc/rfc9309

**Entities, similarity, ranking**

41. OpenFIGI API: https://www.openfigi.com/api/documentation
42. GLEIF API and data: https://www.gleif.org/en/lei-data/gleif-api
43. ROR data dump: https://ror.readme.io/docs/data-dump
44. Wikidata data access: https://www.wikidata.org/wiki/Wikidata:Data_access
45. Manku, Jain, Das Sarma, "Detecting near-duplicates for web crawling", WWW 2007: https://www.www2007.cpsc.ucalgary.ca/papers/paper215.pdf
46. gaoya (MinHash/SimHash in Rust): https://github.com/serega/gaoya
47. Miranda et al., "Multilingual Clustering of Streaming News", EMNLP 2018: https://aclanthology.org/D18-1483/
48. pgvector: https://github.com/pgvector/pgvector
49. Model cards: bge-small-en-v1.5 https://huggingface.co/BAAI/bge-small-en-v1.5 ; nomic-embed-text-v1.5 https://huggingface.co/nomic-ai/nomic-embed-text-v1.5 ; EmbeddingGemma https://huggingface.co/google/embeddinggemma-300m ; Qwen3-Embedding-0.6B https://huggingface.co/Qwen/Qwen3-Embedding-0.6B ; all-MiniLM-L6-v2 https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2
50. fastembed-rs: https://github.com/Anush008/fastembed-rs ; ort: https://github.com/pykeio/ort
51. Manticore, ONNX embeddings speed-up: https://manticoresearch.com/blog/onnx-embeddings-speedup
52. Hugging Face / Intel, fast embeddings on CPU: https://huggingface.co/blog/intel-fast-embedding
53. Harcup and O'Neill, "What is News? News values revisited (again)", 2017: https://eprints.whiterose.ac.uk/id/eprint/95423/
54. Google, How news works: https://www.google.com/search/howsearchworks/how-news-works/
55. Hacker News ranking discussion: https://news.ycombinator.com/item?id=1781013
56. Carbonell and Goldstein, "The use of MMR, diversity-based reranking", SIGIR 1998: https://www.cs.cmu.edu/~jgc/publication/The_Use_MMR_Diversity_Based_LTMIR_1998.pdf *(not re-fetched)*
57. LightGBM parameters (lambdarank): https://lightgbm.readthedocs.io/en/stable/Parameters.html
58. Li et al., "A contextual-bandit approach to personalized news article recommendation", 2010: https://arxiv.org/abs/1003.0146

**Evaluation and Claude Code**

59. Zheng et al., "Judging LLM-as-a-judge", 2023: https://arxiv.org/abs/2306.05685
60. Anthropic, Define success criteria: https://platform.claude.com/docs/en/test-and-evaluate/define-success ; Create strong empirical evaluations: https://platform.claude.com/docs/en/test-and-evaluate/develop-tests
61. Hamel Husain, "Your AI product needs evals": https://hamel.dev/blog/posts/evals/ ; Evals FAQ: https://hamel.dev/blog/posts/evals-faq/
62. Claude Code headless mode: https://code.claude.com/docs/en/headless.md ; CLI reference: https://code.claude.com/docs/en/cli-reference.md
63. Claude Code environment variables: https://code.claude.com/docs/en/env-vars.md
64. Claude Code workflows: https://code.claude.com/docs/en/workflows.md
65. Claude Code skills: https://code.claude.com/docs/en/skills.md ; sub-agents: https://code.claude.com/docs/en/sub-agents.md
66. Claude Code hooks: https://code.claude.com/docs/en/hooks.md ; permissions: https://code.claude.com/docs/en/permissions.md
67. Claude Code MCP (tool search, output limits): https://code.claude.com/docs/en/mcp.md
68. Claude Code prompt caching: https://code.claude.com/docs/en/prompt-caching.md ; costs: https://code.claude.com/docs/en/costs.md ; status line: https://code.claude.com/docs/en/statusline.md
69. Claude Code monitoring (OpenTelemetry): https://code.claude.com/docs/en/monitoring-usage.md
70. Claude Agent SDK overview: https://code.claude.com/docs/en/agent-sdk/overview.md ; legal and compliance: https://code.claude.com/docs/en/legal-and-compliance.md
71. Anthropic, Mitigate jailbreaks and prompt injections: https://platform.claude.com/docs/en/test-and-evaluate/strengthen-guardrails/mitigate-jailbreaks ; Claude Code security: https://code.claude.com/docs/en/security.md
