# ADR 0004: DynamoDB single table, search shards and storage split

- Status: accepted (Phase 6, issue #6)
- Date: 15 November 2026
- Contracts: `contracts/cloud/dynamodb-items.schema.json`, `contracts/bundle/meta.schema.json`

## Context

DynamoDB is the cloud's database of record. The free tier gives 25 read and 25 write capacity units, shared by every table and index in the account, and caps items at 400 KB. A write unit is 1 KB per second, so item size, not item count, sets publishing speed. The Technical Design (TD, "DynamoDB") fixed a single table with `PAGE#`, `HUB#`, `ENT#`, `TAX#`, `URL#` and `HOME#CURATION` keys. The write API also needs idempotency records and the report intake needs somewhere to store reports.

## Decision

One table, string keys `PK` and `SK`, no secondary index. Item shapes are in the items schema; every content record carries `lang`.

| Item | PK | SK | Notes |
| --- | --- | --- | --- |
| Page version | `PAGE#<page_id>` | `V#<000001>` | Written once per version, never changed |
| Live page | `PAGE#<page_id>` | `LATEST` | Moved last on publish: the commit point readers see |
| Path claim | `URL#<page path>` | `PAGE` | One page per URL and R2 prefix; lookup by path |
| Hub entry | `HUB#<hub>` | `<first published_at>#<page_id>` | Hubs: `latest`, `cat/<slug>` (and each parent), `ent/<entity id>`, `region/<region>` |
| Entity | `ENT#<entity id>` | `META` | Its pages are hub `ent/<entity id>` |
| Taxonomy node | `TAX#tree` | `NODE#<slug>` | Changed from the TD's `TAX#<id>` / `META` (below) |
| Redirect | `URL#<old path>` | `REDIRECT` | |
| Curation | `HOME#CURATION` | `V#<000001>` and `LATEST` | ADR 0005 |
| Idempotency | `IDEM#<SHA-256 of key>` | `META` | Bookkeeping, no `lang`; TTL 7 days |
| Report | `REPORT#<YYYY-MM>` | `<created_at>#<report id>` | ADR 0006; TTL 180 days |

- **Versions are zero-padded** (`V#000010`), because sort keys compare as strings and `V#10` would sort before `V#2`. "Newest version" is one `Query` with `ScanIndexForward=false, Limit=1`.
- **Hub entries keep the first publication time** in their sort key, so an update does not move a page to the top of its hubs. Each entry copies the listing fields (title, summary, type, tier, region, dates, index status), so a hub renders from one `Query`. The live page record lists its hubs, so an update deletes the entries of hubs the page left and a withdrawal deletes them all.
- **Taxonomy is one partition.** The TD put each node under its own key; then listing the tree needs a scan or an index. `TAX#tree` / `NODE#<slug>` keeps per-node reads and makes the whole tree one `Query`.
- **Items stay small: metadata only.** Every bundle file, including `page.oui`, lives in R2 under the page's path (`2026/11/<slug>/index.html`, `index.md`, `data.json`, `sources.json`, `meta.json`, `page.oui`, `charts/<id>.csv`). This changes the TD's "items hold metadata, .oui, summary": a 30 KB `.oui` would cost 30 write units on every version and every update. A page record is 2–4 KB and the API refuses one over 64 KB.
- **No secondary index.** Every read path is a key lookup or a single-partition query. The one index the free tier can afford is kept for a later need (for example pages by index status, if sitemaps outgrow the hub listings).
- **Paced writes.** One publish writes about 2 page items, 4–10 hub entries, the entities and 2 idempotency writes: roughly 15–30 write units. The laptop's Press paces publishes (Bureau Design 3.12). The Worker does not retry a throttled write: it releases the idempotency key and answers 503 `throttled` with `Retry-After`, and the laptop retries with the same key. Writes inside one publish run at most 6 at a time.
- **Write order.** Path claim, R2 files, version record (`attribute_not_exists OR same bundle_sha256`), hub entries, entities, search row, then `LATEST` (`attribute_not_exists OR version <= n`), then the purge. Every step is an idempotent put, so a retry after a failure anywhere completes the publish. Only the curation uses a transaction (two small items, ADR 0005).

**Search (D1).** One D1 database per publication year, bound as `SEARCH_<YYYY>` (each free database holds 500 MB). The schema is `workers/shared/migrations/search/0001_pages.sql`: a `pages` table and an external-content FTS5 table over title, summary, entities, tags and categories (with parents), kept in sync by triggers. Queries become quoted words only, so no FTS5 syntax from a reader reaches the database. Shards are searched newest first until the limit is met. A page lives in the shard of its first publication year; the API refuses a page whose year has no shard bound, before writing anything.

**Index status.** `index_status` is stored on the page record, its hub entries and its search row. `POST /v1/pages/{id}/index` changes all three and purges the page's URLs. The robots meta and sitemap entry must follow `index_status` at serving time (an open item for the site and sitemap work, #14).

## Consequences

- Adding a hub kind, an entity or a category needs no schema change.
- A full publish costs about 1 second of the table's write capacity; the publish rate is set on the laptop.
- Items are validated in tests against the items schema, so a change to an item shape must change the contract first.
- For the docs owner (#9): the TD's DynamoDB table should gain the path claim, `IDEM#` and `REPORT#` rows, the `TAX#tree` change and "files in R2, not in items".
