# news-press

Press, the laptop side of publishing (Bureau Design §3.12; ADR 0004 "Write order"; ADR 0006).

| Module | What it does |
| --- | --- |
| `store` | `stage_put`, `stage_withdraw`, `stage_index_promotions` (take `&mut Authorized`: a row is written only inside the transaction that passed the G-tier check), `drain`, `stats` |
| `api` | The write-API client: `Idempotency-Key` on every write, a token bucket of 5 a second with bursts of 10, 5xx / 429 / `page_busy` / lost answers retried with backoff (1, 2, 4 … 30 s, or `Retry-After`), any other 4xx a `bug` |
| `budget` | The index budget's ranking: `score + 10·[Tier 1] + uniqueness`, `templated` pages skipped, capped at `publish.index_per_day` |
| `sync` | `PUT /v1/taxonomy` from the `taxonomy` table, `PUT /v1/redirects` |
| `bundle` | Reads and verifies `work/pages/<page_id>/v<version>/` (`manifest.json` and the files it lists) |
| `prune` | `news prune`: working files of published pages |
| `fake` | A scripted in-memory write API for tests |

## Outbox rows

`publish_outbox` (migrations 0005, 0202, 0900). A row is **to send** (`sent_at` and `dead_at` null),
**sent**, or **dead** (the API answered a 4xx: class `bug`, `last_error` holds the code and message, never
retried). An unsent row with `unknown_since` is one whose last send got no answer; the same key goes again
and the API answers once. Keys: `page:version:sha` for a put, with `:withdraw` or `:index` added for the
other ops. The drain stops on a row with no answer (order matters) and carries on past dead rows.

## Rules

- `news publish` and `news home publish` have no CLI form. Chief calls Press in process as the authorised
  step of a live run. Only `api::ReqwestApi` reads the write-API key (`news_core::secrets`).
- The origin is `NEWS_API_ORIGIN` (https, or loopback for tests); redirects are never followed.
- Entities and their hubs are written by the API from each page's `meta.json`, so only the taxonomy and
  redirects are synced.
- The index budget runs only as Edition step 11 (never in the background).

Tests: `cargo test -p news-press` (client, pacing, budget, bundle; no network, virtual clock) and the
Postgres tests in `cli/crates/news/tests/postgres/press.rs` (`bun run test:integration`).
