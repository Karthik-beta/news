//! Step drivers (ADR 0008 §6.2, §6.4): one task per live run enters its current step, records the
//! outcome, and moves on. Steps are idempotent and keyed, so a driver started after a restart re-enters
//! the current step and finished steps never run again.

use crate::gates::{self, GateOpen};
use crate::presets::{PresetDecl, StepDecl, StepKind};
use crate::row::{self, RunRow};
use crate::state::{self, Change, Status};
use crate::{BoxFut, Chief, presets, wrap};
use chrono::{DateTime, Utc};
use news_core::NewsError;
use news_core::job::ErrorClass;
use news_dispatch::queue::{Enqueued, NewJob};
use news_store::{PgPool, map_err};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Row};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// What a step handler returns.
#[derive(Debug, Clone, PartialEq)]
pub enum StepOutcome {
    /// Output recorded; Chief moves to the next step. A `wrap` object in it is summed into the Wrap.
    Done(Value),
    /// Jobs were enqueued through `cx.enqueue`; call again when none of this step's jobs is live.
    Wait,
    /// A checkpoint (a Catchup chunk): output saved, a pause may take effect, then called again.
    Yield(Value),
    /// An approval step's gates.
    OpenGates(Vec<GateOpen>),
    /// A form step (Huddle): the run waits for Karthik.
    OpenForm(Value),
    /// The step is not needed (the plan gate with review_plan off); the reason is recorded.
    Skip(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum StepError {
    /// Re-entered with backoff, three times, then the run fails.
    Retry(String),
    Fail {
        class: ErrorClass,
        message: String,
    },
}

pub trait StepHandler: Send + Sync + 'static {
    /// Enter or re-enter the step. Must be idempotent: everything it does is keyed.
    fn run<'a>(&'a self, cx: &'a mut StepCx) -> BoxFut<'a, Result<StepOutcome, StepError>>;
}

/// What a handler sees.
#[derive(Debug, Clone)]
pub struct StepCx {
    pub run_id: Uuid,
    pub short_id: String,
    pub preset: &'static PresetDecl,
    pub step: &'static StepDecl,
    pub seq: i16,
    pub attempt: i16,
    pub args: Value,
    pub config: Value,
    pub budget: Value,
    /// This step's saved output (after a `Yield`), if any.
    pub previous: Option<Value>,
    /// Earlier steps' outputs by step id.
    pub outputs: Map<String, Value>,
    pub db: PgPool,
    pub now: DateTime<Utc>,
    /// Cancelled when Chief stops the step (pause `now`, cancel, lost lock); the task is then aborted.
    pub cancel: tokio_util::sync::CancellationToken,
}

impl StepCx {
    /// Enqueue a job of this step. A key that already has a `completed` job of this run is skipped, so
    /// a re-entered step enqueues only what is missing (ADR 0008 §6.4).
    pub async fn enqueue(&self, job: NewJob) -> Result<Option<Enqueued>, NewsError> {
        let job = job.for_step(self.run_id, self.seq);
        if let Some(key) = &job.unique_key {
            let done: bool = sqlx::query_scalar(
                "select exists (select 1 from jobs where run_id = $1 and unique_key = $2 and state = 'completed')",
            )
            .bind(self.run_id)
            .bind(key)
            .fetch_one(&self.db)
            .await
            .map_err(map_err)?;
            if done {
                return Ok(None);
            }
        }
        news_dispatch::queue::enqueue(&self.db, &job)
            .await
            .map(Some)
    }
}

/// Fails with class `bug`: the step's subsystem has not landed (#15–#21).
pub struct NotBuilt;

impl StepHandler for NotBuilt {
    fn run<'a>(&'a self, cx: &'a mut StepCx) -> BoxFut<'a, Result<StepOutcome, StepError>> {
        Box::pin(async move {
            Err(StepError::Fail {
                class: ErrorClass::Bug,
                message: format!("step {} is not built yet (#15–#21)", cx.step.handler),
            })
        })
    }
}

/// Step handlers by name (`<preset>.<step>`), bound in the binary; anything unbound is [`NotBuilt`].
#[derive(Clone, Default)]
pub struct Handlers(HashMap<String, Arc<dyn StepHandler>>);

impl Handlers {
    pub fn with(mut self, name: &str, h: Arc<dyn StepHandler>) -> Self {
        self.0.insert(name.to_owned(), h);
        self
    }

