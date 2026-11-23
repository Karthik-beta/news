# ADR 0010: SSR site routing, data adapter and edge caching

- Status: proposed
- Date: 2026-11-22
- Issue: #14 (Phase 6, wave 4)
- Builds on: ADR 0001 (rendering), 0004 (table, search shards), 0005 (curation and its render-time rules), 0006 (security, report intake)
- Contracts: `contracts/cloud/search-api.openapi.json`, `curation.v1.schema.json`, `dynamodb-items.schema.json`

## Context

Article pages are static files in R2, served by the CDN with no Worker or database on a view (TD "Static article pages"). Everything that lists pages (home, region views, hubs, "Latest", search, feeds, sitemaps) must be rendered on request by the `site` Worker from #6's data layer, cached at the edge for hours or days, and must change when the data changes, with no build or deploy. The write API already purges exact URLs after each change (`workers/shared/src/urls.ts`: `/`, `/region/<code>`, `/latest`, `/<category slug>`, `/entity/<id>`, page files, redirect sources). The Workers free plan gives about 10 ms of CPU and 50 external subrequests per request and a daily request quota; DynamoDB gives 25 read units a second, shared with publishing.

## Decision

### 1. One Worker, Astro on the Cloudflare adapter, articles bypass it

- `site/` keeps `output: "static"` and adds `@astrojs/cloudflare` (pinned exactly). Routes that read data declare `export const prerender = false`; everything else is prerendered. Bun runs `astro dev` and `astro build`; the built Worker runs in workerd (Miniflare locally, Cloudflare in production). Worker name `news-site`, config in `site/wrangler.jsonc`.
- **Zone routes** (applied by #21): `news.karthik.nz/*` → `news-site`; `news.karthik.nz/2*` → no Worker, so article paths (`/<yyyy>/<mm>/<slug>…`) go straight to the R2 custom domain; `news.karthik.nz/mcp*` → `news-mcp`. The search API keeps the contract's `/v1/search` on a hostname #21 picks.
- **Shell assets** (`/_astro/*`), `robots.txt` and the prerendered pages are the site Worker's static assets: Cloudflare serves them without running the Worker. Article HTML in R2 links to the same `/_astro/site.css` and `/_astro/islands.js` (the renderer's fixed names).
- **Reserved first path segments.** Category hubs live at `/<slug>`, so a slug must not shadow a route. `PUT /v1/taxonomy` refuses (400 `taxonomy.reserved_slug`) a slug that does not start with a letter (so none collides with the `/2*` bypass) or is one of: `latest search entity region sitemaps fixtures report data about methodology glossary privacy legal contact corrections feeds api mcp v1 _astro __preview`. The site resolves fixed routes before categories anyway.
- **No AI.** `workers/shared/src/ai-guard.test.ts` also scans `site/` (sources, `package.json`, `wrangler.jsonc`).

### 2. One data adapter

`site/src/data/index.ts` is the only module that touches storage. It reuses #6's code; it adds no second read layer.

```ts
export interface SiteData {
  reader: Reader;                 // workers/shared/src/read.ts, strongly consistent (see below)
  curation(): Promise<Curation | undefined>; // LATEST read strongly, or the preview draft (section 6)
  shards: SearchShard[];          // SEARCH_<YYYY> bindings, newest first (shared/src/search.ts)
  pageJson(path: string, file: string, maxBytes: number): Promise<unknown | undefined>; // R2 PAGES, get only
}
export function siteData(env: SiteEnv): SiteData;
```

- `NEWS_DATA=dynamodb` (production): `DynamoTable` with `consistentRead: true` and the site's own read-only key (IAM as the MCP key in `workers/README.md`, plus `HOME#CURATION`). Every data read on a cache miss is strongly consistent, not only the curation `LATEST`: the api purges right after its writes (curation, page, hub, redirect), and an eventually consistent read in that window would cache the old page for its whole TTL. Strong reads cost twice the read units; misses are rare.
- `NEWS_DATA=local` (dev, tests, fixtures): one `FakeTable` per isolate, seeded from `site/fixtures/data/<set>/*.json` (items validated against the items schema in a test); D1 and R2 are Miniflare's local bindings, the search shard migrated with `workers/shared/migrations/search` and filled with `indexPage` from the same seed. Tests reach the fake through `localTable()` to change data between requests. DynamoDB Local works too (loopback endpoint), for the integration suite.
- With `NEWS_ENV=production` any `NEWS_DATA` other than `dynamodb` throws at startup. `site/wrangler.jsonc` sets `NEWS_ENV=production`, `NEWS_DATA=dynamodb`, `NEWS_SITE_ORIGIN`, `NEWS_REGION_VIEWS`; bindings `PAGES` (R2), `SEARCH_<YYYY>` (D1, `SELECT` only), `SEARCH_RATE_LIMITER` (30/min per IP); secrets for the read-only key.
- **Budget per cache miss:** home is at most 12 DynamoDB calls (curation, one `BatchGetItem` for the slot pages, `HUB#latest`, taxonomy, up to 8 back-fill queries: 5 sections, the lead and 2 side slots, each possibly a different hub) and one R2 read per slot that shows a figure or chart; a hub is 2–3 calls. Well under 50 subrequests. Bun render budget: at most 4 ms p95 with data in memory, to leave headroom for workerd's CPU limit.

