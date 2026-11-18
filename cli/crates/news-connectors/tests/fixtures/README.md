# Connector fixtures

Tests never touch the network (`news-wire/tests/no_network.rs` enforces it). Each file says where it came from.

| File | Origin | Licence |
| --- | --- | --- |
| `bls/CUUR0000SA0.json`, `bls/CES0000000001.json`, `bls/LNS14000000.json` | **Recorded** 16 Nov 2026 from `https://api.bls.gov/publicAPI/v1/timeseries/data/<id>` (v1 needs no key; v2 returns the same shape). Untrimmed. | BLS data, public domain |
| `bls/api-robots.txt` | **Recorded** 16 Nov 2026, `https://api.bls.gov/robots.txt` (`Disallow: /` for every agent) | — |
| `fred/api-robots.txt` | **Recorded** 16 Nov 2026, `https://api.stlouisfed.org/robots.txt` | — |
| `fred/*.json` | **Synthetic**: shaped like the FRED API responses (`series`, `series/observations`, `series/vintagedates`, `release/dates`); values are made up. Recording needs `FRED_API_KEY`. | — |
| `sec/*.json`, `sec/primary-8k.htm` | **Synthetic**: shaped like `submissions` and `api/xbrl/companyfacts` for CIK 320193; accession numbers and FY2026 values are made up. Recording needs `NEWS_CONTACT_EMAIL` (SEC requires a contact in the User-Agent). | — |

**Second batch (#12), recorded 18 Nov 2026** (the sources need no key and no contact e-mail; each file is the response to the exact request the connector builds, trimmed where noted):

| Files | Origin | Licence / note |
| --- | --- | --- |
| `ecb/FM.MRR-*.json`, `ecb/EXR.USD-history.json` | `data-api.ecb.europa.eu/service/data/…?lastNObservations=1\|12&format=jsondata` | ECB; terms not confirmed (registry: redistribution false) |
| `oecd/CPI.USA-*.json` | `sdmx.oecd.org/public/rest/data/…?lastNObservations=1\|12` | OECD; terms not confirmed |
| `abs/CPI.AUS-*.json` | `data.api.abs.gov.au/rest/data/CPI/1.10001.10.50.Q?lastNObservations=1\|8` (SDMX-JSON 2.0) | ABS, CC BY 4.0 |
| `eurostat/HICP.EA-*.json` | `…/eurostat/api/dissemination/statistics/1.0/data/prc_hicp_manr?geo=EA&coicop=CP00&lastTimePeriod=1\|12` | Eurostat, CC BY 4.0 |
| `worldbank/GDP.USA-*.json` | `api.worldbank.org/v2/country/USA/indicator/NY.GDP.MKTP.CD?format=json&mrv=1\|5` | World Bank, CC BY 4.0 |
| `ons/dataset-cpih01.json`, `ons/observations-cpih01.json` | `api.beta.ons.gov.uk/v1/datasets/cpih01` and its version 67 observations, trimmed to 8 of 457 | OGL v3.0 |
| `nasa/search-artemis.json` (3 of 20 hits), `nasa/item-*.json` | `images-api.nasa.gov/search?q=artemis…` and `?nasa_id=…` | NASA media guidelines |
| `openalex/list-field20.json` (3 of 25), `openalex/work-W7234566083.json` | `api.openalex.org/works…`, recorded without a key (OpenAlex answers keyless requests from a small shared daily budget; the Stringer adds a free key, `OPENALEX_API_KEY`, and ships disabled) | CC0 metadata |
| `crossref/list.json` (3 of 25), `crossref/work-10.1007_s00408-021-00426-w.json` (reference list removed) | `api.crossref.org/works…` | CC0 metadata |
| `arxiv/list-econ.xml` (3 of the records), `arxiv/record-2112.14514.xml` | `oaipmh.arxiv.org/oai?verb=ListRecords\|GetRecord…` | CC0 metadata |
| `centralbank/*-feed.xml` | Fed `press_monetary.xml`, ECB `rss/press.html`, BoE `rss/news`, BoC press-release feed, RBA media-release feed | Publishers' terms; used for tests only |
| `centralbank/{fed,boe,boc}-statement-*.html` | The 28 Oct 2026 FOMC statement, the November 2026 MPC summary and the 28 Oct 2026 Bank of Canada decision, cut to the first 8 000 bytes | Publishers' terms; used for tests only |
| `bea/NIPA.T10101-*.json` | **Synthetic**: shaped like the BEA NIPA `GetData` response; every value is made up. Recording needs `BEA_API_KEY`. | BEA is **disabled** until recorded |

Awkward real cases in the second batch: the ECB main refinancing rate is published only when it changes (irregular dates, a 12-month gap between 2025-06-11 and 2026-06-17); the ECB's EXR unit is "US dollar", not in the unit vocabulary (`needs_human`); OECD US CPI has no October 2025 observation; Eurostat writes `2.0` and the trailing zero is kept; the ONS dataset is frozen after January 2026; OpenAlex returns works dated in 2027 (the cursor must not follow them); NASA images with third-party or no credit (edited copy, made inside the test) are never re-hosted.

Awkward cases covered: BLS October 2025 missing (`-`, footnote `X`, the 2025 lapse in appropriations); BLS preliminary values (footnote `P`); a BLS revision (an edited copy of the recorded CES response, made inside the test); a FRED revision (two vintages of August 2026) and a missing month (`.`); an SEC restatement (FY2025 net income as filed and as restated in the FY2026 10-K); Apple's 52/53-week fiscal year ending in late September; a year-to-date fact that must not get a quarter label; an unknown XBRL unit (`USD/Contract`).

To replace the synthetic files with recorded ones (Karthik, once the key and contact are in `.env`):

```sh
curl -A "News/0.0.1 (+https://news.karthik.nz/data-access) $NEWS_CONTACT_EMAIL" https://data.sec.gov/submissions/CIK0000320193.json
curl -A "News/0.0.1 (+https://news.karthik.nz/data-access) $NEWS_CONTACT_EMAIL" https://data.sec.gov/api/xbrl/companyfacts/CIK0000320193.json   # trim to the tested concepts
curl "https://api.stlouisfed.org/fred/series/observations?series_id=CPIAUCSL&realtime_start=1776-07-04&realtime_end=9999-12-31&observation_start=2024-11-13&file_type=json&api_key=$FRED_API_KEY"
```

Then update the expected values in `tests/stringers.rs` and `news-wire/tests/pipeline.rs`.

**SEC EDGAR is disabled** (`enabled = false` in `data/sources/sec-edgar.toml`) until both are done: the `sec/` fixtures are recorded as above, and `NEWS_CONTACT_EMAIL` is set in `.env`. Without the e-mail the Runner refuses every SEC request as `bug`, which would put each SEC job on the dead list. `news sources sync` only sets `enabled` on insert, so after recording, enable it explicitly.