    pub fn get(&self, name: &str) -> Arc<dyn StepHandler> {
        self.0
            .get(name)
            .cloned()
            .unwrap_or_else(|| Arc::new(NotBuilt))
    }
}

enum Next {
    Continue,
    Stop,
    WaitJobs(i16),
    Backoff(Duration),
}

const RETRIES: i16 = 3;

/// SHA-256 of the canonical JSON of a step's inputs (ADR 0008 §6.4).
fn input_hash(
    run: &RunRow,
    p: &PresetDecl,
    step: &StepDecl,
    outputs: &Map<String, Value>,
) -> Vec<u8> {
    let v = json!({"preset": p.id, "preset_version": p.version, "step": step.id, "args": run.args,
                   "settings_version": run.settings_version, "outputs": outputs});
    Sha256::digest(serde_json::to_vec(&v).unwrap_or_default()).to_vec()
}

impl Chief {
    /// Drive one run until it waits (gate, form), pauses, or finishes.
    pub(crate) async fn drive(&self, run_id: Uuid) {
        loop {
            match self.step_once(run_id).await {
                Ok(Next::Continue) => {}
                Ok(Next::Stop) => return,
                Ok(Next::Backoff(d)) => tokio::time::sleep(d).await,
                Ok(Next::WaitJobs(seq)) => match self.wait_jobs(run_id, seq).await {
                    Ok(true) => {}
                    Ok(false) => return,
                    Err(e) => {
                        tracing::warn!(run_id = %run_id, error = %e, "driver wait failed");
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                },
                Err(e) => {
                    tracing::warn!(run_id = %run_id, error = %e, "driver step failed; retrying");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
    }

    /// Wait until no live job of the step is left (true), or the run stops running (false).
    async fn wait_jobs(&self, run_id: Uuid, seq: i16) -> Result<bool, NewsError> {
        loop {
            let (status, live): (String, i64) = sqlx::query_as(
                "select r.status, (select count(*) from jobs j where j.run_id = r.run_id and j.step_seq = $2
                                     and j.state in ('available', 'scheduled', 'running', 'retryable', 'snoozed'))
                   from runs r where r.run_id = $1",
            )
            .bind(run_id)
            .bind(seq)
            .fetch_one(self.db())
            .await
            .map_err(map_err)?;
            if !matches!(status.as_str(), "running" | "waiting_ai") {
                return Ok(false);
            }
            if live == 0 {
                return Ok(true);
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    async fn step_once(&self, run_id: Uuid) -> Result<Next, NewsError> {
        let now = self.now();
        let db = self.db().clone();
        let mut conn = db.acquire().await.map_err(map_err)?;
        let run = row::load(&mut conn, run_id, false).await?;
        if !matches!(run.status(), Status::Running | Status::WaitingAi) {
            return Ok(Next::Stop);
        }
        let Some(p) = presets::preset(&run.preset) else {
            return self.fail_run(run_id, "unknown preset").await;
        };
        let steps: Vec<(i16, String, i16, Option<Value>)> = sqlx::query_as(
            "select seq, status, attempt, output from run_steps where run_id = $1 order by seq",
        )
        .bind(run_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(map_err)?;
        drop(conn);
        let Some((seq, _, attempt, previous)) = steps
            .iter()
            .find(|s| s.1 == "pending" || s.1 == "running")
            .cloned()
        else {
            return self.succeed(run_id).await;
        };
        let step = p
            .step_at(seq)
            .ok_or_else(|| NewsError::internal("step out of range"))?;
        let outputs: Map<String, Value> = steps
            .iter()
            .filter(|s| s.0 < seq)
            .filter_map(|s| {
                Some((
                    p.step_at(s.0)?.id.to_owned(),
                    s.3.clone().unwrap_or(Value::Null),
                ))
            })
            .collect();
        let hash = input_hash(&run, p, step, &outputs);

        // Enter the step: running, its input hash, the run's current step and status.
        let mut tx = db.begin().await.map_err(map_err)?;
        let run = row::load(&mut tx, run_id, true).await?;
        if !matches!(run.status(), Status::Running | Status::WaitingAi) {
            return Ok(Next::Stop);
        }
        let recorded: Option<Vec<u8>> = sqlx::query_scalar(
            "update run_steps set status = 'running', started_at = coalesce(started_at, $3),
                    input_hash = coalesce(input_hash, $4)
              where run_id = $1 and seq = $2 returning input_hash",
        )
        .bind(run_id)
        .bind(seq)
        .bind(now)
        .bind(&hash)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_err)?;
        if recorded.as_deref() != Some(&hash[..]) {
            drop(tx);
            return self
                .fail_run(
                    run_id,
                    &format!("step {} was re-entered with different inputs", step.id),
                )
                .await;
        }
        sqlx::query("update runs set current_step = $2, last_seen_at = $3 where run_id = $1")
            .bind(run_id)
            .bind(step.id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        let want = if step.kind.is_ai() {
            Status::WaitingAi
        } else {
            Status::Running
        };
        if run.status() != want {
            state::transition(&mut tx, run_id, &[run.status()], Change::to(want), now).await?;
        }
        tx.commit().await.map_err(map_err)?;

        if step.kind == StepKind::Approval
            && let Some(next) = self
                .resolve_gates(&run, p, step, seq, previous.as_ref())
                .await?
        {
            return Ok(next);
        }

        let mut cx = StepCx {
            run_id,
            short_id: run.short_id.clone(),
            preset: p,
            step,
            seq,
            attempt,
            args: run.args.clone(),
            config: run.config.clone(),
            budget: run.budget.clone(),
            previous,
            outputs,
            db: db.clone(),
            now,
            cancel: tokio_util::sync::CancellationToken::new(),
        };
        let cancel = cx.cancel.clone();
        let handler = self.handlers().get(step.handler);
        let inline = step.kind == StepKind::Code;
        if inline {
            self.set_inline(run_id, true);
        }
        let task = tokio::spawn(async move { handler.run(&mut cx).await });
        // Chief can stop and await this task (pause `now`, cancel, lost lock) even after this
        // driver is aborted.
        self.track_step(run_id, cancel.clone(), task.abort_handle());
        let joined = self.await_touching(run_id, task).await;
        self.untrack_step(run_id);
        if inline {
            self.set_inline(run_id, false);
        }
        // A stopped step records nothing: the run is being paused or cancelled, or this service lost
        // its lock and another driver will re-enter the step.
        if cancel.is_cancelled() {
            return Ok(Next::Stop);
        }
        let outcome = match joined {
            Ok(r) => r,
            Err(e) => Err(StepError::Fail {
                class: ErrorClass::Bug,
                message: format!("step handler panicked: {e}"),
            }),
        };
        self.record(run_id, step, seq, attempt, outcome).await
    }

    /// Await a step task, touching the run's `last_seen_at` every 30 seconds so a long step is not
    /// taken for a stalled one.
    async fn await_touching<T>(
        &self,
        run_id: Uuid,
        mut task: tokio::task::JoinHandle<T>,
    ) -> Result<T, tokio::task::JoinError> {
        let mut beat = tokio::time::interval(Duration::from_secs(30));
        beat.tick().await;
        loop {
            tokio::select! {
                r = &mut task => return r,
                _ = beat.tick() => {
                    let _ = sqlx::query("update runs set last_seen_at = $2 where run_id = $1")
                        .bind(run_id)
                        .bind(self.now())
                        .execute(self.db())
                        .await;
                }
            }
        }
    }

    /// An approval step whose latest round is fully answered: done (approved or rejected), or back to
    /// its drafting step for `changes`. `None` when gates must be opened by the handler.
    async fn resolve_gates(
        &self,
        run: &RunRow,
        p: &PresetDecl,
        step: &StepDecl,
        seq: i16,
        previous: Option<&Value>,
    ) -> Result<Option<Next>, NewsError> {
        let mut conn = self.db().acquire().await.map_err(map_err)?;
        let round = gates::latest_round(&mut conn, run.run_id, seq).await?;
        let handled = previous
            .and_then(|o| o["rounds_handled"].as_i64())
            .unwrap_or(0) as i16;
        if round <= handled {
            return Ok(None);
        }
        let Some(ds) = gates::decisions(&mut conn, run.run_id, seq, round).await? else {
            // Still open (a restart while waiting): wait again.
            let mut tx = self.db().begin().await.map_err(map_err)?;
            let _ = state::transition(
                &mut tx,
                run.run_id,
                &[Status::Running],
                Change::to(Status::WaitingApproval),
                self.now(),
            )
            .await;
            tx.commit().await.map_err(map_err)?;
            return Ok(Some(Next::Stop));
        };
        drop(conn);
        let now = self.now();
        let decisions: Vec<Value> = ds
            .iter()
            .map(|(g, d, n)| json!({"gate": g, "decision": d, "note": n}))
            .collect();
        let output = json!({"rounds_handled": round, "decisions": decisions});
        let mut tx = self.db().begin().await.map_err(map_err)?;
        row::load(&mut tx, run.run_id, true).await?;
        if let (true, Some(back)) = (ds.iter().any(|d| d.1 == "changes"), step.changes_to) {
            let (from, _) = p
                .step(back)
                .ok_or_else(|| NewsError::internal("changes_to"))?;
            sqlx::query(
                "update run_steps set status = 'pending', input_hash = null, started_at = null, finished_at = null,
                        output = case when seq = $3 then $4 else null end
                  where run_id = $1 and seq between $2 and $3",
            )
            .bind(run.run_id)
            .bind(from)
            .bind(seq)
            .bind(&output)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        } else {
            mark_done(&mut tx, run.run_id, seq, "done", &output, now).await?;
        }
        tx.commit().await.map_err(map_err)?;
        Ok(Some(Next::Continue))
    }

    async fn record(
        &self,
        run_id: Uuid,
        step: &StepDecl,
        seq: i16,
        attempt: i16,
        outcome: Result<StepOutcome, StepError>,
    ) -> Result<Next, NewsError> {
        let now = self.now();
        let mut tx = self.db().begin().await.map_err(map_err)?;
        let run = row::load(&mut tx, run_id, true).await?;
        // The fence on completion: a step of a finished run, or of one being stopped now or
        // cancelled, writes nothing (no `done`, no consumed approvals). A `drain` pause lets it finish.
        if run.status().is_terminal()
            || (run.status() == Status::Pausing && run.stop_mode.as_deref() != Some("drain"))
        {
            return Ok(Next::Stop);
        }
        let next = match outcome {
            Ok(StepOutcome::Done(v)) => {
                mark_done(&mut tx, run_id, seq, "done", &v, now).await?;
                gates::consume(&mut tx, run_id, step.cover, now).await?;
                Next::Continue
            }
            Ok(StepOutcome::Skip(reason)) => {
                mark_done(
                    &mut tx,
                    run_id,
                    seq,
                    "skipped",
                    &json!({"skip": reason}),
                    now,
                )
                .await?;
                Next::Continue
            }
            Ok(StepOutcome::Yield(v)) => {
                sqlx::query("update run_steps set output = $3 where run_id = $1 and seq = $2")
                    .bind(run_id)
                    .bind(seq)
                    .bind(&v)
                    .execute(&mut *tx)
                    .await
                    .map_err(map_err)?;
                Next::Continue
            }
            Ok(StepOutcome::Wait) => Next::WaitJobs(seq),
            Ok(StepOutcome::OpenGates(gs)) => {
                let round = gates::latest_round(&mut tx, run_id, seq).await? + 1;
                gates::open(&mut tx, run_id, seq, round, &gs, now).await?;
                if matches!(run.status(), Status::Running | Status::WaitingAi) {
                    state::transition(
                        &mut tx,
                        run_id,
                        &[run.status()],
                        Change::to(Status::WaitingApproval),
                        now,
                    )
                    .await?;
                }
                tx.commit().await.map_err(map_err)?;
                for g in &gs {
                    self.emit_raw(
                        "gate",
                        json!({"run_id": run_id, "short_id": run.short_id, "preset": run.preset, "gate": g.gate,
                               "round": round, "state": "open", "subject_sha": gates::hex(&gates::gate_sha(g)),
                               "decision": null, "text": g.summary["text"].as_str().unwrap_or("A gate is open")}),
                    );
                }
                self.emit_run_pub(run_id, Some(run.status()), None).await;
                return Ok(Next::Stop);
            }
            Ok(StepOutcome::OpenForm(f)) => {
                sqlx::query("update run_steps set output = $3 where run_id = $1 and seq = $2")
                    .bind(run_id)
                    .bind(seq)
                    .bind(json!({"form": f}))
                    .execute(&mut *tx)
                    .await
                    .map_err(map_err)?;
                if matches!(run.status(), Status::Running | Status::WaitingAi) {
                    state::transition(
                        &mut tx,
                        run_id,
                        &[run.status()],
                        Change::to(Status::WaitingApproval),
                        now,
                    )
                    .await?;
                }
                Next::Stop
            }
            Err(StepError::Retry(m)) if attempt + 1 < RETRIES => {
                sqlx::query("update run_steps set attempt = attempt + 1, error = $3 where run_id = $1 and seq = $2")
                    .bind(run_id)
                    .bind(seq)
                    .bind(json!({"retry": m}))
                    .execute(&mut *tx)
                    .await
                    .map_err(map_err)?;
                Next::Backoff(Duration::from_secs(2u64.pow(attempt as u32 + 1)))
            }
            Err(StepError::Retry(m)) | Err(StepError::Fail { message: m, .. }) => {
                sqlx::query(
                    "update run_steps set status = 'failed', error = $3, finished_at = $4 where run_id = $1 and seq = $2",
                )
                .bind(run_id)
                .bind(seq)
                .bind(json!({"message": m}))
                .bind(now)
                .execute(&mut *tx)
                .await
                .map_err(map_err)?;
                let mut wrap = None;
                if matches!(run.status(), Status::Running | Status::WaitingAi) {
                    wrap = Some(wrap::finish(&mut tx, &run, Status::Failed, Some(&m), now).await?);
                }
                tx.commit().await.map_err(map_err)?;
                self.emit_run_pub(run_id, Some(run.status()), wrap).await;
                return Ok(Next::Stop);
            }
        };
        tx.commit().await.map_err(map_err)?;
        Ok(next)
    }

    async fn succeed(&self, run_id: Uuid) -> Result<Next, NewsError> {
        let now = self.now();
        let mut tx = self.db().begin().await.map_err(map_err)?;
        let run = row::load(&mut tx, run_id, true).await?;
        if !matches!(run.status(), Status::Running | Status::WaitingAi) {
            return Ok(Next::Stop);
        }
        let w = wrap::finish(&mut tx, &run, Status::Succeeded, None, now).await?;
        tx.commit().await.map_err(map_err)?;
        self.emit_run_pub(run_id, Some(run.status()), Some(w)).await;
        Ok(Next::Stop)
    }

    async fn fail_run(&self, run_id: Uuid, reason: &str) -> Result<Next, NewsError> {
        let now = self.now();
        let mut tx = self.db().begin().await.map_err(map_err)?;
        let run = row::load(&mut tx, run_id, true).await?;
        if matches!(run.status(), Status::Running | Status::WaitingAi) {
            let w = wrap::finish(&mut tx, &run, Status::Failed, Some(reason), now).await?;
            tx.commit().await.map_err(map_err)?;
            self.emit_run_pub(run_id, Some(run.status()), Some(w)).await;
        }
        Ok(Next::Stop)
    }
}

async fn mark_done(
    conn: &mut PgConnection,
    run_id: Uuid,
    seq: i16,
    status: &str,
    output: &Value,
    now: DateTime<Utc>,
) -> Result<(), NewsError> {
    sqlx::query(
        "update run_steps set status = $3, output = $4, finished_at = $5, error = null where run_id = $1 and seq = $2",
    )
    .bind(run_id)
    .bind(seq)
    .bind(status)
    .bind(output)
    .bind(now)
    .execute(&mut *conn)
    .await
    .map_err(map_err)?;
    let next: Option<String> = sqlx::query(
        "select step from run_steps where run_id = $1 and seq > $2 and status in ('pending', 'running') order by seq limit 1",
    )
    .bind(run_id)
    .bind(seq)
    .fetch_optional(&mut *conn)
    .await
    .map_err(map_err)?
    .map(|r| r.get("step"));
    sqlx::query("update runs set current_step = coalesce($2, current_step), last_seen_at = $3 where run_id = $1")
        .bind(run_id)
        .bind(next)
        .bind(now)
        .execute(&mut *conn)
        .await
        .map_err(map_err)?;
    Ok(())
}
