# news-connectors: the Stringers

One `Connector` per source (Bureau Design §3.2), named by `sources.connector`. A connector describes requests and reads bytes; it never opens a socket. The Runner in `news-wire` makes every request.

| Connector | Discovery | Fetch | Parse | Event key |
| --- | --- | --- | --- | --- |
| `sec-edgar` | `submissions/CIK##########.json` per tracked CIK; 10-Q, 10-K, and 8-K only with Item 2.02 | 10-Q/10-K: `api/xbrl/companyfacts`; 8-K: primary document | Every fact of the filing plus earlier facts of the same concept and unit; value = the JSON number token | `earnings:<cik10>:<fy><fp>` (none for 8-K) |
| `fred` | `series/vintagedates` per tracked series; `release/dates` → calendar | `series` + `series/observations` over the full real-time window | One point per (date, vintage); `.` is missing | `macro:us.<family>:<YYYY-MM>` |
| `bls` | Latest row of each tracked series (no change feed; the ICS calendar refuses bots) | `timeseries/data/<id>` (v2, `BLS_API_KEY`) | Every row; `-` is missing; footnotes in the locator | `macro:us.<family>:<YYYY-MM>` |

## The contract

```rust
pub trait Connector: Send + Sync {
    fn id(&self) -> &'static str;
    fn source_id(&self) -> &'static str;
    fn parser_version(&self) -> u32;               // recorded as data_points.parser = "<id>@<version>"
    fn discovery_requests(&self, cx: &Cx, cursor: Option<&str>) -> Vec<Request>;
    fn parse_discovery(&self, cx: &Cx, cursor: Option<&str>, snaps: &[Snapshot]) -> Result<Discovery, ParseError>;
    fn fetch_requests(&self, item: &ItemRef) -> Vec<Request>;
    fn parse(&self, item: &ItemRef, snaps: &[Snapshot]) -> Result<Parsed, ParseError>;   // pure
}
```

- `parse` and `parse_discovery` are pure: snapshot bytes in, data points, missing observations, entities, candidates and `needs_human` out. They can be re-run on old snapshots after a fix (Salvage).
- Requests never carry secrets. A free key is a `SecretParam { param, env }` the Runner appends at send time; the stored URL is the redacted one.
- Cursors are connector-owned JSON (`{"<cik or series>": "<last seen>"}`), so known items are skipped.
- `ParseError` carries the locator where parsing failed and maps to a Dispatch class: `Shape`/`Upstream` → `upstream_changed`, `Value` → `invalid_data` (dead list), `NotYet` → `transient` (companyfacts lags a new filing by minutes).
- Values follow ADR 0018: verbatim text, exact decimal, verbatim unit, currency, scale and period label, the company's own fiscal year, a vintage and a locator. Nothing converts.

## Adding a source

Follow `.claude/skills/news-source-connector/SKILL.md`: a `data/sources/<id>.toml` row (the Contacts lint checks licence, attribution, tier and that `rate_per_sec` is at most half the documented limit), a connector module here registered in `connector()` and `IDS`, Clerk rules for its rule keys (`news-clerk/src/rules.rs`), recorded fixtures under `tests/fixtures/<id>/` with their origin in `tests/fixtures/README.md`, and golden tests including a restatement, a non-calendar period, a footnoted value and a missing observation where the source has them.

## Not built yet

- SEC Atom "latest filings" feed and the nightly daily index (discovery is per tracked CIK for now); 8-K press-release text blocks (`documents`/`text_blocks`).
- Shared CSV, SDMX-JSON, JSON-stat and PDF-text parsers (no source built so far needs them).
- `news discover|fetch|parse` and `news sources …` CLI commands, and the job handlers: they need the Bureau service context (#10, #29).
