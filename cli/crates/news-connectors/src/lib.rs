//! Stringers: one [`Connector`] per source (Bureau Design §3.2), named by `sources.connector`.
//!
//! A connector says *what* to request and how to read the bytes; it never opens a socket. The Runner
//! in `news-wire` performs every request (rate limits, robots.txt, conditional GET, Morgue), so:
//!
//! * [`Connector::discovery_requests`] / [`Connector::fetch_requests`] describe requests;
//! * [`Connector::parse_discovery`] and [`Connector::parse`] are **pure** functions of snapshot bytes,
//!   versioned by [`Connector::parser_version`], tested from recorded fixtures and re-runnable on old
//!   snapshots after a parser fix (Salvage).
//!
//! Values come out exactly as published ([`news_ledger::DataPoint`]): nothing converts a currency,
//! unit, scale or fiscal period.

use chrono::{DateTime, NaiveDate, Utc};
use news_core::job::ErrorClass;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use news_ledger::{DataPoint, Missing};

pub mod bls;
pub mod fred;
pub mod sec;
mod util;

/// One request a connector wants made. The URL never carries a secret: a free API key is named by
/// the env var that holds it and appended by the Runner at send time, so snapshots, logs and
/// `fetch_state` only ever see the redacted URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub url: String,
    pub secret: Option<SecretParam>,
    /// Allowed `Content-Type` essences; anything else is aborted as `invalid_data`.
    pub accept: Vec<String>,
    /// Size cap in bytes; a larger body is aborted as `invalid_data`.
    pub max_bytes: u64,
}

/// A query parameter whose value comes from an environment variable (`api_key=$FRED_API_KEY`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretParam {
    pub param: String,
    pub env: String,
}

const MB: u64 = 1024 * 1024;

impl Request {
    pub fn json(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            secret: None,
            accept: vec!["application/json".into()],
            max_bytes: 50 * MB,
        }
    }

    pub fn with_secret(mut self, param: &str, env: &str) -> Self {
        self.secret = Some(SecretParam {
            param: param.into(),
            env: env.into(),
        });
        self
    }
}

/// Bytes as stored in the Morgue, with the provenance a parser may cite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// Redacted URL (no secrets).
    pub url: String,
    pub sha256: [u8; 32],
    pub fetched_at: DateTime<Utc>,
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
}

/// A discovered item: a pointer, not content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemRef {
    pub source_id: String,
    /// Connector-specific kind (`sec.filing`, `fred.series`, `bls.series`).
    pub kind: String,
    /// The item's identity within the source (accession number, series id).
    pub key: String,
    /// Human-facing URL of the item at the source.
    pub url: String,
    pub event_key_hint: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub title: Option<String>,
    /// Connector-specific fields the fetch and parse steps need (form, items, vintage …).
    #[serde(default)]
    pub meta: Value,
}

/// A release the source has announced, for Diary (`calendar_events`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarEntry {
    pub source_id: String,
    pub event_key: String,
    pub title: String,
    pub expected_day: NaiveDate,
    pub expected_at: Option<DateTime<Utc>>,
    /// `exact` | `day` | `window`.
    pub precision: String,
    /// `ics` | `api` | `html` | `derived`.
    pub origin: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Discovery {
    pub refs: Vec<ItemRef>,
    pub calendar: Vec<CalendarEntry>,
    /// Pass back on the next run; `None` keeps the previous cursor.
    pub next_cursor: Option<String>,
}

/// An entity named by an identifier (Roster step 1): created or found by `(scheme, value)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityRef {
    pub scheme: String,
    pub value: String,
    /// `entities.kind`: `company`, `country` …
    pub kind: String,
    pub name: String,
}

/// A candidate before Clerk classifies it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateDraft {
    pub event_key: Option<String>,
    pub url: String,
    pub title: String,
    pub published_at: Option<DateTime<Utc>>,
    /// Key into Clerk's rules table: `form:10-K`, `family:cpi` …
    pub rule_key: String,
    /// Data points of this item (not history carried along for comparisons).
    pub dp_ids: Vec<String>,
    pub entity: Option<(String, String)>,
}

