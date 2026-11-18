//! Seed data (Bureau §3.5, "Seed data, all free"): every loader turns a published file into plain
//! [`Seed`] records; [`crate::store::apply`] writes them. Loaders are pure functions of the file's
//! text, tested from recorded samples.
//!
//! | Loader | File | Entity kind | Identifiers |
//! | --- | --- | --- | --- |
//! | [`sec_tickers`] | SEC `company_tickers_exchange.json` | `company` | `cik`, `ticker@<MIC>` |
//! | [`gleif`] | GLEIF `lei-records` (JSON:API) | `company` | `lei` |
//! | [`ror`] | ROR v2 records (API or dump JSON) | `institution` | `ror` |
//! | [`wikidata_countries`] | Wikidata query: items with ISO 3166 code (P297) | `country` | `iso3166`, `wikidata` |
//! | [`wikidata_currencies`] | Wikidata query: items with ISO 4217 code (P498) | `topic` | `iso4217`, `wikidata` |
//!
//! The SEC does not guarantee its list, so it is a seed, not truth. Currencies are `topic` entities
//! (ADR 0009): Roster has no currency kind and a currency is only ever a subject of a story.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdPair {
    pub scheme: String,
    pub value: String,
}

/// One entity to create or complete.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seed {
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub region: Option<String>,
    /// 1 flagship, 2 major, 3 other (Bureau §4.2). `None` leaves an existing value alone.
    #[serde(default)]
    pub prominence: Option<i16>,
    #[serde(default)]
    pub ids: Vec<IdPair>,
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct SeedError(pub String);

fn err(m: impl Into<String>) -> SeedError {
    SeedError(m.into())
}

pub const KINDS: &[&str] = &[
    "company",
    "country",
    "central_bank",
    "institution",
    "person",
    "topic",
    "agency",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedFile {
    #[serde(default)]
    entity: Vec<Seed>,
}

/// A hand-written seed file (`data/roster/*.toml`, `[[entity]]` tables).
pub fn parse_toml(text: &str) -> Result<Vec<Seed>, SeedError> {
    let f: SeedFile = toml::from_str(text).map_err(|e| err(e.to_string()))?;
    for s in &f.entity {
        if !KINDS.contains(&s.kind.as_str()) {
            return Err(err(format!("{:?} has unknown kind {:?}", s.name, s.kind)));
        }
        if s.name.trim().is_empty() {
            return Err(err("an entity has no name"));
        }
        if let Some(p) = s.prominence
            && !(1..=3).contains(&p)
        {
            return Err(err(format!("{:?}: prominence {p} is not 1-3", s.name)));
        }
    }
    Ok(f.entity)
}

fn id(scheme: &str, value: impl Into<String>) -> IdPair {
    IdPair {
        scheme: scheme.into(),
        value: value.into(),
    }
}

/// Exchange names as the SEC writes them → MIC. Anything else gets no ticker identifier.
fn mic(exchange: &str) -> Option<&'static str> {
    Some(match exchange {
        "Nasdaq" => "XNAS",
        "NYSE" => "XNYS",
        "CBOE" => "BATS",
        _ => return None,
    })
}

/// `company_tickers_exchange.json`: `{"fields": [...], "data": [[cik, name, ticker, exchange], ...]}`.
/// One company with several tickers is one seed.
pub fn sec_tickers(json: &str) -> Result<Vec<Seed>, SeedError> {
    let v: Value = serde_json::from_str(json).map_err(|e| err(format!("not JSON: {e}")))?;
    let fields: Vec<&str> = v
        .get("fields")
        .and_then(Value::as_array)
        .ok_or_else(|| err("no `fields`"))?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let pos = |n: &str| {
        fields
            .iter()
            .position(|f| *f == n)
            .ok_or_else(|| err(format!("`fields` has no {n:?}")))
    };
    let (ci, ni, ti, ei) = (pos("cik")?, pos("name")?, pos("ticker")?, pos("exchange")?);
    let rows = v
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| err("no `data`"))?;
    let mut by_cik: BTreeMap<String, Seed> = BTreeMap::new();
    for (n, row) in rows.iter().enumerate() {
        let r = row
            .as_array()
            .ok_or_else(|| err(format!("data[{n}] is not a row")))?;
        let cik = r
            .get(ci)
            .and_then(Value::as_u64)
            .ok_or_else(|| err(format!("data[{n}]: cik is not a number")))?;
        let name = r
            .get(ni)
            .and_then(Value::as_str)
            .ok_or_else(|| err(format!("data[{n}]: no name")))?;
        let cik10 = format!("{cik:0>10}");
        let seed = by_cik.entry(cik10.clone()).or_insert_with(|| Seed {
            kind: "company".into(),
            name: name.trim().to_owned(),
            region: Some("us".into()),
            prominence: None,
            ids: vec![id("cik", cik10)],
            aliases: vec![],
        });
        if let (Some(t), Some(m)) = (
            r.get(ti).and_then(Value::as_str),
            r.get(ei).and_then(Value::as_str).and_then(mic),
        ) {
            let p = id(&format!("ticker@{m}"), t);
            if !seed.ids.contains(&p) {
                seed.ids.push(p);
            }
        }
    }
    Ok(by_cik.into_values().collect())
}

