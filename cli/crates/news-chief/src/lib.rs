//! Chief, the run engine inside the Bureau service (ADR 0008 §3–§6, Bureau §5.5, §8).
//!
//! Chief owns every change to `runs`: starts (from Board over the Hotline, and from Rota's schedules),
//! the Lineup (one heavy run at a time, the rest queued), pauses, resumes, cancels, gates and the Wrap.
//! It has no Windows code: stopping a run's processes and worker tasks goes through [`RunStopper`],
//! which the service implements. Only the service calls the control functions (`bureau-single-entry`).

pub mod board;
pub mod driver;
pub mod gates;
pub mod presets;
pub mod row;
pub mod state;
pub mod wrap;

use chrono::{DateTime, Duration, Utc};
use news_core::NewsError;
use news_core::clock::Clock;
use news_core::job::{Handler, JobFailure, JobFuture, JobInput};
use news_store::{PgPool, map_err};
use presets::PresetDecl;
use row::RunRow;
use serde_json::{Map, Value, json};
use sqlx::PgConnection;
use state::{Change, PausedReason, Status, StopMode};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// The issue that builds this subsystem.
pub const ISSUE: &str = "#10";

/// Job kinds this subsystem handles (see `news_dispatch::KINDS`): none; Chief runs steps, not jobs.
pub const KINDS: &[&str] = &[];

/// Run one job (none are registered to Chief).
pub fn handle(input: JobInput) -> JobFuture {
    Box::pin(async move { Err(JobFailure::not_built(&input.kind, ISSUE)) })
}

pub const HANDLER: Handler = handle;

pub type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Transaction-scoped advisory lock `(45, schema)` that serialises every path into the heavy slot:
/// starts, promotions, resumes and approvals (#10 review note 1). Taken before any row lock.
const LINEUP_LOCK: &str = "select pg_advisory_xact_lock(45, hashtext(current_schema()))";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Board,
    Schedule,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::Board => "board",
            Origin::Schedule => "schedule",
        }
    }
}

/// What [`StartPolicy::check`] says about a start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Policy {
    Allow,
    /// Not now (quiet hours): a scheduled heavy run queues; a Board start gets `precondition`.
    Hold {
        reason: String,
        text: String,
    },
    /// Not at all (quota cap or reserve): a scheduled run is skipped; a Board start gets `precondition`.
    Refuse {
        reason: String,
        text: String,
    },
}

/// The quota guard's word on `ai` claims (ADR 0008 §7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiClaim {
    Allow,
    Reserve,
    Cap,
}

/// Rota implements it: quiet hours and the quota guard.
pub trait StartPolicy: Send + Sync {
    fn check(&self, preset: &PresetDecl, origin: Origin, now: DateTime<Utc>) -> Policy;
    fn ai_claim(&self, _now: DateTime<Utc>) -> AiClaim {
        AiClaim::Allow
    }
    /// The `usage` object of the contracts (tokens against the cap and reserve).
    fn usage(&self) -> Value {
        json!({"known": false, "bureau_today": null, "bureau_week": null, "total_today": null,
               "total_week": null, "cap_today": null, "cap_week": null, "reserve_today": null,
               "reserve_week": null})
    }
}

/// Allows everything.
pub struct AllowAll;
impl StartPolicy for AllowAll {
    fn check(&self, _: &PresetDecl, _: Origin, _: DateTime<Utc>) -> Policy {
        Policy::Allow
    }
}

/// Stops a run's work for a pause `now` or a cancel (ADR 0008 §4.1 step 2): terminate its Job Object
/// and wait until it is empty, cancel its worker tasks in every pool and await each, then wait for an
/// in-flight publish. Returns only when all of it has stopped; no timeout.
pub trait RunStopper: Send + Sync {
    fn stop<'a>(&'a self, run: Uuid) -> BoxFut<'a, ()>;
    /// Jobs of `run` whose tasks have not stopped yet, and since when (Board's `stopping`).
    fn stopping(&self, _run: Uuid) -> (Vec<i64>, Option<DateTime<Utc>>) {
        (Vec::new(), None)
    }
}

/// For tests and the CLI: nothing to stop.
pub struct NoStopper;
impl RunStopper for NoStopper {
    fn stop<'a>(&'a self, _run: Uuid) -> BoxFut<'a, ()> {
        Box::pin(async {})
    }
}

/// Where Chief sends Hotline events (`topic`, `data`).
pub type Emit = Arc<dyn Fn(&str, Value) + Send + Sync>;

/// What `run.start` does when the heavy slot is busy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IfBusy {
    Offer,
    Queue,
}

/// A start request.
#[derive(Debug, Clone)]
pub struct StartReq<'a> {
    pub preset: &'a str,
    pub args: Map<String, Value>,
    pub origin: Origin,
    /// Scheduled runs: the schedule key and its slot.
    pub schedule: Option<(&'a str, DateTime<Utc>)>,
    pub if_busy: IfBusy,
    /// Scheduled runs: older due slots dropped in favour of this one.
    pub missed_before: i32,
}

impl<'a> StartReq<'a> {
    /// A start from Board.
    pub fn board(preset: &'a str, args: Map<String, Value>, if_busy: IfBusy) -> Self {
        Self {
            preset,
            args,
            origin: Origin::Board,
            schedule: None,
            if_busy,
            missed_before: 0,
        }
    }
}

/// The outcome of a start.
#[derive(Debug, Clone, PartialEq)]
pub enum Started {
    Running(RunRow),
    Queued { run: RunRow, position: i64 },
    Skipped { run: RunRow, reason: String },
}

impl Started {
    pub fn run(&self) -> &RunRow {
        match self {
            Started::Running(r)
            | Started::Queued { run: r, .. }
            | Started::Skipped { run: r, .. } => r,
        }
    }
}

#[derive(Clone)]
pub struct Chief {
    inner: Arc<Inner>,
}

