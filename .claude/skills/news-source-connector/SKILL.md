---
name: news-source-connector
description: "How to add or change a data-source connector in the Rust `news` CLI (cli/crates/connectors): source registry entry with licence and attribution, fetching, parsing into normalised data points, embeds and images, recorded fixtures and tests. Use this whenever you add a source (SEC EDGAR, FRED, IMF, World Bank, Eurostat, RBI, arXiv, NASA, …), fix a connector, or touch parsing of filings, statistics, papers or press releases."
---

# Adding a source connector

Connectors are where the site's trust starts. A connector that silently converts a unit, mislabels a fiscal period or drops a licence puts a wrong figure under Karthik's name. Correctness beats coverage.

Read first: the Technical Design sections "Content system", "Data model" and "Agent toolbox", and the PRD "Sourcing and licensing". Connectors are usually Sonnet 5.5 work; anything that parses XBRL, units, currencies or fiscal periods gets an Opus 5.5 review, and new parsing logic for those is Opus work.

## What a connector delivers

1. **Registry entry** in the `sources` table seed: id, name, base URL, connector, licence (public domain, CC BY, custom, unclear), required attribution text, image policy, redistribution allowed (yes/no), rate limit, region, categories.
   - Unclear redistribution terms → `redistribution: false`: the page links and summarises; data files are not republished.
2. **Discover**: list new items since the last run (ledger by source URL + content hash), as compact candidate rows.
3. **Fetch**: download raw files to `work/raw/<item>/`, respecting rate limits, `robots.txt` and the source's terms; set a descriptive User-Agent with no personal details.
4. **Parse** into normalised `data_points`: value exactly as published, unit, currency, scale (as stated, e.g. "millions"), period label exactly as the source writes it plus machine dates, and a source pointer (document, page, table, XBRL tag, series ID) and retrieval date.
5. **Embeds and images**: record official video embeds and allow-listed images as URL + credit + licence + alt text; never download images to store them.
6. **Events** to ClickHouse for every stage (duration, status, counts, errors).

## Rules

- **Never convert** currencies, units or fiscal periods. Store what the source says; formatting happens at render time.
- **Revisions are data**: when a source revises a figure, store the new value with its vintage and let the ledger queue a page update.
- **No `unwrap` on external data.** Every parse failure becomes a typed error with the source pointer, logged and retried later.
- **Idempotent and resumable**: running a command twice changes nothing the second time.
- **Low CPU**: stream large files; use Docling only for hard PDF tables, one at a time.
- **No paid APIs, no API keys that cost money.** If a source needs a free key, it goes in the local secrets file, never in code or fixtures.

## Tests

- Record real responses once into `cli/crates/connectors/tests/fixtures/<source>/` (trim to what the tests need; keep the licence note). CI never hits the network.
- Test: discover finds new items and skips known ones; parse produces the exact published values, units, scales and period labels; revised values are detected; malformed and partial responses fail cleanly; licence and attribution reach the source record.
- Include at least one awkward real case per source (restated figure, non-calendar fiscal year, a footnoted value, a missing observation).

## Done when

`news discover`, `fetch` and `parse` work for the source on fixtures; registry entry complete; tests green; Opus review done for any XBRL/unit/currency/period logic.