/// GLEIF `lei-records` (JSON:API): legal name, other names, and the legal address country as region.
pub fn gleif(json: &str) -> Result<Vec<Seed>, SeedError> {
    let v: Value = serde_json::from_str(json).map_err(|e| err(format!("not JSON: {e}")))?;
    let data = v
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| err("no `data`"))?;
    let mut out = Vec::new();
    for (n, rec) in data.iter().enumerate() {
        let lei = rec
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| err(format!("data[{n}]: no id")))?;
        let ent = rec
            .pointer("/attributes/entity")
            .ok_or_else(|| err(format!("data[{n}]: no attributes.entity")))?;
        let name = ent
            .pointer("/legalName/name")
            .and_then(Value::as_str)
            .ok_or_else(|| err(format!("data[{n}]: no legal name")))?;
        let aliases = ent
            .get("otherNames")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|o| o.get("name").and_then(Value::as_str).map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        out.push(Seed {
            kind: "company".into(),
            name: name.trim().to_owned(),
            region: ent
                .pointer("/legalAddress/country")
                .and_then(Value::as_str)
                .map(str::to_lowercase),
            prominence: None,
            ids: vec![id("lei", lei.to_ascii_uppercase())],
            aliases,
        });
    }
    Ok(out)
}

/// ROR v2 records: an array, or an API page with `items`.
pub fn ror(json: &str) -> Result<Vec<Seed>, SeedError> {
    let v: Value = serde_json::from_str(json).map_err(|e| err(format!("not JSON: {e}")))?;
    let items = v
        .get("items")
        .and_then(Value::as_array)
        .or_else(|| v.as_array())
        .ok_or_else(|| err("no `items` and not an array"))?;
    let mut out = Vec::new();
    for (n, it) in items.iter().enumerate() {
        let rid = it
            .get("id")
            .and_then(Value::as_str)
            .and_then(|u| u.rsplit('/').next())
            .ok_or_else(|| err(format!("items[{n}]: no id")))?;
        let names = it
            .get("names")
            .and_then(Value::as_array)
            .ok_or_else(|| err(format!("items[{n}]: no names")))?;
        let has = |nm: &Value, t: &str| {
            nm.get("types")
                .and_then(Value::as_array)
                .is_some_and(|ts| ts.iter().any(|x| x.as_str() == Some(t)))
        };
        let display = names
            .iter()
            .find(|nm| has(nm, "ror_display"))
            .and_then(|nm| nm.get("value").and_then(Value::as_str))
            .ok_or_else(|| err(format!("items[{n}]: no ror_display name")))?;
        let aliases = names
            .iter()
            .filter_map(|nm| nm.get("value").and_then(Value::as_str))
            .filter(|v| *v != display)
            .map(str::to_owned)
            .collect();
        out.push(Seed {
            kind: "institution".into(),
            name: display.to_owned(),
            region: it
                .pointer("/locations/0/geonames_details/country_code")
                .and_then(Value::as_str)
                .map(str::to_lowercase),
            prominence: None,
            ids: vec![id("ror", rid)],
            aliases,
        });
    }
    Ok(out)
}

fn wikidata(
    json: &str,
    scheme: &str,
    kind: &str,
    region_from_code: bool,
) -> Result<Vec<Seed>, SeedError> {
    let v: Value = serde_json::from_str(json).map_err(|e| err(format!("not JSON: {e}")))?;
    let rows = v
        .pointer("/results/bindings")
        .and_then(Value::as_array)
        .ok_or_else(|| err("no results.bindings"))?;
    let mut out = Vec::new();
    for (n, b) in rows.iter().enumerate() {
        let get = |k: &str| b.pointer(&format!("/{k}/value")).and_then(Value::as_str);
        let qid = get("item")
            .and_then(|u| u.rsplit('/').next())
            .filter(|q| q.starts_with('Q'))
            .ok_or_else(|| err(format!("bindings[{n}]: no item Q-id")))?;
        let label = get("itemLabel").ok_or_else(|| err(format!("bindings[{n}]: no label")))?;
        // A binding whose label is the bare Q-id has no English label: not usable as a name.
        if label == qid {
            continue;
        }
        let code = get("code").ok_or_else(|| err(format!("bindings[{n}]: no code")))?;
        out.push(Seed {
            kind: kind.into(),
            name: label.to_owned(),
            region: region_from_code.then(|| code.to_lowercase()),
            prominence: None,
            ids: vec![id(scheme, code.to_ascii_uppercase()), id("wikidata", qid)],
            aliases: get("aliases")
                .map(|a| {
                    a.split('|')
                        .filter(|s| !s.trim().is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
        });
    }
    Ok(out)
}

/// Wikidata items with an ISO 3166-1 alpha-2 code (P297), with English labels and aliases.
pub fn wikidata_countries(json: &str) -> Result<Vec<Seed>, SeedError> {
    wikidata(json, "iso3166", "country", true)
}

/// Wikidata items with an ISO 4217 code (P498), with English labels and aliases.
pub fn wikidata_currencies(json: &str) -> Result<Vec<Seed>, SeedError> {
    wikidata(json, "iso4217", "topic", false)
}

/// The prominence seed lists (`data/roster/prominence.toml`): `[[entity]]` with `scheme`, `value` and
/// `prominence`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prominence {
    pub scheme: String,
    pub value: String,
    pub prominence: i16,
    #[serde(default)]
    pub note: Option<String>,
}

pub fn parse_prominence(text: &str) -> Result<Vec<Prominence>, SeedError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct F {
        #[serde(default)]
        entity: Vec<Prominence>,
    }
    let f: F = toml::from_str(text).map_err(|e| err(e.to_string()))?;
    for p in &f.entity {
        if !(1..=3).contains(&p.prominence) {
            return Err(err(format!(
                "{}:{}: prominence {} is not 1-3",
                p.scheme, p.value, p.prominence
            )));
        }
    }
    Ok(f.entity)
}