struct Inner {
    db: PgPool,
    clock: Arc<dyn Clock>,
    policy: Arc<dyn StartPolicy>,
    stopper: Arc<dyn RunStopper>,
    handlers: driver::Handlers,
    emit: Emit,
    /// Runs whose driver is inside an inline code step: a `drain` pause waits for them.
    inline: Mutex<HashSet<Uuid>>,
    /// Runs being stopped (pause `now`, cancel).
    stopping: Mutex<HashSet<Uuid>>,
    drivers: Mutex<HashMap<Uuid, tokio::task::JoinHandle<()>>>,
    /// The step task each driver is awaiting: its cancel switch and abort handle. A pause `now`, a
    /// cancel and a lost lock stop it and wait for it to end before anything else happens.
    steps: Mutex<
        HashMap<
            Uuid,
            (
                tokio_util::sync::CancellationToken,
                tokio::task::AbortHandle,
            ),
        >,
    >,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Chief {
    pub fn new(db: PgPool, clock: Arc<dyn Clock>) -> Self {
        Self::builder(db, clock).build()
    }

    pub fn builder(db: PgPool, clock: Arc<dyn Clock>) -> ChiefBuilder {
        ChiefBuilder {
            db,
            clock,
            policy: Arc::new(AllowAll),
            stopper: Arc::new(NoStopper),
            handlers: driver::Handlers::default(),
            emit: Arc::new(|_, _| {}),
        }
    }

    pub fn db(&self) -> &PgPool {
        &self.inner.db
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.inner.clock.now()
    }

    pub fn policy(&self) -> &Arc<dyn StartPolicy> {
        &self.inner.policy
    }

    pub fn stopper(&self) -> &Arc<dyn RunStopper> {
        &self.inner.stopper
    }

    fn emit(&self, topic: &str, data: Value) {
        (self.inner.emit)(topic, data);
    }

    async fn emit_run(&self, run_id: Uuid, previous: Option<Status>, wrap: Option<Value>) {
        if let Ok(ev) = board::run_event(&self.inner.db, run_id, previous, wrap, self.now()).await {
            self.emit("run", ev);
        }
    }

    // ---- Starts and the Lineup ---------------------------------------------------------------

    /// Start a preset (ADR 0008 §5): `running`, `queued` in the Lineup, or `skipped` (scheduled runs
    /// only). A Board start with the slot busy and `IfBusy::Offer` is `conflict` with a `queue_offer`.
    pub async fn start(&self, req: StartReq<'_>) -> Result<Started, NewsError> {
        let now = self.now();
        let p = presets::preset(req.preset)
            .ok_or_else(|| NewsError::validation(format!("unknown preset {:?}", req.preset)))?;
        let args = p.resolve_args(&req.args)?;
        let mut tx = self.inner.db.begin().await.map_err(map_err)?;
        sqlx::query(LINEUP_LOCK)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        let standing = news_standing::current_on(&mut tx).await?;
        let late_limit = Duration::minutes(standing.int("lineup.queue_late_limit_minutes"));
        let new = NewRun {
            preset: p,
            args: &args,
            origin: req.origin,
            schedule: req.schedule,
            standing: &standing,
            start_by: req
                .schedule
                .filter(|_| p.heavy)
                .map(|(_, slot)| slot + late_limit),
            rank: 1,
        };

        let out = self
            .start_in(&mut tx, p, &req, &new, late_limit, now)
            .await?;
        // A scheduled fire is recorded in the same transaction, once per slot (ADR 0008 §7.1).
        if let Some((key, slot)) = req.schedule {
            let (outcome, reason) = match &out {
                Started::Running(_) => ("started", None),
                Started::Queued { .. } => ("queued", None),
                Started::Skipped { reason, .. } => ("skipped", Some(reason.as_str())),
            };
            let fired: Option<String> = sqlx::query_scalar(
                "insert into rota_fires (schedule_key, slot_at, outcome, reason, missed_before, run_id, settings_version, fired_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8) on conflict do nothing returning schedule_key",
            )
            .bind(key)
            .bind(slot)
            .bind(outcome)
            .bind(reason)
            .bind(req.missed_before)
            .bind(out.run().run_id)
            .bind(standing.version)
            .bind(now)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map_err)?;
            if fired.is_none() {
                return Err(NewsError::conflict(format!(
                    "{key} already fired for {slot}"
                )));
            }
        }
        tx.commit().await.map_err(map_err)?;
        self.emit_run(out.run().run_id, None, None).await;
        Ok(out)
    }

