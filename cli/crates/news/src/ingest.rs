//! Ingest: one source through Wire and Clerk, and the job handlers that run it (#9).
//!
//! The same functions serve the terminal (`news discover|fetch|parse <source>`, tier W) and the Bureau
//! service (`source.discover`, `doc.fetch`, `doc.parse`, `calendar.capture` and `story.detect`), so a
//! preset's ingest jobs and a hand-run command do exactly the same work:
//!
//! 1. [`Ingest::discover`]: the connector's listing through the Runner; calendar entries go to Diary;
//! 2. [`Ingest::fetch`]: each ref's files through the Runner into the Morgue (`snapshots`, `fetch_state`);
//! 3. [`Ingest::parse`]: the connector's pure `parse`, the licence gate and rule-based classification,
//!    then the Ledger's idempotent store (data points with vintages, candidates).
//!
//! `source.discover` runs all three for every ref it finds, inside the one job: the preset's discover
//! step waits for its jobs, so candidates exist when story detection starts. `doc.fetch` does the same
//! for one ref; `doc.parse` re-parses stored snapshots without a request (Salvage, after a parser fix).
//!
//! The service installs one [`Ingest`] ([`install`]), so per-host buckets and robots.txt caches are
//! shared by every job. Without one a handler fails with class `bug`, saying so.
//!
//! `NEWS_WIRE_FIXTURES=<routes.json>` swaps the live transport for recorded files (see [`fixtures`]),
//! so tests and demos never open a socket.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use chrono::{DateTime, Utc};
use news_clerk::candidates::{self, Candidate};
use news_clerk::ledger::{self, StoreReport};
use news_connectors::{Connector, Cx, Discovery, ItemRef, Snapshot, bls, fred, sec};
use news_core::job::{ErrorClass, JobFailure, JobFuture, JobInput};
use news_core::{ErrorCode, NewsError};
use news_store::PgPool;
use news_wire::fake::{FakeTransport, Reply};
use news_wire::morgue::{Morgue, hex};
use news_wire::pace::{SystemClock, VirtualClock};
use news_wire::pipeline::{FetchResult, StateStore, Wire, WireError};
use news_wire::runner::{ReqwestTransport, Runner, RunnerConfig, SourcePolicy};
use news_wire::store::PgState;
use serde::Deserialize;
use serde_json::{Value, json};

/// The one context the Bureau service installs for the ingest job kinds.
static INGEST: OnceLock<Ingest> = OnceLock::new();

/// Install the context for the handlers; `false` if one is already installed.
pub fn install(i: Ingest) -> bool {
    INGEST.set(i).is_ok()
}

/// Everything one source's passes need.
pub struct Ingest {
    pub db: PgPool,
    pub runner: Runner,
    pub morgue: Morgue,
    /// Read listings again instead of sending validators. A terminal run has no cursor, so a 304 on the
    /// listing would leave `news discover` and `news fetch` with nothing to show; the service keeps the
    /// conditional GET.
    pub fresh_listings: bool,
    /// Tracked ids by connector id, ahead of `NEWS_TRACKED_<CONNECTOR>` and the defaults.
    pub tracked: BTreeMap<String, Vec<String>>,
}

/// A source's row, its Runner policy and the tracked ids its connector asks about.
pub struct Source {
    pub id: String,
    pub conn: &'static dyn Connector,
    pub policy: SourcePolicy,
    pub licence: Option<String>,
    pub cx: Cx,
}

#[derive(Debug)]
pub enum IngestError {
    NoSource(String),
    Disabled(String),
    Job(JobFailure),
}

impl From<JobFailure> for IngestError {
    fn from(f: JobFailure) -> Self {
        Self::Job(f)
    }
}
impl From<WireError> for IngestError {
    fn from(e: WireError) -> Self {
        Self::Job(e.into())
    }
}
impl From<sqlx::Error> for IngestError {
    fn from(e: sqlx::Error) -> Self {
        Self::Job(JobFailure::new(ErrorClass::Transient, e.to_string()))
    }
}

impl From<IngestError> for JobFailure {
    fn from(e: IngestError) -> Self {
        match e {
            IngestError::NoSource(id) => {
                JobFailure::new(ErrorClass::InvalidData, format!("no source {id:?}"))
            }
            IngestError::Disabled(id) => JobFailure::new(
                ErrorClass::InvalidData,
                format!("source {id:?} is disabled"),
            ),
            IngestError::Job(f) => f,
        }
    }
}

