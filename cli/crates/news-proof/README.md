# news-proof

Proof (Bureau Design §3.10): `news check <story>` runs the code checks C1 to C14 on a rendered page bundle and prints the Markup. Zero tokens. Render and save are a separate branch of #17; the `item.render` and `item.check` job handlers stay stubs until the service wires them.

```
news check 81234            # work/pages/81234/ (NEWS_WORK_DIR)
news check path/to/bundle   # any directory
```

A page with a blocking failure is a `validation_failed` error (exit 6) whose `details` are the Markup: `{page_id, pass, failures[{check, where, message}], warnings[], checks[{check, name, status}]}`. Contract: `contracts/cli/commands/check.schema.json`. One row per check per page goes to ClickHouse `news.check_results` (`infra/clickhouse/003_ticker.sql`).

## The bundle

`page.oui`, `data.json`, `sources.json`, `meta.json`, `index.html`, `index.md`, `charts/<id>.csv` (`contracts/bundle/`, the renderer's contract). The Copy is the string literals of `page.oui` plus `meta.json`'s `fly_title`, `title` and `summary`, which must hold the page writer's text with `{ref}` tokens (the renderer fills the values into `index.html`). A quote's `locator` is `{"kind": "text_block", "doc": <id>, "block": <n>}`.

## Checks

| # | Check | How | Severity |
| --- | --- | --- | --- |
| C1 | Parse and render | All six files present, JSON parses, `page.oui` brackets, `root`, no unquoted reference or undefined statement, `<article>` and front matter exist | block |
| C2 | References resolve | Every `{ref}` and whole-reference prop exists in `data.json` with a source, period and inputs; no bare reference in prose; no "no data" on the page | block |
| C3 | Values match source | `value` equals `value_text`; each derived value recomputed from its inputs with exact decimals (same unit, currency and scale); each cited figure's digits are on the page | block |
| C4 | Numeric-literal lint | No digit, written-out quantity, fraction, multiplier or unit phrase after a quantity word in prose. Allow-list in `data/seeds/proof-v1.toml`; entity names with digits come from `meta.json`. Skips whole references, URLs and the media fields of `VideoEmbed` | block |
| C5 | Direction words | In a sentence with a `dv_` change, direction words (`proof-v1.toml`, about 60) must agree with its sign | block |
| C6 | Quotes verbatim | Every quote is an exact substring of its stored text block (read from `text_blocks`); a missing block fails | block |
| C7 | Period and unit labels | Used period labels appear as the source writes them; a series' points share its unit, currency and scale | block |
| C8 | Licence | Attribution fields and a sources-list entry for every cited source; no chart CSV holding a value of a `redistribution=false` source | block |
| C9 | Required fields | Title, summary, categories, `<title>`, alt text on every image, disclaimer by category | block |
| C10 | Links and media | Source URLs, image and embed URLs answer (HEAD, then GET; 401, 403 and 429 count as answering); good answers cached 24 h in `work/cache/proof-links.json`; images only from allow-listed hosts | block; a dead image is a warning |
| C11 | Banned words and tone | `data/seeds/banned-words.toml`, the same list the `news-brand` skill gives the writer; no `!` or `?` | block |
| C12 | Size and structure | Bundle bytes against 1 MiB; component count against the page type's range | warn |
| C13 | Accessibility | axe-core through `site/scripts/axe.ts` (Playwright, WCAG 2.2 AA): no serious or critical issue; every Tier 1 page and one Tier 2 page in ten (stable hash of the page id). If axe cannot run, the check fails | block when run |
| C14 | Duplicate page | No other `ready` or `published` page with the same slug, or for the same story | block |

`NEWS_PROOF_LINKS=off` and `NEWS_PROOF_AXE=off` skip C10 and C13 (the Markup says "skipped"), for a machine without network or browser. Without a database a bundle directory skips C14 and cannot verify quotes.

## Tests

`tests/checks.rs`: a passing bundle (`tests/fixtures/good`) and a failing one per check, made by changing the passing one. No network: link checks and axe are fakes. `cargo test -p news-proof -- --test-threads=2`.

The seven fixture pages (`pipeline/fixtures/`, one per page type) are rendered and checked like production pages: `news-fixer/tests/fixture_pages.rs` rebuilds their inputs from recorded source data, runs C1–C14 on the committed bundles and, with Bun, re-renders them byte for byte. `NEWS_WORK_DIR=pipeline/fixtures news check earnings` runs the live checks (links, axe) on one. Decisions: [ADR 0013](../../../docs/adr/0013-proof-and-templated-fallback.md).