    async fn start_in(
        &self,
        tx: &mut PgConnection,
        p: &'static PresetDecl,
        req: &StartReq<'_>,
        new: &NewRun<'_>,
        late_limit: Duration,
        now: DateTime<Utc>,
    ) -> Result<Started, NewsError> {
        let live: Option<(Uuid, String, String)> = sqlx::query_as(
            "select run_id, short_id, status from runs where preset = $1 and status = any($2)",
        )
        .bind(p.id)
        .bind(Status::LIVE.map(Status::as_str).to_vec())
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_err)?;
        if let Some((id, short, status)) = live {
            if req.origin == Origin::Schedule {
                return self.skip_new(tx, new, "preset_live", now).await;
            }
            return Err(NewsError::conflict(format!("{} is already {status} ({short})", p.name))
                .with_details(json!({"reason": "preset_live", "run_id": id, "short_id": short, "status": status}))
                .with_text(format!("{} is already {status} as {short}.", p.name)));
        }
        // Every start checks the late limit first (ADR 0008 §7.1).
        if let Some((_, slot)) = req.schedule
            && now > slot + late_limit
        {
            return self.skip_new(tx, new, "late", now).await;
        }
        let mut hold = false;
        match self.inner.policy.check(p, req.origin, now) {
            Policy::Allow => {}
            Policy::Refuse { reason, text } | Policy::Hold { reason, text }
                if req.origin == Origin::Board =>
            {
                return Err(NewsError::precondition(format!(
                    "{} cannot start now: {reason}",
                    p.name
                ))
                .with_details(json!({"reason": reason}))
                .with_text(text));
            }
            Policy::Refuse { reason, .. } => return self.skip_new(tx, new, &reason, now).await,
            // Light runs are never held (§7.2).
            Policy::Hold { .. } => hold = p.heavy,
        }
        let busy = if p.heavy {
            slot_busy(tx, None).await?
        } else {
            None
        };
        if busy.is_some() || hold {
            if let (Some(b), Origin::Board, IfBusy::Offer) = (&busy, req.origin, req.if_busy) {
                return Err(queue_offer(p, b));
            }
            let run = insert_run(tx, new, Status::Queued, now).await?;
            let position = lineup_position(tx, run.run_id).await?;
            Ok(Started::Queued { run, position })
        } else {
            Ok(Started::Running(
                insert_run(tx, new, Status::Running, now).await?,
            ))
        }
    }

    /// Insert a run that ends `skipped` at once, with its Wrap.
    async fn skip_new(
        &self,
        tx: &mut PgConnection,
        new: &NewRun<'_>,
        reason: &str,
        now: DateTime<Utc>,
    ) -> Result<Started, NewsError> {
        let run = insert_run(tx, new, Status::Skipped, now).await?;
        wrap::finish_new_skip(tx, &run, reason, now).await?;
        Ok(Started::Skipped {
            run,
            reason: reason.to_owned(),
        })
    }

    /// `run.start_after_pause`: pause the named holder (`drain`, `karthik`) and queue the new run at
    /// rank 0, in one transaction. `conflict` with a fresh offer when the holder has changed.
    pub async fn start_after_pause(
        &self,
        preset: &str,
        args: Map<String, Value>,
        pause_run: &str,
    ) -> Result<(RunRow, RunRow, i64), NewsError> {
        let now = self.now();
        let p = presets::preset(preset)
            .ok_or_else(|| NewsError::validation(format!("unknown preset {preset:?}")))?;
        if !p.heavy {
            return Err(NewsError::precondition(format!(
                "{} is light: it starts beside a heavy run, with run.start",
                p.name
            )));
        }
        let args = p.resolve_args(&args)?;
        let mut tx = self.inner.db.begin().await.map_err(map_err)?;
        sqlx::query(LINEUP_LOCK)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        let holder_id = row::resolve(&mut tx, pause_run).await?;
        let holder = row::load(&mut tx, holder_id, true).await?;
        if !(holder.heavy && matches!(holder.status(), Status::Running | Status::WaitingAi)) {
            return Err(match slot_busy(&mut tx, None).await? {
                Some(b) => queue_offer(p, &b).with_hint("the run holding the slot has changed"),
                None => {
                    NewsError::conflict("the heavy slot is free now; start the run with run.start")
                }
            });
        }
        let live: Option<String> =
            sqlx::query_scalar("select short_id from runs where preset = $1 and status = any($2)")
                .bind(p.id)
                .bind(Status::LIVE.map(Status::as_str).to_vec())
                .fetch_optional(&mut *tx)
                .await
                .map_err(map_err)?;
        if let Some(short) = live {
            return Err(NewsError::conflict(format!(
                "{} is already live ({short})",
                p.name
            )));
        }
        state::transition(
            &mut tx,
            holder.run_id,
            &[Status::Running, Status::WaitingAi],
            Change {
                stop_mode: Some(StopMode::Drain),
                paused_reason: Some(PausedReason::Karthik),
                ..Change::to(Status::Pausing)
            },
            now,
        )
        .await?;
        let standing = news_standing::current_on(&mut tx).await?;
        let new = NewRun {
            preset: p,
            args: &args,
            origin: Origin::Board,
            schedule: None,
            standing: &standing,
            start_by: None,
            rank: 0,
        };
        let run = insert_run(&mut tx, &new, Status::Queued, now).await?;
        let position = lineup_position(&mut tx, run.run_id).await?;
        tx.commit().await.map_err(map_err)?;
        let holder = row::load(&mut *self.conn().await?, holder.run_id, false).await?;
        self.emit_run(holder.run_id, Some(Status::Running), None)
            .await;
        self.emit_run(run.run_id, None, None).await;
        Ok((holder, run, position))
    }

    async fn conn(&self) -> Result<sqlx::pool::PoolConnection<sqlx::Postgres>, NewsError> {
        self.inner.db.acquire().await.map_err(map_err)
    }

    /// Continue a run that left a slot-free status (resume, last gate answered): `running` when light
    /// or the slot is free, else `queued` at rank 1. Caller holds the Lineup lock and the run's row.
    async fn continue_run(
        &self,
        tx: &mut PgConnection,
        run: &RunRow,
        from: Status,
        now: DateTime<Utc>,
    ) -> Result<(Status, Option<i64>), NewsError> {
        let busy = if run.heavy {
            slot_busy(tx, Some(run.run_id)).await?.is_some()
        } else {
            false
        };
        if busy {
            state::transition(
                tx,
                run.run_id,
                &[from],
                Change {
                    rejoin: true,
                    ..Change::to(Status::Queued)
                },
                now,
            )
            .await?;
            let pos = lineup_position(tx, run.run_id).await?;
            Ok((Status::Queued, Some(pos)))
        } else {
            state::transition(tx, run.run_id, &[from], Change::to(Status::Running), now).await?;
            ensure_steps(tx, run).await?;
            Ok((Status::Running, None))
        }
    }

    /// `lineup.cancel` and `run.cancel` of a queued run: `cancelled` with a Wrap that says so.
    pub async fn lineup_cancel(&self, run_ref: &str) -> Result<(RunRow, Value), NewsError> {
        let now = self.now();
        let mut tx = self.inner.db.begin().await.map_err(map_err)?;
        let id = row::resolve(&mut tx, run_ref).await?;
        let run = row::load(&mut tx, id, true).await?;
        if run.status() != Status::Queued {
            return Err(NewsError::precondition(format!(
                "{} is {}, not queued",
                run.short_id, run.status
            )));
        }
        let wrap = wrap::finish(
            &mut tx,
            &run,
            Status::Cancelled,
            Some("cancelled in the Lineup"),
            now,
        )
        .await?;
        tx.commit().await.map_err(map_err)?;
        self.emit_run(id, Some(Status::Queued), Some(wrap.clone()))
            .await;
        Ok((run, wrap))
    }

    // ---- Pause, resume, cancel ---------------------------------------------------------------

    /// `run.pause`: `pausing` (`drain`), or `now` when the run is stalled (also for a run already
    /// draining). The tick completes it.
    pub async fn pause(&self, run_ref: &str) -> Result<(RunRow, StopMode, bool), NewsError> {
        let now = self.now();
        let mut tx = self.inner.db.begin().await.map_err(map_err)?;
        let id = row::resolve(&mut tx, run_ref).await?;
        let run = row::load(&mut tx, id, true).await?;
        let standing = news_standing::current_on(&mut tx).await?;
        let stalled = is_stalled(&run, standing.int("chief.stall_minutes"), now);
        let mode = if stalled {
            StopMode::Now
        } else {
            StopMode::Drain
        };
        let status = run.status();
        let draining_and_stalled =
            status == Status::Pausing && run.stop_mode.as_deref() == Some("drain") && stalled;
        match (status, run.stop_mode.as_deref()) {
            (Status::Running | Status::WaitingAi, _) | (Status::Pausing, _)
                if status != Status::Pausing || draining_and_stalled =>
            {
                state::transition(
                    &mut tx,
                    id,
                    &[run.status()],
                    Change {
                        stop_mode: Some(mode),
                        paused_reason: Some(PausedReason::Karthik),
                        ..Change::to(Status::Pausing)
                    },
                    now,
                )
                .await?;
            }
            (Status::Pausing, m) => {
                tx.commit().await.map_err(map_err)?;
                let m = m.and_then(StopMode::parse).unwrap_or(StopMode::Drain);
                return Ok((run, m, stalled));
            }
            (s, _) => {
                return Err(NewsError::precondition(format!(
                    "{} is {}; only a running run can be paused",
                    run.short_id,
                    s.as_str()
                )));
            }
        }
        tx.commit().await.map_err(map_err)?;
        self.emit_run(id, Some(run.status()), None).await;
        Ok((run, mode, stalled))
    }

    /// Set runs `pausing` (`drain`) for a reason of Chief's own (usage limit, quota reserve).
    async fn pause_for(
        &self,
        runs: &[Uuid],
        reason: PausedReason,
        resume_at: Option<DateTime<Utc>>,
    ) -> Result<(), NewsError> {
        let now = self.now();
        for id in runs {
            let mut tx = self.inner.db.begin().await.map_err(map_err)?;
            let r = state::transition(
                &mut tx,
                *id,
                &[Status::Running, Status::WaitingAi],
                Change {
                    stop_mode: Some(StopMode::Drain),
                    paused_reason: Some(reason),
                    resume_at,
                    ..Change::to(Status::Pausing)
                },
                now,
            )
            .await;
            if r.is_ok() {
                tx.commit().await.map_err(map_err)?;
                self.emit_run(*id, Some(Status::Running), None).await;
            }
        }
        Ok(())
    }

    /// Step 3 of the pause procedure (ADR 0008 §4.1), once the run's work has stopped: revoke its
    /// running jobs' leases, mark sent-but-unanswered publishes, then `paused`, or for a cancel cancel
    /// its jobs and write the Wrap. The slot is free at this commit.
    pub async fn finish_pause(&self, run_id: Uuid) -> Result<Option<Status>, NewsError> {
        let now = self.now();
        let mut tx = self.inner.db.begin().await.map_err(map_err)?;
        let run = row::load(&mut tx, run_id, true).await?;
        if run.status() != Status::Pausing {
            return Ok(None);
        }
        news_dispatch::queue::revoke_run_leases(&mut tx, run_id).await?;
        mark_publishes_unknown(&mut tx, run_id, now).await?;
        let (to, wrap) = if run.stop_mode.as_deref() == Some("cancel") {
            news_dispatch::queue::cancel_run_jobs(&mut tx, run_id, None, None).await?;
            let w = wrap::finish(
                &mut tx,
                &run,
                Status::Cancelled,
                Some("cancelled by Karthik"),
                now,
            )
            .await?;
            (Status::Cancelled, Some(w))
        } else {
            state::transition(
                &mut tx,
                run_id,
                &[Status::Pausing],
                Change::to(Status::Paused),
                now,
            )
            .await?;
            (Status::Paused, None)
        };
        tx.commit().await.map_err(map_err)?;
        self.emit_run(run_id, Some(Status::Pausing), wrap).await;
        Ok(Some(to))
    }

    /// `run.resume`: a paused run continues (`running`, or `queued` when heavy and the slot is busy).
    /// Publishes left without an answer are handed back for reconciliation.
    pub async fn resume(&self, run_ref: &str) -> Result<(RunRow, Status, Option<i64>), NewsError> {
        let now = self.now();
        let mut tx = self.inner.db.begin().await.map_err(map_err)?;
        sqlx::query(LINEUP_LOCK)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        let id = row::resolve(&mut tx, run_ref).await?;
        let run = row::load(&mut tx, id, true).await?;
        if run.status() != Status::Paused {
            return Err(NewsError::precondition(format!(
                "{} is {}; only a paused run can be resumed",
                run.short_id, run.status
            )));
        }
        let (to, pos) = self
            .continue_run(&mut tx, &run, Status::Paused, now)
            .await?;
        reconcile_publishes(&mut tx, id).await?;
        tx.commit().await.map_err(map_err)?;
        self.emit_run(id, Some(Status::Paused), None).await;
        Ok((run, to, pos))
    }

    /// `run.cancel`: a queued, waiting or paused run ends `cancelled` at once with its Wrap; a running
    /// one goes `pausing` (`cancel`) and ends when its work has stopped.
    pub async fn cancel(
        &self,
        run_ref: &str,
    ) -> Result<(RunRow, Status, Option<Value>), NewsError> {
        let now = self.now();
        let mut tx = self.inner.db.begin().await.map_err(map_err)?;
        let id = row::resolve(&mut tx, run_ref).await?;
        let run = row::load(&mut tx, id, true).await?;
        let out = match run.status() {
            Status::Queued => {
                drop(tx);
                let (run, wrap) = self.lineup_cancel(&id.to_string()).await?;
                return Ok((run, Status::Cancelled, Some(wrap)));
            }
            Status::WaitingApproval | Status::Paused => {
                news_dispatch::queue::cancel_run_jobs(&mut tx, id, None, None).await?;
                let w = wrap::finish(
                    &mut tx,
                    &run,
                    Status::Cancelled,
                    Some("cancelled by Karthik"),
                    now,
                )
                .await?;
                (Status::Cancelled, Some(w))
            }
            s @ (Status::Running | Status::WaitingAi | Status::Pausing) => {
                state::transition(
                    &mut tx,
                    id,
                    &[s],
                    Change {
                        stop_mode: Some(StopMode::Cancel),
                        paused_reason: Some(PausedReason::Karthik),
                        ..Change::to(Status::Pausing)
                    },
                    now,
                )
                .await?;
                (Status::Pausing, None)
            }
            s => {
                return Err(NewsError::precondition(format!(
                    "{} has already finished ({})",
                    run.short_id,
                    s.as_str()
                )));
            }
        };
        tx.commit().await.map_err(map_err)?;
        self.emit_run(id, Some(run.status()), out.1.clone()).await;
        Ok((run, out.0, out.1))
    }

    // ---- The tick and recovery ---------------------------------------------------------------

    /// One Chief tick (every second): skip late queued runs, promote the Lineup, pause runs for usage
    /// limits and the quota reserve, complete pauses whose work has stopped, resume runs whose reason
    /// has cleared.
    pub async fn tick(&self) -> Result<(), NewsError> {
        self.promote().await?;
        self.usage_and_quota().await?;
        self.complete_pauses().await?;
        self.auto_resume().await?;
        Ok(())
    }

    /// End queued runs past `start_by` as `skipped`, then start the first queued heavy run if the slot
    /// is free and the policy allows it.
    pub async fn promote(&self) -> Result<(), NewsError> {
        let now = self.now();
        let mut tx = self.inner.db.begin().await.map_err(map_err)?;
        sqlx::query(LINEUP_LOCK)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        let late: Vec<Uuid> = sqlx::query_scalar(
            "select run_id from runs where status = 'queued' and start_by < $1 order by queued_at for update",
        )
        .bind(now)
        .fetch_all(&mut *tx)
        .await
        .map_err(map_err)?;
        let mut events = Vec::new();
        for id in &late {
            let run = row::load(&mut tx, *id, true).await?;
            let w = wrap::finish(&mut tx, &run, Status::Skipped, Some("late"), now).await?;
            events.push((*id, Status::Queued, Some(w)));
        }
        let holder: Option<Uuid> = sqlx::query_scalar(
            "select run_id from runs where heavy and status in ('running', 'waiting_ai', 'pausing')",
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_err)?;
        if holder.is_none() {
            let next: Option<Uuid> = sqlx::query_scalar(
                "select run_id from runs where status = 'queued' and (start_by is null or start_by >= $1)
                  order by lineup_rank, queued_at, run_id limit 1 for update",
            )
            .bind(now)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map_err)?;
            if let Some(id) = next {
                let run = row::load(&mut tx, id, true).await?;
                let p = presets::preset(&run.preset);
                let origin = if run.origin == "schedule" {
                    Origin::Schedule
                } else {
                    Origin::Board
                };
                let allow =
                    p.is_none_or(|p| self.inner.policy.check(p, origin, now) == Policy::Allow);
                if allow {
                    state::transition(
                        &mut tx,
                        id,
                        &[Status::Queued],
                        Change::to(Status::Running),
                        now,
                    )
                    .await?;
                    ensure_steps(&mut tx, &run).await?;
                    events.push((id, Status::Queued, None));
                }
            }
        }
        tx.commit().await.map_err(map_err)?;
        for (id, prev, wrap) in events {
            self.emit_run(id, Some(prev), wrap).await;
        }
        Ok(())
    }

    /// Runs with live `ai` jobs while the `ai` pool is paused for a usage limit pause until it resets;
    /// at the quota reserve they pause; at the cap their live `ai` jobs are cancelled.
    async fn usage_and_quota(&self) -> Result<(), NewsError> {
        let now = self.now();
        let db = &self.inner.db;
        let paused_until: Option<DateTime<Utc>> = sqlx::query_scalar(
            "select paused_until from pools where pool = 'ai' and paused_until > $1",
        )
        .bind(now)
        .fetch_optional(db)
        .await
        .map_err(map_err)?;
        let with_ai = || async {
            sqlx::query_as::<_, (Uuid, String)>(
                "select r.run_id, r.short_id from runs r
                  where r.status in ('running', 'waiting_ai')
                    and exists (select 1 from jobs j where j.run_id = r.run_id and j.pool = 'ai'
                                  and j.state in ('available', 'scheduled', 'running', 'retryable', 'snoozed'))
                  order by r.run_id",
            )
            .fetch_all(db)
            .await
            .map_err(map_err)
        };
        if let Some(until) = paused_until {
            let runs = with_ai().await?;
            if !runs.is_empty() {
                let ids: Vec<Uuid> = runs.iter().map(|r| r.0).collect();
                self.pause_for(&ids, PausedReason::UsageLimit, Some(until))
                    .await?;
                self.emit_usage(
                    "limit_reached",
                    Some(until),
                    &runs,
                    "Claude usage limit reached; runs pause until it resets.",
                );
            }
        }
        match self.inner.policy.ai_claim(now) {
            AiClaim::Allow => {}
            AiClaim::Reserve => {
                let runs = with_ai().await?;
                if !runs.is_empty() {
                    let ids: Vec<Uuid> = runs.iter().map(|r| r.0).collect();
                    self.pause_for(&ids, PausedReason::Quota, None).await?;
                    self.emit_usage(
                        "reserve_reached",
                        None,
                        &runs,
                        "Usage reached the reserve; AI work pauses until it falls back.",
                    );
                }
            }
            AiClaim::Cap => {
                let runs = with_ai().await?;
                for (id, _) in &runs {
                    let mut tx = db.begin().await.map_err(map_err)?;
                    row::load(&mut tx, *id, true).await?;
                    news_dispatch::queue::cancel_run_jobs(&mut tx, *id, None, Some("ai")).await?;
                    tx.commit().await.map_err(map_err)?;
                }
                if !runs.is_empty() {
                    self.emit_usage(
                        "cap_reached",
                        None,
                        &runs,
                        "Bureau reached its token cap; AI work of running runs was cancelled.",
                    );
                }
            }
        }
        Ok(())
    }

    fn emit_usage(
        &self,
        kind: &str,
        resets_at: Option<DateTime<Utc>>,
        runs: &[(Uuid, String)],
        text: &str,
    ) {
        self.emit(
            "usage",
            json!({"kind": kind, "resets_at": resets_at, "runs": runs.iter().map(|r| &r.1).collect::<Vec<_>>(),
                   "usage": self.inner.policy.usage(), "text": text}),
        );
    }

    /// Complete every `pausing` run whose work has stopped: `drain` when it has no running job and its
    /// driver is not inside an inline step; `now` and `cancel` after the [`RunStopper`] returns.
    async fn complete_pauses(&self) -> Result<(), NewsError> {
        let pausing: Vec<(Uuid, String, i64)> = sqlx::query_as(
            "select r.run_id, r.stop_mode,
                    (select count(*) from jobs j where j.run_id = r.run_id and j.state = 'running')
               from runs r where r.status = 'pausing' order by r.run_id",
        )
        .fetch_all(&self.inner.db)
        .await
        .map_err(map_err)?;
        for (id, mode, running) in pausing {
            if mode == "drain" {
                if running == 0 && !lock(&self.inner.inline).contains(&id) {
                    self.finish_pause(id).await?;
                }
                continue;
            }
            if !lock(&self.inner.stopping).insert(id) {
                continue; // already being stopped
            }
            let me = self.clone();
            tokio::spawn(async move {
                me.inner.stopper.stop(id).await;
                // The driver's step task too (an inline code step), awaited: the slot is freed only
                // once nothing of the run is still running.
                me.stop_step(id).await;
                if let Err(e) = me.finish_pause(id).await {
                    tracing::warn!(run_id = %id, error = %e, "finishing a pause failed; retried next tick");
                }
                lock(&me.inner.stopping).remove(&id);
            });
        }
        Ok(())
    }

    /// Resume runs paused for a usage limit whose `resume_at` has passed (with the pool no longer
    /// paused), and runs paused for the quota once the guard allows `ai` claims again.
    async fn auto_resume(&self) -> Result<(), NewsError> {
        let now = self.now();
        let quota_ok = self.inner.policy.ai_claim(now) == AiClaim::Allow;
        let due: Vec<(Uuid, String, String)> = sqlx::query_as(
            "select r.run_id, r.short_id, r.paused_reason from runs r
              where r.status = 'paused'
                and ((r.paused_reason = 'usage_limit' and r.resume_at <= $1
                      and not exists (select 1 from pools where pool = 'ai' and paused_until > $1))
                     or (r.paused_reason = 'quota' and $2))
              order by r.run_id",
        )
        .bind(now)
        .bind(quota_ok)
        .fetch_all(&self.inner.db)
        .await
        .map_err(map_err)?;
        let mut reset = Vec::new();
        for (id, short, reason) in due {
            if self.resume(&id.to_string()).await.is_ok() && reason == "usage_limit" {
                reset.push((id, short));
            }
        }
        if !reset.is_empty() {
            self.emit_usage(
                "limit_reset",
                None,
                &reset,
                "The usage limit has reset; paused runs continue.",
            );
        }
        Ok(())
    }

    /// Recovery at service start (ADR 0008 §9 step 6), before anything claims: the takeover fence
    /// (every running job's lease is revoked, so a late report from an instance that lost the singleton
    /// lock is refused), then every `pausing` run is completed (its processes died with the old
    /// service), then the rescuer.
    pub async fn recover(&self) -> Result<Value, NewsError> {
        let revoked = news_dispatch::queue::revoke_all_leases(&self.inner.db).await?;
        let pausing: Vec<Uuid> =
            sqlx::query_scalar("select run_id from runs where status = 'pausing' order by run_id")
                .fetch_all(&self.inner.db)
                .await
                .map_err(map_err)?;
        for id in &pausing {
            self.finish_pause(*id).await?;
        }
        let rescued = news_dispatch::queue::rescue(&self.inner.db).await?;
        Ok(
            json!({"revoked": revoked, "pauses_completed": pausing.len(),
                  "rescued": rescued.retryable.len() + rescued.discarded.len()}),
        )
    }

    // ---- Drivers ------------------------------------------------------------------------------

    /// Start a driver for every `running`/`waiting_ai` run that has none (each re-enters its current
    /// step), and forget finished drivers.
    pub async fn ensure_drivers(&self) -> Result<(), NewsError> {
        let live: Vec<Uuid> = sqlx::query_scalar(
            "select run_id from runs where status in ('running', 'waiting_ai') order by run_id",
        )
        .fetch_all(&self.inner.db)
        .await
        .map_err(map_err)?;
        let mut drivers = lock(&self.inner.drivers);
        drivers.retain(|_, h| !h.is_finished());
        for id in live {
            if !drivers.contains_key(&id) {
                let me = self.clone();
                drivers.insert(id, tokio::spawn(async move { me.drive(id).await }));
            }
        }
        Ok(())
    }

    /// Stop every driver (the service is stopping or lost its lock) and wait for them.
    pub async fn stop_drivers(&self) {
        let handles: Vec<_> = lock(&self.inner.drivers).drain().map(|(_, h)| h).collect();
        for h in handles {
            h.abort();
            let _ = h.await;
        }
        // Aborting a driver detaches its step task; stop those too, or a new driver would run the
        // same step again beside it.
        let runs: Vec<Uuid> = lock(&self.inner.steps).keys().copied().collect();
        for run in runs {
            self.stop_step(run).await;
        }
        lock(&self.inner.inline).clear();
    }

    /// Cancel and abort the step task of `run`, and wait until it has ended (no timeout).
    pub async fn stop_step(&self, run: Uuid) {
        let Some((cancel, abort)) = lock(&self.inner.steps).get(&run).cloned() else {
            return;
        };
        cancel.cancel();
        abort.abort();
        while !abort.is_finished() {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        lock(&self.inner.steps).remove(&run);
    }

    pub(crate) fn track_step(
        &self,
        run: Uuid,
        cancel: tokio_util::sync::CancellationToken,
        abort: tokio::task::AbortHandle,
    ) {
        lock(&self.inner.steps).insert(run, (cancel, abort));
    }

    pub(crate) fn untrack_step(&self, run: Uuid) {
        lock(&self.inner.steps).remove(&run);
    }

    pub(crate) fn set_inline(&self, run: Uuid, inside: bool) {
        let mut s = lock(&self.inner.inline);
        if inside {
            s.insert(run);
        } else {
            s.remove(&run);
        }
    }

    pub(crate) fn handlers(&self) -> &driver::Handlers {
        &self.inner.handlers
    }

    /// The emitter, for the gate and driver modules.
    pub(crate) fn emit_raw(&self, topic: &str, data: Value) {
        self.emit(topic, data);
    }

    pub(crate) async fn emit_run_pub(
        &self,
        run_id: Uuid,
        previous: Option<Status>,
        wrap: Option<Value>,
    ) {
        self.emit_run(run_id, previous, wrap).await;
    }
}

