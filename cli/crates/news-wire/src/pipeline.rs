//! Discover and fetch for any connector: Runner → Morgue → `snapshots` / `fetch_state`.
//!
//! State goes through [`StateStore`], so the pipeline runs against PostgreSQL ([`crate::store::PgState`])
//! or in memory ([`MemoryState`], for tests and dry runs).

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use news_connectors::{Connector, Cx, Discovery, ItemRef, ParseError, Request, Snapshot};
use news_core::job::{ErrorClass, JobFailure};

use crate::morgue::{Morgue, retain_until};
use crate::pace::BoxFut;
use crate::runner::{FetchError, Fetched, Outcome, Runner, SourcePolicy, Validators};

/// What the last successful fetch of a URL left behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prior {
    pub validators: Validators,
    pub sha256: [u8; 32],
    pub fetched_at: DateTime<Utc>,
    pub content_type: Option<String>,
}

pub trait StateStore: Send + Sync {
    fn prior<'a>(&'a self, url: &'a str) -> BoxFut<'a, Result<Option<Prior>, WireError>>;
    /// Record a 200: the `snapshots` row (once per hash) and `fetch_state`.
    fn record<'a>(
        &'a self,
        source_id: &'a str,
        f: &'a Fetched,
        storage_path: &'a str,
        retain_until: DateTime<Utc>,
    ) -> BoxFut<'a, Result<(), WireError>>;
    /// Record a 304 in `fetch_state`.
    fn not_modified<'a>(
        &'a self,
        source_id: &'a str,
        url: &'a str,
        at: DateTime<Utc>,
    ) -> BoxFut<'a, Result<(), WireError>>;
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum WireError {
    #[error(transparent)]
    Fetch(#[from] FetchError),
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error("morgue: {0}")]
    Morgue(String),
    #[error("database: {0}")]
    Db(String),
}

impl From<WireError> for JobFailure {
    fn from(e: WireError) -> Self {
        match e {
            WireError::Fetch(f) => f.into(),
            WireError::Parse(p) => JobFailure::new(p.class(), p.to_string()),
            WireError::Morgue(m) => JobFailure::new(ErrorClass::Transient, m),
            WireError::Db(m) => JobFailure::new(ErrorClass::Transient, m),
        }
    }
}

/// The result of fetching one item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchResult {
    /// One snapshot per request: fresh bytes, or the stored bytes of a 304.
    pub snapshots: Vec<Snapshot>,
    /// False when every request came back 304: nothing to parse.
    pub changed: bool,
}

pub struct Wire<'a> {
    pub runner: &'a Runner,
    pub morgue: &'a Morgue,
    pub state: &'a dyn StateStore,
}

enum Got {
    Fresh(Snapshot),
    Unchanged(Option<Prior>),
}

impl Wire<'_> {
    async fn get(&self, policy: &SourcePolicy, req: &Request) -> Result<Got, WireError> {
        let prior = self.state.prior(&req.url).await?;
        match self
            .runner
            .fetch(policy, req, prior.as_ref().map(|p| &p.validators))
            .await?
        {
            Outcome::NotModified => {
                self.state
                    .not_modified(&policy.source_id, &req.url, self.runner.now())
                    .await?;
                Ok(Got::Unchanged(prior))
            }
            Outcome::Fetched(f) => {
                let path = self
                    .morgue
                    .put(&f.sha256, &f.bytes)
                    .map_err(|e| WireError::Morgue(e.to_string()))?;
                let keep = retain_until(f.content_type.as_deref(), f.fetched_at);
                self.state
                    .record(&policy.source_id, &f, &path, keep)
                    .await?;
                Ok(Got::Fresh(Snapshot {
                    url: f.url,
                    sha256: f.sha256,
                    fetched_at: f.fetched_at,
                    content_type: f.content_type,
                    bytes: f.bytes,
                }))
            }
        }
    }

    /// Run a discovery pass. Listings that came back 304 are absent from what the connector parses.
    pub async fn discover(
        &self,
        policy: &SourcePolicy,
        conn: &dyn Connector,
        cx: &Cx,
        cursor: Option<&str>,
    ) -> Result<Discovery, WireError> {
        let mut snaps = Vec::new();
        for req in conn.discovery_requests(cx, cursor) {
            if let Got::Fresh(s) = self.get(policy, &req).await? {
                snaps.push(s);
            }
        }
        Ok(conn.parse_discovery(cx, cursor, &snaps)?)
    }

    /// Download one item. A 304 on every file means `changed: false` and no parse.
    pub async fn fetch(
        &self,
        policy: &SourcePolicy,
        conn: &dyn Connector,
        item: &ItemRef,
    ) -> Result<FetchResult, WireError> {
        let mut snapshots = Vec::new();
        let mut changed = false;
        for req in conn.fetch_requests(item) {
            match self.get(policy, &req).await? {
                Got::Fresh(s) => {
                    changed = true;
                    snapshots.push(s);
                }
                Got::Unchanged(Some(p)) => {
                    let bytes = self
                        .morgue
                        .get(&p.sha256)
                        .map_err(|e| WireError::Morgue(e.to_string()))?;
                    snapshots.push(Snapshot {
                        url: req.url.clone(),
                        sha256: p.sha256,
                        fetched_at: p.fetched_at,
                        content_type: p.content_type,
                        bytes,
                    });
                }
                Got::Unchanged(None) => {
                    return Err(WireError::Fetch(FetchError {
                        class: ErrorClass::Bug,
                        message: format!("304 for {} with no stored snapshot", req.url),
                        retry_after: None,
                    }));
                }
            }
        }
        Ok(FetchResult { snapshots, changed })
    }
}

/// In-memory state for tests and dry runs.
#[derive(Default)]
pub struct MemoryState {
    urls: Mutex<HashMap<String, Prior>>,
    pub snapshots: Mutex<Vec<(String, [u8; 32], String)>>,
    pub not_modified: Mutex<u32>,
}

impl StateStore for MemoryState {
    fn prior<'a>(&'a self, url: &'a str) -> BoxFut<'a, Result<Option<Prior>, WireError>> {
        Box::pin(async move { Ok(self.urls.lock().ok().and_then(|m| m.get(url).cloned())) })
    }

    fn record<'a>(
        &'a self,
        source_id: &'a str,
        f: &'a Fetched,
        storage_path: &'a str,
        _retain_until: DateTime<Utc>,
    ) -> BoxFut<'a, Result<(), WireError>> {
        Box::pin(async move {
            if let Ok(mut m) = self.urls.lock() {
                m.insert(
                    f.url.clone(),
                    Prior {
                        validators: Validators {
                            etag: f.etag.clone(),
                            last_modified: f.last_modified.clone(),
                        },
                        sha256: f.sha256,
                        fetched_at: f.fetched_at,
                        content_type: f.content_type.clone(),
                    },
                );
            }
            if let Ok(mut s) = self.snapshots.lock() {
                s.push((source_id.to_owned(), f.sha256, storage_path.to_owned()));
            }
            Ok(())
        })
    }

    fn not_modified<'a>(
        &'a self,
        _source_id: &'a str,
        _url: &'a str,
        _at: DateTime<Utc>,
    ) -> BoxFut<'a, Result<(), WireError>> {
        Box::pin(async move {
            if let Ok(mut n) = self.not_modified.lock() {
                *n += 1;
            }
            Ok(())
        })
    }
}