### 3. Routes and cache policy

One table in code, `site/src/server/routes.ts`, gives each route its rendering and cache policy; Astro middleware applies it. Responses carry `Cache-Control: public, max-age=<browser>, s-maxage=<edge>`.

| Route | Reads | Rendered | Edge | Browser | Purged by the api on |
|---|---|---|---|---|---|
| `/` | curation, slot pages, `HUB#latest`, taxonomy | SSR | 5 min (1 h once #21 verifies purge by URL) | 60 s | curation publish and rollback; every page publish, withdrawal, index change |
| `/region/<code>` (codes in `NEWS_REGION_VIEWS`) | curation, slot pages, `HUB#region/<code>` | SSR | 5 min (1 h once verified) | 60 s | curation; any change to a page in that region |
| `/latest`, `/<category>[/<sub>…]`, `/entity/<id>` (first page, no query) | the hub (+ taxonomy; entity: `ENT#`, one R2 `data.json`) | SSR | 1 day | 60 s | any change to a page in that hub |
| The same with `?after=<cursor>` or `?region=<code>` | the hub | SSR | 1 h | 60 s | not purged |
| `/search?q&category&region&type&from&to` | D1 | SSR, `noindex` | 10 min | 60 s | not purged |
| `/rss.xml`, `/atom.xml`, `/<category>/rss.xml`, `/<category>/atom.xml` | newest 20 of the hub | SSR | 1 h | 15 min | not purged |
| `/sitemap.xml` (index), `/sitemaps/<yyyy>-<mm>.xml` | D1 | SSR | 1 day | 1 h | not purged |
| `/llms.txt` | taxonomy, config | SSR | 1 day | 1 h | not purged |
| Redirect: a path with `URL#<path>` / `REDIRECT` | DynamoDB | SSR, the stored status (301 or 308) | 1 day | 1 day | redirect write |
| 404 | (the lookups that missed) | SSR, prerendered template | 10 min | 60 s | — |
| `/robots.txt`, `/methodology`, `/about`, `/glossary`, `/privacy`, `/legal`, `/contact`, `/report`, `/data`, `/feeds` | config at build | prerendered asset | Cloudflare's asset cache, replaced on deploy | 1 h | — |
| `/_astro/*` | — | asset | as above | content-hashed: 1 year, `immutable`; fixed names: 1 h | — |
| Search daily-cap answer | — | SSR, 503, `Retry-After` to 00:00 UTC | `no-store` | `no-store` | — |
| Preview mode, `/fixtures/*` | — | local builds only | `no-store` | `no-store` | — |

- **Edge cache = the Workers Cache API.** A Worker's responses are not cached by the CDN on their own, so the middleware looks up and stores (`ctx.waitUntil(caches.default.put(…))`) under a **canonical key**: `NEWS_SITE_ORIGIN` + the canonical path + only the known query parameters of that route, in a fixed order. Unknown parameters (`utm_*`) are ignored and left out of the key, so `/?utm_source=x` is the purged `/`. Entity paths are keyed as `hubPath()` writes them (`encodeURIComponent` of the ID), so the api's purge URL and the key are the same string. One shared origin constant and one region list serve both Workers (`workers/shared`), and `bun run validate` fails if the site's and api's wrangler values for `NEWS_SITE_ORIGIN` and `NEWS_REGION_VIEWS` differ. **Stale copy:** each response is stored with a hard TTL longer than its fresh time (`s-maxage` is the fresh time). On a render or DynamoDB error the Worker serves the stored copy with `s-maxage=30`; with none, a plain 503 page (`no-store`). A trailing slash on a hub answers 301 to the path without it; a known parameter with an invalid value (unknown region) is a 404. Cursors are opaque and validated (shape, length, decodable key for that hub); a malformed one is a 400.
- **Abuse limits.** The taxonomy is cached in the isolate for about 60 s. An unknown first path segment is rejected against it (404) before any `URL#` lookup. The existing `SEARCH_RATE_LIMITER` binding also applies to cache misses of 404, cursor and filter requests, not only search.
- **How a curation shows up without a build or deploy:** every data route reads storage on a cache miss and nothing data-dependent is imported at build time (a test fails if a `prerender = false` route imports from `site/fixtures/` or any JSON). `PUT /v1/home/curation` commits `LATEST`, then purges `/` and every `/region/<code>`; the next request misses and renders the new curation. The 5-minute edge TTL on home and region views bounds staleness if a purge is lost or does not evict Cache API entries; #21 verifies purge-by-URL on the real zone before the TTL is raised to 1 h (one constant).
- **Robots.** Dynamic pages: home, region views, first hub pages and entity hubs are `index, follow`; cursor pages, filtered views, search and 404 are `noindex, follow`. Article pages (closing ADR 0004's open item): the renderer writes `<meta name="robots" content="noindex">` or `<meta name="robots" content="index, follow">` from `index_status`. R2 cannot attach a per-object `X-Robots-Tag`, and articles never run a Worker, so the tag in the file is the only per-page switch. **Rewriting that tag in R2's `index.html` is out of #14** (tracked in #56): it becomes its own security-reviewed issue (Opus), which must define the order against the `index_status` update, crash and retry behaviour, that a retried publish never overwrites a later promotion, and bundles with no tag.

### 4. Lists

- **Hubs paginate at 20** with cursors, not page numbers: `?after=<cursor>` is `encodeCursor` of the last entry's sort key (`read.ts`). DynamoDB has no offset, and a cursor page does not shift when a newer page is published. "Older" links only; the first page has no parameter.
- **Region filter on category hubs** (`?region=<code>`): the Worker reads the hub newest first and keeps matching entries until it has 20 or has examined 200 entries; the cursor is the last examined key, so the next page continues the scan. No per-region hub keys (they would add writes to every publish, ADR 0004).
- **Entity hubs** group their 20 entries by the year of `published_at`. The **latest-figures strip** shows up to 4 points from the entity's newest page: the first four entries of that page's `data.json` `points`, formatted by the shared formatters with label, period and source, linking to the page. `data.json` over 256 KB is not read (no strip): parsing costs about 8 ms of CPU per MiB (ADR 0006).
- **Feeds**: RSS 2.0 and Atom 1.0, the newest 20 entries of `HUB#latest` or `HUB#cat/<slug>`, with the hub entry's title, summary, absolute link, first-publication and update times. Every published page appears, whatever its index status.
- **Sitemaps** list only pages whose `index_status` is `index`, from the D1 shards (the hubs would mean reading every page ever published). A new migration `0002_index_status.sql` adds `CREATE INDEX pages_index_status ON pages (index_status, published_at)`, so the queries read only indexed rows. `/sitemap.xml` lists one child per month that has indexed pages; a child lists that month's pages with `lastmod` = `updated_at` (50,000 URLs a month would need 1,600 indexed pages a day).
- **`llms.txt`** is rendered from the taxonomy and config: what the site is, the top-level categories, methodology, the data-access page, feeds, the sitemap, the MCP endpoint. **`robots.txt`** allows all agents, disallows `/search`, names `/sitemap.xml`. **Print:** print CSS ships with the shell; there is no separate print route.
- **Redirects** are looked up only after a fixed route, a category and an entity have missed, so a normal hit costs no extra read. Category renames (a new node plus a redirect) and merged entities are the uses.

### 5. Home and region views

Render-time rules are ADR 0005's "Between curations"; this fixes what that left open. The pure functions live in `site/src/home/compose.ts` and take already-read data, so fixtures test them without storage:

```ts
export function backfillHubs(c: Curation, pages: Map<string, PageRecord>): Map<SlotRef, string>; // slots needing one → hub to read
export function composeHome(input: HomeInput): HomeModel;   // lead, side[], sections[], latest[]
export function composeRegion(input: HomeInput, region: string, regionEntries: HubEntry[]): HomeModel;
```

- **Slot text comes only from the page record** (`title`, `fly_title`, `summary`, `path`, `tier`, `region`, dates). A curation carries no text. A figure or chart is read by code from the page's R2 `data.json` or `charts/<id>.json` and formatted by the shared formatters; one that cannot be read is left out, the story stays.
- **Back-fill.** A slot whose live record is missing or `withdrawn` is replaced by the best entry of a hub: for a section slot `HUB#cat/<section.category>`; for the lead and side slots the withdrawn record's first category (the record keeps its categories; each of the three can read a different hub), or `HUB#latest` when the record is gone. Candidates are that hub's newest 20 entries not already on the page; best is lowest `tier`, then newest `published_at`. A back-filled slot shows no figure or chart. With no candidate the slot is dropped; a section left empty is hidden; the lead falls back to `HUB#latest`.
- **"Latest"** is the newest 10 entries of `HUB#latest` (read 30) that are not in a slot; fewer when fewer exist, and an empty state when none.
- **Region views** show the curated stories whose page `region` is the code, in curation order, then the newest entries of `HUB#region/<code>` not already shown, to 12 stories; an "Older" link continues as a 20-per-page hub (`?after=`). A code not in `NEWS_REGION_VIEWS` is a 404, because only those are purged.

### 6. Local preview of a draft curation (for `news home preview`, #20)

- **Built only on request.** With `NEWS_PREVIEW=1` at build time, an integration (like the fixtures one in `astro.config.mjs`) injects `PUT` and `DELETE /__preview/curation`. A production build contains no preview code.
- **Refused at run time too.** The handlers answer 404 unless `NEWS_ENV=local` and the request host is `127.0.0.1`, `localhost` or `[::1]`.
- **Behaviour.** `PUT` validates the body against `curation.v1` (the precompiled validators) and holds it in the process's memory; `curation()` then returns it instead of `LATEST`; `DELETE` clears it. `news home check` remains the place for the rule checks. Preview requests skip the cache lookup and the put. Every response of a preview server is `no-store`, carries `noindex`, and carries `X-News-Preview: <sha256 of the draft>` only while a draft is held (the hash of the bytes as `PUT`), and is otherwise identical to production markup, so Board's screenshots show what readers will see. #20 asserts the header equals the hash of the draft it pushed.
- #20 runs the preview build with `wrangler dev --local` on 127.0.0.1, with `NEWS_DATA=dynamodb` and the site's read-only key (live pages, so a withdrawn page back-fills in the preview as it will in production), or `NEWS_DATA=local` in its tests.

### 7. Search

- **The `search` Worker is the public JSON API** (`/v1/search`, the existing contract). **The site's `/search` page queries D1 itself** through the same shared module, with no hop between Workers: one implementation, two thin callers. Swapping the engine (TD "upgrade path") changes `workers/shared/src/search.ts`.
- Both: at most 8 words (`toFtsQuery`), filters `category`, `region`, `type`, `from`, `to`; the site shows 20 results and no further pages ("refine the search"); 30 requests a minute per IP (per location); responses cached under a normalised key (lower-cased words in order, filters sorted) for 10 minutes, so popular queries are answered from the edge.
- **Daily cap.** Any D1 failure is a `SearchUnavailableError` from the shared module. The API answers 503 `search_unavailable`; the page answers 503 with `Retry-After` and a plain message (search has a daily limit on the free database and returns after 00:00 UTC; browse the categories or "Latest" meanwhile), with links. When the error is D1's daily-limit error (its text recorded from a real one in #21), the isolate stops querying until 00:00 UTC, as the report intake does (ADR 0006). An empty result is a 200 with its own empty-state message.

### 8. Static pages and the report form

Prerendered from `site/src/config/` at build: methodology, about, glossary, privacy, legal, contact, `/report`, `/data` (data access and MCP instructions), `/feeds` (the feed addresses, linked from the footer), 404. No statistic is typed into their prose; configured values are inserted from config. `/report` holds one island (`ReportForm.client.tsx`) that posts JSON to the api's `POST /v1/reports` (origin from `NEWS_API_ORIGIN` at build): the form's `what` is the intake's `text`, its `page` field takes a page ID or the address of a page (the last path segment is the ID; `?page=<page_id>` fills it in), the `website` honeypot is hidden from people, the limits are checked before sending, and the copy's "Form text" table is every word the form shows, including the confirmation on 202; without JavaScript the page says so and gives the contact address.

## Tests that prove it (#14 "Done when")

| Decision | Test |
|---|---|
| Curation is data, not a build | Miniflare: request `/`, write a new `LATEST` into `localTable()`, purge the key, request again: the next request after the purge differs (at worst within 5 minutes in production) and the Worker bundle is unchanged. A test fails if a dynamic route imports fixture or JSON data |
| Back-fill, region top-up, "Latest" | `compose.test.ts` over the fixture sets `normal`, `withdrawn-lead`, `region`, `empty-latest`: back-fill hub and pick, figure dropped, 12-story region view, Latest count and exclusion; slot text equals the page record's, and an updated page record changes the slot |
| Pagination and filters | A 45-entry hub gives 20/20/5 with working `after` links; region filter across a scan boundary; malformed cursor 400 |
| Search | Filtered results from local D1; empty state; D1 throwing gives the 503 page and API answer; daily-limit error stops queries until UTC midnight (fake clock) |
| Machine access | Sitemap lists only `index` pages (one `noindex` fixture absent); RSS and Atom parsed and checked for the required elements, unique IDs, absolute links and date formats; `robots.txt` and `llms.txt` served; a redirect answers its stored 301 with `Location` |
| Purge keys | For every URL that `homeUrls`, `hubPath` and the api's `affectedUrls` produce, the middleware's cache key for that URL is equal to it |
| Cache policy | Every row of the route table requested in Miniflare: `Cache-Control` matches `routes.ts`, and `routes.ts` matches this ADR; `/?utm_source=x` hits the entry for `/`; `bun run validate` fails on differing site/api origin or region values |
| Stale copy and abuse | A render error with a stored copy serves it with `s-maxage=30`, without one a 503; an unknown first segment makes no `URL#` read; the rate limiter is hit on 404, cursor and filter misses; a malformed cursor is 400; the taxonomy is read once per ~60 s |
| Preview | Preview requests neither read nor write the cache; `X-News-Preview` is absent with no draft held and equals the sha256 of the draft while one is held (#20 asserts the same against the draft it pushed) |
| Production guards | A production build has no `/__preview/curation` route string and no preview code in `dist/server`; preview handlers 404 with `NEWS_ENV=production` or a non-loopback host; `NEWS_DATA=local` with `NEWS_ENV=production` fails; `ai-guard` covers `site/` |
| api changes | Reserved category slugs, and slugs not starting with a letter, refused |
| Report form | Miniflare with the api Worker and local `REPORTS`: the form posts, the row is stored, the confirmation shows |
| Quality | axe clean on every template, light and dark; home and hub meet `site/TESTING.md`'s browser budgets, and their server render (data in memory) stays under 4 ms p95 in Bun |

## Consequences

- Only `/2*` article views skip the Worker. Every listing view (home, hubs, feeds, search) runs the Worker and counts toward the free daily request quota; the Cache API saves only DynamoDB and render work. Rough estimate: 100,000 views a day with 20% on listings is 20,000 Worker requests, against the free quota of 100,000 a day. At the cap we accept errors in v1, and Board shows a usage alarm (#21). TD's risk row says otherwise and needs correcting in #18.
- Feeds, cursor pages and filtered views can lag a publish by up to an hour; search by 10 minutes; sitemaps by a day. None of them is purged, which keeps the api's purge list (and its subrequest plan, ADR 0004) as it is.
- #14 also changes `workers/api` (reserved slugs) and adds a search migration; the api change needs the Opus security review. The robots-tag rewrite is a separate issue (section 3).
- A new year's search shard means a binding in `site/wrangler.jsonc` too, and a deploy, once a year.
- Once the robots-tag issue lands, `index.html` in R2 may differ from the bundle's by that tag; nothing checks `index.html` against the manifest after publish, and nothing may start to.
- For #21: the three zone routes; the search API hostname; the site's read-only DynamoDB key; check on the real zone that a purge by URL evicts a Cache API entry (then raise the home and region TTL to 1 h; if not, keep 5 minutes), build the quota usage alarm in Board, and record D1's daily-limit error text. R2's own 404 answers a missing article path.
- For the docs owner (#18): TD "Dynamic pages" and "Cloud services" should say shell assets are the site Worker's static assets (not R2), and add the route split, the robots tag and the cursor pagination.

## Alternatives considered

- **The Worker serves every path, articles from R2 through a binding.** Simplest routing, but every article view would spend the free daily request quota.
- **Hubs under a prefix (`/c/<slug>`)** to avoid the catch-all route: changes the URL design and `urls.ts`; the `/2*` exclusion and reserved slugs make it unnecessary.
- **Page-number pagination**: needs an offset DynamoDB lacks, and every publish shifts every page.
- **The site calling the search Worker over a service binding**: an extra hop and request for the same code.
- **Sitemaps from `HUB#latest`**: reads every page ever published to list the indexed few.
- **Purging feeds on publish**: up to 2 more URLs per hub per publish against the 45-call plan, for feeds that tolerate an hour.
- **Preview by a curation passed at server start**: a restart per draft; the in-memory `PUT` is as safe behind the build flag and loopback check.
