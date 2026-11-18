//! Crossref REST API (papers family). Metadata is free to reuse (CC0 for bibliographic fields).
//!
//! Registered as needing the contact e-mail: the Runner adds `NEWS_CONTACT_EMAIL` to the
//! User-Agent, which puts requests in Crossref's polite pool. Shipped **disabled** until the e-mail
//! is set. `Cx::tracked("crossref")` lists extra `filter` terms (`type:journal-article`,
//! `prefix:10.1016`). Discovery lists works indexed since the cursor date, newest first.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::json;

use news_ledger::eventkey;

use crate::util::{date, day_vintage, find, shape};
use crate::{
    CandidateDraft, Connector, Cx, Discovery, ItemRef, ParseError, Parsed, Request, Snapshot,
};

pub const ID: &str = "crossref";
pub const PARSER_VERSION: u32 = 1;
const BASE: &str = "https://api.crossref.org/works";
const FIELDS: &str = "DOI,title,issued,indexed,container-title,URL,type";
const FIRST_LOOKBACK: i64 = 1;

pub struct Crossref;

#[derive(Deserialize)]
struct Envelope<T> {
    message: T,
}

#[derive(Deserialize)]
struct List {
    items: Vec<Work>,
}

#[derive(Deserialize)]
struct Work {
    #[serde(rename = "DOI")]
    doi: String,
    #[serde(default)]
    title: Vec<String>,
    #[serde(default, rename = "URL")]
    url: Option<String>,
    #[serde(default)]
    issued: Option<Issued>,
    #[serde(default)]
    indexed: Option<Indexed>,
}

#[derive(Deserialize)]
struct Issued {
    #[serde(rename = "date-parts")]
    parts: Vec<Vec<Option<i32>>>,
}

#[derive(Deserialize)]
struct Indexed {
    #[serde(rename = "date-time")]
    at: String,
}

impl Issued {
    /// The issue date as far as Crossref states it: a full date, or `None` for year-only records
    /// (never padded to January 1st).
    fn day(&self) -> Option<NaiveDate> {
        let p = self.parts.first()?;
        NaiveDate::from_ymd_opt(
            p.first().copied().flatten()?,
            p.get(1).copied().flatten()?.try_into().ok()?,
            p.get(2).copied().flatten()?.try_into().ok()?,
        )
    }
}

fn cursor_map(c: Option<&str>) -> BTreeMap<String, String> {
    c.and_then(|c| serde_json::from_str(c).ok())
        .unwrap_or_default()
}

fn list_url(filter: &str, since: NaiveDate) -> String {
    let f = if filter.is_empty() {
        format!("from-index-date:{since}")
    } else {
        format!("from-index-date:{since},{filter}")
    };
    format!("{BASE}?filter={f}&rows=25&sort=indexed&order=desc&select={FIELDS}")
}

/// The filters to run: the tracked ones, or one unfiltered listing when none are given.
fn filters(cx: &Cx) -> Vec<String> {
    let t = cx.tracked(ID);
    if t.is_empty() {
        vec![String::new()]
    } else {
        t.to_vec()
    }
}

impl Connector for Crossref {
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
        filters(cx)
            .iter()
            .map(|f| {
                let since = seen
                    .get(f)
                    .and_then(|d| date(d))
                    .unwrap_or(cx.today - chrono::Duration::days(FIRST_LOOKBACK));
                Request::json(list_url(f, since))
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
        for f in filters(cx) {
            let since = seen
                .get(&f)
                .and_then(|d| date(d))
                .unwrap_or(cx.today - chrono::Duration::days(FIRST_LOOKBACK));
            let Some(snap) = snaps.iter().find(|s| s.url == list_url(&f, since)) else {
                continue;
            };
            let list: Envelope<List> =
                serde_json::from_slice(&snap.bytes).map_err(|e| shape("$.message.items", e))?;
            let mut newest = seen.get(&f).cloned().unwrap_or_default();
            for w in list.message.items {
                if let Some(day) = w
                    .indexed
                    .as_ref()
                    .and_then(|i| date(&i.at[..10.min(i.at.len())]))
                {
                    newest = newest.max(day.to_string());
                }
                out.refs.push(ItemRef {
                    source_id: ID.into(),
                    kind: "crossref.work".into(),
                    key: w.doi.clone(),
                    url: w
                        .url
                        .clone()
                        .unwrap_or_else(|| format!("https://doi.org/{}", w.doi)),
                    event_key_hint: eventkey::paper("doi", &w.doi),
                    published_at: w.issued.as_ref().and_then(Issued::day).map(day_vintage),
                    title: w.title.first().cloned(),
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
        vec![Request::json(format!("{BASE}/{}", item.key))]
    }

    fn parse(&self, item: &ItemRef, snaps: &[Snapshot]) -> Result<Parsed, ParseError> {
        let snap = find(snaps, &format!("{BASE}/{}", item.key))?;
        let env: Envelope<Work> =
            serde_json::from_slice(&snap.bytes).map_err(|e| shape("$.message", e))?;
        let w = env.message;
        let title = w
            .title
            .first()
            .filter(|t| !t.trim().is_empty())
            .cloned()
            .ok_or_else(|| shape("$.message.title", "work has no title"))?;
        let mut out = Parsed::default();
        out.candidates.push(CandidateDraft {
            event_key: eventkey::paper("doi", &w.doi),
            url: w
                .url
                .unwrap_or_else(|| format!("https://doi.org/{}", w.doi)),
            title,
            published_at: w.issued.as_ref().and_then(Issued::day).map(day_vintage),
            rule_key: "family:paper".into(),
            dp_ids: vec![],
            entity: None,
        });
        Ok(out)
    }
}
