# Connector fixtures

Tests never touch the network (`news-wire/tests/no_network.rs` enforces it). Each file says where it came from.

| File | Origin | Licence |
| --- | --- | --- |
| `bls/CUUR0000SA0.json`, `bls/CES0000000001.json`, `bls/LNS14000000.json` | **Recorded** 16 Nov 2026 from `https://api.bls.gov/publicAPI/v1/timeseries/data/<id>` (v1 needs no key; v2 returns the same shape). Untrimmed. | BLS data, public domain |
| `bls/api-robots.txt` | **Recorded** 16 Nov 2026, `https://api.bls.gov/robots.txt` (`Disallow: /` for every agent) | — |
| `fred/api-robots.txt` | **Recorded** 16 Nov 2026, `https://api.stlouisfed.org/robots.txt` | — |
| `fred/*.json` | **Synthetic**: shaped like the FRED API responses (`series`, `series/observations`, `series/vintagedates`, `release/dates`); values are made up. Recording needs `FRED_API_KEY`. | — |
| `sec/*.json`, `sec/primary-8k.htm` | **Synthetic**: shaped like `submissions` and `api/xbrl/companyfacts` for CIK 320193; accession numbers and FY2026 values are made up. Recording needs `NEWS_CONTACT_EMAIL` (SEC requires a contact in the User-Agent). | — |

Awkward cases covered: BLS October 2025 missing (`-`, footnote `X`, the 2025 lapse in appropriations); BLS preliminary values (footnote `P`); a BLS revision (an edited copy of the recorded CES response, made inside the test); a FRED revision (two vintages of August 2026) and a missing month (`.`); an SEC restatement (FY2025 net income as filed and as restated in the FY2026 10-K); Apple's 52/53-week fiscal year ending in late September; a year-to-date fact that must not get a quarter label; an unknown XBRL unit (`USD/Contract`).

To replace the synthetic files with recorded ones (Karthik, once the key and contact are in `.env`):

```sh
curl -A "News/0.0.1 (+https://news.karthik.nz/data-access) $NEWS_CONTACT_EMAIL" https://data.sec.gov/submissions/CIK0000320193.json
curl -A "News/0.0.1 (+https://news.karthik.nz/data-access) $NEWS_CONTACT_EMAIL" https://data.sec.gov/api/xbrl/companyfacts/CIK0000320193.json   # trim to the tested concepts
curl "https://api.stlouisfed.org/fred/series/observations?series_id=CPIAUCSL&realtime_start=1776-07-04&realtime_end=9999-12-31&observation_start=2024-11-13&file_type=json&api_key=$FRED_API_KEY"
```

Then update the expected values in `tests/stringers.rs` and `news-wire/tests/pipeline.rs`.
