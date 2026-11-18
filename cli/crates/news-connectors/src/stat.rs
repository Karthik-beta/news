//! The shared shape of the statistics Stringers (ECB, Eurostat, OECD, ABS, World Bank, IMF, ONS,
//! BEA): a catalogue of followed series, a cheap *probe* request per series, and the *history*
//! request that downloads it.
//!
//! These sources publish no change feed (or only a coarse one), so discovery is a hash comparison:
//! the probe asks for the latest observation only; the Runner's conditional GET makes an unchanged
//! probe a 304, and a probe whose bytes differ from the one the cursor remembers becomes an
//! [`ItemRef`]. The history request then returns the recent observations in full, which is what
//! `parse` reads, so a revised value arrives as a new vintage. The source's own `updated` timestamp
//! is the vintage when it states one; otherwise the day the bytes were fetched.

use std::collections::BTreeMap;

use serde_json::json;

use crate::util::sha_hex;
use crate::{Connector, Cx, Discovery, ItemRef, ParseError, Parsed, Request, Snapshot};

/// One followed series.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatSeries {
    /// Our id for it, unique within the source (`FM.MRR`, `CPI.AU`); what `Cx::tracked` lists.
    pub id: &'static str,
    /// Event-key country (`ea`, `au`, `us`) and series family (`policy_rate`, `cpi`).
    pub country: &'static str,
    pub family: &'static str,
    /// Source-specific query: SDMX flow and key, Eurostat dataset and filters, World Bank indicator.
    pub spec: &'static str,
    /// Entity the series is about: `(scheme, value, name)`; `iso3166` for countries.
    pub entity: (&'static str, &'static str, &'static str),
}

pub trait StatSource: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    fn parser_version(&self) -> u32;
    fn catalogue(&self) -> &'static [StatSeries];
    /// The cheap request used for change detection.
    fn probe(&self, s: &StatSeries) -> Request;
    /// The requests that download the series' recent history (and any metadata it needs).
    fn history(&self, s: &StatSeries) -> Vec<Request>;
    /// Human-facing page of the series at the source.
    fn page(&self, s: &StatSeries) -> String;
    /// Pure: the history snapshots to points. `parser` is `id@version`.
    fn parse_series(
        &self,
        s: &StatSeries,
        item: &ItemRef,
        snaps: &[Snapshot],
        parser: &str,
    ) -> Result<Parsed, ParseError>;
}

fn cursor_map(cursor: Option<&str>) -> BTreeMap<String, String> {
    cursor
        .and_then(|c| serde_json::from_str(c).ok())
        .unwrap_or_default()
}

impl<T: StatSource> Connector for T {
    fn id(&self) -> &'static str {
        StatSource::id(self)
    }
    fn source_id(&self) -> &'static str {
        StatSource::id(self)
    }
    fn parser_version(&self) -> u32 {
        StatSource::parser_version(self)
    }

    fn discovery_requests(&self, cx: &Cx, _cursor: Option<&str>) -> Vec<Request> {
        self.catalogue()
            .iter()
            .filter(|s| cx.tracked(StatSource::id(self)).iter().any(|t| t == s.id))
            .map(|s| self.probe(s))
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
        for s in self
            .catalogue()
            .iter()
            .filter(|s| cx.tracked(StatSource::id(self)).iter().any(|t| t == s.id))
        {
            let probe = self.probe(s);
            let Some(snap) = snaps.iter().find(|n| n.url == probe.url) else {
                continue; // 304 or not requested: nothing new
            };
            let sha = sha_hex(&snap.sha256);
            if seen.get(s.id) == Some(&sha) {
                continue;
            }
            seen.insert(s.id.to_owned(), sha);
            out.refs.push(ItemRef {
                source_id: StatSource::id(self).into(),
                kind: format!("{}.series", StatSource::id(self)),
                key: s.id.into(),
                url: self.page(s),
                event_key_hint: None,
                published_at: Some(snap.fetched_at),
                title: None,
                meta: json!({}),
            });
        }
        out.next_cursor = serde_json::to_string(&seen).ok();
        Ok(out)
    }

    fn fetch_requests(&self, item: &ItemRef) -> Vec<Request> {
        self.catalogue()
            .iter()
            .find(|s| s.id == item.key)
            .map(|s| self.history(s))
            .unwrap_or_default()
    }

    fn parse(&self, item: &ItemRef, snaps: &[Snapshot]) -> Result<Parsed, ParseError> {
        let s = self
            .catalogue()
            .iter()
            .find(|s| s.id == item.key)
            .ok_or_else(|| {
                crate::util::shape("$.key", format!("{} is not in the catalogue", item.key))
            })?;
        self.parse_series(s, item, snaps, &self.parser())
    }
}
