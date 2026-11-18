//! The central-bank statement family (Bureau §3.2): one Stringer per bank, one shared shape.
//!
//! Discovery reads the bank's press feed (RSS or RDF) and keeps the entries whose title matches that
//! bank's statement wording; the event key is `policy:<bank>:<decision date>` with the feed's
//! publication day. Fetch downloads the statement page; parse reads nothing from it but its title
//! (the page itself is the proof in the Morgue, hashed for change detection). Rates and wording are
//! not extracted by code here: statements are text, and numbers on pages come from data sources.
//!
//! Built: Fed, ECB, BoE, BoC, RBA. **Not built**: BoJ (the site answers automated requests with a
//! "Network Busy" page) and RBI (its servers answer "Unauthorised Access"): both refuse automated
//! access, so they stay out until they publish a feed we may read.

use std::collections::BTreeSet;

use serde_json::json;

use news_ledger::eventkey;

use crate::feed;
use crate::util::{find, shape, value_err};
use crate::{
    CandidateDraft, Connector, Cx, Discovery, EntityRef, ItemRef, ParseError, Parsed, Request,
    Snapshot,
};

pub const PARSER_VERSION: u32 = 1;

pub struct Bank {
    /// Connector and source id (`cb-fed`).
    pub id: &'static str,
    /// Event-key bank (`fed`) and Roster identifier value for scheme `cb`.
    pub key: &'static str,
    pub name: &'static str,
    pub feed: &'static str,
    /// Lower-case title fragments that mark a monetary-policy statement.
    pub statements: &'static [&'static str],
    /// When not empty, the title must also contain one of these decision verbs: a notice *about*
    /// the policy rate (a published schedule) is not a decision.
    pub verbs: &'static [&'static str],
}

pub static BANKS: [Bank; 5] = [
    Bank {
        id: "cb-fed",
        key: "fed",
        name: "Federal Reserve",
        feed: "https://www.federalreserve.gov/feeds/press_monetary.xml",
        statements: &["fomc statement"],
        verbs: &[],
    },
    Bank {
        id: "cb-ecb",
        key: "ecb",
        name: "European Central Bank",
        feed: "https://www.ecb.europa.eu/rss/press.html",
        statements: &["monetary policy decisions"],
        verbs: &[],
    },
    Bank {
        id: "cb-boe",
        key: "boe",
        name: "Bank of England",
        feed: "https://www.bankofengland.co.uk/rss/news",
        statements: &["monetary policy summary"],
        verbs: &[],
    },
    Bank {
        id: "cb-boc",
        key: "boc",
        name: "Bank of Canada",
        feed: "https://www.bankofcanada.ca/content_type/press-releases/feed/",
        statements: &["policy rate", "policy interest rate"],
        verbs: &[
            "maintains",
            "holds",
            "keeps",
            "raises",
            "increases",
            "lowers",
            "reduces",
            "cuts",
        ],
    },
    Bank {
        id: "cb-rba",
        key: "rba",
        name: "Reserve Bank of Australia",
        feed: "https://www.rba.gov.au/rss/rss-cb-media-releases.xml",
        statements: &["statement by the monetary policy board"],
        verbs: &[],
    },
];

pub fn bank(id: &str) -> Option<&'static Bank> {
    BANKS.iter().find(|b| b.id == id)
}

impl Bank {
    fn is_statement(&self, title: &str) -> bool {
        let t = title.to_lowercase();
        self.statements.iter().any(|s| t.contains(s))
            && (self.verbs.is_empty() || self.verbs.iter().any(|v| t.contains(v)))
    }
}

impl Connector for Bank {
    fn id(&self) -> &'static str {
        self.id
    }
    fn source_id(&self) -> &'static str {
        self.id
    }
    fn parser_version(&self) -> u32 {
        PARSER_VERSION
    }

    fn discovery_requests(&self, _cx: &Cx, _cursor: Option<&str>) -> Vec<Request> {
        vec![Request::xml(self.feed)]
    }

    fn parse_discovery(
        &self,
        _cx: &Cx,
        cursor: Option<&str>,
        snaps: &[Snapshot],
    ) -> Result<Discovery, ParseError> {
        let Some(snap) = snaps.iter().find(|s| s.url == self.feed) else {
            return Ok(Discovery::default());
        };
        let mut seen: BTreeSet<String> = cursor
            .and_then(|c| serde_json::from_str(c).ok())
            .unwrap_or_default();
        let mut out = Discovery::default();
        for e in feed::entries(&snap.bytes)? {
            if !self.is_statement(&e.title) || seen.contains(&e.id) {
                continue;
            }
            let Some(day) = e.day() else { continue };
            out.refs.push(ItemRef {
                source_id: self.id.into(),
                kind: "cb.statement".into(),
                key: e.id.clone(),
                url: e.link.clone(),
                event_key_hint: eventkey::policy(self.key, day),
                published_at: e.published,
                title: Some(e.title.clone()),
                meta: json!({}),
            });
            seen.insert(e.id);
        }
        // Keep the cursor bounded: the feeds list a few dozen entries.
        let keep: Vec<String> = seen.iter().rev().take(200).cloned().collect();
        out.next_cursor = serde_json::to_string(&keep).ok();
        Ok(out)
    }

    fn fetch_requests(&self, item: &ItemRef) -> Vec<Request> {
        vec![Request::html(item.url.clone())]
    }

    fn parse(&self, item: &ItemRef, snaps: &[Snapshot]) -> Result<Parsed, ParseError> {
        let snap = find(snaps, &item.url)?;
        if snap.bytes.len() < 200 {
            return Err(value_err("$", "statement page is empty"));
        }
        let title = feed::html_title(&snap.bytes)
            .or_else(|| item.title.clone())
            .ok_or_else(|| shape("$.title", "no <title> and no feed title"))?;
        let entity = ("cb".to_owned(), self.key.to_owned());
        let mut out = Parsed::default();
        out.candidates.push(CandidateDraft {
            event_key: item.event_key_hint.clone(),
            url: item.url.clone(),
            title,
            published_at: item.published_at,
            rule_key: "kind:statement".into(),
            dp_ids: vec![],
            entity: Some(entity.clone()),
        });
        out.entities.push(EntityRef {
            scheme: entity.0,
            value: entity.1,
            kind: "central_bank".into(),
            name: self.name.into(),
        });
        Ok(out)
    }
}