/// The CLI's error codes for a failed pass. `upstream_changed` and `invalid_data` have no code of their
/// own (§6.2), so they are `internal` with the class in the message.
impl From<IngestError> for NewsError {
    fn from(e: IngestError) -> Self {
        match e {
            IngestError::NoSource(id) => {
                NewsError::not_found(format!("no source {id:?}")).with_hint("news sources list")
            }
            IngestError::Disabled(id) => {
                NewsError::precondition(format!("source {id:?} is disabled"))
                    .with_hint("a source is enabled from Board")
            }
            IngestError::Job(f) => {
                let code = match f.class {
                    ErrorClass::RateLimited => ErrorCode::RateLimited,
                    ErrorClass::LicenceBlocked => ErrorCode::LicenceBlocked,
                    // The Runner refuses a request it must not make (SEC without a contact e-mail).
                    ErrorClass::Bug => ErrorCode::Precondition,
                    ErrorClass::Transient => ErrorCode::UpstreamUnavailable,
                    _ => ErrorCode::Internal,
                };
                let message = match code {
                    ErrorCode::Internal => format!("{}: {}", f.class.as_str(), f.message),
                    _ => f.message,
                };
                NewsError::new(code, message)
            }
        }
    }
}

/// What `fetch` left in the Morgue for one file.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SnapshotInfo {
    pub url: String,
    pub sha256: String,
    pub bytes: usize,
    pub content_type: Option<String>,
}

/// What `parse` produced for one item.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ParseDone {
    pub points: usize,
    pub candidates: Vec<Candidate>,
    pub stored: StoreReport,
}

/// One item through fetch and parse.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ItemDone {
    pub item: ItemRef,
    pub changed: bool,
    pub snapshots: Vec<SnapshotInfo>,
    pub parsed: ParseDone,
}

/// One whole pass over a source.
pub struct Pass {
    pub discovery: Discovery,
    pub items: Vec<ItemDone>,
    /// Items whose fetch or parse failed with a class a retry cannot fix: `(key, failure)`.
    pub failed: Vec<(String, JobFailure)>,
}

impl Pass {
    /// Every event key the pass saw: the refs' hints and the candidates' keys.
    pub fn event_keys(&self) -> BTreeSet<String> {
        let hints = self
            .discovery
            .refs
            .iter()
            .filter_map(|r| r.event_key_hint.clone());
        let keys = self
            .items
            .iter()
            .flat_map(|i| i.parsed.candidates.iter())
            .filter_map(|c| c.draft.event_key.clone());
        hints.chain(keys).collect()
    }

    pub fn totals(&self) -> StoreReport {
        let mut t = StoreReport::default();
        for i in &self.items {
            let s = &i.parsed.stored;
            t.points_inserted += s.points_inserted;
            t.points_unchanged += s.points_unchanged;
            t.revisions += s.revisions;
            t.candidates_inserted += s.candidates_inserted;
            t.missing += s.missing;
            t.needs_human += s.needs_human;
        }
        t
    }

    fn summary(&self, source: &str) -> Value {
        json!({
            "source_id": source,
            "refs": self.discovery.refs.len(),
            "calendar_entries": self.discovery.calendar.len(),
            "items": self.items.len(),
            "failed": self.failed.iter().map(|(k, f)| json!({"key": k, "class": f.class.as_str(), "message": f.message})).collect::<Vec<_>>(),
            "stored": self.totals(),
            "event_keys": self.event_keys(),
        })
    }
}

/// `NEWS_TRACKED_<CONNECTOR>` (`NEWS_TRACKED_SEC_EDGAR=320193,789019`): the ids a connector follows,
/// when the registry's defaults are not what is wanted.
fn tracked_env(connector: &str) -> Option<Vec<String>> {
    let name = format!(
        "NEWS_TRACKED_{}",
        connector.to_uppercase().replace('-', "_")
    );
    let v = std::env::var(name).ok()?;
    let ids: Vec<String> = v
        .split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();
    (!ids.is_empty()).then_some(ids)
}

