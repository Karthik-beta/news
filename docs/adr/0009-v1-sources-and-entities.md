# ADR 0009: v1 sources, Diary and Roster

- Status: proposed
- Date: 2026-11-18
- Issue: #12 (Phase 5, wave 3)
- Context documents: Bureau Design §3.1–§3.5; ADR 0018 (Ledger and provenance); the `news-source-connector` skill

## Context

The first batch of Stringers (SEC EDGAR, FRED, BLS) covers the US. Issue #12 adds the other v1 regions and page types, a release calendar (Diary) and one identity per entity (Roster). Karthik asked for speed and a small usage budget, so the batch is what the issue lists, kept as small as the rules allow, and anything a source's own terms or servers refuse is left out and named here.

## Decisions

### Stringers

- **Statistics Stringers share one shape** (`stat.rs`): a catalogue of followed series, a cheap probe (`lastNObservations=1`, `lastTimePeriod=1`, `mrv=1`), and a history request. None of these sources has a change feed, so a probe whose bytes differ from the one the cursor remembers is a new item; the Runner's conditional GET makes an unchanged probe a 304. The probe and the history URL always differ, or the history fetch would answer 304 and nothing would be parsed.
- **Readers**: one SDMX-JSON reader (`sdmx.rs`: ECB, OECD, ABS; the 1.0 and ABS 2.0 layouts), one JSON-stat reader (Eurostat), one RSS/RDF/Atom reader (`feed.rs`: the central banks). Values are read as raw JSON text, so Eurostat's `2.0` stays `2.0`.
- **Vintages.** Eurostat's `updated` stamp, the World Bank's `lastupdated` date and BEA's `UTCProductionTime` are the publication vintages; the others (ECB, OECD, ABS, ONS) state none and use the fetch time, as BLS does (ADR 0018). The Ledger already refuses to store a re-observation of an unchanged value as a new vintage.
- **Units** are the sources' own names (ECB `UNIT`, OECD `UNIT_MEASURE`, ABS `MEASURE`, Eurostat's `unit` label, ONS `unit_of_measure`, BEA `CL_UNIT`). The World Bank API leaves `unit` empty, so the unit is the parenthetical of the indicator name (`current US$`). The vocabulary gained the strings these sources use; anything else is `needs_human`, for instance the ECB's `US dollar` for its exchange rate (a rate is not an amount of money, and the Ledger does not guess).
- **Event keys.** `macro:<country>.<family>:<period>` as before, with families that do not collide with the US ones (`cpi_yoy` for the OECD's growth rate, `gdp_wb`, `policy_rate`). New patterns: `policy:<bank>:<decision date>` and `paper:doi|arxiv|openalex:<id>`. Central-bank statements are candidates only: statements are text, and numbers on pages come from data sources by code.
- **Papers** (OpenAlex, Crossref, arXiv) yield candidates, not data points. A record dated in the future (OpenAlex returns 2027 data deposits) never moves a discovery cursor past today.
- **Images.** `Parsed.images` carries URL, credit, licence, alt text and a `rehost` flag. The registry's `image_policy` is `none`, `link` or `rehost`; the lint refuses `rehost` unless `redistribution` is true, and the connector decides per item. NASA is `rehost` only for items whose credit line starts `NASA/` (NASA's guidelines say its own content is generally not copyrighted and may include third-party material); anything else stays a link to NASA's file. The connector skill no longer says images always stay remote.
- **Contacts rows** for sources whose terms were not read in full (ECB, OECD) are `custom` with `redistribution = false`: pages link and summarise, no data files are republished, until Karthik confirms the reuse notices. Eurostat, the World Bank and the ABS state CC BY 4.0.

### Left out, and why

