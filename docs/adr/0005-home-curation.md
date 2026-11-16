# ADR 0005: Home page curation record and checks

- Status: accepted (Phase 6, issue #6)
- Date: 15 November 2026
- Contracts: `contracts/cloud/curation.v1.schema.json`, `contracts/cloud/dynamodb-items.schema.json#/$defs/curationRecord`
- Code: `workers/shared/src/curation.ts` (checks), `workers/api/src/home.ts` (publish and rollback)

## Context

The home page is curated every 2–3 days by the Frontpage preset (`/bureau curate home`; TD "Home page curation", Bureau Design 8.6). The curation is data the SSR Worker reads at request time; publishing one must never rebuild or redeploy anything. `news home check` validates a draft on the laptop; the write API must validate it again, because pages can be withdrawn between drafting and publishing.

## Decision

**Slots.** One lead, exactly two side stories, three to five sections of three or four stories each. Every slot is a page ID with at most one figure (`figure_id`, a data ID of that page) or one chart (`chart_id`, a chart of that page). The lead may carry `still_leading: true`. Slots never copy page text: an updated page shows its new title and figures.

**Candidate signals** (computed on the laptop by `news home candidates`, Bureau Design 4.5, not stored in the cloud): tier, freshness, category, region, page type, key figures by data ID, chart options and traffic percentile.

**Checks** (run by `news home check` and again by `PUT /v1/home/curation`; each failure is one detail in a 400 `invalid_curation`):

| Rule | Detail code |
| --- | --- |
| Body matches `curation.v1`, including slot counts and no rationale field | `schema.*` |
| Every page exists | `page_missing` |
| Every page has a live record (versions without `LATEST` mean a publish never finished) | `page_unpublished` |
| No page is withdrawn | `page_withdrawn` |
| No page appears twice | `duplicate_page` |
| Every figure and chart belongs to its page | `figure_not_on_page`, `chart_not_on_page` |
| At least 3 primary categories (top level) across the slots | `breadth_categories` |
| At least 3 regions across the slots | `breadth_regions` |
| At most 2 stories about one entity | `breadth_entity` |
| At least one section outside finance | `breadth_non_finance` |
| The lead is at most 7 days old, unless it is Tier 1 and marked `still_leading` | `lead_stale` |

The draft's own check results travel with it (`checks.passed` must be `true`) and are stored; the API stores its own run as `server_checks`.

**Record.** `HOME#CURATION` / `V#<n>` holds the curation as sent plus `version`, `published_at` and `server_checks`; a few KB. `LATEST` is a copy of the live version with `made_live_at`. Publishing writes `V#<n>` (`n` = newest + 1) and moves `LATEST` in one transaction, conditional on `LATEST` still being the version read, so two publishes cannot interleave. The same transaction writes `IDEM#<hash of the Idempotency-Key>` / `RESULT` with the version and the request hash (and `V#<n>` keeps `request_id`). A retry of the same request (same key and body, after a failed purge, a lost response or a crash) reads that record first, before the draft is validated again, and only purges, answering with the version it committed: one curation never becomes two versions, however many curations were published in between, and a page withdrawn since the commit does not turn the retry into a validation failure. The record expires with the idempotency record (7 days). Rollback copies an existing `V#<n>` to `LATEST`; the next publish still continues after the newest version, so version numbers are never reused.

**Rationale stays local.** The curator's one-line rationale per pick stays in PostgreSQL (`home_curations`); the schema refuses it.

**Purges.** Publish and rollback purge exactly the home page and the region views: `https://news.karthik.nz/` and `/region/<code>` for each region in `NEWS_REGION_VIEWS` (default: north-america, europe, india, china, asia, australia, global, from the Bureau coverage regions). Nothing else is purged; nothing is rebuilt.

**Between curations** (render-time rules for the SSR site, #14): a withdrawn or deleted curated page is skipped and its slot back-filled with the best recent page of the same section by tier and freshness; the "Latest" list is the newest 8–10 entries of `HUB#latest`; a region view shows the curated stories for that region topped up with that region's newest pages (`HUB#region/<code>`).

## Consequences

- Publishing a curation costs two small transactional writes and one purge call.
- The table-driven test in `workers/api/src/home.test.ts` covers every rule above; the shared module is the single implementation, which `news home check` (Rust, #5) must match. A Rust port should run the same fixture cases.
- Open for the site (#14): the region-view URL shape `/region/<code>` is fixed here; if the site serves region views elsewhere, change `workers/shared/src/urls.ts` and this ADR together.