pub struct ChiefBuilder {
    db: PgPool,
    clock: Arc<dyn Clock>,
    policy: Arc<dyn StartPolicy>,
    stopper: Arc<dyn RunStopper>,
    handlers: driver::Handlers,
    emit: Emit,
}

impl ChiefBuilder {
    pub fn policy(mut self, p: Arc<dyn StartPolicy>) -> Self {
        self.policy = p;
        self
    }
    pub fn stopper(mut self, s: Arc<dyn RunStopper>) -> Self {
        self.stopper = s;
        self
    }
    pub fn handlers(mut self, h: driver::Handlers) -> Self {
        self.handlers = h;
        self
    }
    pub fn emit(mut self, e: Emit) -> Self {
        self.emit = e;
        self
    }
    pub fn build(self) -> Chief {
        Chief {
            inner: Arc::new(Inner {
                db: self.db,
                clock: self.clock,
                policy: self.policy,
                stopper: self.stopper,
                handlers: self.handlers,
                emit: self.emit,
                inline: Mutex::new(HashSet::new()),
                stopping: Mutex::new(HashSet::new()),
                drivers: Mutex::new(HashMap::new()),
                steps: Mutex::new(HashMap::new()),
            }),
        }
    }
}

// ---- Helpers ------------------------------------------------------------------------------------