impl Ingest {
    pub fn new(db: PgPool, runner: Runner, morgue: Morgue) -> Self {
        Self {
            db,
            runner,
            morgue,
            fresh_listings: false,
            tracked: BTreeMap::new(),
        }
    }

    /// The live Runner over `work` (the Morgue's root), or the recorded files of `NEWS_WIRE_FIXTURES`.
    pub fn from_env(db: PgPool, work: PathBuf) -> Result<Self, String> {
        match std::env::var_os("NEWS_WIRE_FIXTURES").filter(|v| !v.is_empty()) {
            Some(file) => {
                tracing::warn!("NEWS_WIRE_FIXTURES is set: sources are read from recorded files");
                Self::with_fixtures(db, work, Path::new(&file))
            }
            None => {
                news_wire::runner::install_tls_provider();
                let transport = ReqwestTransport::new().map_err(|e| e.message)?;
                let runner = Runner::new(
                    Arc::new(transport),
                    Arc::new(SystemClock),
                    RunnerConfig::from_env(),
                );
                Ok(Self::new(db, runner, Morgue::new(work)))
            }
        }
    }

    /// A Runner that answers from a routes file ([`fixtures`]) on a virtual clock: no socket, no waiting.
    pub fn with_fixtures(db: PgPool, work: PathBuf, routes: &Path) -> Result<Self, String> {
        let (transport, now) = fixtures(routes)?;
        let config = RunnerConfig {
            contact_email: Some("fixtures@example.org".into()),
            // The fixture transport never sends it anywhere.
            secret: Arc::new(|_| Some("FIXTURE".into())),
        };
        let runner = Runner::new(
            Arc::new(transport),
            Arc::new(VirtualClock::new(now)),
            config,
        );
        Ok(Self::new(db, runner, Morgue::new(work)))
    }

    /// The ids a connector follows: this context's own list, else `NEWS_TRACKED_<CONNECTOR>`.
    fn tracked_ids(&self, connector: &str) -> Option<Vec<String>> {
        self.tracked
            .get(connector)
            .cloned()
            .or_else(|| tracked_env(connector))
    }

    /// The source's row, policy and tracked ids.
    pub async fn source(&self, id: &str) -> Result<Source, IngestError> {
        let row: Option<(String, Option<String>)> =
            sqlx::query_as("select connector, licence from sources where source_id = $1")
                .bind(id)
                .fetch_optional(&self.db)
                .await?;
        let (connector, licence) = row.ok_or_else(|| IngestError::NoSource(id.to_owned()))?;
        let policy = news_wire::store::policy(&self.db, id)
            .await?
            .ok_or_else(|| IngestError::Disabled(id.to_owned()))?;
        let conn = news_connectors::connector(&connector).ok_or_else(|| {
            JobFailure::new(
                ErrorClass::Bug,
                format!("source {id:?} names connector {connector:?}, which is not built"),
            )
        })?;
        let cx = self.cx(conn).await?;
        Ok(Source {
            id: id.to_owned(),
            conn,
            policy,
            licence,
            cx,
        })
    }

    // shortcut: tracked lists have no home until Standing has keys for them. FRED and BLS follow their
    // whole catalogue, SEC the companies the Ledger has fiscal facts for, the rest nothing; each can be
    // set with NEWS_TRACKED_<CONNECTOR>. Replace with Standing keys when #29 adds the settings screen.
    async fn cx(&self, conn: &dyn Connector) -> Result<Cx, sqlx::Error> {
        let set = self.tracked_ids(conn.id());
        let sec_ciks = match (conn.id() == sec::ID, &set) {
            (true, Some(ids)) => ids.clone(),
            (true, None) => news_diary::store::tracked_ciks(&self.db).await?,
            _ => Vec::new(),
        };
        let catalogue = |ids: Vec<String>| set.clone().unwrap_or(ids);
        let mut tracked = BTreeMap::new();
        if let Some(ids) = &set {
            tracked.insert(conn.id().to_owned(), ids.clone());
        }
        Ok(Cx {
            today: self.runner.now().date_naive(),
            sec_ciks,
            fred_series: if conn.id() == fred::ID {
                catalogue(fred::SERIES.iter().map(|s| s.id.to_owned()).collect())
            } else {
                Vec::new()
            },
            bls_series: if conn.id() == bls::ID {
                catalogue(bls::SERIES.iter().map(|s| s.id.to_owned()).collect())
            } else {
                Vec::new()
            },
            tracked,
        })
    }

