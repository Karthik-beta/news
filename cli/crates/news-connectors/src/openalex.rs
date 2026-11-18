//! OpenAlex (papers family). CC0 metadata. Needs a free key (`OPENALEX_API_KEY`, added by the Runner): shipped disabled.
//!
//! `Cx::tracked("openalex")` lists OpenAlex `filter` expressions (`primary_topic.field.id:20`).
//! Discovery lists works published since the cursor date, per filter, newest first; fetch reads one
//! work; parse turns it into a paper candidate keyed `paper:doi:<doi>` (or `paper:openalex:<id>`).
//! Papers have no data points: numbers inside papers are not extracted by code.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::json;

use news_ledger::eventkey;

use crate::util::{date, day_vintage, find, shape};
use crate::{
    CandidateDraft, Connector, Cx, Discovery, ItemRef, ParseError, Parsed, Request, Snapshot,
};

pub const ID: &str = "openalex";
pub const PARSER_VERSION: u32 = 1;
const BASE: &str = "https://api.openalex.org/works";
const FIELDS: &str = "id,doi,title,publication_date,type,primary_location,authorships,open_access";
/// How far back the first discovery looks, in days.
const FIRST_LOOKBACK: i64 = 3;

pub struct OpenAlex;

#[derive(Deserialize)]
struct List {
    results: Vec<Work>,
}

#[derive(Deserialize)]
pub(crate) struct Work {
    id: String,
    #[serde(default)]
    doi: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    publication_date: Option<String>,
    #[serde(default)]
    primary_location: Option<Location>,
}

#[derive(Deserialize)]
struct Location {
    #[serde(default)]
    landing_page_url: Option<String>,
}

/// `https://openalex.org/W123` → `W123`.
fn short(id: &str) -> &str {
    id.rsplit('/').next().unwrap_or(id)
}

/// `https://doi.org/10.1/x` → `10.1/x`.
fn bare_doi(doi: &str) -> &str {
    doi.trim_start_matches("https://doi.org/")
        .trim_start_matches("http://doi.org/")
}

fn cursor_map(c: Option<&str>) -> BTreeMap<String, String> {
    c.and_then(|c| serde_json::from_str(c).ok())
        .unwrap_or_default()
}

fn list_url(filter: &str, since: NaiveDate) -> String {
    format!(
        "{BASE}?filter={filter},from_publication_date:{since}&sort=publication_date:desc&per-page=25&select={FIELDS}"
    )
}

impl Connector for OpenAlex {
    fn id(&self) -> &'static str {
        ID
    }
    fn source_id(&self) -> &'static str {
        ID
    }
    fn parser_version(&self) -> u32 {
        PARSER_VERSION
    }

    fn discovery_requests(&self, cx: &Cx, cursor: Option<&str>) -> Vec<Request> {
        let seen = cursor_map(cursor);
        cx.tracked(ID)
            .iter()
            .map(|f| {
                let since = seen
                    .get(f)
                    .and_then(|d| date(d))
                    .unwrap_or(cx.today - chrono::Duration::days(FIRST_LOOKBACK));
                Request::json(list_url(f, since)).with_secret("api_key", "OPENALEX_API_KEY")
            })
            .collect()
    }

    fn parse_discovery(
        &self,
        cx: &Cx,
        cursor: Option<&str>,
        snaps: &[Snapshot],
    ) -> Result<Discovery, ParseError> {
        let mut seen = cursor_map(cursor);
        let mut out = Discovery::default();
        for f in cx.tracked(ID) {
            let since = seen
                .get(f)
                .and_then(|d| date(d))
                .unwrap_or(cx.today - chrono::Duration::days(FIRST_LOOKBACK));
            let Some(snap) = snaps.iter().find(|s| s.url == list_url(f, since)) else {
                continue;
            };
            let list: List =
                serde_json::from_slice(&snap.bytes).map_err(|e| shape("$.results", e))?;
            let mut newest = seen.get(f).cloned().unwrap_or_default();
            for w in list.results {
                let Some(published) = w.publication_date.as_deref().and_then(date) else {
                    continue;
                };
                // Some records are dated in the future (Mendeley Data deposits carry 2027 dates);
                // they must not carry the cursor past today, or nothing new would be listed.
                if published <= cx.today {
                    newest = newest.max(published.to_string());
                }
                let id = short(&w.id).to_owned();
                out.refs.push(ItemRef {
                    source_id: ID.into(),
                    kind: "openalex.work".into(),
                    key: id,
                    url: w.doi.clone().unwrap_or_else(|| w.id.clone()),
                    event_key_hint: w
                        .doi
                        .as_deref()
                        .and_then(|d| eventkey::paper("doi", bare_doi(d)))
                        .or_else(|| eventkey::paper("openalex", short(&w.id))),
                    published_at: Some(day_vintage(published)),
                    title: w.title,
                    meta: json!({"filter": f}),
                });
            }
            if !newest.is_empty() {
                seen.insert(f.clone(), newest);
            }
        }
        out.next_cursor = serde_json::to_string(&seen).ok();
        Ok(out)
    }

    fn fetch_requests(&self, item: &ItemRef) -> Vec<Request> {
        vec![
            Request::json(format!("{BASE}/{}?select={FIELDS}", item.key))
                .with_secret("api_key", "OPENALEX_API_KEY"),
        ]
    }

    fn parse(&self, item: &ItemRef, snaps: &[Snapshot]) -> Result<Parsed, ParseError> {
        let snap = find(snaps, &format!("{BASE}/{}?", item.key))?;
        let w: Work = serde_json::from_slice(&snap.bytes).map_err(|e| shape("$", e))?;
        let title = w
            .title
            .filter(|t| !t.trim().is_empty())
            .ok_or_else(|| shape("$.title", "work has no title"))?;
        let event_key = w
            .doi
            .as_deref()
            .and_then(|d| eventkey::paper("doi", bare_doi(d)))
            .or_else(|| eventkey::paper("openalex", short(&w.id)));
        let url = w
            .doi
            .clone()
            .or_else(|| w.primary_location.and_then(|l| l.landing_page_url))
            .unwrap_or_else(|| w.id.clone());
        let mut out = Parsed::default();
        out.candidates.push(CandidateDraft {
            event_key,
            url,
            title,
            published_at: w
                .publication_date
                .as_deref()
                .and_then(date)
                .map(day_vintage),
            rule_key: "family:paper".into(),
            dp_ids: vec![],
            entity: None,
        });
        Ok(out)
    }
}