struct NewRun<'a> {
    preset: &'static PresetDecl,
    args: &'a Map<String, Value>,
    origin: Origin,
    schedule: Option<(&'a str, DateTime<Utc>)>,
    standing: &'a news_standing::Standing,
    start_by: Option<DateTime<Utc>>,
    rank: i16,
}

/// Insert a run in `status` (`running`, `queued` or `skipped`, the last finished by the caller).
async fn insert_run(
    conn: &mut PgConnection,
    new: &NewRun<'_>,
    status: Status,
    now: DateTime<Utc>,
) -> Result<RunRow, NewsError> {
    let running = status == Status::Running;
    let first_step = new.preset.steps.first().map(|s| s.id);
    for _ in 0..5 {
        let sql = format!(
            "insert into runs (run_id, short_id, preset, args, heavy, origin, schedule_key, slot_at, settings_version,
                               status, lineup_rank, queued_at, start_by, current_step, config, budget, created_at,
                               started_at, last_seen_at, finished_at, summary)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, '{{}}', $16, $17, $17, $18, $19)
             on conflict (short_id) do nothing
             returning {}",
            row::RUN_COLUMNS
        );
        let skipped = status == Status::Skipped;
        let got: Option<RunRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(Uuid::new_v4())
            .bind(row::new_short_id())
            .bind(new.preset.id)
            .bind(Value::Object(new.args.clone()))
            .bind(new.preset.heavy)
            .bind(new.origin.as_str())
            .bind(new.schedule.map(|s| s.0))
            .bind(new.schedule.map(|s| s.1))
            .bind(new.standing.version)
            .bind(status.as_str())
            .bind(new.rank)
            .bind((status == Status::Queued).then_some(now))
            .bind(new.start_by.filter(|_| status == Status::Queued || skipped))
            .bind(first_step)
            .bind(Value::Object(new.standing.effective()))
            .bind(now)
            .bind(running.then_some(now))
            .bind(skipped.then_some(now))
            .bind(skipped.then(|| json!({"text": "skipped"})))
            .fetch_optional(&mut *conn)
            .await
            .map_err(|e| {
                let m = map_err(e);
                if m.message.contains("runs_one_heavy") {
                    NewsError::conflict("the heavy slot is busy")
                } else {
                    m
                }
            })?;
        if let Some(run) = got {
            if running {
                ensure_steps(conn, &run).await?;
            }
            return Ok(run);
        }
    }
    Err(NewsError::internal("could not draw a free short id"))
}

