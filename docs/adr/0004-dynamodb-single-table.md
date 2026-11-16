# ADR 0004: DynamoDB single table, search shards and storage split

- Status: accepted (Phase 6, issue #6)
- Date: 15 November 2026
- Contracts: `contracts/cloud/dynamodb-items.schema.json`, `contracts/bundle/meta.schema.json`

## Context

DynamoDB is the cloud's database of record. The free tier gives 25 read and 25 write capacity units, shared by every table and index in the account, and caps items at 400 KB. A write unit is 1 KB per second, so item size, not item count, sets publishing speed. The Technical Design (TD, "DynamoDB") fixed a single table with `PAGE#`, `HUB#`, `ENT#`, `TAX#`, `URL#` and `HOME#CURATION` keys. The write API also needs idempotency records and a way to keep two operations on one page apart.

## Decision

One table, string keys `PK` and `SK`, no secondary index. Item shapes are in the items schema; every content record carries `lang` (bookkeeping items, idempotency records and page locks, do not).

| Item | PK | SK | Notes |
| --- | --- | --- | --- |
| Page version | `PAGE#<page_id>` | `V#<000001>` | Written once per version, never changed |
| Live page | `PAGE#<page_id>` | `LATEST` | Moved last on publish: the commit point readers see |
| Page lock | `PAGE#<page_id>` | `LOCK` | One write operation per page at a time; 2-minute lease; bookkeeping, no `lang` |
| Path claim | `URL#<page path>` | `PAGE` | One page per URL and R2 prefix; lookup by path |
| Hub entry | `HUB#<hub>` | `<first published_at>#<page_id>` | Hubs: `latest`, `cat/<slug>` (and each parent), `ent/<entity id>`, `region/<region>` |
| Entity | `ENT#<entity id>` | `META` | Its pages are hub `ent/<entity id>` |
| Taxonomy node | `TAX#tree` | `NODE#<slug>` | Changed from the TD's `TAX#<id>` / `META` (below) |
| Redirect | `URL#<old path>` | `REDIRECT` | |
| Curation | `HOME#CURATION` | `V#<000001>` and `LATEST` | ADR 0005 |
| Idempotency | `IDEM#<SHA-256 of key>` | `META` | Bookkeeping, no `lang`; TTL 7 days |

- **Versions are zero-padded** (`V#000010`), because sort keys compare as strings and `V#10` would sort before `V#2`. "Newest version" is one `Query` with `ScanIndexForward=false, Limit=1`.
- **Hub entries keep the first publication time** in their sort key, so an update does not move a page to the top of its hubs. Each entry copies the listing fields (title, summary, type, tier, region, dates, index status), so a hub renders from one `Query`. The live page record lists its hubs, so an update deletes the entries of hubs the page left and a withdrawal deletes them all.
- **Taxonomy is one partition.** The TD put each node under its own key; then listing the tree needs a scan or an index. `TAX#tree` / `NODE#<slug>` keeps per-node reads and makes the whole tree one `Query`.
- **Items stay small: metadata only.** Every bundle file, including `page.oui`, lives in R2 under the page's path (`2026/11/<slug>/index.html`, `index.md`, `data.json`, `sources.json`, `meta.json`, `page.oui`, `charts/<id>.csv`). This changes the TD's "items hold metadata, .oui, summary": a 30 KB `.oui` would cost 30 write units on every version and every update. A page record is 2–4 KB and the API refuses one over 64 KB.
- **No secondary index.** Every read path is a key lookup or a single-partition query. The one index the free tier can afford is kept for a later need (for example pages by index status, if sitemaps outgrow the hub listings).
- **Paced writes.** A write unit is 1 KB, per item, rounded up. A typical publish (a 3 KB page record, 6–10 hubs, 1–3 entities) writes: page lock 2, idempotency 2–3, path claim 1, version and LATEST 3 each, hub entries 2 each (12–20), new or changed entities 1 each: **about 30 write units**, over a second of the free table's 25 per second. The schema's worst case is far larger: 8 categories with 4 levels each plus 50 entities is 84 hubs (about 170 units, and as many again to delete them when a page leaves them), 50 entities, and a page record of up to 64 KB (2,000 data IDs) written twice: **500–600 units, about 20 seconds of capacity**. So the safe pace is **at most one page per second, and fewer when pages are large**; DynamoDB's burst capacity (300 seconds of unused units) absorbs short runs. The Bureau Design's Press default of 5 pages a second (3.12) is too fast for this table: flagged for #9 and #20. Entities the live version already lists with the same name and kind are not rewritten. The Worker does not retry a throttled write: it releases the idempotency key and answers 503 `throttled` with `Retry-After`, and the laptop retries with the same key. Writes inside one publish run at most 6 at a time.
- **One writer per page.** Publish, withdraw and index promotion take the page lock (`PAGE#<id>` / `LOCK`, a create-only put; one older than 2 minutes is stale and may be taken over) before they read the page, and give it back when done; a second operation meanwhile gets 409 `page_busy`. Page files are served by the CDN straight from R2 at the page's public path, so they are rewritten in place; versioned R2 keys would not remove the need to serialise the writers of that public copy, and would double storage against the 10 GB free tier. Each file is stamped with its version (R2 custom metadata), so a reader (the MCP server) can tell a file written ahead of `LATEST` and re-read the record instead of mislabelling it.
- **Write order.** Body checks (no lock, no writes), page lock, reads and conflict checks (an older version, a reused version number with other content, a URL change, the version of a withdrawn page: refused before any write), path claim, R2 files, version record (`attribute_not_exists OR same bundle_sha256`), hub entries, entities, search row, then `LATEST` (`attribute_not_exists OR version <= n`) carrying a `cleanup` note of the hubs and files the previous version had and this one dropped; then those hub entries and files are removed, the purge runs, and the note is cleared. Every step is idempotent and the note is part of the commit, so a retry after a failure anywhere (or the next version) finishes the job, including the purge of the hubs and files the page left. Withdraw and index promotion re-run all their steps on every attempt. A curation version records the request's key hash, so a retry finds the version it already wrote instead of publishing another. A write that got no answer (timeout, 5xx) may have been applied: the Worker keeps the idempotency key and page lock until they go stale and answers 503 `outcome_unknown`. Only the curation and redirects use transactions (ADR 0005).
- **Read consistency.** The write API reads strongly (it reads what it just wrote). Public readers (the MCP server, the site) read eventually consistently, which costs half the read units and lags by about a second.
- **Timestamps** are stored in one form, `Date.toISOString()` (three fractional digits), so they compare and sort as times; days that do not exist are refused.

**Reports are not in this table.** Reader reports live in their own D1 database (ADR 0006), so report traffic can never spend the write units publishing needs.

**Search (D1).** One D1 database per publication year, bound as `SEARCH_<YYYY>` (each free database holds 500 MB). The schema is `workers/shared/migrations/search/0001_pages.sql`: a `pages` table and an external-content FTS5 table over title, summary, entities, tags and categories (with parents), kept in sync by triggers (the update trigger fires only when an indexed column changes, so an index-status change is one row write). Queries become quoted words only, so no FTS5 syntax from a reader reaches the database. Shards are searched newest first until the limit is met. A page lives in the shard of its first publication year; the API refuses a page whose year has no shard bound, before writing anything.

**Index status.** `index_status` is stored on the page record, its hub entries and its search row. `POST /v1/pages/{id}/index` changes all three and purges the page's URLs. The robots meta and sitemap entry must follow `index_status` at serving time (an open item for the site and sitemap work, #14).

## Consequences

- Adding a hub kind, an entity or a category needs no schema change.
- A typical publish costs about a second of the table's write capacity and a worst-case one about 20 seconds; the publish rate is set on the laptop (at most one page a second).
- Items are validated in tests against the items schema, so a change to an item shape must change the contract first.
- For the docs owner (#9): the TD's DynamoDB table should gain the path claim, `IDEM#` and page-lock rows, the `TAX#tree` change, "files in R2, not in items" and "reports in D1, not in DynamoDB"; the Bureau Design's Press pace (5 pages a second) should drop to at most one (#9, #20).
