# ADR 0018: Ledger and provenance

- Status: proposed
- Date: 2026-11-16
- Issue: #9 (Phase 5, wave 2)
- Context documents: Bureau Design §3.1–§3.4; Technical Design "Data model"; PRD "Sourcing and licensing"; decisions D5 (derived figures computed by code) and D7 (snapshot retention)

## Context

Every figure on a page must trace back to the exact bytes a source published, and must say what the source said: its value, unit, currency, scale, period and when it was published. A number typed by a model, a silent conversion or a mislabelled fiscal period puts a wrong figure under Karthik's name. Sources revise figures, and pages must be able to show both the first print and the revision.

## Decision

**Values verbatim, identity by hash, provenance on every row.**

- **Verbatim value.** `value_text` is the source's text as published: a JSON string (`"337.511"`), or the JSON number token itself for sources that publish numbers (SEC companyfacts, read through `serde_json::value::RawValue`, so `8.10` stays `8.10`). `value` is the exact decimal of that text, parsed by `news_ledger::decimal::parse` into a canonical string and stored as PostgreSQL `numeric`; no float ever holds a figure. A `Style` (sign character, thousands separator, bare dot, exponent form) records how the text was written, and a property test checks `style.format(value) == value_text` for every accepted form.
- **Nothing converts.** `unit`, `currency`, `scale`, `decimals`, `seasonal_adj` and `period_label` are stored as the source states them. A unit embedded in its words ("Thousands of Persons", "All employees, thousands") stays in `unit`; `scale` is only set when the source states one separately. Currencies come only from the unit (`USD`, `USD/shares`).
- **Unknown units** are stored verbatim and reported as `needs_human`; the vocabulary (`news_ledger::units`) is never extended by guessing. A BLS series without a catalogue entry gets no data points at all, because BLS responses carry no unit.
- **Period labels.** Where the source writes a label it is kept (BLS `periodName` + `year`: "October 2026"; FRED's observation date "2026-10-01"). SEC companyfacts has no label, so `period_label` is the fact's own `start/end` (ISO 8601 interval) or `end` for an instant: the source's fields, not a calendarisation.
- **The company's own fiscal year.** companyfacts gives `fy`/`fp` of the *filing*, not of each fact. A fact gets `fiscal_year`/`fiscal_period` only when its period is the filing's own period (it ends on the filing's latest duration end and has the length `fp` implies), or the same period one year earlier (`fy − 1`, the comparative column). Year-to-date and other periods keep their dates and no fiscal label. A missing label is better than a wrong one.
- **`dp_id`** is `dp_` + the first 16 characters of lower-case base32 of `sha256(series_key | context_key | period | vintage)`, where `context_key` is the sorted `key=value` identity pairs (SEC: `accn` and `unit`; none for FRED and BLS), `period` is `type|start|end|label`, and `vintage` is RFC 3339 UTC to the second. Re-parsing the same snapshot gives the same IDs (tested for every Stringer).
- **Vintages.** `vintage` is when the value was published: FRED `realtime_start`; SEC `filed` (a date, stored as 00:00 UTC); BLS, which publishes no vintages, the snapshot's fetch time. Each SEC filing's report of a period is its own point (the accession number is in the context), so a restatement in a later 10-K is a new point.
- **`supersedes`** is set by the Ledger store, not the parser: when a point is stored, the latest earlier vintage of the same `series_key`, unit and period is linked. A re-observation with the same value and context (BLS polled again) is not stored, so only real revisions create vintages; a revision is counted in the store report for Rewrite.
- **Provenance on every row.** `snapshot_sha` (the Morgue file the value was read from, a foreign key to `snapshots`), `locator` (where in those bytes: `xbrl` with accession, tag, unit and JSON path; `json` with path, series and FRED real-time window) and `parser` (`<connector>@<PARSER_VERSION>`) are required columns. Source footnotes (BLS `P` preliminary, `X` unavailable) travel in the locator as `footnotes`, verbatim.
- **Missing observations** (`.` from FRED, `-` from BLS) are not data points. Parsers return them separately with their locator and footnote, so a page can say "not published" and no guard can compute across them.
- **Derived points** (D5) live in `derived_points` with the formula, ordered inputs and a `guard` record. `news_ledger::guard::derive` refuses unless unit, currency, scale and seasonal adjustment are identical and the periods are comparable: the same length class (month, quarter, half, nine months, year), and for year-on-year the same fiscal period with ends 357–373 days apart (52/53-week years); for sequential, adjacent periods; for shares, the same period. `pct_change` is refused on percentages and `pp_change` requires them. Arithmetic is exact (i128 at scale ≤ 18, rounded half away from zero). A property test checks that no combination of unlike points passes.
- **Snapshots.** The Morgue stores bytes exactly as received (after transfer decoding), zstd level 3, at `work/raw/sha256/ab/cd/<sha>.zst`, written atomically and verified against the hash on read. `retain_until` is 400 days for structured data and 90 for documents (D7); the hash, URL and locators are kept for ever.

## Consequences

- A figure on a page is reproducible: `dp_id` → `snapshot_sha` → Morgue file → `locator` → the published text, while the snapshot is retained.
- The Ledger grows with every SEC filing (each restates its comparatives). Storage is idempotent (`on conflict do nothing`, existence checks), so re-running a parse adds nothing.
- Fiscal labels are absent for some SEC facts (year-to-date, stub periods). Fixer must use dates for those, never infer a quarter.
- Not built in #9 and recorded here so the gap is visible: a `needs_human` column or table (reported in the job result for now, because the 0100 migration block waits for the store's core-block check to admit it), XBRL dimensions (companyfacts has none; inline XBRL is later), and SEC 8-K text blocks.