/// The run's steps, `pending`, the first time it starts (idempotent).
async fn ensure_steps(conn: &mut PgConnection, run: &RunRow) -> Result<(), NewsError> {
    let Some(p) = presets::preset(&run.preset) else {
        return Ok(());
    };
    for (i, s) in p.steps.iter().enumerate() {
        sqlx::query(
            "insert into run_steps (run_id, seq, step, kind, status) values ($1, $2, $3, $4, 'pending')
             on conflict (run_id, seq) do nothing",
        )
        .bind(run.run_id)
        .bind(i as i16 + 1)
        .bind(s.id)
        .bind(s.kind.as_str())
        .execute(&mut *conn)
        .await
        .map_err(map_err)?;
    }
    Ok(())
}

/// What blocks a new heavy start: the run holding the slot, or failing that a rank-0 run still queued
/// (the slot counts as busy then, so nothing overtakes it; #10 review notes 1 and 2). `except` leaves
/// out the run asking.
async fn slot_busy(
    conn: &mut PgConnection,
    except: Option<Uuid>,
) -> Result<Option<Value>, NewsError> {
    let holder: Option<Uuid> = sqlx::query_scalar(
        "select run_id from runs where heavy and status in ('running', 'waiting_ai', 'pausing')
           and run_id is distinct from $1",
    )
    .bind(except)
    .fetch_optional(&mut *conn)
    .await
    .map_err(map_err)?;
    let id = match holder {
        Some(h) => Some(h),
        None => sqlx::query_scalar(
            "select run_id from runs where status = 'queued' and lineup_rank = 0 and run_id is distinct from $1
              order by queued_at, run_id limit 1",
        )
        .bind(except)
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_err)?,
    };
    match id {
        None => Ok(None),
        Some(id) => Ok(Some(board::blocking(conn, id).await?)),
    }
}

