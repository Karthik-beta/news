//! The `news work` loop (§5.6): claims jobs from its pools, runs each handler with a heartbeat every
//! lease/3, records the outcome, and every `maintenance_every` runs the rescuer and the schedules.

use crate::queue::{self, Job};
use crate::{kinds, schedule};
use news_core::NewsError;
use news_core::job::{ErrorClass, Handler, JobFailure, JobLease, LeaseCheck};
use serde::Serialize;
use sqlx::PgPool;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use tokio::task::JoinSet;
use uuid::Uuid;

/// Finds the handler for a job kind.
pub type Resolver = fn(&str) -> Option<Handler>;

#[derive(Debug, Clone)]
pub struct WorkOptions {
    pub pools: Vec<String>,
    pub worker_id: String,
    /// Pause between claim rounds.
    pub poll: Duration,
    /// How often to run the rescuer and the schedules.
    pub maintenance_every: Duration,
    /// Stop claiming after this long, let in-flight jobs finish, and return.
    pub max_duration: Option<Duration>,
    /// One maintenance pass and one claim round, then wait for in-flight jobs and return.
    pub once: bool,
    /// Jobs claimed per pool per round.
    pub batch: i64,
}

impl WorkOptions {
    pub fn new(pools: Vec<String>, worker_id: impl Into<String>) -> Self {
        Self {
            pools,
            worker_id: worker_id.into(),
            poll: Duration::from_secs(1),
            maintenance_every: Duration::from_secs(30),
            max_duration: None,
            once: false,
            batch: 16,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct WorkReport {
    pub claimed: u64,
    pub completed: u64,
    pub failed: u64,
    /// Jobs whose lease was lost while running (rescued and possibly re-claimed elsewhere).
    pub lost: u64,
    pub rescued: u64,
    pub scheduled: u64,
}

enum Outcome {
    Completed,
    Failed,
    Lost,
}

/// Run until `shutdown` resolves, `max_duration` passes, or after one round with `once`.
pub async fn run(
    db: PgPool,
    opts: WorkOptions,
    resolve: Resolver,
    shutdown: impl Future<Output = ()>,
) -> Result<WorkReport, NewsError> {
    let started = Instant::now();
    let mut report = WorkReport::default();
    let mut tasks: JoinSet<Outcome> = JoinSet::new();
    let mut last_maintenance: Option<Instant> = None;
    tokio::pin!(shutdown);

    loop {
        if last_maintenance.is_none_or(|t| t.elapsed() >= opts.maintenance_every) {
            maintenance(&db, &mut report).await;
            last_maintenance = Some(Instant::now());
        }
        for pool in &opts.pools {
            let lease = kinds::pool_lease(pool);
            match queue::claim(&db, pool, opts.batch, lease, &opts.worker_id).await {
                Ok(c) => {
                    for job in c.jobs {
                        report.claimed += 1;
                        tasks.spawn(run_one(db.clone(), job, resolve, opts.worker_id.clone()));
                    }
                }
                Err(e) => tracing::warn!(pool = %pool, error = %e, "claim failed"),
            }
        }
        while let Some(done) = tasks.try_join_next() {
            tally(&mut report, done);
        }
        if opts.once || opts.max_duration.is_some_and(|d| started.elapsed() >= d) {
            break;
        }
        tokio::select! {
            () = &mut shutdown => break,
            () = tokio::time::sleep(opts.poll) => {}
        }
    }
    while let Some(done) = tasks.join_next().await {
        tally(&mut report, done);
    }
    Ok(report)
}

fn tally(report: &mut WorkReport, done: Result<Outcome, tokio::task::JoinError>) {
    match done {
        Ok(Outcome::Completed) => report.completed += 1,
        Ok(Outcome::Failed) => report.failed += 1,
        Ok(Outcome::Lost) | Err(_) => report.lost += 1,
    }
}

async fn maintenance(db: &PgPool, report: &mut WorkReport) {
    match queue::rescue(db).await {
        Ok(r) => report.rescued += (r.retryable.len() + r.discarded.len()) as u64,
        Err(e) => tracing::warn!(error = %e, "rescuer failed"),
    }
    match schedule::tick(db).await {
        Ok(Some(fired)) => report.scheduled += fired.iter().filter(|f| !f.existing).count() as u64,
        Ok(None) => {}
        Err(e) => tracing::warn!(error = %e, "schedules failed"),
    }
}

/// The lease check behind `JobLease::ensure_lease`: a flag shared with the heartbeat loop, then one
/// primary-key select. A check that finds the lease gone wakes the loop, which cancels the handler.
struct DbLease {
    db: PgPool,
    job_id: i64,
    lease_id: Uuid,
    lost: Arc<AtomicBool>,
    wake: Arc<Notify>,
}

impl DbLease {
    fn lost_failure(&self) -> JobFailure {
        JobFailure::new(
            ErrorClass::Transient,
            format!(
                "job {} lost its lease {}; stopped before writing",
                self.job_id, self.lease_id
            ),
        )
    }
}

impl LeaseCheck for DbLease {
    fn ensure(&self) -> Pin<Box<dyn Future<Output = Result<(), JobFailure>> + Send + '_>> {
        Box::pin(async move {
            if self.lost.load(Ordering::SeqCst) {
                return Err(self.lost_failure());
            }
            match queue::holds_lease(&self.db, self.job_id, self.lease_id).await {
                Ok(true) => Ok(()),
                Ok(false) => {
                    self.lost.store(true, Ordering::SeqCst);
                    self.wake.notify_one();
                    Err(self.lost_failure())
                }
                Err(e) => Err(JobFailure::new(
                    ErrorClass::Transient,
                    format!("cannot check the lease of job {}: {e}", self.job_id),
                )),
            }
        })
    }
}

/// Run one claimed job: the handler in its own task (so a panic becomes a `bug` failure, not a stuck
/// lease), a heartbeat every lease/3, then complete or fail. The handler is cancelled as soon as the
/// lease is known to be lost: a failed or stalled heartbeat, or an `ensure_lease` that finds it gone.
async fn run_one(db: PgPool, job: Job, resolve: Resolver, worker: String) -> Outcome {
    let spec = kinds::spec(&job.kind);
    let lease = Duration::from_secs(u64::from(spec.map_or(120, |s| s.lease_secs)));
    // A registered kind runs only in its own pool. Enqueue enforces it; a row that got into another
    // pool anyway (raw SQL) is failed, never run, so a publish handler can never run in `news work`.
    if let Some(s) = spec.filter(|s| s.pool != job.pool) {
        let f = JobFailure::new(
            ErrorClass::Bug,
            format!(
                "job kind {:?} runs only in pool {:?}, not {:?}; refused",
                job.kind, s.pool, job.pool
            ),
        );
        return finish_failed(&db, &job, &worker, &f).await;
    }
    let Some(handler) = resolve(&job.kind) else {
        let f = JobFailure::new(
            ErrorClass::Bug,
            format!("no handler for job kind {:?}", job.kind),
        );
        return finish_failed(&db, &job, &worker, &f).await;
    };
    let lost = Arc::new(AtomicBool::new(false));
    let wake = Arc::new(Notify::new());
    let mut input = job.input();
    input.lease = JobLease::new(
        job.job_id,
        job.lease_id,
        Arc::new(DbLease {
            db: db.clone(),
            job_id: job.job_id,
            lease_id: job.lease_id.unwrap_or_default(),
            lost: lost.clone(),
            wake: wake.clone(),
        }),
    );
    // The handler is called inside the task, so a panic before it returns its future is caught too.
    let mut handle = tokio::spawn(async move { handler(input).await });
    let mut beat = tokio::time::interval(lease / 3);
    beat.tick().await; // the first tick is immediate
    let joined = loop {
        tokio::select! {
            r = &mut handle => break r,
            () = wake.notified() => {
                tracing::warn!(job_id = job.job_id, "lease lost; handler cancelled");
                handle.abort();
                return Outcome::Lost;
            }
            _ = beat.tick() => {
                // A heartbeat that cannot finish within lease/3 counts as lost, so the handler never
                // outlives its lease waiting on a stuck database call.
                let beat_result = tokio::time::timeout(
                    lease / 3,
                    queue::heartbeat(&db, job.job_id, job.lease(&worker), lease),
                )
                .await;
                if !matches!(beat_result, Ok(Ok(_))) {
                    tracing::warn!(job_id = job.job_id, "lease lost; handler cancelled");
                    lost.store(true, Ordering::SeqCst);
                    handle.abort();
                    return Outcome::Lost;
                }
            }
        }
    };
    match joined {
        Ok(Ok(result)) => match queue::complete(&db, job.job_id, job.lease(&worker), &result).await
        {
            Ok(()) => Outcome::Completed,
            Err(e) => {
                tracing::warn!(job_id = job.job_id, error = %e, "complete failed");
                Outcome::Lost
            }
        },
        Ok(Err(failure)) => finish_failed(&db, &job, &worker, &failure).await,
        Err(join) => {
            let f = JobFailure::new(ErrorClass::Bug, format!("handler panicked: {join}"));
            finish_failed(&db, &job, &worker, &f).await
        }
    }
}

async fn finish_failed(db: &PgPool, job: &Job, worker: &str, f: &JobFailure) -> Outcome {
    match queue::fail(db, job.job_id, job.lease(worker), f).await {
        Ok(_) => Outcome::Failed,
        Err(e) => {
            tracing::warn!(job_id = job.job_id, error = %e, "fail failed");
            Outcome::Lost
        }
    }
}