    fn wire<'a>(&'a self, state: &'a PgState<'a>) -> Wire<'a> {
        Wire {
            runner: &self.runner,
            morgue: &self.morgue,
            state,
        }
    }

    /// The listing; its calendar entries go to Diary.
    pub async fn discover(&self, s: &Source) -> Result<Discovery, IngestError> {
        let state = PgState { pool: &self.db };
        let wire = self.wire(&state);
        let d = if self.fresh_listings {
            wire.discover_fresh(&s.policy, s.conn, &s.cx, None).await?
        } else {
            wire.discover(&s.policy, s.conn, &s.cx, None).await?
        };
        let entries: Vec<_> = d
            .calendar
            .iter()
            .filter(|e| e.source_id == s.id)
            .cloned()
            .collect();
        if !entries.is_empty() {
            news_diary::store::upsert(&self.db, &entries).await?;
        }
        Ok(d)
    }

    /// One item's files into the Morgue.
    pub async fn fetch(&self, s: &Source, item: &ItemRef) -> Result<FetchResult, IngestError> {
        let state = PgState { pool: &self.db };
        Ok(self.wire(&state).fetch(&s.policy, s.conn, item).await?)
    }

    /// Parse snapshots (pure), classify, and store points and candidates. Always runs, even when
    /// nothing changed: an earlier parse may have failed after its fetch, and storing is idempotent.
    pub async fn parse(
        &self,
        s: &Source,
        item: &ItemRef,
        snaps: &[Snapshot],
    ) -> Result<ParseDone, IngestError> {
        let parsed = s.conn.parse(item, snaps).map_err(WireError::from)?;
        let cands = candidates::prepare(&s.id, s.licence.as_deref(), &parsed)
            .map_err(|e| JobFailure::new(ErrorClass::LicenceBlocked, e.to_string()))?;
        let stored = ledger::store(&self.db, &parsed, &cands, self.runner.now()).await?;
        Ok(ParseDone {
            points: parsed.points.len(),
            candidates: cands,
            stored,
        })
    }

    /// The snapshots a previous fetch left for `item`, read from the Morgue with no request.
    pub async fn stored(&self, s: &Source, item: &ItemRef) -> Result<Vec<Snapshot>, IngestError> {
        let state = PgState { pool: &self.db };
        let mut out = Vec::new();
        for req in s.conn.fetch_requests(item) {
            let prior = state.prior(&req.url).await?;
            let p = prior.ok_or_else(|| {
                JobFailure::new(
                    ErrorClass::InvalidData,
                    format!("no stored snapshot for {}: fetch it first", req.url),
                )
            })?;
            let bytes = self
                .morgue
                .get(&p.sha256)
                .map_err(|e| WireError::Morgue(e.to_string()))?;
            out.push(Snapshot {
                url: req.url,
                sha256: p.sha256,
                fetched_at: p.fetched_at,
                content_type: p.content_type,
                bytes,
            });
        }
        Ok(out)
    }

    /// Fetch and parse one item.
    pub async fn item(&self, s: &Source, item: &ItemRef) -> Result<ItemDone, IngestError> {
        let got = self.fetch(s, item).await?;
        let parsed = self.parse(s, item, &got.snapshots).await?;
        Ok(ItemDone {
            item: item.clone(),
            changed: got.changed,
            snapshots: info(&got.snapshots),
            parsed,
        })
    }

    /// Discover, then fetch and parse every ref. A failure a retry can fix (`transient`,
    /// `rate_limited`) ends the pass at once, so the job is retried or snoozed (every step is
    /// idempotent); any other failure is that item's, and the rest go on. When no item succeeded the
    /// first failure is the pass's.
    pub async fn pass(&self, s: &Source) -> Result<Pass, IngestError> {
        let discovery = self.discover(s).await?;
        let (mut items, mut failed) = (Vec::new(), Vec::new());
        for r in &discovery.refs {
            match self.item(s, r).await {
                Ok(done) => items.push(done),
                Err(IngestError::Job(f))
                    if !matches!(f.class, ErrorClass::Transient | ErrorClass::RateLimited) =>
                {
                    failed.push((r.key.clone(), f));
                }
                Err(e) => return Err(e),
            }
        }
        if items.is_empty()
            && let Some((_, f)) = failed.first()
        {
            return Err(IngestError::Job(f.clone()));
        }
        Ok(Pass {
            discovery,
            items,
            failed,
        })
    }
}

