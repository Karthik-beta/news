---
name: news-source-connector
description: "How to add or change a data-source connector in the Rust `news` CLI (cli/crates/news-connectors): source registry entry with licence and attribution, fetching, parsing into normalised data points, embeds and images, recorded fixtures and tests. Use this whenever you add a source (SEC EDGAR, FRED, IMF, World Bank, Eurostat, RBI, arXiv, NASA, …), fix a connector, or touch parsing of filings, statistics, papers or press releases."
---

# Adding a source connector

Connectors are where the site's trust starts. A connector that silently converts a unit, mislabels a fiscal period or drops a licence puts a wrong figure under Karthik's name. Correctness beats coverage.

Read first: the Bureau design §3.1–§3.4 (Contacts; Wire: Stringers, Runner, Morgue; Diary; Clerk and Ledger), the Technical Design sections "Content system", "Data model" and "Agent toolbox", and the PRD "Sourcing and licensing". In Bureau's names a connector is a **Stringer**: one per source, an implementation of the `Connector` trait in `cli/crates/news-connectors` (read its README and ADR 0018 first; SEC EDGAR, FRED and BLS are the worked examples). Connectors are usually Sonnet 5.5 work; anything that parses XBRL, units, currencies or fiscal periods gets an Opus 5.5 review, and new parsing logic for those is Opus work.

## What a connector delivers

1. **Registry entry** (Contacts) as `data/sources/<id>.toml`, loaded into `sources` by `news_contacts::sync` and checked by the Contacts lint (also record `documented_rate_per_sec` and `documented_limit`; set `requires_contact_email` for sources like SEC): id, name, publisher to credit, base URL, connector, source tier, licence (public domain, CC BY, custom, unclear), required attribution text, image policy, redistribution allowed (yes/no), rate limit (the documented limit, halved), concurrency, daily quota, the *name* of the Credential Manager entry holding a free key, regions, categories, calendar kind.
   - Unclear redistribution terms → `redistribution: false`: the page links and summarises; data files are not republished.
2. **Discover** (`discover`): a cheap listing since a cursor (feed, index, API "updated after"), returning item refs with an event-key hint, not content.
3. **Fetch** (`fetch`) through the shared **Runner**, never your own HTTP client: conditional GET (`ETag` / `Last-Modified`, so a 304 costs no parse), the per-host rate limit, `Retry-After` (the job is snoozed, not failed) and `robots.txt`. The Runner stores the bytes exactly as received in Morgue (`work/raw/sha256/…`, content-addressed, with a `snapshots` row) and sets the User-Agent; a contact e-mail (the `NEWS_CONTACT_EMAIL` setting, read from Standing, the versioned settings table; `.env` holds only bootstrap values) is added only for sources that require one, such as SEC EDGAR.
4. **Parse** (`parse`) into normalised `data_points`. `parse` must be **pure**: a function of the snapshot bytes only, with no network, versioned by `PARSER_VERSION`, so it can be tested from fixtures and re-run on old snapshots. Each data point carries the value exactly as published (`value_text`, plus an exact decimal, never a float), unit, currency, scale (as stated, e.g. "millions"), period label exactly as the source writes it plus machine dates, the vintage (when the value was published or filed), and a **locator** that points to the value inside the stored snapshot (XBRL tag and context, JSON path, PDF page and table cell, SDMX key and observation).
5. **Event keys**: candidates for structured items carry their event key, following the patterns in Bureau design §3.4 (for example `earnings:<cik>:<fiscal_year><fiscal_period>`, `macro:<country>.<series_family>:<reference_period>`, `policy:<central_bank>:<decision_date>`), so story identity comes from the key, not from similarity.
6. **Calendar entries**: if the source publishes a release calendar (ICS, API or a meeting-calendar page), map its entries to event keys for Diary, so releases are pre-warmed and captured on time.
7. **Embeds and images**: record official video embeds and allow-listed images as URL + credit + licence + alt text (`ImageRef` in `Parsed.images`). The registry's `image_policy` says what Press may do: `none` (no images), `link` (the page links to the source's file, the default) or `rehost` (copy the file to our own storage, R2). Use `rehost` only when the licence permits re-hosting (public domain, CC BY, or a custom licence that allows it) and `redistribution = true`; the Contacts lint refuses `rehost` otherwise. When a source mixes its own and third-party images, the connector decides per item and sets `rehost` only for the source's own (NASA: credit line starts `NASA/`). Never copy an image under an unclear licence, or one with no credit line.
8. **Events** to ClickHouse for every stage (duration, status, counts, errors).

## Rules

- **Never convert** currencies, units or fiscal periods. Store what the source says; formatting happens at render time.
- **Revisions are data**: when a source revises a figure, store the new value as a new vintage that supersedes the old one and let Rewrite queue a page update.
- **No `unwrap` on external data.** Every parse failure becomes a typed error with the locator where it failed; the job goes to the dead list, not into blind retries. After a parser fix, the Salvage workflow (started from Board) re-parses the stored snapshots.
- **No AI.** Discovery, fetching, parsing and classification of structured items are code only.
- **Idempotent and resumable**: running a command twice changes nothing the second time.
- **Low CPU**: stream large files; use Docling only for hard PDF tables, one at a time.
- **No paid APIs, no API keys that cost money.** If a source needs a free key, Karthik stores it in Windows Credential Manager (entered through Board, ADR 0007), and the registry records only the entry's name; never in code, fixtures, `.env` or PostgreSQL.

## Tests

- Record real responses once into `cli/crates/news-connectors/tests/fixtures/<source>/` (trim to what the tests need) and record each file's origin and licence in `tests/fixtures/README.md`; mark anything synthetic as synthetic. CI never hits the network: `news-wire/tests/no_network.rs` fails if a test builds the live transport, and pipeline tests use `news_wire::fake::FakeTransport`.
- Test: discover finds new items and skips known ones; parse produces the exact published values, units, scales, period labels and locators, and `value` formats back to `value_text`; event keys and calendar entries match the patterns; revised values are detected; malformed and partial responses fail cleanly; licence and attribution reach the source record.
- Include at least one awkward real case per source (restated figure, non-calendar fiscal year, a footnoted value, a missing observation).

## Done when

`news discover`, `fetch` and `parse` work for the source on fixtures; registry entry complete; tests green; Opus review done for any XBRL/unit/currency/period logic.