fn queue_offer(p: &PresetDecl, blocking: &Value) -> NewsError {
    let status = blocking["status"].as_str().unwrap_or("running");
    let offers: Vec<&str> = if status == "running" || status == "waiting_ai" {
        vec!["queue", "pause_running"]
    } else {
        vec!["queue"]
    };
    let who = format!(
        "{} {}",
        blocking["preset"].as_str().unwrap_or("?"),
        blocking["short_id"].as_str().unwrap_or("?")
    );
    let text = match status {
        "queued" => format!(
            "{} cannot start yet: {who} is first in the Lineup for the heavy slot. Queue it behind?",
            p.name
        ),
        "pausing" => format!(
            "{} cannot start yet: {who} is pausing and still holds the heavy slot. Queue it?",
            p.name
        ),
        _ => format!(
            "{} cannot start now: {who} is {status} (step {} of {}). Queue it, or pause {who} and start this?",
            p.name, blocking["step_no"], blocking["steps"]
        ),
    };
    NewsError::conflict(format!("the heavy slot is busy ({who})"))
        .with_details(json!({"reason": "queue_offer", "requested": p.id, "blocking": blocking, "offers": offers}))
        .with_text(text)
}

/// A queued run's place in the Lineup: rank 0 first, then oldest first.
async fn lineup_position(conn: &mut PgConnection, run_id: Uuid) -> Result<i64, NewsError> {
    sqlx::query_scalar(
        "select count(*) + 1 from runs q, runs me
          where me.run_id = $1 and q.status = 'queued' and q.run_id <> me.run_id
            and (q.lineup_rank, q.queued_at, q.run_id) < (me.lineup_rank, me.queued_at, me.run_id)",
    )
    .bind(run_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(map_err)
}

/// Whether a run has gone quiet for longer than `stall_minutes`.
pub fn is_stalled(run: &RunRow, stall_minutes: i64, now: DateTime<Utc>) -> bool {
    matches!(
        run.status(),
        Status::Running | Status::WaitingAi | Status::Pausing
    ) && run
        .last_seen_at
        .is_some_and(|t| now - t > Duration::minutes(stall_minutes))
}

/// Publishes of the run sent without an answer: marked so the resume reconciles them (Press, #20).
async fn mark_publishes_unknown(
    conn: &mut PgConnection,
    run_id: Uuid,
    now: DateTime<Utc>,
) -> Result<u64, NewsError> {
    sqlx::query(
        "update publish_outbox set unknown_since = $2
          where run_id = $1 and sent_at is null and attempts > 0 and unknown_since is null",
    )
    .bind(run_id)
    .bind(now)
    .execute(&mut *conn)
    .await
    .map(|r| r.rows_affected())
    .map_err(map_err)
}

/// On resume, publishes marked `unknown_since` go back to the outbox as unsent: Press sends them again
/// under the same idempotency key, which the write API answers once (Bureau §5.8).
async fn reconcile_publishes(conn: &mut PgConnection, run_id: Uuid) -> Result<u64, NewsError> {
    sqlx::query(
        "update publish_outbox set unknown_since = null where run_id = $1 and unknown_since is not null and sent_at is null",
    )
    .bind(run_id)
    .execute(&mut *conn)
    .await
    .map(|r| r.rows_affected())
    .map_err(map_err)
}