pub fn info(snaps: &[Snapshot]) -> Vec<SnapshotInfo> {
    snaps
        .iter()
        .map(|s| SnapshotInfo {
            url: s.url.clone(),
            sha256: hex(&s.sha256),
            bytes: s.bytes.len(),
            content_type: s.content_type.clone(),
        })
        .collect()
}

/// `NEWS_WIRE_FIXTURES`: a JSON file naming recorded responses, answered by URL prefix (the first
/// matching route wins, so list the longer prefix first). `file` is relative to the routes file.
///
/// ```json
/// {"now": "2026-11-16T08:00:00Z",
///  "routes": [{"prefix": "https://api.bls.gov/robots.txt", "status": 404},
///             {"prefix": "https://api.bls.gov/publicAPI/v2/timeseries/data/CUUR0000SA0",
///              "content_type": "application/json", "file": "bls/CUUR0000SA0.json"}]}
/// ```
///
/// Time is a virtual clock starting at `now`, so pacing never waits.
pub fn fixtures(file: &Path) -> Result<(FakeTransport, DateTime<Utc>), String> {
    #[derive(Deserialize)]
    struct Routes {
        now: DateTime<Utc>,
        routes: Vec<Route>,
    }
    #[derive(Deserialize)]
    struct Route {
        prefix: String,
        #[serde(default = "ok")]
        status: u16,
        content_type: Option<String>,
        file: Option<String>,
    }
    fn ok() -> u16 {
        200
    }
    let text = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    let r: Routes = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", file.display()))?;
    let dir = file.parent().unwrap_or(Path::new("."));
    let fake = FakeTransport::new();
    for route in r.routes {
        let mut reply = match &route.file {
            Some(f) => {
                let bytes = std::fs::read(dir.join(f))
                    .map_err(|e| format!("cannot read {f} named by the routes file: {e}"))?;
                Reply::ok(
                    route.content_type.as_deref().unwrap_or("application/json"),
                    bytes,
                )
            }
            None => Reply::status(route.status),
        };
        reply.status = route.status;
        fake.route(&route.prefix, vec![reply]);
    }
    Ok((fake, r.now))
}

// ---- job handlers --------------------------------------------------------------------------------

fn invalid(message: impl Into<String>) -> JobFailure {
    JobFailure::new(ErrorClass::InvalidData, message)
}

fn transient(e: sqlx::Error) -> JobFailure {
    JobFailure::new(ErrorClass::Transient, e.to_string())
}

fn arg<'a>(input: &'a JobInput, name: &str) -> Result<&'a str, JobFailure> {
    input.args[name]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| invalid(format!("{} needs a string argument {name:?}", input.kind)))
}

fn item_arg(input: &JobInput) -> Result<ItemRef, JobFailure> {
    serde_json::from_value(input.args["item"].clone()).map_err(|e| {
        invalid(format!(
            "{} needs an \"item\" (an ItemRef from a discovery): {e}",
            input.kind
        ))
    })
}

fn news_failure(e: NewsError) -> JobFailure {
    let class = if e.retryable {
        ErrorClass::Transient
    } else {
        ErrorClass::InvalidData
    };
    JobFailure::new(class, e.message)
}

