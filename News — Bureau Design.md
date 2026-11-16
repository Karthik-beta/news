# News — Bureau Design

Nov 14, 2026 · @Karthik · Draft for review · Updated 16 Nov 2026: Bureau runs as a Windows service and Board is its desktop app ([ADR 0007](docs/adr/0007-bureau-service-and-board.md); the changes are listed in [§14.2](#142-bureau-service-and-board-16-nov-2026))

**Bureau** is the part of News that runs on Karthik's laptop: where the reporting happens, away from head office, while the cloud side only serves readers. This document designs all of Bureau: ingestion, normalisation, story detection, scoring and ranking, content generation, checking, orchestration, and the preset workflows Karthik runs from Board, the desktop app of the Bureau service. It builds on the [PRD](News%20—%20Product%20Requirements%20Document.md), the [Technical Design](News%20—%20Technical%20Design.md) (TD) and the [Development Plan](News%20—%20Development%20Plan.md). Where it departs from them, the change is listed in [Changes to existing documents](#14-changes-to-existing-documents) and has been applied to those documents.

**Names.** Every part of Bureau has an approved one-word newsroom name (Wire, Stringers, Morgue, Diary, Clerk, Ledger, Roster, Taster, Nose, Desk, Fixer, Reporter, Proof, Press, Chief, Dispatch, Ghost …). The full reference table is in [§15 Names](#15-names). Commands are verb-based: the `news` CLI (`news run status`, `news jobs retry`) and the actions Board sends over the Hotline. Existing names (`news` CLI commands, `news-home-curation`, the `page-*` and `news-brand` skills, PostgreSQL tables named in the TD) stay as they are; the TD's `curate-home` workflow becomes Frontpage, which runs on Karthik's schedule or from Board.

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
8. [Board, the Bureau service and the preset workflows](#8-board-the-bureau-service-and-the-preset-workflows)
9. [Quality, observability, security, performance](#9-quality-observability-security-performance)
10. [Build order](#10-build-order)
11. [Decisions](#11-decisions)
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
| The Bureau service, Board and every preset workflow, including Frontpage (home curation) | |
| Ticker events and Board's screens for this system | |
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
| P8 | **Karthik's Start or schedule is the approval** | Background jobs (calendar sync, release-time fetching, source probes) run without Karthik. A run that calls a model starts only from Karthik: a Start in Board, or a schedule he set in Board, which counts as his approval to start. Gated presets still stop and wait in Board. AI usage is on his plan, so the quota guard keeps Bureau inside a cap and a reserve (§7.4) (PRD "Content production"; ADR 0007). |
| P9 | **Light on a 4-core laptop** | Small fixed worker pools, one heavy run at a time, network-bound work overlapped with AI waits, no extra servers beyond the PostgreSQL, ClickHouse and OTel containers, the Bureau service and the warm render server. Quiet hours keep heavy runs off the laptop when Karthik wants it (§5.6). |
| P10 | **Everything measured** | Every step emits events and spans to ClickHouse, joined by `run_id` and `job_id`, including each `claude -p` call. |
| P11 | **One control surface** | Karthik starts and controls every preset and run from Board, and every H-tier action is accepted only from Board (§6.3). His terminal (the `news` CLI over the Hotline) reads status and runs the low-risk tiers only. Chief is the only way a preset runs, actions arrive only over the Hotline and never from a process the service spawned for `claude -p`, and nothing bypasses Chief's gates (§5.5, §8.1, §9.3). |

---

## 2. System map

### 2.1 Subsystems and data flow

```
                       ┌──────────────────────────── BUREAU (laptop) ──────────────────────────────────────┐
 Karthik ──Board────▶  │  Chief (preset runs, gates, Wrap) ──enqueues/advances──▶ Dispatch (queue + workers)│
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
                       │   Ghost (Rust) ─claude -p──▶ Reporter ──▶ Copy                                     │
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
 Karthik ──Board (news board, egui)──┐   status board, gates, run detail, settings and schedules, Docker health
         ──terminal (news …)─────────┤   status and low-risk commands only; no H-tier action
                                     ▼
          Hotline: Karthik's user only, local only; control commands refused
          from any process the service spawned for claude -p (§9.3, ADR 0007)
                                     ▼
          Bureau service (news service run, a Windows service logged on as Karthik)
            Chief       runs and steps, gates, the Lineup, the Wrap
            scheduler   preset schedules (Karthik's settings), missed slots, quiet hours, quota guard (§5.6, §7.4)
            Dispatch    workers for every pool: net, cpu, docling (always on); render, ai, publish (inside runs)
            Ghost       one claude -p per AI job, in the service's job object, env allow-list, usage-limit pause
                 │                          │ HTTP on 127.0.0.1
                 │                          ▼
                 │                   render server (Bun + React/OpenUI, kept warm)
                 ▼
          PostgreSQL (runs, run_steps, jobs, approvals, settings …) = single source of truth
```

Board holds no state of its own. It shows what the service reports and sends Karthik's actions over the Hotline. Runs carry on with Board closed; Board raises Windows toasts for the things that need him (§8.1).

### 2.3 Life of one item (US CPI, October release)

| Time (ET) | Step | Code or AI | Output |
| --- | --- | --- | --- |
| Day −1 | Diary has `macro:us.cpi:2026-10` expected 08:30 from the BLS ICS feed [22] | Code | `calendar_events` row |
| Day −1 | Pre-warm: history of the CPI series and components fetched, entity and template context cached | Code | data points (prior vintages) |
| 08:32 | A Dispatch worker runs the BLS Stringer (conditional GET; retries every 5 min until new data) | Code | snapshot in Morgue + new data points |
| 08:33 | Clerk stores values exactly as published in Ledger; event key `macro:us.cpi:2026-10` | Code | `candidates` row |
| 08:33 | Taster: event key already has a story → join; no embedding needed | Code | `stories` member |
| 08:33 | Nose: score 75.2 (worked example in §4.6) | Code | `scores` row, Pitch |
| 08:40 | Board's status board shows the CPI story captured and the next scheduled Edition at 11:00 | Code | Status board |
| 11:00 | The Edition schedule Karthik set fires (or he presses Start in Board); Desk puts the story in Tier 1, Economy/North America bucket | Code | Budget row |
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
  auth_env         text,                        -- NAME of the Credential Manager entry holding a free key, never the key
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

| Source | Limit or rule | Default in Contacts |
| --- | --- | --- |
| SEC EDGAR | Max 10 requests/s; declared User-Agent with organisation and contact e-mail [18] | 4 req/s; UA contact from the `NEWS_CONTACT_EMAIL` setting in Standing (never committed) |
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

**AI use.** None. Contacts rows are written by developers (Sonnet docs writer, Opus review), never by a runtime model.

**Tools.** `news sources list|show|health|probe` (agents); `enable|disable` (Karthik from Board, or Rollcall's step after its `actions` approval).

**Failure modes.** Missing licence → the Stringer refuses to emit candidates (`licence_missing`). Free key absent → source `amber`, skipped, reported by the Rollcall workflow.

**Tests.** Schema test that every enabled source has licence, attribution, rate and tier; a lint fails CI if `rate_per_sec` is above half the documented limit recorded in `notes`.

### 3.2 Wire: Stringers, Runner, Morgue

**Responsibility.** Discover new items per source, download them politely and store the exact bytes received with their provenance. No interpretation beyond what is needed to list items.

**Stringer contract.** One Stringer per source; in code each Stringer is an implementation of the `Connector` trait (Rust, in `cli/crates/connectors`), and `sources.connector` names it:

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
| Rate limits | Token bucket per host from Contacts; global network pool of 6 | Source terms (table above) |
| `Retry-After` | Honour seconds or HTTP-date on 429/503; the job is *snoozed* (not counted as a failed attempt) | RFC 9110 [39]; River's snooze [5] |
| robots.txt | Cached ≤ 24 h; 4xx on robots.txt = allowed, 5xx = disallow all; case-insensitive UA match falling back to `*` | RFC 9309 [40] |
| User-Agent | `News/<version> (+https://news.karthik.nz/data-access)`, plus the contact e-mail (`NEWS_CONTACT_EMAIL`, from Standing) only for sources that require one (SEC) | SEC fair access [18] |
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
| ONS | `api.beta.ons.gov.uk/v1`; legacy endpoints are being retired [26] | Hash comparison; Stringer flagged "beta API" |
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

**Tests.** Recorded fixtures per Stringer (no network in CI); a fake HTTP server for the 304 path, both `Retry-After` forms, robots 4xx/5xx semantics and token-bucket pacing on a virtual clock.

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
| HTML | FOMC, ECB, BoE, BoJ, RBA, RBI meeting calendars [38] | Parsed by the source's Stringer, plus a hand-checked yearly seed file, because these matter most and change rarely |
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

Guards keep the no-conversion rule: a change is computed only between points with identical unit, currency, scale and seasonal adjustment and comparable periods (the same fiscal period a year apart, or as the page-type skill says). If a guard fails, no derived point exists and Reporter cannot mention that change.

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

**Tests.** Golden parse fixtures per Stringer with at least one awkward case (restatement, non-calendar fiscal year, footnoted value, missing observation); property tests that derived-point guards never allow a change across different units, currencies or scales; a round-trip test that `value` formats back to `value_text`.

**Metrics.** Data points per snapshot, parse failures by Stringer and class, unknown units, derived points refused by guards.

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

**Failure modes.** Reuse of a stock ticker after delisting → validity dates; similarly named companies → the margin rule refuses ambiguous matches.

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
  action text not null,           -- 'new' | 'update' (direction flip, A1) | 'revise' (value-only re-render, code)
  primary key (run_id, story_id)
);
```

### 3.8 Fixer

**Responsibility.** Build the brief (the TD's research pack) for one story: everything Reporter needs and nothing else. Code only, except optional condensation of long source text (A5).

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

- **Facts carry values** so Reporter understands direction and size, but it may output only `ref` tokens. The numeric-literal lint enforces this (C4).
- **Reference formats.** Fixer issues exactly six kinds of reference, all lower-case `prefix_body` with `_`-separated parts: `dp_…` (data point), `dv_…` (derived value), `sr_…` (series, e.g. `sr_cpi_yoy_5y`), `pd_…` (period label, e.g. `pd_2026_10` for "October 2026"), `q_…` (quote) and `cx_…` (context line). The Copy schema (§3.9) accepts these and nothing else, and C2 checks each one exists in this brief.
- **Periods are references too.** Reporter never types a date or year: it writes `pd_…` and code renders the label from the brief, so C4 can forbid digits outright.
- **Context lines are computed by code** ("lowest since …", "first cut since …", "record in the series"), each with the rule that produced it, so it can be re-checked. They are the news hooks Reporter can use without inventing anything.
- **Untrusted text is fenced**: source text sits under `source_text` with `"untrusted": true`, JSON-encoded; Reporter's instructions say it is data, not instructions (§9.3).
- **Component specs are pre-selected** by code from the page-type skill's typical layout and placed in the cached prefix, so Reporter needs no tool calls (D4).
- **Size budget**: above 3k tokens (local tokenizer estimate), code drops the lowest-priority facts and series first, then condenses text (A5).

**Tests.** Snapshot tests per page type; size budget; no untrusted text outside `source_text`. **Metrics.** Brief tokens by page type, facts per brief, condensation calls.

### 3.9 Reporter

**Responsibility.** Turn a brief into a Copy: headline, fly title, summary and OpenUI Lang with data references. The main AI step (A1) and the only place reader-facing prose is written.

**Call shape** (Ghost, inside the Bureau service, spawns one `claude -p` per item, `NEWS_PARALLEL` = 3):

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
# child process: created suspended, put in the service's AI job object (the spawn marker), then resumed;
#                no breakaway, so everything it starts stays in the job and is refused control commands
# child env: ANTHROPIC_API_KEY unset; CLAUDE_CODE_DISABLE_AUTO_MEMORY=1; NEWS_SPAWNED_BY=<run>:<job> (label only);
#            OTEL_RESOURCE_ATTRIBUTES=news.run_id=<run>,news.job_id=<job>,news.step=A1
```

**The spawn marker.** The service starts every `claude -p` in a Windows Job Object it owns, created without breakaway rights, so the call and every process it starts stay in that job. The Hotline refuses start, approve, cancel, settings and every other control command from any process in such a job (§9.3, ADR 0007). The `NEWS_SPAWNED_BY` variable is a label for logs only; the check never relies on it.

Why each flag [62][63][68]: `--tools ""` disables all built-in tools (`--allowedTools` only pre-approves, it does not restrict); `--strict-mcp-config` with an empty config stops project MCP servers loading, and without it `-p` also runs project hooks and `.mcp.json` servers with no trust prompt; `--json-schema` makes the harness validate the result and re-prompt, returning `structured_output`, or the subtype `error_max_structured_output_retries`; `--max-budget-usd` is a client-side estimate that can overshoot, used as a runaway guard, not a bill; if `ANTHROPIC_API_KEY` is set, `-p` uses it instead of the subscription, so Ghost removes it; `--bare` must **not** be used, because it never reads subscription credentials. The settings file and `--setting-sources` keep project hooks and plugins out of page calls *(the exact combination is confirmed in the Phase 7 lean-call spike; unverified)*.

**Prompt layout for caching.** The prefix (instructions + `news-brand` + one `page-*` skill + component index + that page type's pre-selected component specs) is one file named by its hash, identical for every call of that page type. Ghost orders each batch by page type so calls with the same prefix run back to back. On a subscription, `-p` main-conversation requests get a 1-hour cache TTL while sub-agents and workflow agents get 5 minutes [68], one reason page writing runs through Ghost rather than workflow agents. Cache reads are checked per call from `usage` in the JSON result.

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

**Validation (code, `news ai accept`).** JSON schema → OpenUI Lang parses → every ref exists in the brief → numeric-literal lint → banned words (brand rules) → component props valid against specs → length rules. On failure: one repair call (A2) with the error list and the previous Copy; a second failure sends the item back to the queue as `ai_invalid_output` for the next run, and the run carries on.

**Fallback.** For Tier 2 items on which Reporter fails twice, a **templated page** built only by code (KeyFigures + main chart + data table + sources, headline from a page-type pattern with refs, no prose) may be published, flagged `templated` so a later run upgrades it (D10).

**Token budget.** ≤ 2 turns; brief ≤ 3k tokens; prefix 6–10k tokens (mostly cache reads); output ≤ 4k tokens; per-run token ceiling `NEWS_AI_RUN_TOKENS` (new).

**Tests.** Without AI: validator tests with hand-made good and bad Copies (digits in prose, unknown ref, wrong props, banned words). With AI: golden set G1 (§9.1). **Metrics.** Tokens in/out, cache-read ratio, turns, validation failures by rule, repair rate, wall time, cost estimate.

### 3.10 Proof

**Responsibility.** Decide whether a rendered page may be published: code checks on every page (zero tokens), a Haiku sample check on a share (A7). Extends TD "Quality checks".

| # | Check | Rule | Severity |
| --- | --- | --- | --- |
| C1 | Parse and render | OpenUI Lang parses; renderer returns HTML, Markdown, JSON | block |
| C2 | References resolve | Every ref in `.oui`, headline and summary exists in `data.json` with a source | block |
| C3 | Values match source | Each rendered value equals the data point's `value` through the shared formatter; derived values recomputed from inputs | block |
| C4 | **Numeric-literal lint** | The lint first finds and removes the Copy's reference tokens (`{dp_…}`, `pd_…` and the other five kinds, each validated against the brief by C2), then checks the remaining prose: it may contain no digit sequence at all, except an entity name containing digits that Roster resolved for this brief and a short allow-list (G7, G20, COVID-19, 5G …). Periods, years and dates are written only as `pd_…` references and rendered by code (§3.8), so there is no year exemption to misuse: "revenue was 2024 USD" is blocked whether or not 2024 is a period in the brief. Written-out quantities are blocked the same way, even when `refs_used` is empty: a word list (cardinals zero to ninety-nine and hundred, thousand, million, billion, trillion; fractions half, third, quarter, tenth used as amounts; multipliers twice, double/doubled, triple/tripled, halved; "per cent", "percentage points", "basis points" after any quantity word) is matched in the stripped prose, with a short allow-list of fixed expressions that are not amounts ("one of the", "one-off"); period words such as "first quarter" never appear in prose, because periods are `pd_` references. So "three per cent" or "doubled" without a reference blocks the Copy; Reporter uses `{dp_…}`/`{dv_…}` tokens, whose values code renders | block |
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

**Tools.** `news check <id>` → the Markup, `{pass, failures:[{check, where, message}]}`. **Metrics.** Pass rate per check and page type, block reasons, sample flag rate, confirmed-flag rate.

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

**Index budget.** In Edition step 11, never in the background: take published `noindex` pages, excluding any flagged `templated` (they stay `noindex` until Reporter upgrades them, D10), rank them by `index_priority = score + 10·[Tier 1] + uniqueness` (uniqueness = distinct data series and charts no indexed page has) and promote until `NEWS_INDEX_PER_DAY` promotions have been made that day. Code only.

**Failure modes.** 5xx → retry with backoff (safe because of the idempotency key); 4xx validation → `bug` class, dead list; throttling → snooze the publish pool.

**Metrics.** Publish latency, retries, throttles, pages per index status.

### 3.13 Ticker (summary)

Every subsystem emits rows to the existing `news.events` table and OTel spans; new columns, tables and Board panels are in [§9.2](#92-observability).

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

**Updates** are planned in the same Budget but outside the breadth caps: value-only revisions enter it as `revise` items (code re-renders them, no AI) and direction flips as `update` items, up to the `bulk` threshold of 100 pages (§8.3); AI updates take at most 10 % of the run. Because they are in the Budget, the run's start or its `plan` approval authorises publishing them (§5.5).

### 4.5 Index priority and home candidates

- **Index priority** (Edition step 11, Press): `S + 10·[Tier 1] + uniqueness`.
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
| Sanity views | Score histograms by page type; top 20 selected vs top 20 rejected, with Pitches | Huddle |
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
- Start runs on Karthik's schedules, unattended, and queue heavy runs for the one heavy slot.
- Run every job in Rust inside the Bureau service, calling Plate, the warm Bun render server for rendering.
- Add no server beyond the Bureau service and Plate; PostgreSQL 17 is already there.

### 5.2 Options compared

| Option | Language | Needs | Licence | Weight on this laptop | Fit |
| --- | --- | --- | --- | --- | --- |
| **Custom queue on `FOR UPDATE SKIP LOCKED`**, River-style semantics | Rust (SQL) | PostgreSQL | ours | None | Full control; one implementation of retry, snooze, pools, in the same Rust service as everything else [1][2] |
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
- **One owner of the rules.** Rust owns the data model and every job runs in the Rust service (Ghost included, ADR 0007), so retry, snooze and idempotency are written once.
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
    -- 3. A job of a run is claimed only while that run is live; background jobs have no run.
    --    The run row is locked FOR SHARE, so a claim and a pause on the same run are serialised (see below).
    and (run_id is null or exists (select 1 from runs r where r.run_id = jobs.run_id
                                    and r.status in ('running','waiting_ai') for share of r))
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

**Jobs of paused runs.** Step 3 of the claim skips jobs whose run is queued, pausing, paused, waiting at a gate or finished, so pausing a run (including the at-once pause of a stalled run, §6.4) stops all its work in every pool, and nothing of it is claimed until it resumes. The claim and the pause synchronise on the run row: the claim holds a `FOR SHARE` lock on it until it commits, and the pause's first step (`update runs set status = 'pausing'`) needs the row's write lock. So a pause waits for any claim already holding the row to commit, and a claim that starts after the pause has the row waits for it and then, under READ COMMITTED, re-checks the updated row, sees `pausing` and takes nothing. Every claim of the run therefore either committed before the `pausing` transaction, so its jobs are `running` when the pause's cancellation scan runs after it, or finds nothing. The pause drains exactly those jobs before it frees the slot.

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
| `ai_usage_limit` | Claude plan limit reached | Pause pool `ai` until the reset time (`paused_until`); run → `paused`; the service resumes it on its own at the reset (§5.9) |
| `ai_invalid_output` | schema or validator failure after repair | Discard for this run; re-plan next run (max 2 runs) |
| `check_failed` | Proof block | One AI repair if the failure is in model text; else discard with reason |
| `upstream_changed` | feed or schema drift | Discard; source health red |
| `invalid_data` | unparsable value, unknown unit | Discard; `needs_human` |
| `licence_blocked` | robots disallow, licence missing | Discard; never retried automatically |
| `bug` | panics, 4xx from our own API, invariant broken | Discard; event with stack; dev task |

**Unique keys** stop duplicate work: enqueueing a job whose `unique_key` is live is a no-op that returns the existing `job_id` (River's unique jobs [3]).

### 5.5 Preset runs (Chief)

Chief runs inside the Bureau service (ADR 0007). It advances every live run itself; no chat session drives it.

```sql
create table runs (
  run_id uuid primary key, short_id text unique not null,   -- 'r_7f3c', shown in Board and the terminal
  preset text not null, args jsonb not null,
  heavy boolean not null,   -- from the preset declaration (§8.1)
  origin text not null check (origin in ('board','schedule')),
  schedule_key text,        -- the schedule setting that started it, when origin = 'schedule'
  settings_version int not null,                 -- the settings version the run started with (§5.6)
  status text not null,     -- 'queued' | 'running' | 'waiting_approval' | 'waiting_ai' | 'pausing' | 'paused'
                            -- | 'succeeded' | 'failed' | 'cancelled' | 'skipped'
  queued_at timestamptz, start_by timestamptz,   -- a queued scheduled run is skipped after start_by (§5.6)
  paused_reason text,       -- 'karthik' | 'usage_limit' | 'quiet_hours' | 'quota' | 'update'
  current_step text, config jsonb not null,      -- snapshot of the settings, scorer and prompt versions at start
  budget jsonb not null,                         -- {"ai_tokens": 2500000, "pages": 100}
  started_at timestamptz, last_seen_at timestamptz,   -- touched by every step and Ghost heartbeat
  finished_at timestamptz, summary jsonb,        -- summary = the Wrap
  check (status not in ('succeeded','failed','cancelled','skipped') or summary is not null)   -- every run ends in a Wrap
);
-- One live or queued run per preset; one heavy run holding the laptop at a time (§8.1).
create unique index runs_one_per_preset on runs (preset)
  where status in ('queued','running','waiting_ai','waiting_approval','pausing','paused');
create unique index runs_one_heavy on runs ((true))
  where heavy and status in ('running','waiting_ai','pausing');   -- a pausing run keeps the slot until its workers stop
create table run_steps (
  run_id uuid references runs, seq smallint, step text not null,
  kind text not null,       -- 'code' | 'ai_batch' | 'draft' | 'form' | 'approval'
  status text not null,     -- 'pending' | 'running' | 'done' | 'skipped' | 'failed'
  input_hash bytea, output jsonb, started_at timestamptz, finished_at timestamptz,
  primary key (run_id, seq)
);
create table approvals (
  run_id uuid, gate text, round smallint not null default 1,   -- a 'changes' answer opens a new round
  decision text not null,   -- 'approved' | 'rejected' | 'changes'
  note text, decided_at timestamptz not null,
  subject_sha bytea not null,   -- hash of exactly what was shown at the stop (plan, curation draft, diff)
  origin text not null check (origin = 'board'),   -- received over the Hotline from Board only (§9.3)
  consumed_at timestamptz,      -- set when the step this approval covers is done; then it authorises nothing
  primary key (run_id, gate, round)
);
```

The first design's `start_tickets` table is dropped: a run can start only through the Hotline or from a schedule, so there is nothing to ticket.

Each preset is declared in Rust as an ordered list of steps, together with its arguments, heavy flag and gates (§8.1). Chief advances a run step by step:

1. **Code steps** enqueue the step's jobs, which the service's workers drain within the pool limits; Chief records the step output and moves on.
2. **AI batch steps** enqueue `ai` jobs. Ghost, in the same service, claims them, calls `claude -p`, stores outputs through `news ai accept` and heartbeats `runs.last_seen_at`. There is no time slice: the batch runs until it is done, paused or cancelled.
3. **Draft steps** are an AI batch of one: Frontpage's curator (A8) and Rollcall's proposals (A11) run as `claude -p` through Ghost and write a draft, validated against the step's schema. Board shows the draft at the approval stop that follows.
4. **Form steps** wait for Karthik to fill a form in Board, with no AI: Huddle's pairwise questions (§8.12). The answers arrive over the Hotline from Board (`huddle.answer`) and are stored with `news scores label`.
5. **Approval steps** set the run to `waiting_approval`, store the summary and `subject_sha`, and send a `gate` event, so Board raises a toast. Only an approval received over the Hotline from Board (`origin` `board`) records the decision, bound to `subject_sha`. The step behind the gate runs only under the G-tier rule below, so the gate is enforced by code, and an approval cannot be reused for a changed draft or a later step.
6. At the end Chief writes the Wrap.

Every step is idempotent: after a crash or a service restart, Chief finishes the current step and carries on. A heavy run moving from `waiting_approval` or `paused` back to `running` needs the heavy slot; if another heavy run holds it, the run joins the queue (below) with its gate marked approved.

**The Lineup** (supersedes D16; ADR 0007). A heavy run that cannot get the slot is not refused:

- A **scheduled** heavy run is inserted as `queued` with `start_by` = its scheduled time plus the late limit (setting `queue_late_limit`, starting at 2 hours). If the slot is still busy at `start_by`, the run ends as `skipped` with a Wrap that says why, and Board notes it. `start_by` applies only to a run's first start: Chief clears it when the run starts, so a run that has already started and later rejoins the Lineup (after a gate, a pause or a resume) is never skipped for being late.
- A **manual** Start while the slot is busy returns `queue_offer`. Board offers "Queue it" (the run is inserted as `queued` with no `start_by`, and waits until it starts or Karthik cancels it) or "Pause the running one and start this" (the running heavy run stops at its next step boundary as `paused`, reason `karthik`, and the new run takes the slot; the paused run stays on the board with Resume).
- A gated run whose gate is approved while the slot is busy, and a Resume while the slot is busy, join the queue the same way.
- When the slot frees, the oldest queued run whose `start_by` is unset or has not passed starts. Board lists the Lineup and can cancel entries (`lineup.cancel`). Quiet hours and the quota guard also hold queued runs (§5.6, §7.4).
- A second run of a preset that is already live or queued is not queued: Board shows the live run with Resume or Cancel, and a schedule that fires for it is skipped and noted.

**G-tier authorisation** (one rule for every G-tier command: `news publish`, `news home publish`, `news eval promote`, `news corrections resolve --action update`, and `news scores adopt`, `news sources enable|disable` and `news jobs retry` above 50 when they run as a step). Chief runs the command only when all three hold, read from PostgreSQL and never from environment variables such as `NEWS_RUN_ID`:

1. The run is in status `running`, and the command belongs to its current `running` step.
2. The item it acts on belongs to what was started or approved: the run's Budget (its planned items) for an ungated step; for a gated step, the plan, draft or diff whose hash is the approval's `subject_sha` (a plan hash covers its story ids).
3. The step is authorised either by an approval row for the gate that covers it (`decision = 'approved'`, matching `subject_sha`, `origin` `board`, `consumed_at` null), or, for a step the preset declares ungated, by the run's start: a Start from Board, or a schedule Karthik set in Board, which counts as his approval of the plan the run makes (P8).

An approval covers one step: every item that step publishes is checked against it, each outbox row is inserted in the same transaction as its check, and `consumed_at` is set in the transaction that marks the step done. Called any other way, a G-tier command returns `gated` (exit 7). The presets table in §8.1 lists which authorisation covers each preset's G-tier step.

### 5.6 Scheduling

The Bureau service is always on, so all scheduling happens inside it (ADR 0007). Task Scheduler and `news work` are retired.

| What | Mechanism |
| --- | --- |
| Preset runs (AI) | Started from Board, or by a **preset schedule** Karthik set in Board, which counts as his approval to start (P8). A schedule names the preset, its arguments (for Edition, `review_plan` on or off) and its times. Schedules are settings (versioned, §8.1); each run records the schedule and the settings version that started it. Default schedule times are Karthik's to set later. |
| Background code jobs | The service's Dispatch workers run pools `net`, `cpu` and `docling` all the time. `ai`, `render` and `publish` jobs exist only inside a live run, and only Press reads the write-API key, inside an authorised step (§9.3). |
| Recurring code jobs | `schedules` table (`kind`, cron expression in UTC, `next_at`), evaluated every minute by the service under an advisory lock: `calendar.sync` 06:00 UTC daily, source discovery every 30–120 min per source, `work.prune` daily. Index promotion is not scheduled: it runs only as Edition step 11. |
| Calendar-driven jobs | Pre-warm and capture jobs created by `calendar.sync` with `run_after` set (§3.3). |
| Missed preset schedules | Laptop asleep or off at a scheduled time: on wake, one run for the latest missed slot only, and none if the next slot is less than the missed-slot limit away (setting `missed_slot_limit`, starting at 2 hours). Older missed slots are noted, not run. |
| Missed code jobs | Each recurring job keeps only its latest due run (no catch-up storms). |
| Heavy slot busy | Scheduled heavy runs queue for the slot (§5.5). |
| Quiet hours | Hours Karthik sets in Board, with one of two modes he chooses. **Don't start:** a heavy run due in quiet hours does not start; it waits in the Lineup until they end, and if that is later than its `start_by` (the late limit, §5.5) it is skipped and noted. **Reduced parallelism:** a heavy run due in quiet hours starts on time, with the `ai` pool at the reduced size he sets, and the late limit applies as usual. A heavy run already going when quiet hours begin carries on under the same mode's pool size. Light runs and background fetching are not affected. The times, the mode and the reduced size are his settings, set later. |
| Quota guard | No new run starts when the Bureau token cap or the reserve would be crossed (§7.4); a scheduled run held by it is skipped and noted. |
| Weekly presets | Rollcall runs on its schedule and waits at its `actions` gate in Board. Huddle's Debrief and questions are prepared on its schedule, and a toast reminds Karthik to answer them in Board (§8.7, §8.12). |

### 5.7 Concurrency limits on a 4-core laptop

| Pool | `max_running` | Why |
| --- | --- | --- |
| `net` | 6 (plus per-host buckets) | Network-bound; cheap on CPU |
| `cpu` (parse, XBRL, embed, score, brief) | 2 | Leaves cores for Claude Code processes, Docker and the renderer |
| `docling` | 1 | Heavy; one at a time (Development Plan) |
| `render` | 2 | A warm Bun render server on localhost serves both slots, avoiding start-up cost per page |
| `ai` | `NEWS_PARALLEL` (3) | Plan limits and laptop memory; the laptop mostly waits on the network |
| `publish` | 1 | Paced by the write-API token bucket |

Heavy development jobs (Playwright matrices, release builds) should not overlap a run; the preflight of every heavy preset warns, without blocking, if one is running (§8.2).

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
| Service or Ghost crash | Leases expire; the rescuer makes jobs `retryable`; Windows restarts the service, and Chief continues every live run from its last finished step |
| Service stops while a run is `pausing` | On start, before any claim, Chief completes the pause; it never reverses it, because Karthik asked for it. The old service's worker tasks ended with its process; Chief terminates the run's named Job Objects (AI and child processes) if any outlived it and waits until they are empty; a publish that was in flight becomes `sent_unknown` and is reconciled on resume (§6.4); then, in one transaction, it sets the run `paused` and moves its `running` jobs to `retryable` with no holder. Only then is the heavy slot free |
| Laptop sleeps mid-run | Same as a crash; leases expire on wake |
| PostgreSQL restart | Transactions roll back; jobs stay in their last committed state |
| Plan usage limit | `-p` calls fail rather than wait [64]. Ghost pauses pool `ai` until the reset time the error gives and sets the run `paused` (reason `usage_limit`); Board raises a toast. At the reset time the service unpauses the pool and resumes the run on its own. The error's shape and reset time are confirmed in the service spike (ADR 0007) |
| Service update | Started from Board only (`service.update`, H tier); waits for a safe point (no AI call or publish in flight), swaps the binary and restarts; live runs resume from PostgreSQL |
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
| Use from code | Same binary as the Bureau service and Board; used by tests, and by Karthik's terminal for status and debugging | Needs an MCP client |
| Testable without AI | Yes: golden JSON tests per command | Yes, but needs a harness |
| Page calls | Not needed at all (Reporter has no tools) | Not needed |
| Fits TD | Yes (TD "Agent toolbox") | New component |

**Recommendation (D3): CLI first, for everything.** Each command can print its input and output JSON Schemas (`news schema <command>`), so a thin read-only MCP adapter (`news mcp`, stdio) can be generated later if it would help, without a second implementation. Page-writing and drafting calls use no tools at all; if a drafting step turns out to need one, it gets only the read-only commands it needs. Claude Code permission rules match command text, so a permission rule is a guardrail, not a security boundary [66]; the enforcement is in code (the Hotline and the spawn marker, the G-tier rule and approvals bound to `subject_sha`, §5.5, §9.3) and in the secrets split (§9.3).

### 6.2 Conventions for every command

- **Envelope.** stdout carries exactly one JSON object; logs go to stderr and to telemetry.

  ```json
  {"ok": true, "data": {}, "warnings": [], "truncated": false}
  {"ok": false, "error": {"code": "not_found", "message": "story 81234 not found", "retryable": false, "hint": "news stories similar --text …"}}
  ```

  `error.details` (optional) carries structured context, for example the run that blocks a start (§8.1).
- **Code-rendered text.** Any command whose result is shown to Karthik (the status board, run details, approval stops, queue offers, the Wrap) also returns `text`, in `data` or `error`, rendered by code. Board and the terminal show it verbatim, so no count, time or figure on Karthik's screen is ever typed by a model. The Hotline carries the same envelope (ADR 0007).
- **Exit codes:** 0 ok · 2 usage · 3 not found · 4 conflict or precondition failed · 5 retryable (network, lock, busy) · 6 validation failed · 7 gated (approval required) · 10 internal bug.
- **Error codes** (stable): `usage`, `not_found`, `conflict`, `precondition`, `gated`, `validation_failed`, `rate_limited`, `upstream_unavailable`, `licence_blocked`, `budget_exceeded`, `internal`.
- **Size discipline.** Default output ≤ ~2k tokens; `--limit`, `--fields`, `--cursor` for more; `"truncated": true` when cut. Long text never returned inline unless asked for.
- **Determinism.** Same input and database state → same output (stable ordering), so commands have golden-output tests.
- **Every call** emits an event with `run_id` and `job_id` taken from `NEWS_RUN_ID` / `NEWS_JOB_ID` env vars.

### 6.3 Permission tiers

| Tier | Meaning | Who |
| --- | --- | --- |
| R | Read-only | Board; Karthik's terminal (in-process, or over the Hotline for status); any `claude -p` call given tools |
| W | Writes run-scoped state (drafts, AI outputs, job outcomes) | Chief and Ghost inside the service |
| G | Gated: runs only as the current step of a live run, authorised by that step's approval or, for a step the preset declares ungated, by the run's start from Board or Karthik's schedule (§5.5) | Chief inside a run step; never called directly by agents |
| H | Human only: starts, gate answers, pauses, resumes, cancels, Lineup and Standing changes, and the maintenance actions below | Karthik, **from Board only**, over the Hotline. Not from his terminal, a script or a development Claude Code session, and never from a process the service spawned for `claude -p` (§9.3, ADR 0007). The CLI has no H-tier form: each H entry in §6.4 is a Board action, and the matching command runs only as Chief's G-tier step where one is listed |

### 6.4 Catalogue

Existing TD commands keep their names; each new command group is a tool family named after the part of Bureau that owns it (`news run` = Chief tools, `news jobs` = Dispatch tools, and so on; full list in [§15](#15-names)).

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
| `news publish <story>` | — | `{url, version}` | `gated` unless authorised (§5.5), `rate_limited` | G |
| `news update <page>` | — | `{page, changed_refs}` | `not_found` | R |
| `news prune` / `news stats` | — | counts / short table | — | W / R |
| `news home candidates` | — | ≤ 40 candidates (§4.5) | — | R |
| `news home save <file>` / `check <draft>` / `preview <draft>` | curation JSON | `{draft_id}` / `{pass, failures}` / `{url}` | `validation_failed` | W |
| `news home publish <draft>` | — | `{version}` | `gated` unless authorised by the `publish` approval (§5.5) | G |
| `news home rollback <v>` | — | `{version}` | `not_found` | H (Board only) |
| `news home show` | — | live curation and age | — | R |

**Chief tools** (`news run …`, Chief). H-tier actions are Board actions sent over the Hotline; the rows marked H below describe what Board's action does and have no CLI form. Read commands (R) run in the calling process, or over the Hotline for status.

| Command | Input | Output | Errors | Tier |
| --- | --- | --- | --- | --- |
| `news run presets` | — | the preset catalogue from the Rust declarations: id, name, arguments, heavy, writes, publishes, gates | — | R |
| Start (Board: `run.start`, `run.start_after_pause`) | preset id, args | `{run_id, short_id, steps:[…], status}`; `runs.origin` is `board`. When the heavy slot is busy: `queue_offer` with code-rendered `text`; Board then queues the run or pauses the running heavy run first (§5.5). No CLI form | `conflict` (reason `preset_live`: this preset already has a live or queued run), `precondition` (preflight, quiet hours, quota guard), `gated` (sent from a spawned process) | H |
| `news run status [<run>]` | — | no run: the status board (§8.1); with a run: step table, counts, budget used, open gate with its summary, Wrap if finished | `not_found` | R |
| Approve / Request changes / Reject (Board: `gate.approve\|changes\|reject`) | run, gate, round, the `subject_sha` Board showed; a note for changes | `{decision, round, subject_sha}` | `precondition` (gate not waiting), `gated` (hash differs, or not sent by Board) | H |
| Resume (Board: `run.resume`) | run | `{status}`; joins the Lineup when the heavy slot is busy. A stalled run is still `running`, so it is paused first (it pauses at once), then resumed | `precondition` (not paused or waiting) | H |
| Pause (Board: `run.pause`) | run | `{pausing, after_step}`; the run stops at the next step boundary and frees the heavy slot. On a stalled run (`last_seen_at` more than 15 minutes old) it takes effect at once, and nothing of the run keeps working once the slot is free: (0) in its own transaction Chief first sets the run `pausing`, so from that moment the claim query (§5.4) starts none of its jobs, and a claim racing the pause either committed before it (and is cancelled in step 2) or finds nothing; (1) Chief terminates the run's AI Job Object, ending any `claude -p` still going; (2) it cancels the run's in-flight jobs in every pool (`cpu`, `docling`, `render`, `publish` as well as `ai`): the workers are tasks inside the service and any child process they start (Docling, the renderer call) runs in a per-run Job Object, so Chief signals the tasks, terminates that Job Object and waits until every task has stopped; the heavy slot stays held, with the run shown as `pausing` in Board, for as long as any task has not stopped, so no other run can start beside it; (3) it waits for any publish request already sent to the write API to return or time out (the same "no publish in flight" rule as an update, §5.9), and Press sends none of the run's other outbox rows until it resumes. A timeout does not prove the write API stopped: that one page may still go live after the slot is freed. This is the weaker guarantee, and it is acceptable because the page was already planned and passed Proof; it cannot publish anything else, and it is reconciled, not repeated: the outbox row stays `sent_unknown`, and on resume Press re-sends it with the same idempotency key (§3.12), which returns the stored result if it committed or completes it if it did not, and only then marks the row and updates the run's records; (4) in one transaction it sets the run `paused` and moves all its `running` jobs, in every pool, to `retryable` with a new attempt and no holder. Because the claim query skips jobs whose run is not `running` or `waiting_ai` (§5.4), none of them can be claimed again until the run resumes. Only then is the slot free. A worker that still reports afterwards (a race with step 4) finds it no longer holds its job: its heartbeat, completion or failure is rejected (`conflict`, the holder check of ADR 0003), so its result is never recorded, and any write it made before that is keyed and idempotent (§5.8), so the re-run after resume is safe | `precondition` (not running) | H |
| Cancel (Board: `run.cancel`) | run | `{cancelled, wrap}`. Before the run ends, Press reconciles any `sent_unknown` outbox row by re-sending it with its idempotency key (a stored result is returned, or the publish completes), so the Wrap lists every page that actually went live; if the write API cannot be reached, the row is recorded as unresolved in the Wrap and in Board, and Followup reconciles it on its next run. Unsent rows are dropped | | H |
| `news run lineup` | — | the Lineup in order, with `start_by` (cancelling an entry is a Board action, `lineup.cancel`) | — | R |
| `news run list [--active]` | — | recent runs | | R |

The first design's `news run intent`, `next`, `submit`, `hook-prompt` and `ticket` are retired (ADR 0007): Chief advances runs inside the service, and drafts arrive through `news ai accept`.

**Service and Board** (`news service …`, `news board`; ADR 0007)

| Command | Does | Tier |
| --- | --- | --- |
| `news service run` | The Bureau service itself; Windows starts it. In a terminal it runs in the foreground, for development and tests | — |
| `news service install` / `uninstall` | Registers or removes the Windows service under Karthik's account; Windows asks for his password. Run by Karthik in his own terminal, because they come before the service exists; Windows administration, not a Hotline action | Karthik only |
| Update (Board: `service.update`) | Waits for a safe point, swaps the binary, restarts; live runs resume | H |
| `news board [--open <kind>:<id>]` | Opens Board, optionally on one run, gate or source (used by toasts) | — |
| `news standing show [<key>]` / `history [<key>]` | Settings and schedules from Standing, the versioned settings table (§8.1). Changing them is a Board action (`standing.set`), which writes one new version | R |

**Dispatch tools** (`news jobs …`, the queue)

| Command | Output | Tier |
| --- | --- | --- |
| `news jobs list [--state s] [--pool p] [--run r]` | counts and ≤ 20 jobs | R |
| `news jobs dead [--class c]` | discarded jobs grouped by `error_class` and message fingerprint | R |
| `news jobs claim --pool ai --limit n --worker w` | jobs with args and input file paths | W (Ghost only) |
| `news jobs heartbeat <job>` / `complete <job> --result f` / `fail <job> --class c --message m` | `{ok}` | W (Ghost only) |
| `news jobs retry <job \| --class c>` | `{requeued}` | W in Salvage up to 50 at once; above 50, G as Salvage's step after its `requeue` approval; H from Board only |
| `news jobs pause <pool> --until t` / `unpause <pool>` | `{paused_until}` | H |

**Ghost tools** (`news ai …`, contracts for every AI step)

| Command | Input | Output | Tier |
| --- | --- | --- | --- |
| `news ai input <job>` | — | path of the step's input file (brief, pairs, text) | W |
| `news ai accept <job> <output.json>` | model output | `{valid, errors:[{rule, where, message}], stored}` — runs schema + semantic validators | W |
| `news ai schema <step>` | — | JSON Schema of the step's output | R |
| `news ai fallback <job>` | — | applies the step's code fallback | W |

**Contacts tools** (`news sources …`): `list`, `show <source>`, `health [--since 7d]` (green/amber/red with reasons, 304 ratio, error classes, last success, quota), `probe <source>` (one live discovery call with no writes; R), `enable|disable <source>` (G as Rollcall's step after its `actions` approval; H from Board only).

**Diary tools** (`news calendar …`): `upcoming [--days 7] [--region r]` (R), `sync` (W, code step), `late` (R).

**Taster tools** (`news stories …`): `show <story>`, `members <story>`, `similar --text t | --cand c` (R); `merge <a> <b>`, `split <story> <cands…>` (H).

**Nose tools** (`news scores …`): `explain <story>` (the Pitch; R), `weights` (current config; R), `propose --from-labels` (fits new weights, prints a diff; W), `label --run <run> <file>` (records Karthik's pairwise answers from Huddle's Board form; H, from Board over the Hotline), `adopt <version>` (G as Huddle's step after its `change:<n>` approval; H from Board only).

**Roster tools** (`news entities …`): `resolve <text> [--kind k]` (R), `show <entity>` (R), `unresolved` (R), `alias add|merge` (H).

**Critic tools** (`news eval …`): `run <suite> [--prompt-version v]` (W, spends tokens only for AI suites, under the run budget), `report <eval_run>` (R), `label <set>` (interactive labelling helper; H), `promote <step> <prompt_version>` (G).

**Huddle tools** (`news review …`): `weekly` (the Debrief: coverage vs targets, top rejected stories, failures, tokens per page, index budget, curation age; R).

**Errata tools** (`news corrections …`): `pull` (fetches new reader error reports from the cloud; W), `show <report>` (R), `resolve <report> --action update|dismiss` (G for update, W for dismiss).

**`news schema <command>`** prints input and output schemas for any command (R).

---

## 7. AI usage ledger

Every place a model is called. Each is a contract (P2): code builds the input, a JSON Schema bounds the output, a code validator decides acceptance, a fallback needs no AI, and a budget caps it. All calls go through Ghost with `--json-schema`, `--tools ""`, an empty strict MCP config and `--output-format json` (§3.9), including the drafting steps A8 and A11. There is no interactive session (ADR 0007).

Models: `NEWS_MODEL_MAIN` = Sonnet 5.5, `NEWS_MODEL_LIGHT` = Haiku 5.5, `NEWS_MODEL_CURATOR` = Sonnet 5.5. Models are per-step settings in Board (§8.1), starting at these values. Opus 5.5 is never called at runtime (development only, as the TD says).

### 7.1 The ledger

| ID | Step | Model | Input (built by code) | Output schema (sketch) | Validation (code) | Fallback (no AI) | Token budget | Volume per run *(speculative)* |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **A1** | Write page (new, or update after a direction flip) | Sonnet | Brief (§3.8); for updates also the old page and the data diff | `page-draft.v1` (§3.9) | Schema, OpenUI parse, refs, C4 numeric lint, C5 direction, banned words, props, lengths | Tier 2: templated page (D10); Tier 1: re-queue | ≤ 2 turns; ≤ 4k output; `--max-budget-usd 0.60` | ~100 |
| **A2** | Repair a Copy | Sonnet | Previous Copy + validator or check errors + brief | `page-draft.v1` | Same as A1 | Re-queue for next run | 1 call per item; ≤ 2 turns | ~5–15 |
| **A3** | Classify an ambiguous item (page type, categories, region) | Haiku | Title, first 300 words (fenced), source, rule candidates, allowed label sets | `{"page_type": enum, "categories": [enum ≤ 3], "region": enum, "confidence": "high"\|"medium"\|"low"}` | Enums from the taxonomy table; must include the rule's top candidate unless `confidence = high` | Rule's best guess, flagged `page_type_by = rule-unsure` | Batched 20 items/call; 1 turn | 0–3 calls |
| **A4** | Same story? (grey zone) | Haiku | Up to 20 pairs: titles, first 150 words, shared entities, dates | `{"pairs": [{"a": id, "b": id, "same": bool, "basis": enum["same_event","same_entity_different_event","different"]}]}` | IDs must match input; one answer per pair | Keep separate | Batched; 1 turn | 0–5 calls |
| **A5** | Condense long source text for a brief | Haiku | Text blocks with IDs (fenced, untrusted) + page type + what the brief needs | `{"points": [{"text": str ≤ 200, "support": [block_id]}], "quotes": [{"block": id, "start": int, "end": int}]}` | No digits in `text`; every `support` and quote offset exists; quotes are spans of real blocks; ≤ 12 points | Code extract: first paragraph + headings + sentences with entity mentions | 1 turn; ≤ 1.5k output | ~10–20 |
| **A6** | Pick an entity from a shortlist (optional, off by default) | Haiku | Mention + context sentence + ≤ 5 candidate entities from code | `{"choice": entity_id \| "none"}` | Choice ∈ shortlist ∪ {none} | Leave unresolved | Batched 30/call | 0–2 calls |
| **A7** | Sample spot check | Haiku | Page Markdown + brief | `{"verdict": "pass"\|"flag", "issues": [{"kind": enum["unsupported_claim","wrong_direction","missing_context","tone","attribution","other"], "where": str ≤ 120, "evidence": [ref or block_id], "severity": "low"\|"high"}]}` | Kinds from enum; `where` must be a substring of the page; evidence IDs exist | Skip sample (logged) | 1 turn | ~5 (+ all Tier 1 in month 1) |
| **A8** | Home curation (Frontpage) | Sonnet (curator) | ≤ 40 candidates from `news home candidates`, built by code; on a fix round also the check errors, on a changes round also Karthik's note | TD curation JSON (lead, sides, sections, figure IDs, rationale) | `news home check` (TD rules) | Keep the live curation | ≤ 2 fix rounds (code calls it again with the errors); no tools | Every 2–3 days |
| **A9** | Triage a reader error report | Haiku | Report text (fenced, untrusted) + page refs and labels | `{"kind": enum["figure","wording","link","image","other","spam"], "ref": ref\|null, "claim": str ≤ 200}` | `ref` must belong to the page | Manual triage in Errata | Batched 20/call | Weekly |
| **A10** | Alt text when the image source gives none (optional) | Haiku | Image caption, title, credit, page context (text only; no image download) | `{"alt": str 20–150}` | No digits unless in the caption; length | Neutral alt from caption or title | Batched | Rare |
| **A11** | Rollcall proposals (draft) | Sonnet | Built by code: source health, probe results, error classes, `late` calendar events, parse-failure rates, failing fixture paths | `{"proposals": [{"source_id": id, "action": enum["disable","retry_later","dev_task"], "reason": str ≤ 200, "evidence": [str]}]}` | Source ids exist and are red or amber; actions from the enum; no digits in `reason` | A Sitrep with no proposals; Karthik decides in Board | 1 turn; no tools | Weekly |
| **A12** | Eval judge (only in the Audit workflow) | Sonnet judging Haiku steps; Haiku + Sonnet cross-check for A1 prose | Output + reference + one rubric question | `{"pass": bool, "reason_code": enum}` (one judge per failure mode) | Judges themselves validated against Karthik's labels (TPR/TNR) [61] | Deterministic metrics only | Per eval budget | Weekly or per prompt change |

**Removed from the TD's model roles:** "Sonnet: selection judgement where code scoring is unsure". Selection is fully code (§4); disagreements are handled by Karthik's weekly pairwise labels, not by a per-run model call. **Narrowed:** Haiku "summarising long parsed text" becomes A5, extractive and digit-free with verifiable quote spans.

### 7.2 Token budget per run *(speculative; replaced by telemetry after the first runs)*

| Step | Calls | Input per call (mostly cached prefix) | Output per call | Total input | Total output |
| --- | --- | --- | --- | --- | --- |
| A1 | 100 | ~11k (8k cached + 3k brief) | ~3k | ~1.1M (≈ 0.8M cache reads) | ~300k |
| A2 | 10 | ~14k | ~3k | ~140k | ~30k |
| A3–A6 | ~25 | ~4k | ~1k | ~100k | ~25k |
| A7 | 5 | ~8k | ~0.5k | ~40k | ~3k |
| A8, A11 | 0–1 each | ~10k | ~2k | ~10k | ~2k |

`NEWS_AI_RUN_TOKENS` (new; default 2.5M input-equivalent) stops new AI jobs when reached; the run finishes code work and reports what was left.

### 7.3 Deliberately not done with AI

| Task | Done by | Why not AI |
| --- | --- | --- |
| Finding new items, fetching, change detection | Stringers, Runner, Diary | Deterministic, cheap, auditable |
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
| Weekly statistics, the Debrief and Huddle's questions | SQL; a form in Board | Board shows what code computed; no model |

### 7.4 Quota guard

Bureau shares Karthik's Claude plan with his development work, and scheduled runs start without him, so the service keeps Bureau inside limits he sets (ADR 0007):

- **Bureau token cap.** A daily and a weekly cap on Bureau's own tokens, counted from `ai_calls`. A run that would start over the cap does not start; a running run stops claiming new AI jobs at the cap, finishes its code work and says so in its Wrap, as `NEWS_AI_RUN_TOKENS` does per run.
- **Reserve.** Near the plan limit, no new run starts, and running runs stop claiming new AI jobs (Ghost checks the reserve before every claim, as it checks the cap); they finish their code work and in-flight calls, wait in `paused` (reason `quota`), and resume on their own when usage falls back under the reserve. So Karthik keeps quota for development. The plan limit is not published, so "near" is the reserve Karthik sets. Total usage comes from one source only: Claude Code's own OpenTelemetry metrics (`claude_code.token.usage` [69]) for every session that exports to the collector. Bureau's `claude -p` calls export there too (Ghost tags them with `news.run_id`), so they are already in the total; `ai_calls` is used for Bureau's own share and the cap, and is never added to the total. A session that does not export is not seen.
- **Usage limit reached anyway.** The run pauses, Board raises a toast, and the run resumes on its own when the limit resets (§5.9).
- **In Board.** Settings & schedules holds the caps and the reserve; the status board shows Bureau usage against the total for the day and the week.

The cap and reserve values are Karthik's settings, set later in Board; this document does not choose them.

---

## 8. Board, the Bureau service and the preset workflows

### 8.1 Board and the Bureau service

**Requirement (Karthik, 16 Nov 2026).** One compiled Rust desktop app is the main control surface for the whole project: orchestration, analytics and project settings, backed by an always-on Windows service. `claude -p` stays the AI harness. This replaces the first design's `/bureau` Claude Code skill and the planned Bun web dashboard.

**Decision.** [ADR 0007](docs/adr/0007-bureau-service-and-board.md) (D17–D21 in §11). In short:

- **The Bureau service** (`news service run`) is a Windows service that logs on as Karthik's account, because `claude -p` uses his Claude login and Docker runs under his user. It hosts Chief, Rota, the Dispatch workers for every pool and Ghost (Rust). It calls Plate, the warm Bun render server on 127.0.0.1. Runs continue with Board closed.
- **Board** (`news board`) is a pure-Rust egui/eframe desktop app in the same `news.exe`. It talks only to the service. It opens minimised at log-on. If the service is down it shows an offline banner, a "Start service" button and the last error. It follows the Windows light or dark setting, with a manual override, and its themes are generated from `design/tokens/tokens.json` (the `news-design-system` skill applies).
- **Control** goes only over a named pipe locked to Karthik's user and to local clients. Only Board may send H-tier actions; Karthik's terminal (the `news` CLI) may read status and use the low-risk tiers; any process the service spawned for `claude -p` is refused (§9.3).
- **Laptop only.** Nothing is exposed to the network and there is no remote access.

#### Board's screens, in build order

Control comes first; later screens land in Board as their features are built.

| Step | Screen | Shows and does |
| --- | --- | --- |
| 1 | **Runs & approvals** | The status board (below): running, waiting, queued, due, problems, next. Start a preset (a form with its arguments), pause, resume, cancel; answer gates (Approve, Request changes with a note, Reject) with the plan, draft or preview in view; run detail and the Wrap; the Lineup |
| 1 | **Settings & schedules** | Preset schedules; targets (`NEWS_TARGET_PAGES` and the other settings in TD "Configuration"); parallelism; budgets; the Bureau token cap and reserve (§7.4); quiet hours (§5.6); models per step; sources on and off. Every change is a new settings version with who, when, and old and new value. Secrets are entered here and stored in Windows Credential Manager |
| 1 | **Docker health** | Health of the News containers (PostgreSQL, the OTel collector) with Start and Stop; the shared ClickHouse is health-checked only, never stopped |
| 2 | **Service log and AI calls** | A live service log, and one record per `claude -p` call: step, model, tokens, time, result, error, with its brief and output viewable (from `ai_calls`, §9.2) |
| 3 | **Content & sources** | Stories and their scores (the Pitch), pages, source health, the Diary, dead jobs with retry |
| 4 | **Analytics** | The panels of §9.2 and TD "Telemetry", read from ClickHouse and drawn with simple egui_plot charts styled by the tokens |
| 5 | **Whole project** | Published-content actions: withdraw or correct a page, roll back the home curation, Errata reports. Cloud free-tier usage and alarms (Cloudflare Workers, R2, D1; DynamoDB). Deploy buttons that trigger the existing GitHub Actions deploy workflow for `main`, with its run history (no local wrangler deploys). Development status: PRs, CI and issues, read-only |

Step 5 reads cloud and GitHub data with tokens Karthik creates (Cloudflare analytics, read-only; AWS read-only for DynamoDB and CloudWatch metrics; a GitHub fine-grained token for this repository with read access to pull requests, issues and checks, and Actions read and write, which GitHub requires to dispatch a workflow. Karthik approved Board running the deploy workflow. Actions write also allows other Actions operations, such as re-running or cancelling runs; Board itself only dispatches the deploy workflow on `main`); he enters them in Board, which stores them in Credential Manager.

#### Presets

Declared in Rust; `news run presets` prints this table as JSON, and Board builds its Start forms and schedule forms from it. Any preset can be started from Board or put on a schedule.

| Preset (id) | Arguments | Heavy | Writes or publishes | Approval gates | G-tier step authorised by (§5.5) | § |
| --- | --- | --- | --- | --- | --- | --- |
| Edition (`edition`) | `target`, `categories`, `review_plan` (per schedule) | yes | publishes pages | `plan`, only with `review_plan` on | the run's start; the `plan` approval with `review_plan` on | 8.3 |
| Flash (`flash`) | event keys or story ids | yes | publishes pages | none | the run's start | 8.4 |
| Catchup (`catchup`) | `from`, `to`, `page_types`, `sources`, `chunk` | yes | publishes pages | `plan` | the `plan` approval | 8.5 |
| Frontpage (`frontpage`) | — | no | publishes the home curation | `publish` | the `publish` approval | 8.6 |
| Rollcall (`rollcall`) | `source` | no | source state, development-task drafts | `actions` | the `actions` approval | 8.7 |
| Salvage (`salvage`) | `class`, `since` | yes | re-queues jobs, publishes pages | `reexecute`; `requeue` above 50 jobs | the run's start for code retries (≤ 50 jobs); the `reexecute` and `requeue` approvals | 8.8 |
| Followup (`followup`) | `source` | yes | publishes page updates | `bulk` above 100 pages | the run's start; the `bulk` approval above 100 pages | 8.9 |
| Errata (`errata`) | — | no | publishes corrected pages | one per report (`report:<id>`) | that report's approval | 8.10 |
| Audit (`audit`) | `suite`, `prompt_version` | yes | eval results; promotes a prompt | `promote` | the `promote` approval | 8.11 |
| Huddle (`huddle`) | — | no | new config versions | one per proposed change (`change:<n>`) | that change's approval | 8.12 |
| Reprint (`reprint`) | `renderer`, `page_types`, `limit` | yes | publishes re-rendered pages | `sample` | the `sample` approval | 8.13 |

A preset is **heavy** when it writes or re-renders pages in bulk: it fills the `ai` pool with page-writing jobs or the `render` and `publish` pools with many pages. Only one heavy run holds the laptop at a time; others queue (§5.5). The four light presets may run beside a heavy run, sharing the pool limits of §5.7.

**Scheduled runs** (P8). A schedule Karthik sets counts as his approval to start. A scheduled Edition publishes the pages that pass Proof without stopping, unless its schedule switches `review_plan` on. The gated presets (Catchup's plan, Reprint's sample, Audit's promote, Salvage's re-execute, Followup's bulk, Errata, Huddle's changes) still stop at their gates and wait in Board, scheduled or not.

#### The status board

The top of Runs & approvals. `news run status` builds it from PostgreSQL with no network calls and returns JSON plus code-rendered `text`; Board shows the same content as panels, the terminal prints the text.

| Section | Shows | Read from |
| --- | --- | --- |
| Running | Each live run: preset, id, step *n* of *m*, progress inside the step, start time, origin (Board or schedule); `stalled` if `last_seen_at` is more than 15 minutes old | `runs`, `run_steps`, `jobs` |
| Waiting for you | Open gates with their one-line summary and the Approve, Request changes and Reject actions; Huddle questions to answer; paused runs and why (usage limit and its reset time, quiet hours, quota, paused by Karthik) | `runs`, `run_steps`, `approvals` |
| Queued | Heavy runs waiting for the slot, in order, with `start_by` for scheduled ones | `runs` |
| Due | The next preset schedules; Diary releases today (expected, captured or late) and captured stories not yet on a page; Frontpage: age of the live curation, due from day 2 (the 2–3-day cadence) and stale at `NEWS_HOME_STALE_DAYS` (TD, default 4); Huddle and Rollcall: due 7 days after the last one | settings, `calendar_events`, `stories`, `scores`, Press's record of the last published curation, `runs` |
| Problems | Red and amber sources with reasons and the latest Sitrep; dead jobs by class since the last Salvage; runs failed or skipped in the last 7 days; refused control attempts from spawned processes; Bureau usage against the cap and reserve | `sources`, `jobs`, `runs`, service log, `ai_calls` |
| Next | One suggested action, from the first rule below that matches | Code |

**Next-action rules**, first match wins *(thresholds are starting values, kept as data; speculative)*:

1. A heavy run is stalled → Pause it (a stalled run pauses at once), then Resume it.
2. Tier 1 stories are captured but not on a page, the heavy slot is free and no Edition is scheduled within the hour → Start Edition (or Flash with their keys when there are at most 3 and an Edition finished in the last 3 hours).
3. A gate is open → open that run, so Karthik reads the summary before answering.
4. A source that today's Diary needs is red → Start Rollcall for that source.
5. The live curation is stale → Start Frontpage.
6. At least 20 stories with S ≥ 40 are waiting, the last Edition finished 6 or more hours ago and none is scheduled within the hour → Start Edition.
7. At least 10 dead jobs since the last Salvage → Start Salvage.
8. An Edition deferred revisions above the `bulk` threshold (§8.3) → Start Followup.
9. Huddle questions are waiting → answer them; then Frontpage due and not scheduled → Start Frontpage; then Rollcall due and not scheduled → Start Rollcall.
10. Otherwise: nothing due.

**Example board** (as rendered by code for the terminal; times in the laptop's time zone, ET here to match §2.3; the schedule times are an illustration, not settings):

```
Bureau · Thu 12 Nov 2026 08:40 ET · service up · settings v12

RUNNING
  Nothing running. Heavy slot free.

WAITING FOR YOU
  Catchup r_51d2      gate plan, open since Wed 21:05
                      340 items in 7 chunks, estimated 4.1M input tokens, about 6 h
                      answer in Board: Approve · Request changes · Reject   (news run status r_51d2 to read it)

QUEUED
  Nothing queued.

DUE
  Schedules           Edition today 11:00 · Frontpage Fri 09:00 · Rollcall Mon 07:00
  Diary today         07:30  ECB account of the October meeting      captured  story 81229  S 58.1  Tier 2
                      08:30  US CPI, October 2026                    captured  story 81234  S 75.2  Tier 1
                      16:30  Fed balance sheet (H.4.1)               expected
  Stories waiting     23 (3 Tier 1). Last Edition r_6b90 finished Wed 18:10 (14 h ago)
  Frontpage           Curation v41 is 3.2 days old: due (stale at 4)
  Huddle              Last Huddle r_3e11 on Wed 4 Nov (8 days ago): due
  Rollcall            Last Sitrep r_44aa on Tue 10 Nov (2 days ago): not due

PROBLEMS
  Sources             ons   red    upstream_changed since Tue 10 Nov (Sitrep r_44aa)
                      bea   amber  free key not set
  Dead jobs           7 since Salvage r_29c0: transient 4, ai_invalid_output 3
  Failed runs         none in 7 days
  Usage               Bureau today 0.4M tokens · cap and reserve not set

NEXT
  Start Edition       3 Tier 1 stories captured and not yet on a page (next scheduled Edition 11:00)
```

**The JSON behind it** (`news run status`, abridged; times in UTC):

```json
{"ok": true, "warnings": [], "truncated": false, "data": {
  "as_of": "2026-11-12T13:40:00Z",
  "service": {"state": "up", "settings_version": 12},
  "heavy_slot": null,
  "running": [],
  "waiting": [
    {"run_id": "r_51d2", "preset": "catchup", "kind": "gate", "gate": "plan", "since": "2026-11-12T02:05:00Z",
     "summary": {"items": 340, "chunks": 7, "est_input_tokens": 4100000, "est_hours": 6},
     "subject_sha": "9c1e…",
     "actions": ["gate.approve", "gate.changes", "gate.reject"]}],
  "queued": [],
  "due": {
    "schedules": [{"preset": "edition", "next_at": "2026-11-12T16:00:00Z"}],
    "diary": [
      {"event_key": "macro:us.cpi:2026-10", "title": "US CPI, October 2026", "expected_at": "2026-11-12T13:30:00Z",
       "status": "captured", "story_id": 81234, "score": 75.2, "tier": 1}],
    "stories_waiting": {"total": 23, "tier1": 3,
                        "last_edition": {"run_id": "r_6b90", "finished_at": "2026-11-11T23:10:00Z"}},
    "frontpage": {"version": 41, "age_days": 3.2, "due_after_days": 2, "stale_after_days": 4, "state": "due"},
    "huddle":    {"last_run": "r_3e11", "age_days": 8, "every_days": 7, "state": "due"},
    "rollcall":  {"last_sitrep": "r_44aa", "age_days": 2, "every_days": 7, "state": "not_due"}},
  "problems": {
    "sources": [{"source_id": "ons", "health": "red", "reason": "upstream_changed", "since": "2026-11-10", "sitrep": "r_44aa"},
                {"source_id": "bea", "health": "amber", "reason": "free key not set"}],
    "dead_jobs": {"total": 7, "by_class": {"transient": 4, "ai_invalid_output": 3}, "since_salvage": "r_29c0"},
    "failed_runs": [], "refused_control": [],
    "usage": {"bureau_today": 400000, "cap_today": null, "reserve": null}},
  "next": {"action": "run.start", "preset": "edition", "rule": "tier1_waiting",
           "reason": "3 Tier 1 stories captured and not yet on a page"},
  "text": "Bureau · Thu 12 Nov 2026 08:40 ET · service up · settings v12\n\nRUNNING\n  Nothing running. Heavy slot free.\n…"}}
```

#### Control actions

Every action is an H-tier command over the Hotline, accepted only from Board (ADR 0007 lists the protocol). The terminal can read the same status but has none of these actions.

| In Board | Does | Hotline command |
| --- | --- | --- |
| Start (a preset's form) | Starts a run with arguments; if the heavy slot is busy, offers "Queue it" or "Pause the running one and start this" (below) | `run.start`, `run.start_after_pause` |
| Approve / Request changes / Reject | Answers the open gate, bound to the `subject_sha` of what Board showed. Request changes needs a note and sends the run back to its drafting step (Frontpage, Rollcall, Huddle); Reject skips the gated work and continues with the next step (for per-item gates, the next item), and the run ends with its Wrap as usual | `gate.approve`, `gate.changes`, `gate.reject` |
| Pause | Stops after the current step (after the current chunk in Catchup; Ghost stops claiming and lets in-flight calls finish); the run becomes `paused` and frees the heavy slot. On a stalled run it takes effect at once (§6.4) | `run.pause` |
| Resume | Continues a paused or waiting run from its last finished step; queues it if the heavy slot is busy. For a stalled run, Pause first (it pauses at once), then Resume | `run.resume` |
| Cancel | Stops now; finished pages stay published, queued jobs are cancelled, the Wrap is written | `run.cancel` |
| Cancel queued | Removes a run from the Lineup (it ends with a Wrap that says so) | `lineup.cancel` |
| Save Standing | Writes one new settings version, schedules included | `standing.set` |

#### Starting a heavy run while the slot is busy

Karthik presses Start on Catchup while Edition r_7f3c is writing pages. `run.start` returns:

```json
{"ok": false, "error": {
  "code": "conflict", "retryable": true,
  "message": "Edition r_7f3c holds the heavy slot.",
  "details": {"reason": "queue_offer", "requested": "catchup",
              "blocking": {"run_id": "r_7f3c", "preset": "edition", "step": "write", "step_no": 7, "steps": 12,
                           "progress": {"done": 40, "total": 112}, "started_at": "2026-11-12T16:00:00Z"},
              "offers": ["queue", "pause_running"]},
  "text": "Catchup is a heavy workflow, and Edition r_7f3c is running …"}}
```

Board shows the code-rendered text with two buttons:

```
Catchup is a heavy workflow, and Edition r_7f3c is running
(step 7 of 12, write pages, 40 of 112 done, started 11:00).
  [Queue it]                               Catchup starts when the Edition finishes or is paused
  [Pause the running one and start this]   the Edition stops after its current step and waits, paused
```

A scheduled heavy run gets no offer: it queues, and is skipped and noted if it would start more than the late limit after its time (§5.5). For a preset that is already live or queued the reason is `preset_live`, and Board opens that run with Resume and Cancel.

#### What Chief guarantees

| Guarantee | How code enforces it |
| --- | --- |
| **One way in** | A run starts only from `run.start` over the Hotline, sent by Board, or from a preset schedule, which is a setting changed only over the same pipe. The service refuses control commands from any process in its AI job objects (§9.3). Chief refuses preset ids it does not declare. A CI test (`bureau-single-entry`) fails if anything other than Chief's own sources and tests inserts into `runs`. The background pools run code jobs only and can never start a preset. |
| **Nothing bypasses gates** | Every G-tier command (`news publish`, `news home publish`, `news eval promote`, `news corrections resolve --action update`, and the gated uses of `news scores adopt`, `news sources enable\|disable` and `news jobs retry`) runs only under the G-tier rule (§5.5): as the current step of a live run, for an item in what was started or approved, authorised by the step's approval with a matching `subject_sha` or, for an ungated step, by the run's start. Anything else returns `gated` (exit 7). |
| **One heavy run at a time** | The partial unique index `runs_one_heavy` (§5.5) admits one heavy run in `running` or `waiting_ai`. Another heavy run queues for the slot instead of being refused (D20): scheduled ones up to the late limit, manual ones until started or cancelled, or Karthik pauses the running one. A run waiting at a gate or paused does not hold the slot. |
| **Every run resumable** | All state is in PostgreSQL (§5.5, §5.8). The service continues live runs after a crash, a restart, an update or a closed lid, and resumes usage-limit pauses on its own at the reset time. |
| **Every run ends in a Wrap** | Success, failure, cancel and a skipped queued run all go through one function that writes `runs.summary` (the Wrap) and the terminal status in one transaction, and a check constraint rejects a terminal status without a summary (§5.5). Pause is not an end. |
| **Approvals come only from Karthik** | Gate answers are H-tier actions: accepted only over the Hotline, only from Board (the installed `news.exe` running as `news board`), from Karthik's user, from a process outside the service's AI job objects, and only for the `subject_sha` of what is waiting. Gate answers are accepted only from Board. Each approval records its origin (`board`) and is listed in the Wrap. This protects against a model acting on its own or on injected text; it is not a boundary against other software running as Karthik's user (ADR 0007, residual risks). |

#### Notifications

Board raises native Windows toasts (a service cannot show them) for: a gate waiting, a run failed, a usage limit reached, a source turned red, and Huddle questions ready. Clicking a toast opens Board on that item. If Board was not running, it shows what it missed on the status board when it opens.

#### Settings, secrets, install and update

- **Settings** live in a versioned PostgreSQL table: every change records who (always `board`), when, and old and new value. Each run snapshots the settings version it started with (`runs.settings_version`). Schedules, targets, parallelism, budgets, the token cap and reserve, quiet hours, models per step and sources on or off are all settings.
- **Secrets** (the write-API key, the read-only Cloudflare and AWS tokens, and the GitHub token) live in Windows Credential Manager under Karthik's account. Board writes them; the service reads them when a step needs one. Nothing secret crosses the pipe or sits in PostgreSQL. `.env` keeps only bootstrap values.
- **Install and update.** `news service install` registers the service under Karthik's account (Windows asks for his password); `uninstall` removes it; both run in Karthik's terminal, because they come before the service exists. An update is started from Board only (`service.update`): it waits for a safe point, swaps the binary, restarts the service, and runs resume from PostgreSQL.

### 8.2 How every preset works

**Entry.** Every preset starts from Board or from one of Karthik's schedules (§8.1). Presets are declarations in Chief (code), not skills or commands, so every preset is testable without AI.

A scheduled Edition, as the service runs it:

```
11:00  scheduler: Edition schedule fires (settings v12; target 120)       → runs row r_7f3c, origin schedule
       preflight: services, Sitrep, quiet hours, quota guard              → pass
       steps 2–6 (code)                                                   → 112 planned items
       step 7 write (ai_batch): Ghost, 3 claude -p at a time              → Board: "write 40/112 ok=38 repair=2"
       steps 8–11: render, check, repair once, sample check, publish, index promotion
11:52  Wrap  {"published":104,"tier1":17,"failed":5,"queued_next":3,"tokens_in":1210000,…}   → toast only if it failed
```

**Drafts and forms instead of session steps.** The first design had three steps done in an interactive session. Now:

- Frontpage's curator (A8) and Rollcall's proposals (A11) run as `claude -p` through Ghost, with inputs built by code and no tools, and write a draft. Board shows the draft (the home page preview; the proposals with their evidence) at the gate, with Approve, Request changes (with a note) and Reject.
- Huddle's 30 or so pairwise questions are a Board form with no AI (§8.12).

**Why not Claude Code workflows for presets.** Claude Code's dynamic workflows are JavaScript modules run by name, good at fan-out, but they cannot take user input mid-run, `-p`-launched runs do not pause for usage limits, and their agents get a 5-minute cache TTL [64][68]. Chief in the service gives durable state in PostgreSQL, real approval stops answered in Board, and automatic resume after a usage limit.

**Common rules.** One live or queued run per preset, and one heavy run at a time; others queue (§5.5). Every preset starts with a preflight: services up (the same checks as `bun run doctor`), Sitrep not red for the sources it needs, quiet hours (§5.6) and the quota guard (§7.4); a heavy preset also warns in Board, without blocking, if a heavy development job is running (§5.7). Diagnostic presets are exempt from the Sitrep check, because looking at red sources is their job: Rollcall's preflight checks only services and the quota guard (and Salvage skips the Sitrep check for the sources it is retrying). Every preset ends with a Wrap written to `runs.summary` and ClickHouse.

### 8.3 Edition

| | |
| --- | --- |
| **Trigger** | On Karthik's schedules, 2–3 times a day (the times are his settings), or Start in Board; arguments `target`, `categories`, `review_plan` |
| **Purpose** | The main production run (PRD "How a run works") |

| # | Step | Kind | Output |
| --- | --- | --- | --- |
| 1 | Preflight | Code | Pass or blockers |
| 2 | Discover all due sources; capture any calendar events now due | Code | New candidates |
| 3 | Normalise, resolve entities, detect stories | Code (+ A3, A4 batches if needed) | Stories |
| 4 | Score, select, tier (Budget); plan the revisions due now: value-only re-renders as `revise` items and direction flips as `update` items, up to the `bulk` threshold of 100 pages | Code | Plan: counts by bucket and tier, top 10 with Pitches, revisions |
| 5 | *Optional approval* of the plan (only with `review_plan` on) | Approval | — |
| 6 | Fetch, parse, brief for planned stories | Code (+ A5 for long texts) | Briefs |
| 7 | Write pages | AI batch (A1) | Copies |
| 8 | Render, check; repair failures once | Code + AI batch (A2) | Ready pages |
| 9 | Sample spot check | AI batch (A7) | Flags |
| 10 | Publish via outbox; run Followup's code steps (value-only re-render, check, publish) for the `revise` items planned in step 4, so every revision published is in the Budget the start or the `plan` approval covers (§5.5). Revisions beyond the 100-page `bulk` threshold, or found after step 4, are deferred: the Wrap says so and the status board suggests Followup. Direction flips were planned in step 4 as `update` items and written in step 7 | Code | URLs |
| 11 | Index promotion; prune working files | Code | — |
| 12 | Summary | Code | Wrap |

**Approval stops.** None by default: Karthik's Start, or the schedule he set, is the approval (P8), and authorises the publish step (§5.5). A scheduled Edition publishes the pages that pass Proof automatically. `review_plan` (per schedule, or on the Start form) adds one stop after step 4, and the `plan` approval then authorises publishing.
**Failure handling.** Per-item failures never stop the run; they go to the dead list with a class. The run stops early only for: preflight blockers, `ai_usage_limit` (paused, resumed on its own at the reset), or the token budget or cap reached (finishes code work, publishes what passed).
**Resumability.** The service continues it after a crash or restart; Resume in Board (or `news run resume <run>`) after a pause. Finished steps and items are skipped by their keys.

### 8.4 Flash

| | |
| --- | --- |
| **Trigger** | Start in Board with event keys or story ids (e.g. right after an FOMC decision) |
| **Steps** | Capture now → normalise → brief → write (A1) → render, check, repair → publish. Same as Edition for 1–10 named items, skipping discovery and selection; tier from the score. |
| **Approval** | None; the run's start authorises publishing (§5.5) |
| **Failure / resume** | As Edition |

### 8.5 Catchup

| | |
| --- | --- |
| **Trigger** | Start in Board with `from` (`BACKFILL_FROM` by default; current year only, per PRD), `to`, `page_types`, `sources`, `chunk` (default 50) |
| **Steps** | 1 Enumerate historical events from sources and calendars (code) → 2 Score with F ignored, select (code) → 3 **Approval**: shows item count, chunk plan, estimated tokens and hours → 4 For each chunk: fetch, parse, brief, write, render, check, publish (same as Edition steps 6–10) → 5 Wrap |
| **Approval** | One, at step 3, in Board; it authorises step 4's publishing for every chunk (the plan hash covers the story ids) |
| **Failure** | Chunk-level progress; per-item failures to the dead list; stops cleanly at the token budget or cap |
| **Resume** | Resume in Board; the next unfinished chunk continues. Catchup is heavy, so a scheduled Edition queues behind it, or Karthik pauses Catchup (it stops after the current chunk and frees the slot), the Edition runs, then he resumes Catchup (§8.1). Backfill jobs use priority 4 |

### 8.6 Frontpage (home curation)

| | |
| --- | --- |
| **Trigger** | Every 2–3 days on Karthik's schedule, or Start in Board (TD "Home page curation"); replaces the TD's `curate-home` workflow, a retired name |
| **Steps** | 1 `news home candidates` (code, §4.5) → 2 Curator draft (A8: `claude -p` through Ghost, `news-home-curation` skill, inputs built by code, no tools) → 3 `news home save` + `check` (code; up to two fix rounds, the curator called again with the errors) → 4 `news home preview` (code; light and dark, phone and desktop; shown in Board) → 5 **Approval** in Board → 6 `news home publish` (gated) → 7 Wrap |
| **Approval** | Step 5: Approve, Request changes (back to step 2 with Karthik's note), or Reject. A scheduled Frontpage waits here like any other |
| **Failure** | Check failures after two rounds → stop and show the errors in Board; the live curation stays |
| **Resume** | Draft and check results are stored; the run returns to the approval stop |
| **Rollback** | Board's published-content actions (H, Board only) |

### 8.7 Rollcall

| | |
| --- | --- |
| **Trigger** | Weekly on Karthik's schedule, or Start in Board when the status board shows a red source |
| **Steps** | 1 `news sources health` (code: last success, error classes, 304 ratio, quota, robots changes, calendar `late` events, parse-failure rate vs 30-day baseline) → 2 `probe` each red or amber source (code, no writes) → 3 Proposals draft (A11: `claude -p` through Ghost, inputs built by code, no tools): disable, retry later, or a development task with the failing fixture attached → 4 **Approval** in Board (`actions`) for disabling a source or for drafting development tasks → 5 Apply approved actions (`news sources disable` as a G-tier step; task drafts) → 6 Wrap |
| **Approval** | Step 4. A scheduled Rollcall runs to this gate and waits; Board raises a toast |
| **AI** | Only the proposals call (A11). Stringer fixes are development work (Sonnet builder, Opus review), not runtime |
| **Output** | Sitrep in ClickHouse and Board |

### 8.8 Salvage

| | |
| --- | --- |
| **Trigger** | Start in Board (the status board lists dead jobs by class), with `class` and `since` |
| **Steps** | 1 `news jobs dead` grouped by class and fingerprint (code) → 2 Auto-retry `transient` and `rate_limited` groups (code) → 3 For `ai_invalid_output` and `check_failed`: re-validate stored outputs with current validators (no tokens) → 4 **Approval** to re-execute AI for the rest (shows count and estimated tokens) → 5 AI batch (A1/A2) → 6 Render, check, publish → 7 Wrap, listing anything still failing with suggested owner (parser bug, prompt, source) |
| **Approval** | Step 4, and any requeue of more than 50 jobs |
| **Resume** | As any run |

### 8.9 Followup

| | |
| --- | --- |
| **Trigger** | Edition plans the revisions due at its step 4 (up to 100 pages) and runs Followup's code steps for them in its step 10, and direction flips enter Edition's plan as `update` items written in its step 7; standalone, Start in Board after a big revision day (e.g. annual GDP benchmark) or when an Edition deferred revisions |
| **Steps** | 1 Find superseded data points referenced by pages (code) → 2 Classify (value-only, direction flip, restatement) → 3 Re-render value-only pages (code) → 4 AI update for direction flips (A1 update mode) → 5 Check, publish with "Updated on …" |
| **Approval** | Only if more than 100 pages would change at once (`bulk`); otherwise the run's start authorises publishing |

### 8.10 Errata

| | |
| --- | --- |
| **Trigger** | Start in Board, weekly or when Board shows new reports (it can also be scheduled; its gates wait in Board) |
| **Steps** | 1 `news corrections pull` (code) → 2 Triage (A9) → 3 For `figure` reports: code re-fetches the source and compares the value at the cited locator → 4 **Approval** per report in Board, with code's evidence (confirmed / not reproduced) → 5 Confirmed: fix (parser fix as a dev task, or re-render, or A1 update) → 6 Publish with "Updated on …"; resolve the report |
| **Output** | Corrections log (feeds the methodology page's corrections count, if Karthik wants one) |

### 8.11 Audit

| | |
| --- | --- |
| **Trigger** | Start in Board with `suite` (`writer`, `classify`, `storymatch`, `spotcheck`, `curator` or `all`) and `prompt_version`; required before promoting any prompt, skill or model change |
| **Steps** | 1 Critic loads the golden set (§9.1) → 2 Deterministic metrics on stored or fresh outputs (code) → 3 AI calls for the candidate version (A1–A9 as relevant) → 4 Judges on a sample (A12) → 5 Comparison report in Board: candidate vs live, per metric, with regressions highlighted → 6 **Approval** to promote → 7 `news eval promote` (gated) |
| **Token budget** | Its own, shown before step 3 |

### 8.12 Huddle

| | |
| --- | --- |
| **Trigger** | Weekly on Karthik's schedule (or Start in Board). The service prepares the Debrief and the questions, and a toast reminds Karthik to answer them in Board |
| **Steps** | 1 `news review weekly` (code: coverage vs targets, pages by tier/type/region, failures by class, tokens per page, cache-read ratio, index budget use, curation age, unresolved entities) → 2 Ranking calibration: a **form** in Board (no AI) asks Karthik about 30 pairwise questions chosen by code, plus the Search Console status he reads and the 5 pages he read (D15); the answers arrive over the Hotline from Board and are stored with `news scores label --run <run>` → 3 `news scores propose --from-labels` (code fit, §4.7) → 4 Board shows the Debrief and the proposed changes (weights, coverage targets, `NEWS_INDEX_PER_DAY` step, `SAMPLE_RATE`) → 5 **Approval** per change → 6 Apply approved changes (`news scores adopt` and new settings versions, as G-tier steps) |
| **Output** | Debrief; versioned config changes |

### 8.13 Reprint

| | |
| --- | --- |
| **Trigger** | Start in Board with `renderer`, `page_types`, `limit`, after component improvements (TD "Renderer versions") |
| **Steps** | 1 Select pages on older renderer versions (code) → 2 Re-render from stored `.oui` and data (code, no tokens) → 3 Checks → 4 **Approval** in Board with a sample of 10 before/after previews → 5 Paced publish (code) |

### 8.14 Background jobs (code only, not presets)

| Job | Schedule | Notes |
| --- | --- | --- |
| `calendar.sync` | Daily 06:00 UTC | Creates pre-warm and capture jobs |
| `calendar.capture` | At release times | Fetch, parse, normalise, detect, score; no AI. Captured items wait for the next Edition or Flash |
| `source.discover` | Every 30–120 min per source | Feeds candidates |
| `work.prune` | Daily | Working files for published items; snapshot retention |
| Analytics pull | Daily | Cloudflare Web Analytics into `traffic_daily` for later ranking (§4.8) |

Run by the Dispatch workers inside the Bureau service, which is always on (ADR 0007); a preset's own code steps still run any of these that are due when it starts. Index promotion is not a background job: it runs only as Edition step 11, because it sends `index_promote` ops with the write-API key (§9.3).

---

## 9. Quality, observability, security, performance

### 9.1 Quality and evaluation

**Layers, cheapest first** (deterministic checks before any model judge, as Anthropic's and Hamel Husain's guidance both recommend [60][61]). Critic, the evaluation harness (`news eval`), runs the AI layers (golden sets, seeded faults, judges) in Audit:

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

Ticker, the telemetry subsystem, records everything in this section; Board shows it (Analytics, and the service log and AI-call records, §8.1).

**Events.** Ticker keeps the existing `news.events` table, and add columns for fields that are filtered and grouped on (they are currently only reachable in `attrs`):

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

Daily materialised views roll these up for Board.

**Traces.** Rust uses `tracing` with an OpenTelemetry OTLP exporter to the existing collector, which already writes `otel_traces` to ClickHouse. One trace per run; spans per step and job. Ghost sets `OTEL_RESOURCE_ATTRIBUTES=news.run_id=…,news.job_id=…,news.step=…` on each `claude -p`, so Claude Code's own `claude_code.api_request` events and `claude_code.token.usage` metrics [69] join to our jobs.

**Board's Analytics screen for this system** (adds to the TD list; egui_plot charts styled by the tokens):

| Area | Panels |
| --- | --- |
| Runs | Run timeline (steps as bars), status, pages per hour, time per stage |
| Queue | Jobs by pool and state; dead list by error class; oldest waiting job |
| Ingestion | Source health matrix (green/amber/red, last success, 304 ratio, errors, quota); release → capture → publish latency for calendar events; `late` events |
| Stories and ranking | Score distribution by page type; selected vs rejected; coverage vs targets (category, region, page type) over 7 and 30 days |
| AI | Tokens per page by type and tier; cache-read ratio; validation failures by rule; repair rate; calls per step A1–A12; run budget used; usage-limit pauses; Bureau usage against the cap, the reserve and total usage (§7.4) |
| Quality | Check failures by check; A7 flag rate; corrections; revision updates |
| Publishing | Index status counts; promotions per day; write-API retries and throttles |
| Laptop | CPU and memory during runs (OTel host-metrics receiver), disk used by `work/raw` |

### 9.3 Security and safety

**Prompt injection from source text.** Source text (press releases, papers, news pages) and reader reports are untrusted. Anthropic's guidance is to keep untrusted content in clearly marked data, tell the model it is data, constrain outputs, and apply least privilege [71]; Claude Code's docs warn against piping untrusted content straight into an agent with tools [71]. Defences here:

1. **No tools in AI calls.** Every step, A1–A12, runs through Ghost with `--tools ""` and an empty strict MCP config, and no project hooks or plugins load. Injected text has nothing to act with. The drafting steps (A8, A11) get their inputs from code like every other step; if the spike shows one needs a tool, it gets only the read-only `news` commands it needs. Their drafts publish nothing until Karthik approves them in Board, and the G-tier rule covers the step behind the gate (§5.5).
2. **Fenced data.** Source text sits in JSON fields marked `"untrusted": true`; the prefix says such fields are material to describe, never instructions.
3. **Bounded outputs.** JSON Schema, enums and validators: numbers cannot appear (C4), links must come from the brief's source list, quotes must be verbatim (C6), refs must exist (C2).
4. **Consequences live in code.** No AI output can publish, approve, delete or fetch; only code steps can, and the risky ones are gated.
5. **Tested.** G1 includes injection cases; A7 flags off-topic or promotional text.

**The control path** ([ADR 0007](docs/adr/0007-bureau-service-and-board.md) has the full model).

- Control commands (start, gate answers, pause, resume, cancel, queue and settings changes) reach the service only over a named pipe whose DACL grants Karthik's SID alone, created as the first instance of its name and rejecting remote clients. The service checks each client's token; Board and the CLI check that the server is the installed service.
- Every `claude -p` runs in a Job Object the service owns, without breakaway, so it and everything it starts carry the spawn marker; the pipe refuses control commands from any process in such a job and logs the attempt for Board's Problems.
- H-tier actions (starts, gate answers, pauses, resumes, cancels, Lineup and Standing changes, maintenance actions) are accepted only from Board: the service checks that the client is the installed `news.exe` running as `news board`. Karthik's terminal reads status and runs the low-risk tiers only. There is no `CLAUDECODE` guard.
- Approvals are bound to `subject_sha` and record their origin (`board`); schedules are settings, changed only from Board.
- Residual risks: other software running as Karthik can open the Hotline and read status, and to act it would have to drive Board itself or replace the installed `news.exe`. A Claude Code development session can read status and run R and W commands, but no H-tier action (ADR 0007).

**The service as Karthik.** The service logs on as Karthik's account, so it has his rights: it is a convenience boundary for his own laptop, not a sandbox. His password is entered only by him, at `news service install`.

**Secrets.**

- `claude -p` children get an allow-listed environment (`PATH`, profile and app-data folders, OTel variables, `NEWS_RUN_ID`, `NEWS_JOB_ID`, `NEWS_SPAWNED_BY`); never `ANTHROPIC_API_KEY`, AWS or Cloudflare keys or the write-API key.
- Secrets live in Windows Credential Manager under Karthik's account, written through Board; `.env` keeps only bootstrap values (how to reach PostgreSQL, ClickHouse and the OTel collector). Agents never see either.
- Only Press (`news publish`, `news home publish`) reads the write-API key, and only inside a run step the G-tier rule authorises (§5.5); the background pools never read it.
- The Cloudflare and AWS tokens behind Board's whole-project screen are read-only. The GitHub token has read access to pull requests, issues and checks, and Actions read and write, which GitHub requires to dispatch a workflow. Karthik approved Board running the deploy workflow. Actions write also allows other Actions operations, such as re-running or cancelling runs; Board itself only dispatches the deploy workflow on `main` (§8.1).

**Least privilege.** Tiers R/W/G/H (§6.3); H-tier actions only from Board over the Hotline, refused for spawned processes; the G-tier rule and approvals checked in code (§5.5).

**Downloaded files.** Treated as untrusted: parsed by memory-safe Rust with size limits and no `unwrap` on external data; Docling runs in a container with `--network none` and a read-only mount *(proposed)*; nothing downloaded is executed or opened by Claude.

**Licensing.** Enforced by C8 and Contacts' `redistribution` flag.

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

**Memory:** PostgreSQL 2–3 GB (`shared_buffers` 2 GB), ClickHouse capped by its settings profile, OTel collector ~100 MB, three `claude` processes, the Bun render server ~0.5 GB, the embedding model ~150 MB, the Bureau service and Board (small; Board only while open). Comfortable within 24 GB.

**Disk:** snapshot retention as §3.2; PostgreSQL grows roughly with data points (a few GB a year).

---

## 10. Build order

Mapped onto the Development Plan. Agent choices follow its rules (Opus for judgement and correctness-critical code, Sonnet for well-specified volume work). For the service and Board (ADR 0007): service work is built by Opus with an Opus security review; Board screens are built by Sonnet, reviewed by Opus, and a different Opus agent checks the token styling.

| Phase | Work from this document | Agent | Exit evidence |
| --- | --- | --- | --- |
| 4 Content system design | ADRs from this document: Dispatch and Chief (D1, D17–D21), tool surface (D3), Ledger and provenance model, Nose v1 and Desk, Taster, AI step contracts and Reporter call (D4), derived figures (D5) | Opus architect | ADRs merged |
| 4 | Embedding spike: bge-small vs nomic vs EmbeddingGemma vs MiniLM on the i5-8250U (docs/s, memory, G3 precision) | Opus spike | Numbers and choice in an ADR |
| 4 | Golden-set labelling: G2, G3, G7 (Karthik with Opus); event weights, prominence lists, coverage targets as seed data | Opus + Karthik | Sets in `pipeline/eval/` |
| 5 Rust CLI | 1 Core crates, migrations, JSON envelope, error codes, telemetry, Dispatch (claim, leases, rescuer, retry classes) | Opus feature builder (one agent) | Golden CLI and concurrency tests |
| 5 | 2 **Right after wave 1, before the connectors.** Chief in the Bureau service: runs and steps, approvals, the G-tier rule, the Lineup, Rota (preset schedules, missed slots, quiet hours, quota guard), Dispatch workers hosted in the service, the Hotline and the spawn marker; the `claude -p`-from-service spike (login under the service account, usage-limit error shape, job-object inheritance, ClickHouse reachability) | Opus; Opus security review | Pipe and marker tests; crash and queue tests; spike findings in ADR 0008 |
| 5 | 3 **Then:** the Windows service (`news service install\|uninstall`; updates from Board, `service.update`), Standing and Credential Manager, and the Board shell: Runs & approvals, Settings & schedules, Docker health (screens step 1), toasts; remove `dashboard/` | Opus for the service parts; Sonnet for Board; Opus review; a different Opus agent checks token styling | Karthik starts, approves and cancels a fixture run from Board |
| 5 | 4 Runner + Morgue + `fetch_state` | Sonnet; Opus review | Fake-server tests |
| 5 | 5 Contacts rows and Stringers in parallel: SEC EDGAR, FRED, BLS, ECB, arXiv, central-bank statements, then IMF, World Bank, Eurostat, ONS, StatCan | Sonnet ×3; Opus review for XBRL/units/periods | Fixture tests |
| 5 | 6 Clerk and Ledger (data points, derived points with guards) | Opus | Property tests |
| 5 | 7 Diary | Sonnet | ICS/API fixture tests |
| 5 | 8 Roster's resolver and seeds | Sonnet; Opus review | G7 ≥ 0.99 precision |
| 5 | 9 Taster and embeddings | Sonnet after the spike | G3 precision |
| 5 | 10 Nose, Desk, Nose tools | Opus | Worked-example test reproduces §4.6 |
| 5 | 11 Fixer | Opus | Size and fencing tests |
| 5 | 12 Proof C1–C14, Ghost tools validators | Opus | Contract tests |
| 5 | 13 Press outbox and index budget (dry-run) | Sonnet; Opus security review | Idempotency tests |
| 6 Cloud | Write API honours `Idempotency-Key`; corrections endpoint for `news corrections pull` | Sonnet; Opus security review | Replay returns the first result |
| 7 Orchestration | Lean-call spike: confirm flags (§3.9), settings isolation, cache TTL and hit ratio | Opus spike | Findings in an ADR |
| 7 | Ghost (Rust, in the service): claim, spawn in the job object, env allow-list, validation, heartbeats, pause and auto-resume on usage limits, the quota guard's per-call accounting | Opus; Opus security review | Fake-`claude` tests |
| 7 | Runtime skills and prefixes: Reporter (A1/A2 prompts), A3–A8, A9, A11 skills | Opus (judgement and prose) | G1, G2, G6 pass |
| 7 | Critic: `news eval`, golden sets G1–G7 from `pipeline/eval/`, A12 judges checked against Karthik's labels | Opus (judgement), Sonnet glue | Audit started from Board produces G1 and G6 reports |
| 7 | Preset step bodies for Edition, Flash, Frontpage (curator draft, preview and approval in Board), Salvage and Audit; Board screens step 2 (service log and AI-call records) | Opus (orchestration); Sonnet for Board | Karthik runs each once from Board |
| 7 | First 10-page run, then 100, then a scheduled Edition | Karthik, from Board | 100 pages pass checks |
| 8 Telemetry | Ticker (ClickHouse columns and tables, traces); Board screens step 3 (Content & sources) and step 4 (Analytics, §9.2) | Sonnet; Opus review; a different Opus agent checks token styling | Analytics shows a real run |
| 9 Launch and ramp | Huddle (with its Board form), calibration, Rollcall, Errata, Catchup, Followup, Reprint; Board screens step 5 (published-content actions, cloud usage, Deploy, development status) | Sonnet builders, Opus review | First weekly review completed |

---

## 11. Decisions

Settled on 15 November 2026: Karthik chose D2 himself and adopted the recommendation for every other decision when he started the build. On 16 November 2026 he chose D17–D21, which supersede D2, D8, D11 and D16 and the start tickets and prompt hook of the first design ([ADR 0007](docs/adr/0007-bureau-service-and-board.md)). Change one by editing this table in a PR before the work it affects begins.

| # | Decision | Options | Recommendation |
| --- | --- | --- | --- |
| D1 | Orchestration engine | (a) custom Rust queue on PostgreSQL with River-style semantics; (b) apalis-postgres; (c) DBOS TypeScript in Bun; (d) Restate or Temporal | **(a)**, with (b) as the fallback if it takes more than a week of agent time. No new servers; one owner of the rules |
| D2 | Form of preset workflows | (a) one user-invoked skill, `/bureau` (`disable-model-invocation`), driving a CLI state machine (Chief); (b) `.claude/workflows/` files | **Superseded by D17** (16 Nov 2026). It was (a). Claude Code workflows are still not used for presets: they can't take input mid-run and get short cache TTLs |
| D3 | Agent tool surface | (a) `news` CLI only, schemas exportable, MCP adapter later if wanted; (b) local MCP server now | **(a)** |
| D4 | Reporter call shape | (a) no tools; code pre-selects component specs into the cached prefix; (b) Reporter calls `news component` on demand | **(a)**: fewer turns, warmer cache, nothing for injected text to use; repair round supplies any missing spec |
| D5 | Derived figures | (a) code computes changes and shares with guards, labelled "Calculated by News from …"; (b) only published figures | **(a)**. Headlines like "rose 39 % from a year earlier" need them; guards keep the no-conversion rule |
| D6 | Embedding model | bge-small-en-v1.5, nomic v1.5, EmbeddingGemma (gated licence), MiniLM | **bge-small-en-v1.5**, confirmed by the Phase 4 spike |
| D7 | Raw snapshot retention | (a) prune after publish (TD); (b) structured data 400 days, documents 90 days, hashes for ever | **(b)**, so every figure stays provable |
| D8 | Background code-only worker | (a) Dispatch worker via Task Scheduler for discovery and calendar capture (no publishing or index promotion); (b) everything only inside presets | **Superseded by D17**: the Bureau service hosts the Dispatch workers, always on. It was (a) |
| D9 | Earnings dates | (a) derived windows from filing history + SEC 8-K polling; (b) unofficial feeds | **(a)**; no reliable free official source exists |
| D10 | Templated fallback pages | (a) publish Tier 2 code-only pages when Reporter fails twice, kept `noindex` until upgraded; (b) re-queue only | **(a)**, flagged and never indexed until Reporter upgrades them |
| D11 | Model for the preset session | Sonnet or Opus | **Superseded by D17**: there is no session. The drafting steps' models are per-step settings, starting at Sonnet 5.5 |
| D12 | Optional AI steps A6 (entity shortlist) and A10 (alt text) | On or off at launch | **Off**; switch on if the unresolved queue or missing alt text proves it necessary |
| D13 | Plan review stop in Edition | On or off by default | **Off**; `review_plan=yes` when wanted |
| D14 | SEC contact e-mail in the User-Agent | Personal address or a dedicated one | **A dedicated address**, kept in Standing as `NEWS_CONTACT_EMAIL` (in `.env` before ADR 0007; SEC requires a contact [18]) |
| D15 | Human reading sample | None, or 5 random pages a week | **5 a week** in the Huddle workflow; it calibrates A7 |
| D16 | A second heavy run while one is running | (a) refuse with the blocking run and the next commands; (b) queue it | **Superseded by D20**. It was (a), because nothing would have driven a queued run |
| D17 | Host and control surface | (a) a Windows service logged on as Karthik, hosting Chief, Rota, the Dispatch workers and Ghost (Rust), with Board, an egui desktop app, in one `news.exe`; (b) keep `/bureau` and add a service; (c) a Bun web dashboard; (d) a webview app | **(a)**, Karthik's choice (ADR 0007, §8.1). One control surface, Rust only, runs continue with Board closed. The renderer stays a warm Bun server |
| D18 | How control commands reach Chief | (a) a named pipe (the Hotline) locked to Karthik's user and local clients, H-tier actions accepted only from Board, refused for processes the service spawned for `claude -p` (Job Object marker); (b) start tickets and a prompt hook; (c) an HTTP API on 127.0.0.1 | **(a)**, Karthik's choice (ADR 0007, §9.3). Replaces start tickets, the prompt hook and the agent deny-list. On 16 Nov 2026 Karthik added: H-tier actions only from Board; the terminal reads status and runs low-risk tiers; no `CLAUDECODE` guard |
| D19 | Scheduled AI runs | (a) allowed: a schedule Karthik sets counts as his approval to start; scheduled Editions publish pages that pass Proof; gated presets still wait in Board; (b) only runs Karthik starts | **(a)**, Karthik's choice (P8 as rewritten, §5.6). Missed slots run once for the latest only; usage limits pause and resume on their own; quiet hours and the quota guard apply (§7.4) |
| D20 | A heavy run while another holds the slot | (a) queue it: scheduled runs wait up to the late limit, a manual Start offers "Queue it" or "Pause the running one and start this"; (b) refuse (D16) | **(a)**, Karthik's choice (§5.5, §8.1) |
| D21 | Settings and secrets | (a) a versioned PostgreSQL settings table, a settings snapshot per run, secrets in Windows Credential Manager, `.env` for bootstrap only; (b) environment variables and `.env` | **(a)**, Karthik's choice (§8.1) |

---

## 12. Risks

| Risk | Effect | Mitigation |
| --- | --- | --- |
| Lean-call flags behave differently from the docs (settings sources, hooks) | Page calls load project context or tools | Lean-call spike before building Ghost; env and config isolation as a second layer |
| Plan usage limits hit mid-run | Partial runs | Run token budget, pause and automatic resume at the reset, back-to-back cached prefixes |
| Unattended scheduled runs publish a bad batch | Wrong pages live before Karthik looks | Only pages that pass Proof publish; sample checks; `review_plan` per schedule; withdraw and correct from Board; Errata |
| Bureau uses the quota Karthik needs for development | Development stalls | The Bureau token cap and reserve: near the limit no new run starts and running runs stop claiming AI jobs, checked before every claim (§7.4); Board shows Bureau usage against the total |
| The service runs as Karthik's account | It has his rights; a password change stops it logging on | Pipe locked to his user; spawn marker; Board's offline banner shows the last error and he re-enters the password with `news service install` |
| `claude -p` does not use his login when started by a service (session 0) | No AI runs from the service | The service spike in the step after wave 1, before anything else is built on it (ADR 0007) |
| Hand-set weights pick a narrow mix | Repetitive coverage | Bucket diversity, coverage gap, weekly pairwise calibration |
| Grey-zone thresholds wrong | Merged or split stories | G3 calibration; keep-separate fallback; update detection |
| Source APIs change (ONS and IMF are mid-migration [26][29]) | Gaps | Rollcall, weekly on schedule; hash-based change detection; fixtures |
| Numeric lint too strict | Good Copies rejected | Allow-list maintained from G1 failures; repair round |
| Engine bugs (custom code) | Stuck or duplicated jobs | Unique keys, crash tests, idempotent steps; apalis fallback |
| Disk growth from snapshots | Disk pressure | Retention policy; Board panel |
| A development Claude Code session acts through the CLI | A run started or a gate answered without Karthik | H-tier actions are accepted only from Board (Karthik, 16 Nov 2026); the CLI reads status and runs low-risk tiers only; approvals stay bound to `subject_sha` and are listed in the Wrap |

---

## 13. Glossary of terms used

| Term | Meaning here |
| --- | --- |
| Event key | A deterministic identity for a structured story, such as `macro:us.cpi:2026-10` |
| Story | A group of candidates that describe the same event |
| Brief | The compact JSON research pack given to Reporter (TD term) |
| Ref | An ID (`dp_`, `dv_`, `sr_`, `pd_`, `q_`, `cx_`) the model writes instead of a number, series, period, quote or context line |
| Vintage | The time at which a published value was valid (a release or filing date) |
| Locator | Where inside a stored snapshot a value was read |
| Lease | A time-limited claim on a job; expires if the worker dies |
| Snooze | Postpone a job without counting a failed attempt |
| Gate | An approval stop whose decision is recorded and checked by code |
| Hotline | The named pipe between Board and the Bureau service; H-tier actions only from Board, status for the terminal (§8.1, §9.3) |
| Spawn marker | Membership of a Job Object the service owns; every `claude -p` and everything it starts carries it, and the pipe refuses their control commands (§3.9) |
| Heavy-slot queue | Heavy runs waiting for the one heavy slot (§5.5) |
| Heavy run | A run of a preset that writes or re-renders pages in bulk; only one runs at a time, others queue (§5.5) |
| Preset | A workflow declared in Chief and started from Board or a schedule (§8) |

---

## 14. Changes to existing documents

### 14.1 The first design (15 Nov 2026)

A historical record. Rows that name `/bureau`, start tickets, the prompt hook, `news work` or the web dashboard were superseded on 16 Nov 2026 by §14.2.

Applied to the documents named below in commit 44ade09 (see the Applied column). Where a change depends on an open decision (§11), the target marks it "(Bureau design, decision Dn)".

| Document | Section | Now | Proposed change | Why | Applied |
| --- | --- | --- | --- | --- | --- |
| TD | Keeping each `claude -p` call lean | "Minimal tools via `--allowedTools`" | Use `--tools ""` (or a short `--tools` list) to restrict; `--allowedTools` only pre-approves [62] | Correctness | Yes, 44ade09 |
| TD | Keeping each `claude -p` call lean | — | Add: `--json-schema`, `--max-budget-usd`, `--no-session-persistence`, `--disable-slash-commands`, explicit `--settings`/`--setting-sources`; unset `ANTHROPIC_API_KEY`; never `--bare` on a subscription; `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1` [62][63] | Subscription auth, lean context, no project hooks in page calls | Yes, 44ade09 |
| TD | Keeping each `claude -p` call lean | "Component catalog on demand" via `news component` | Code pre-selects component specs into the cached prefix; Reporter has no tools (D4) | Fewer turns, better caching, injection defence | Yes, 44ade09 |
| TD | Agent orchestration, How a run works | Main session calls `news plan`, then a Bun runner | Main session drives a preset through `news run start/next`; `news plan` becomes a step inside it | Durable, resumable runs with approval gates | Yes, 44ade09 |
| TD | Model roles | Sonnet "selection judgement where code scoring is unsure"; Haiku "summarising long parsed text" | Remove the Sonnet selection role; Haiku condensation becomes extractive, digit-free, with quote spans (A5); add A3, A4, A7, A9 contracts | Code-first selection; verifiable condensation | Yes, 44ade09 |
| TD | Content system | "Haiku assists in scoring … where code is unsure" | Haiku only for ambiguous classification (A3) and grey-zone story matching (A4) | Scoring stays fully code | Yes, 44ade09 |
| TD | Agent toolbox | Command table | Add command groups `news run`, `jobs`, `ai`, `sources`, `calendar`, `stories`, `scores`, `entities`, `eval`, `review`, `corrections`, `work` and `news schema`; JSON envelope and exit codes (§6) | Purpose-built tools | Yes, 44ade09 |
| TD | Data model, PostgreSQL | Table list | Add `snapshots`, `fetch_state`, `calendar_events`, `documents`, `text_blocks`, `derived_points`, `entity_ids`, `entity_aliases`, `stories`, `scores`, `run_plans`, `jobs`, `pools`, `runs`, `run_steps`, `approvals`, `publish_outbox`, `pages`, `page_refs`, `event_weights`, `schedules`, `traffic_daily`, `start_tickets`; extend `sources`, `candidates`, `data_points` (vintage, locator, verbatim value) | Provenance, orchestration | Yes, 44ade09 |
| TD | Local folders | `work/raw/` pruned after publish | Content-addressed `work/raw/sha256/…`, retention 400 days for structured data, 90 days for documents (D7) | Provable figures | Yes, 44ade09 |
| TD | Telemetry | `events` columns | Add `job_id`, `attempt`, `preset`, `step`, `source_id`, `error_class`, `prompt_version`, `trace_id`, `span_id`; new tables `ai_calls`, `fetches`, `scores`, `check_results`, `runs` | Joins and dashboards | Yes, 44ade09 |
| TD | Quality checks | Code checks list | Add numeric-literal lint, direction-word check, verbatim quotes, licence, duplicate-page checks (C4–C8, C14) | Number fidelity | Yes, 44ade09 |
| TD | Environment variables | List | Add `NEWS_AI_RUN_TOKENS`, `NEWS_CONTACT_EMAIL`, `NEWS_RAW_RETAIN_DAYS_DATA` (400), `NEWS_RAW_RETAIN_DAYS_DOCS` (90) | Budgets, SEC UA, retention | Yes, 44ade09 |
| TD | Open items: "Home curation workflow form" | Workflow file vs slash command | Settled: the Frontpage preset, started with `/bureau curate home` through the one user-invoked skill and Chief's CLI state machine (D2, §8.1) | Approval stops in-session | Yes, 44ade09 |
| TD | Home page curation | "the preset `curate-home` workflow" | Rename to Frontpage, started with `/bureau curate home`; `curate-home` is retired | One entry point | Yes, 44ade09 |
| TD | Updating a page | Agent revises the page | Value-only revisions are code re-renders; AI only for direction flips (§3.11) | Fewer tokens, fewer errors | Yes, 44ade09 |
| TD | Security | — | Start tickets, the G-tier rule and approvals bound to `subject_sha` (§5.5); the `UserPromptSubmit` hook (`news run hook-prompt`) for gate answers and starts; agent deny rules for G/H commands, `psql` and direct database clients in `.claude/settings.json` (§8.1, §8.2); env allow-list for `claude -p`; Docling without network | Least privilege | Yes, 44ade09 |
| TD | Agent orchestration (introduction and step 1) | "One prompt in Claude Code starts a run"; "run today's batch" | Runs start only when Karthik types `/bureau`, for the main run `/bureau run daily`; the prompt hook issues the start ticket (§5.5) | One entry point | Yes, 44ade09 |
| TD | Home page curation, step 5 | "approve, change or reject the draft in the Claude Code session" | Gate answers are typed `/bureau approve\|changes\|reject`, executed by the prompt hook; a plain "yes" approves nothing | Approvals only from Karthik's typed command | Yes, 44ade09 |
| TD | Build plan, step 8 | "the `curate-home` workflow" | The Frontpage preset, started with `/bureau curate home`, and the `/bureau` skill | Retired name | Yes, 44ade09 |
| PRD | Content production, step 2 | "scores, clusters and selects" | No change in substance; add that releases are captured on schedule by code from official calendars | Clarity | Yes, 44ade09 |
| PRD | Content production | "started by a prompt (manually, or by a local scheduler)" | Runs that call a model start only when Karthik types `/bureau`; a local scheduler runs only code-only background jobs (calendar sync, capture, discovery) | P8; single entry | Yes, 44ade09 |
| PRD | Trust and editorial; methodology | "Numbers come from data" | Add that some figures (changes, shares) are calculated by News from source figures and labelled as such (D5) | Honesty about derived figures | Yes, 44ade09 |
| Development Plan | Repository tree; Phase 7 | `.claude/workflows/` for presets, starting with `curate-home` | One user-invoked skill, `/bureau`, in `.claude/skills/` driving `news run`; every preset is an argument to it (`curate-home` becomes `/bureau curate home`); `.claude/workflows/` optional for fan-out | D2 | Yes, 44ade09 |
| Development Plan | Summary; Phase 7 exit; "Home page curation across the phases" (text, Phase 7 row, "Done means"); Progress checklist | The `curate-home` workflow; "one prompt produces 100+ pages" | The Frontpage preset, started with `/bureau curate home`; the Phase 7 exit reads "`/bureau run daily` produces 100+ pages … Karthik runs `/bureau curate home` once end to end"; "Done means": Karthik types `/bureau curate home`, and `news home rollback <version>` in a terminal restores the previous version | Retired name; one entry point | Yes, 44ade09 |
| Development Plan | Tools table | `news-home-curation` "in the `curate-home` workflow"; a `curate-home` workflow row in `.claude/workflows/` | The curator in the Frontpage preset; replace the workflow row with the `/bureau` skill in `.claude/skills/bureau/` | D2 | Yes, 44ade09 |
| Development Plan | Phase 4 | Work list | Add ADRs listed in §10, the embedding spike's measurements, golden-set labelling (G2, G3, G7) | Build order | Yes, 44ade09 |
| Development Plan | Phase 5 | Work list | Add, in §10 order: core crates; Dispatch and Chief; Runner and Morgue; Contacts rows and Stringers; Clerk and Ledger; Diary; Roster and its resolver; Taster; Nose and Desk; Fixer; Proof and the Ghost tools validators; Press (dry-run) | Build order | Yes, 44ade09 |
| Development Plan | Phase 7 | Work list | Add the lean-call spike, Ghost env allow-list and pause-on-limit, Critic (golden sets G1/G6, A12 judges), the Audit workflow | Build order | Yes, 44ade09 |
| `skills/README.md` | Table | Page skills and curation | Add runtime skills for A3, A4, A5, A7, A9 (and A6, A10 if enabled) | Every AI step has a skill | Yes, 44ade09 |
| `skills/README.md` | `news-home-curation` row | "in the `curate-home` workflow" | "in the Frontpage preset (`/bureau curate home`)" | Retired name | Yes, 44ade09 |
| CLAUDE.md | Home page line | "by the `curate-home` workflow" | "by the Frontpage preset, started with `/bureau curate home`" | Retired name | Yes, 44ade09 |
| `design/layout.md` | "The home page is curated and dynamic" | "through the `curate-home` workflow" | "through the Frontpage preset (`/bureau curate home`)" | Retired name | Yes, 44ade09 |
| `news-source-connector` skill | What a connector delivers | Discover, fetch, parse | Add: `parse` must be pure; locators per data point; conditional GET via the shared Runner; calendar entries; event-key rule | Provenance and politeness | Yes, 44ade09 |

Also applied in 44ade09, beyond the rows above, so the documents agree with this design: TD Summary (one `/bureau run daily` command, link to this design), Content system stage table (Bureau names), Model roles (Haiku "small updates" removed: updates are code or A1), token budget (component specs in the prefix), Quality checks intro and failure handling (A2 repair, D10), the overlap of the TD's `clusters`, `queue` and `ledger` tables with Bureau's tables (left to the Phase 4 ADRs), Renderer versions (Reprint) and the prompt-hook open item; PRD home curation (`/bureau curate home`, `/bureau approve`), raw snapshot retention (D7) and the Frontpage name; `skills/README.md` page calls get no tools (`--tools ""`), not "only the `news` CLI tools"; the connector skill's `work/raw/` path (Morgue) and parse-failure handling (dead list, then Salvage); Development Plan Phases 6, 8 and 9 and the reading list.

### 14.2 Bureau service and Board (16 Nov 2026)

Karthik replaced the `/bureau` entry point with the Bureau service and Board ([ADR 0007](docs/adr/0007-bureau-service-and-board.md); D17–D21). Applied in this change, on branch `docs/architecture-service-board`:

| Document | Section | Change |
| --- | --- | --- |
| This design | §1 | P8 rewritten (a Start or a schedule Karthik set is the approval to start); P9 and P11 for the service and Board; scope |
| This design | §2.2, §2.3 | Who drives what: Board over the Hotline to the service (the terminal reads status only); the CPI example runs on a schedule |
| This design | §3.9 | Ghost is Rust in the service; the spawn marker (Job Object) on every `claude -p` |
| This design | §5.1–§5.9 | `runs.origin` is `board` or `schedule`; `start_tickets` dropped; approvals' origin `board`; draft and form steps replace session steps; the Lineup; the service hosts the workers and Rota (preset schedules, missed slots, quiet hours, quota guard); usage limits resume on their own; service update |
| This design | §6 | H tier is Board only, over the Hotline; `news run intent`, `next`, `submit`, `hook-prompt` and `ticket` retired; `news service`, `news board` and settings commands added |
| This design | §7 | No interactive session; A8 and A11 are `claude -p` drafts; Huddle has no AI; §7.4 quota guard |
| This design | §8 | §8.1 replaced by "Board and the Bureau service" (screens in build order, presets with arguments, status board, control actions, queue offer, guarantees, notifications, settings); §8.2 drafts and forms instead of session steps, the deny-list rationale removed; §8.3–§8.13 triggers are schedules and Board |
| This design | §9–§15 | Security of the control path and the service account; Board's Analytics screen; build order with the service step after wave 1; D17–D21; risks; glossary; names |
| TD | Summary, Design principles, Local environment, Agent orchestration, Model roles, Agent toolbox, Data model, Home page curation, Telemetry ("Board"), Configuration and security, Free-tier budget, Build plan, Open items | Service and Board; scheduled runs; settings table, Credential Manager and the named pipe; cloud usage in Board |
| PRD | Content production, How a run works, Home page curation, Technical architecture, Roadmap, Risks | Board as the control surface; scheduled runs; curation approved in Board |
| Development Plan | Summary, repository tree, roles, skills, Phases 0 and 5–9, home curation across the phases, risks, checklist | The service and Board shell after wave 1 and before #9; `dashboard/` and `.claude/skills/bureau/` dropped; Board in `cli/`; the service spike replaces the prompt-hook spike |
| CLAUDE.md | Bureau line, Layout | Board and the service; no `dashboard/` |
| `design/layout.md` | Home page | Frontpage on schedule or from Board |
| `skills/README.md` | `news-home-curation` row; Ghost | A `claude -p` call; Ghost in the service |
| `.claude/` skills and agents | `news-subagent-workflow`, `news-design-system`, `news-source-connector`, `feature-builder` | "dashboard" → Board; Salvage started from Board |
| All of the above | Names and H tier | Rota, Hotline, Plate, Lineup and Standing (`news standing`); H-tier actions accepted only from Board, the terminal reads status and runs low-risk tiers (Karthik, 16 Nov 2026) |
| ADRs 0002, 0003, 0005 (other branches) | See ADR 0007, "Amendments to apply on other branches" | `CLAUDECODE` guard → pipe and marker; `news work` → the service; Frontpage wording |

---

## 15. Names

The approved names (Karthik, final). **Bureau** is the whole laptop system, and it runs as the Bureau service; **Board** is its desktop app, where Karthik starts and controls every workflow (§8.1). Tool families take their owner's name. Names that were already in use stay as they are: the TD's `news` CLI commands, the `news-home-curation`, `news-brand` and `page-*` skills, the TD's PostgreSQL tables and the TD term "brief". Table names in the SQL sketches, the pipe's path and settings keys are implementation detail, not product names.

**System**

| Name | What it is | Kind | How it starts, or CLI group | Where described |
| --- | --- | --- | --- | --- |
| Bureau | Everything in this document: the laptop side of News, running as a Windows service logged on as Karthik's account | System, service | `news service run\|install\|uninstall` (updates: Board `service.update`) | §1, §2, §8.1 |
| Board | The desktop app (egui) in `news.exe`: runs and approvals, settings and schedules, Docker health, the service log, content and sources, analytics, whole-project actions. Replaces `/bureau` and the planned web dashboard (TD "Board") | App | `news board` | §8.1, §9.2 |

**Gathering**

| Name | What it is | Kind | Command or CLI group | Where described |
| --- | --- | --- | --- | --- |
| Contacts | Source registry: one row per source with licence, attribution, rate limits and tier (`sources`) | Subsystem, data store | `news sources` | §3.1 |
| Wire | Ingestion: discovering new items and fetching them politely | Subsystem | `news discover`, `news fetch` | §3.2 |
| Stringers | Connectors, one Stringer per source (implementations of the `Connector` trait) | Component | — | §3.2 |
| Runner | The shared polite HTTP fetcher: conditional GET, rate limits, `Retry-After`, robots.txt | Component | — | §3.2 |
| Morgue | Raw snapshot store: content-addressed files plus the `snapshots` table | Data store | — | §3.2 |
| Diary | Release calendar: expected releases and meetings; drives pre-warm and capture jobs | Subsystem, data store | `news calendar` | §3.3 |

**Understanding**

| Name | What it is | Kind | Command or CLI group | Where described |
| --- | --- | --- | --- | --- |
| Clerk | Parsing snapshots into data points, text blocks, entity mentions and candidates | Subsystem | `news parse` | §3.4 |
| Ledger | Data point store: `data_points` and `derived_points`, with vintages and locators | Data store | — | §3.4 |
| Roster | Entity graph: entities, identifiers, aliases, edges | Data store | `news entities` | §3.5 |
| Roster's resolver | Deterministic mention-to-entity resolution | Component | `news entities resolve` | §3.5 |
| Taster | Story detection: event keys, de-duplication, clustering, update detection | Subsystem | `news stories` | §3.6 |

**Deciding**

| Name | What it is | Kind | Command or CLI group | Where described |
| --- | --- | --- | --- | --- |
| Nose | Scoring and ranking: the score formula and its calibration | Subsystem | `news scores` | §3.7, §4.1–§4.3, §4.7 |
| Desk | Selection: breadth-first run filling, tiers and limits | Component | `news plan` | §3.7, §4.4 |
| Budget | The run plan: stories chosen for a run, with tier, bucket and reason (`run_plans`) | Artefact | `news plan` | §3.7, §4.4 |
| Pitch | The score card: one line per score component (`scores.explain`) | Artefact | `news scores explain` | §3.7, §4.1 |

**Writing and checking**

| Name | What it is | Kind | Command or CLI group | Where described |
| --- | --- | --- | --- | --- |
| Fixer | Brief builder: Reporter's compact research pack | Subsystem | `news brief` | §3.8 |
| Ghost | AI runner inside the Bureau service (Rust): every `claude -p` call, with the spawn marker, env allow-list, validation, and pause and resume on usage limits | Component | `news ai` | §3.9, §5.5, §7 |
| Reporter | AI page writing (A1, A2) | Subsystem | — | §3.9 |
| Copy | Page draft: headline, fly title, summary and OpenUI Lang with refs | Artefact | `news save` | §3.9 |
| Proof | Checks: code checks C1–C14 and the sample check (A7) | Subsystem | `news check` | §3.10 |
| Markup | Check report for one page | Artefact | `news check` | §3.10 |
| Rewrite | Revision handling: re-rendering or rewriting pages when sources revise data | Subsystem | `news update`; the Followup preset | §3.11, §8.9 |

**Running**

| Name | What it is | Kind | Command or CLI group | Where described |
| --- | --- | --- | --- | --- |
| Press | Publisher: outbox, paced publishing, index budget | Subsystem | `news publish`, `news home publish` | §3.12 |
| Dispatch | Job queue and the service's workers: jobs, pools, leases, retries, recurring code jobs | Subsystem | `news jobs` | §5.3, §5.4, §5.6 |
| Chief | Preset engine inside the Bureau service: runs, steps, gates, the Lineup, the status board, the Wrap | Subsystem | `news run` | §5.5, §8.1, §8.2 |
| Ticker | Telemetry: events, spans and AI call records into ClickHouse | Subsystem | — | §3.13, §9.2 |
| Critic | Evaluation harness: golden sets, judges, comparisons | Subsystem | `news eval` | §9.1 |

**Bureau service parts** (named by Karthik on 16 Nov 2026, ADR 0007)

| Name | What it is | Kind | Command or CLI group | Where described |
| --- | --- | --- | --- | --- |
| Rota | The scheduler inside the service: preset schedules, missed slots, quiet hours and the quota guard | Component | — (schedules live in Standing) | §5.6, §7.4 |
| Hotline | The named pipe between Board and the service; H-tier actions only from Board, status for the terminal | Component | — | §8.1, §9.3 |
| Lineup | Heavy runs waiting for the heavy slot, in Chief | Component | `news run lineup` | §5.5 |
| Plate | The warm local Bun + React/OpenUI render server the service calls on 127.0.0.1 | Component | — | §2.2, §5.7 |
| Standing | The versioned PostgreSQL settings store, schedules included | Data store | `news standing` | §8.1 |

**Not yet named:** the spawn marker, the Job Object every `claude -p` runs in (§3.9, §9.3); a name is to be chosen with Karthik.

**Reports**

| Name | What it is | Kind | Command or CLI group | Where described |
| --- | --- | --- | --- | --- |
| Wrap | Run summary, written when every run ends (`runs.summary`) | Report | Board's run detail; `news run status <run>` | §5.5, §8.1, §8.2 |
| Sitrep | Source health report, the output of Rollcall | Report | Board; the Rollcall preset | §8.7 |
| Debrief | Weekly review report, the output of Huddle | Report | Board; `news review weekly` | §8.12 |

**Workflows** (presets; started from Board or a schedule; heavy ones hold the heavy slot one at a time, §8.1)

| Name | What it is | Kind | Command or CLI group | Where described |
| --- | --- | --- | --- | --- |
| Edition | The main production run | Preset (heavy) | Board `run.start` (edition) or a Rota schedule | §8.3 |
| Flash | A few named items now | Preset (heavy) | Board `run.start` (flash) or a Rota schedule | §8.4 |
| Catchup | Current-year backfill | Preset (heavy) | Board `run.start` (catchup) or a Rota schedule | §8.5 |
| Frontpage | Home curation | Preset (light) | Board `run.start` (frontpage) or a Rota schedule | §8.6 |
| Rollcall | Source checks and fixes | Preset (light) | Board `run.start` (rollcall) or a Rota schedule | §8.7 |
| Salvage | Dead jobs and replays | Preset (heavy) | Board `run.start` (salvage) or a Rota schedule | §8.8 |
| Followup | Source revisions (its code steps also run as Edition step 10) | Preset (heavy) | Board `run.start` (followup) or a Rota schedule | §8.9 |
| Errata | Reader error reports | Preset (light) | Board `run.start` (errata) or a Rota schedule | §8.10 |
| Audit | Golden-set evaluation and prompt promotion | Preset (heavy) | Board `run.start` (audit) or a Rota schedule | §8.11 |
| Huddle | Weekly statistics and ranking calibration (a Board form) | Preset (light) | Board `run.start` (huddle) or a Rota schedule | §8.12 |
| Reprint | Re-rendering pages on a new renderer | Preset (heavy) | Board `run.start` (reprint) or a Rota schedule | §8.13 |

**Tool families** (named after their owner)

| Name | What it is | Kind | Command or CLI group | Where described |
| --- | --- | --- | --- | --- |
| Chief tools | presets, status, lineup, list (start, gate answers, pause, resume and cancel are Board actions) | Tool family | `news run` | §5.5, §6.4, §8.1 |
| Dispatch tools | list, dead, claim, heartbeat, complete, fail, retry, pause, unpause | Tool family | `news jobs` | §5.4, §6.4 |
| Ghost tools | input, accept, schema, fallback | Tool family | `news ai` | §3.9, §6.4 |
| Contacts tools | list, show, health, probe, enable, disable | Tool family | `news sources` | §3.1, §6.4, §8.7 |
| Diary tools | upcoming, sync, late | Tool family | `news calendar` | §3.3, §6.4 |
| Taster tools | show, members, similar, merge, split | Tool family | `news stories` | §3.6, §6.4 |
| Nose tools | explain, weights, propose, label, adopt | Tool family | `news scores` | §4.7, §6.4, §8.12 |
| Roster tools | resolve, show, unresolved, alias | Tool family | `news entities` | §3.5, §6.4 |
| Critic tools | run, report, label, promote | Tool family | `news eval` | §6.4, §8.11 |
| Huddle tools | weekly | Tool family | `news review` | §6.4, §8.12 |
| Errata tools | pull, show, resolve | Tool family | `news corrections` | §6.4, §8.10 |

**Retired**

| Name | What it is | Kind | Replaced by | Where described |
| --- | --- | --- | --- | --- |
| `curate-home` | The TD's name for the home curation workflow | Retired name | Frontpage | §8.6, §14 |
| `/bureau` | The first design's Claude Code entry point (one user-invoked skill) | Retired name | Board and the Bureau service | §8.1, ADR 0007 |
| Start tickets, the prompt hook (`news run hook-prompt`, `news run ticket`, `news run intent`) | How the first design let a run start only from what Karthik typed | Retired mechanism | The Hotline and the spawn marker | §5.5, §9.3, ADR 0007 |
| `news work` | The first design's background worker, started by Task Scheduler | Retired command | The Dispatch workers inside the service | §5.6, ADR 0007 |

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
