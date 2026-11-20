//! arXiv through OAI-PMH (`oaipmh.arxiv.org`), papers family.
//!
//! arXiv allows one request every 3 seconds on one connection, for all machines together; the
//! registry sets that pace and concurrency 1. The query API host (`export.arxiv.org`) is not used:
//! its robots.txt disallows everything (recorded 18 Nov 2026), and the OAI-PMH host publishes no
//! robots.txt, so the Runner may read it.
//!
//! `Cx::tracked("arxiv")` lists OAI set names (`econ`, `q-fin`, `cs:cs:AI`). Discovery lists records
//! changed since the cursor date per set (`ListRecords`, first page); fetch reads one record
//! (`GetRecord`); parse makes a paper candidate keyed `paper:arxiv:<id>` (no version suffix: a new
//! version is the same paper, seen again with a newer datestamp). Licences are not extracted.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde_json::json;

use news_ledger::eventkey;

use crate::util::{date, day_vintage, find, shape};
use crate::{
    CandidateDraft, Connector, Cx, Discovery, ItemRef, ParseError, Parsed, Request, Snapshot,
};

pub const ID: &str = "arxiv";
pub const PARSER_VERSION: u32 = 1;
const OAI: &str = "https://oaipmh.arxiv.org/oai";
const FIRST_LOOKBACK: i64 = 1;

pub struct Arxiv;

fn cursor_map(c: Option<&str>) -> BTreeMap<String, String> {
    c.and_then(|c| serde_json::from_str(c).ok())
        .unwrap_or_default()
}

fn list_url(set: &str, from: NaiveDate) -> String {
    format!("{OAI}?verb=ListRecords&metadataPrefix=arXiv&set={set}&from={from}")
}

fn get_url(id: &str) -> String {
    format!("{OAI}?verb=GetRecord&metadataPrefix=arXiv&identifier=oai:arXiv.org:{id}")
}

fn child<'a>(n: roxmltree::Node<'a, 'a>, name: &str) -> Option<roxmltree::Node<'a, 'a>> {
    n.children()
        .find(|c| c.is_element() && c.tag_name().name() == name)
}

fn text_of(n: roxmltree::Node, name: &str) -> Option<String> {
    child(n, name)
        .and_then(|c| c.text())
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|t| !t.is_empty())
}

fn doc(bytes: &[u8]) -> Result<roxmltree::Document<'_>, ParseError> {
    let s = std::str::from_utf8(bytes).map_err(|e| shape("$", e))?;
    roxmltree::Document::parse(s).map_err(|e| shape("$", e))
}

/// An OAI-PMH error element (`noRecordsMatch` is "nothing new", not a failure).
fn oai_error(d: &roxmltree::Document<'_>) -> Option<(String, String)> {
    d.descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "error")
        .map(|n| {
            (
                n.attribute("code").unwrap_or("").to_owned(),
                n.text().unwrap_or("").to_owned(),
            )
        })
}

impl Connector for Arxiv {
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
            .map(|set| {
                let from = seen
                    .get(set)
                    .and_then(|d| date(d))
                    .unwrap_or(cx.today - chrono::Duration::days(FIRST_LOOKBACK));
                Request::xml(list_url(set, from))
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
        for set in cx.tracked(ID) {
            let from = seen
                .get(set)
                .and_then(|d| date(d))
                .unwrap_or(cx.today - chrono::Duration::days(FIRST_LOOKBACK));
            let Some(snap) = snaps.iter().find(|s| s.url == list_url(set, from)) else {
                continue;
            };
            let d = doc(&snap.bytes)?;
            if let Some((code, msg)) = oai_error(&d) {
                if code == "noRecordsMatch" {
                    continue;
                }
                return Err(shape("$.error", format!("{code}: {msg}")));
            }
            let mut newest = seen.get(set).cloned().unwrap_or_default();
            for rec in d
                .descendants()
                .filter(|n| n.is_element() && n.tag_name().name() == "record")
            {
                let Some(h) = child(rec, "header") else {
                    continue;
                };
                if h.attribute("status") == Some("deleted") {
                    continue;
                }
                let ident = text_of(h, "identifier")
                    .ok_or_else(|| shape("$.record.header.identifier", "missing"))?;
                let id = ident.trim_start_matches("oai:arXiv.org:").to_owned();
                let stamp = text_of(h, "datestamp").unwrap_or_default();
                newest = newest.max(stamp.clone());
                let title = child(rec, "metadata")
                    .and_then(|m| child(m, "arXiv"))
                    .and_then(|a| text_of(a, "title"));
                out.refs.push(ItemRef {
                    source_id: ID.into(),
                    kind: "arxiv.record".into(),
                    key: id.clone(),
                    url: format!("https://arxiv.org/abs/{id}"),
                    event_key_hint: eventkey::paper("arxiv", &id),
                    published_at: date(&stamp).map(day_vintage),
                    title,
                    meta: json!({"set": set, "datestamp": stamp}),
                });
            }
            if !newest.is_empty() {
                seen.insert(set.clone(), newest);
            }
        }
        out.next_cursor = serde_json::to_string(&seen).ok();
        Ok(out)
    }

    fn fetch_requests(&self, item: &ItemRef) -> Vec<Request> {
        vec![Request::xml(get_url(&item.key))]
    }

    fn parse(&self, item: &ItemRef, snaps: &[Snapshot]) -> Result<Parsed, ParseError> {
        let snap = find(snaps, &get_url(&item.key))?;
        let d = doc(&snap.bytes)?;
        if let Some((code, msg)) = oai_error(&d) {
            return Err(shape("$.error", format!("{code}: {msg}")));
        }
        let a = d
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == "arXiv")
            .ok_or_else(|| shape("$.GetRecord.record.metadata.arXiv", "missing"))?;
        let id = text_of(a, "id").ok_or_else(|| shape("$..arXiv.id", "missing"))?;
        let title = text_of(a, "title").ok_or_else(|| shape("$..arXiv.title", "missing"))?;
        let created = text_of(a, "created").and_then(|c| date(&c));
        let mut out = Parsed::default();
        out.candidates.push(CandidateDraft {
            event_key: eventkey::paper("arxiv", &id),
            url: format!("https://arxiv.org/abs/{id}"),
            title,
            published_at: created.map(day_vintage),
            rule_key: "family:paper".into(),
            dp_ids: vec![],
            entity: None,
        });
        Ok(out)
    }
}