/// A unit the vocabulary does not know: stored verbatim, flagged for a human, never guessed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeedsHuman {
    pub reason: String,
    pub series_key: String,
    pub locator: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Parsed {
    pub points: Vec<DataPoint>,
    pub missing: Vec<Missing>,
    pub entities: Vec<EntityRef>,
    pub candidates: Vec<CandidateDraft>,
    pub needs_human: Vec<NeedsHuman>,
}

/// A parse failure with the locator where it failed. Never retried blindly: a parser fix plus the
/// Salvage workflow re-runs it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{kind:?} at {locator}: {message}")]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub locator: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParseErrorKind {
    /// The response no longer has the shape we know (`upstream_changed`).
    Shape,
    /// A value inside a known shape is malformed (`invalid_data`).
    Value,
    /// The snapshot the parser needs is not among those given.
    MissingSnapshot,
    /// The source reported an error in a 200 response (BLS `REQUEST_NOT_PROCESSED`).
    Upstream,
    /// The source has not caught up yet (companyfacts minutes behind a new filing): retry later.
    NotYet,
}

impl ParseError {
    pub fn new(
        kind: ParseErrorKind,
        locator: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            locator: locator.into(),
            message: message.into(),
        }
    }

    pub fn class(&self) -> ErrorClass {
        match self.kind {
            ParseErrorKind::Shape | ParseErrorKind::Upstream => ErrorClass::UpstreamChanged,
            ParseErrorKind::Value => ErrorClass::InvalidData,
            ParseErrorKind::MissingSnapshot => ErrorClass::Bug,
            ParseErrorKind::NotYet => ErrorClass::Transient,
        }
    }
}

/// What a connector may know about "now" and what is tracked. Passed in, so parsing stays pure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cx {
    pub today: NaiveDate,
    /// SEC CIKs to follow (10-digit or bare).
    pub sec_ciks: Vec<String>,
    /// FRED series ids to follow; each must be in [`fred::SERIES`].
    pub fred_series: Vec<String>,
    /// BLS series ids to follow; each must be in [`bls::SERIES`].
    pub bls_series: Vec<String>,
}

pub trait Connector: Send + Sync {
    /// `sources.connector`.
    fn id(&self) -> &'static str;
    /// The `sources.source_id` this connector serves.
    fn source_id(&self) -> &'static str;
    /// Bumped whenever parsing output changes; recorded in `data_points.parser`.
    fn parser_version(&self) -> u32;
    fn parser(&self) -> String {
        format!("{}@{}", self.id(), self.parser_version())
    }
    /// The cheap listing requests for a discovery pass since `cursor`.
    fn discovery_requests(&self, cx: &Cx, cursor: Option<&str>) -> Vec<Request>;
    /// Pure: item refs (not content) and calendar entries from the listing snapshots. Snapshots that
    /// came back 304 are simply absent.
    fn parse_discovery(
        &self,
        cx: &Cx,
        cursor: Option<&str>,
        snaps: &[Snapshot],
    ) -> Result<Discovery, ParseError>;
    /// The requests that download one item (one ref may need several files).
    fn fetch_requests(&self, item: &ItemRef) -> Vec<Request>;
    /// Pure: normalised output from the item's snapshots. No network, no clock.
    fn parse(&self, item: &ItemRef, snaps: &[Snapshot]) -> Result<Parsed, ParseError>;
}

/// Every connector, by `sources.connector` id.
pub fn connector(id: &str) -> Option<&'static dyn Connector> {
    static SEC: sec::SecEdgar = sec::SecEdgar;
    static FRED: fred::Fred = fred::Fred;
    static BLS: bls::Bls = bls::Bls;
    match id {
        sec::ID => Some(&SEC),
        fred::ID => Some(&FRED),
        bls::ID => Some(&BLS),
        _ => None,
    }
}

/// Every connector id built so far.
pub const IDS: &[&str] = &[sec::ID, fred::ID, bls::ID];