/// Run one ingest job on `ing`; the handlers below call it with the service's context.
///
/// * `source.discover {source_id, from?, to?, backfill?}`: a whole pass. The catch-up window is
///   accepted and not used: a connector lists what its source lists, and a backfill is the same pass.
/// * `doc.fetch {source_id, item}`: fetch and parse one item (an `ItemRef` from a discovery).
/// * `doc.parse {source_id, item}`: the stored snapshots again with the current parser, no request.
/// * `calendar.capture {event_key}`: a pass over the event's source; the event is captured when its
///   key turns up. Not there yet is a result, not a failure: Diary marks it `late` on its own
///   schedule and the next pass looks again.
/// * `story.detect {}`: Taster over every new candidate. No embedder is installed (the `embed`
///   feature of `news-taster` is off), so grouping is by exact copy, SimHash and event key.
pub async fn run_kind(ing: &Ingest, input: JobInput) -> Result<Value, JobFailure> {
    match input.kind.as_str() {
        "source.discover" => {
            let id = arg(&input, "source_id")?;
            let s = ing.source(id).await?;
            input.lease.ensure_lease().await?;
            Ok(ing.pass(&s).await?.summary(id))
        }
        "doc.fetch" => {
            let (id, item) = (arg(&input, "source_id")?, item_arg(&input)?);
            let s = ing.source(id).await?;
            input.lease.ensure_lease().await?;
            let done = ing.item(&s, &item).await?;
            Ok(json!({
                "source_id": id, "key": item.key, "changed": done.changed,
                "snapshots": done.snapshots, "stored": done.parsed.stored,
            }))
        }
        "doc.parse" => {
            let (id, item) = (arg(&input, "source_id")?, item_arg(&input)?);
            let s = ing.source(id).await?;
            let snaps = ing.stored(&s, &item).await?;
            input.lease.ensure_lease().await?;
            let done = ing.parse(&s, &item, &snaps).await?;
            Ok(
                json!({"source_id": id, "key": item.key, "points": done.points, "stored": done.stored}),
            )
        }
        "calendar.capture" => {
            let key = arg(&input, "event_key")?;
            let source: Option<String> = sqlx::query_scalar(
                "select source_id from calendar_events
                  where event_key = $1 and status in ('expected', 'late')",
            )
            .bind(key)
            .fetch_optional(&ing.db)
            .await
            .map_err(transient)?;
            let Some(source) = source else {
                return Ok(
                    json!({"event_key": key, "captured": false, "reason": "no open calendar event"}),
                );
            };
            let s = ing.source(&source).await?;
            input.lease.ensure_lease().await?;
            let pass = ing.pass(&s).await?;
            let seen = pass.event_keys().contains(key);
            if seen {
                news_diary::store::mark_captured(&ing.db, key, ing.runner.now())
                    .await
                    .map_err(transient)?;
            }
            Ok(json!({
                "event_key": key, "source_id": source, "captured": seen,
                "pass": pass.summary(&source),
            }))
        }
        "story.detect" => {
            let t = news_taster::config::v1().map_err(|e| JobFailure::new(ErrorClass::Bug, e))?;
            input.lease.ensure_lease().await?;
            let report = news_taster::detect::detect(&ing.db, &news_taster::detect::NoEmbedder, &t)
                .await
                .map_err(news_failure)?;
            serde_json::to_value(report)
                .map_err(|e| JobFailure::new(ErrorClass::Bug, e.to_string()))
        }
        other => Err(JobFailure::new(
            ErrorClass::Bug,
            format!("{other} is not an ingest job"),
        )),
    }
}

/// The handler for an ingest kind: the installed context runs it, and without one it fails as `bug`.
fn handle(input: JobInput) -> JobFuture {
    Box::pin(async move {
        let ing = INGEST.get().ok_or_else(|| {
            JobFailure::new(
                ErrorClass::Bug,
                format!(
                    "handler for {} needs the Bureau service's ingest context: run it under \
                     `news service run` (Wire and Clerk are built in #9)",
                    input.kind
                ),
            )
        })?;
        run_kind(ing, input).await
    })
}

/// Wire's kinds. `work.prune` stays the stub until Morgue retention has a service schedule.
pub fn wire(input: JobInput) -> JobFuture {
    match input.kind.as_str() {
        "source.discover" | "doc.fetch" => handle(input),
        _ => news_wire::handle(input),
    }
}

/// Clerk's kinds. `doc.parse_docling` is #12's.
pub fn clerk(input: JobInput) -> JobFuture {
    match input.kind.as_str() {
        "doc.parse" => handle(input),
        _ => news_clerk::handle(input),
    }
}

/// Diary's kinds. `calendar.sync` stays the stub: the calendar feeds are #12's.
pub fn diary(input: JobInput) -> JobFuture {
    match input.kind.as_str() {
        "calendar.capture" => handle(input),
        _ => news_diary::handle(input),
    }
}

/// Taster's one kind, `story.detect`.
pub fn taster(input: JobInput) -> JobFuture {
    handle(input)
}