| Source | Reason |
| --- | --- |
| StatCan WDS | The host answers automated requests with 403 (recorded 18 Nov 2026). |
| IMF DataMapper | Its edge returned 403 for some requests with the declared User-Agent, and the values come with no unit. The newer SDMX host answered 502. Revisit when it is stable. |
| BoJ, RBI | Their sites refuse automated access ("Network Busy", "Unauthorised Access"). |
| arXiv query API | `export.arxiv.org/robots.txt` disallows everything. The OAI-PMH host (`oaipmh.arxiv.org`) publishes no robots.txt and is used instead, paced at one request every 6 seconds (the lint requires at most half of arXiv's one-in-3-seconds). |
| BIS calendar API, Eurostat ICS | Not recorded in this batch; the calendar is an accelerator, so feed discovery still finds these releases. |
| NASA news releases | The Image and Video Library holds media, not releases. |
| Docling and Tesseract | No source in this batch needs a PDF table. The container, the `docling` pool and the output parsers belong with the first PDF source. |

BEA (key) and Crossref (contact e-mail) ship disabled with the rule for sec-edgar: BEA's fixtures are synthetic; OpenAlex also ships disabled because its keyless budget is tiny.

### Diary

- `calendar_events` is filled from: BLS and BEA ICS files (`ics.rs`, mapped to event keys by a per-source rule table that includes only releases a built Stringer can capture), the FOMC page (`fomc.rs`), FRED's `release/dates` (already in its discovery), a hand-checked seed file (`data/calendar/seed.toml`; every entry names its page and the day it was checked), and derived earnings windows.
- **Derived earnings**: the company's own fiscal year and period as filed, next year's number with the same period code, expected one calendar year after the first filing of that period, plus or minus 7 days; polled every 10 minutes for 8-Ks inside the window. Nothing maps a fiscal period to a calendar quarter.
- **Schedule** (pure functions, tested on a virtual clock): pre-warm at the start of the day before; capture from 2 minutes after the release (or the start of the day, or of the window), every 5 minutes for 2 hours, then hourly until 26 hours after the start, then `late`. A `late` event that arrives is captured after all. A sync never moves an event that is no longer `expected`.
- The job handlers (`calendar.sync`, `calendar.capture`) refuse with `bug` until the Bureau service gives them a context, like Wire's and Clerk's. `news calendar sync` runs the offline part (seed file, derived windows).

### Roster

- Tables as in §3.5 (already migrated) plus `unresolved_mentions` (migration 0300).
- **Resolver** (`resolver.rs`, pure): identifier, exact alias, gazetteer (longest span wins; one entity or no answer; short single words only ever add doubt), pg_trgm-style fuzzy (similarity at least 0.6, margin at least 0.1), else unresolved with a reason and up to five candidates. It never creates an entity and never chooses between equals. The PostgreSQL path fetches only the rows that could matter for one text and runs the same function.
- **Normalisation**: NFKC, case-fold, punctuation to spaces, apostrophes dropped, a leading `the` and trailing legal forms (`Inc`, `plc`, `AG`, `S.A.` …) stripped, never to nothing.
- **Seeds**: SEC `company_tickers_exchange.json`, GLEIF `lei-records`, ROR v2, Wikidata (ISO 3166 by P297, ISO 4217 by P498), plus `data/roster/central-banks.toml` and `prominence.toml`. **ISO 4217 comes from Wikidata (CC0)**, the same source and licence as ISO 3166, so no further download is needed. Currencies are `topic` entities. Ticker identifiers use `ticker@<MIC>` only for exchanges mapped without guessing (Nasdaq, NYSE, Cboe).
- `alias add` and `merge` are H-tier library functions for Board; no command form exists. `merge` updates every table that references `entities`; a test compares that list with the catalogue.
- **G7**: `tests/g7/` holds a hand-written test universe and the labelled mentions; precision on resolved mentions must be at least 0.99 and recall is reported. The set is a starter drafted by Claude for Karthik to confirm; it has fewer than the 300 the issue asks for.

## Consequences

- Eight new keyless sources and five central banks are enabled the moment `news sources sync` runs; BEA, Crossref and OpenAlex wait for a key or the contact e-mail.
- Every figure from these sources keeps the source's own unit and period label; none is converted.
- The registry's `rate_per_sec` for OECD (one request every 2 minutes) and arXiv (one every 6 seconds) is deliberately slower than the documented limits so the lint holds.
