//! Queue operations. Every state change is one short transaction.
//!
//! Capacity is counted from jobs in state `running`, so a slot frees itself the moment a job leaves
//! that state; there is no counter to drift (§5.4, "Releasing a slot").

use crate::{backoff, kinds};
use chrono::{DateTime, Utc};
use news_core::NewsError;
use news_core::job::{ErrorClass, JobFailure, JobInput, Policy};
use news_store::map_err;
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool, Row};
use std::time::Duration;
use uuid::Uuid;

/// Default snooze for `rate_limited` without a `Retry-After`.
pub const DEFAULT_SNOOZE: Duration = Duration::from_secs(60);
/// Default pause for `ai_usage_limit` without a reset time.
pub const DEFAULT_USAGE_PAUSE: Duration = Duration::from_secs(3600);
/// Errors kept per job.
pub const ERRORS_KEPT: usize = 5;

const JOB_COLUMNS: &str = "job_id, kind, pool, run_id, item_ref, args, unique_key, priority, state, attempt, \
    max_attempts, snoozes, run_after, lease_until, locked_by, lease_id, error_class, errors, result, created_at, finished_at, step_seq";

/// A row of `jobs`.
#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct Job {
    pub job_id: i64,
    pub kind: String,
    pub pool: String,
    pub run_id: Option<Uuid>,
    pub item_ref: Option<String>,
    pub args: Value,
    pub unique_key: Option<String>,
    pub priority: i16,
    pub state: String,
    pub attempt: i16,
    pub max_attempts: i16,
    pub snoozes: i16,
    pub run_after: DateTime<Utc>,
    pub lease_until: Option<DateTime<Utc>>,
    pub locked_by: Option<String>,
    /// The token of the current claim; set exactly while the job is `running`.
    pub lease_id: Option<Uuid>,
    pub error_class: Option<String>,
    pub errors: Value,
    pub result: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    /// The run step that enqueued it (Chief, ADR 0008 §6.2).
    pub step_seq: Option<i16>,
}

/// Who holds a running job: the worker that claimed it and the `lease_id` that claim created.
/// Heartbeat, complete and fail must name both, so a holder whose lease expired (and whose job was
/// rescued, snoozed or retried and claimed again, even under the same worker name and attempt number)
/// gets `conflict` instead of finishing someone else's claim. `attempt` is only retry accounting: a
/// snooze gives it back and a retry resets it, so it cannot identify a claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lease<'a> {
    pub worker: &'a str,
    pub lease_id: Uuid,
}

impl<'a> Lease<'a> {
    pub fn new(worker: &'a str, lease_id: Uuid) -> Self {
        Self { worker, lease_id }
    }
}

impl Job {
    /// The lease a claim gave `worker` on this job (a nil id, which matches nothing, when it has none).
    pub fn lease<'a>(&self, worker: &'a str) -> Lease<'a> {
        Lease::new(worker, self.lease_id.unwrap_or_default())
    }

    pub fn input(&self) -> JobInput {
        JobInput {
            job_id: self.job_id,
            kind: self.kind.clone(),
            pool: self.pool.clone(),
            attempt: self.attempt,
            run_id: self.run_id,
            item_ref: self.item_ref.clone(),
            args: self.args.clone(),
            // The worker attaches the claim's lease check before it runs the handler.
            lease: news_core::job::JobLease::default(),
        }
    }
}

/// A job to enqueue. `pool` defaults to the kind's pool from the registry.
#[derive(Debug, Clone, Default)]
pub struct NewJob {
    pub kind: String,
    pub pool: Option<String>,
    pub run_id: Option<Uuid>,
    /// The run step that enqueues it; set with `run_id` (Chief's `cx.enqueue`).
    pub step_seq: Option<i16>,
    pub item_ref: Option<String>,
    pub args: Value,
    pub unique_key: Option<String>,
    pub priority: Option<i16>,
    pub max_attempts: Option<i16>,
    /// Earliest start; `None` means now.
    pub run_after: Option<DateTime<Utc>>,
}

impl NewJob {
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            args: json!({}),
            ..Self::default()
        }
    }
    pub fn pool(mut self, pool: impl Into<String>) -> Self {
        self.pool = Some(pool.into());
        self
    }
    pub fn unique_key(mut self, key: impl Into<String>) -> Self {
        self.unique_key = Some(key.into());
        self
    }
    pub fn args(mut self, args: Value) -> Self {
        self.args = args;
        self
    }
    pub fn priority(mut self, p: i16) -> Self {
        self.priority = Some(p);
        self
    }
    pub fn max_attempts(mut self, n: i16) -> Self {
        self.max_attempts = Some(n);
        self
    }
    /// The run and step this job belongs to.
    pub fn for_step(mut self, run_id: Uuid, step_seq: i16) -> Self {
        self.run_id = Some(run_id);
        self.step_seq = Some(step_seq);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Enqueued {
    pub job_id: i64,
    /// True when a live job with the same unique key already existed; nothing was inserted.
    pub existing: bool,
}

/// Enqueue a job. With a `unique_key` that is live, nothing is inserted and the existing `job_id` is
/// returned (River's unique jobs, §5.4).
pub async fn enqueue(pool: &PgPool, job: &NewJob) -> Result<Enqueued, NewsError> {
    let mut conn = pool.acquire().await.map_err(map_err)?;
    enqueue_on(&mut conn, job).await
}

/// [`enqueue`] on a given connection or transaction.
pub async fn enqueue_on(conn: &mut PgConnection, job: &NewJob) -> Result<Enqueued, NewsError> {
    let pool_name = match (&job.pool, kinds::spec(&job.kind)) {
        // A registered kind runs only in its own pool: a publish job in `net` would reach the background worker.
        (Some(p), Some(spec)) if p != spec.pool => {
            return Err(NewsError::usage(format!(
                "job kind {:?} runs in pool {:?}, not {p:?}",
                job.kind, spec.pool
            )));
        }
        (Some(p), _) => p.clone(),
        (None, Some(spec)) => spec.pool.to_owned(),
        (None, None) => {
            return Err(NewsError::usage(format!(
                "unknown job kind {:?} and no pool given",
                job.kind
            )));
        }
    };
    if !crate::is_pool(&pool_name) {
        return Err(NewsError::usage(format!("unknown pool {pool_name:?}")));
    }
    let priority = job.priority.unwrap_or(2);
    if !(0..=4).contains(&priority) {
        return Err(NewsError::usage(
            "priority must be 0 (urgent) to 4 (backfill)",
        ));
    }
    // Two tries: a live job can finish between the conflicting insert and the lookup.
    for _ in 0..3 {
        let inserted: Option<i64> = sqlx::query_scalar(
            "insert into jobs (kind, pool, run_id, item_ref, args, unique_key, priority, max_attempts, run_after, state, step_seq)
             values ($1, $2, $3, $4, $5, $6, $7, coalesce($8, 6::smallint), coalesce($9, now()),
                     case when coalesce($9, now()) > now() then 'scheduled' else 'available' end, $10)
             on conflict (unique_key)
               where unique_key is not null and state not in ('completed', 'discarded', 'cancelled')
             do nothing
             returning job_id",
        )
        .bind(&job.kind)
        .bind(&pool_name)
        .bind(job.run_id)
        .bind(&job.item_ref)
        .bind(&job.args)
        .bind(&job.unique_key)
        .bind(priority)
        .bind(job.max_attempts)
        .bind(job.run_after)
        .bind(job.step_seq)
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_err)?;
        if let Some(job_id) = inserted {
            return Ok(Enqueued {
                job_id,
                existing: false,
            });
        }
        let existing: Option<i64> = sqlx::query_scalar(
            "select job_id from jobs where unique_key = $1
               and state not in ('completed', 'discarded', 'cancelled')",
        )
        .bind(&job.unique_key)
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_err)?;
        if let Some(job_id) = existing {
            return Ok(Enqueued {
                job_id,
                existing: true,
            });
        }
    }
    Err(NewsError::new(
        news_core::ErrorCode::UpstreamUnavailable,
        "unique key kept changing state; retry",
    ))
}

/// The result of a claim.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Claimed {
    pub jobs: Vec<Job>,
    /// Set when the pool is paused; no job was claimed.
    pub paused_until: Option<DateTime<Utc>>,
}

/// Claim up to `limit` ready jobs from `pool_name`, leasing them to `worker` for `lease`.
///
/// One claimer per pool at a time: the pool's row is locked `FOR UPDATE` first (others wait, they do
/// not skip), so the running count cannot change under us and `max_running` is never exceeded.
///
/// Every claim first runs the rescuer (a cheap check when no lease has expired), so expired leases come
/// back even when no worker is running: Ghost's `news jobs claim` is then the only claimer.
pub async fn claim(
    db: &PgPool,
    pool_name: &str,
    limit: i64,
    lease: Duration,
    worker: &str,
) -> Result<Claimed, NewsError> {
    if limit < 1 {
        return Err(NewsError::usage("--limit must be at least 1"));
    }
    if let Err(e) = rescue(db).await {
        tracing::warn!(error = %e, "rescuer failed before a claim");
    }
    let mut tx = db.begin().await.map_err(map_err)?;
    let claimed = claim_in_tx(&mut tx, pool_name, limit, lease, worker).await?;
    tx.commit().await.map_err(map_err)?;
    Ok(claimed)
}

/// The claim inside the caller's transaction, which must commit it. Tests use it to hold a claim open
/// against a pause (ADR 0008 §4.3).
///
/// A job of a run is claimable only while the run is `running` or `waiting_ai`, and the claim holds
/// that run's row `FOR SHARE` until it commits: a pause (`update runs set status = 'pausing'`) waits for
/// a claim already holding the row, and a claim that starts after the pause re-checks the committed
/// row, sees `pausing` and takes nothing of that run.
pub async fn claim_in_tx(
    tx: &mut PgConnection,
    pool_name: &str,
    limit: i64,
    lease: Duration,
    worker: &str,
) -> Result<Claimed, NewsError> {
    if limit < 1 {
        return Err(NewsError::usage("--limit must be at least 1"));
    }
    let row = sqlx::query(
        "select max_running, paused_until, (paused_until is not null and paused_until > now()) as paused
           from pools where pool = $1 for update",
    )
    .bind(pool_name)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_err)?
    .ok_or_else(|| NewsError::not_found(format!("pool {pool_name:?} does not exist")))?;
    let max_running: i16 = row.get("max_running");
    let paused: bool = row.get("paused");
    if paused {
        let until: Option<DateTime<Utc>> = row.get("paused_until");
        return Ok(Claimed {
            jobs: Vec::new(),
            paused_until: until,
        });
    }
    // The lease runs from this statement, not from `now()` (the transaction's start): the wait for the
    // pool row above must not eat into it.
    let sql = format!(
        "with c as (
           select job_id from jobs
            where pool = $1 and state in ('available', 'scheduled', 'retryable', 'snoozed') and run_after <= now()
              and (jobs.run_id is null or exists (select 1 from runs r
                     where r.run_id = jobs.run_id and r.status in ('running', 'waiting_ai') for share of r))
            order by priority, run_after, job_id
            for update skip locked
            limit greatest(0, least($2, $3 - (select count(*) from jobs where pool = $1 and state = 'running')))
         )
         update jobs j set state = 'running', attempt = j.attempt + 1,
                lease_until = statement_timestamp() + make_interval(secs => $4), locked_by = $5,
                lease_id = gen_random_uuid()
           from c where j.job_id = c.job_id
         returning {}",
        JOB_COLUMNS
            .split(", ")
            .map(|c| format!("j.{c}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut jobs: Vec<Job> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(pool_name)
        .bind(limit)
        .bind(i64::from(max_running))
        .bind(lease.as_secs_f64())
        .bind(worker)
        .fetch_all(&mut *tx)
        .await
        .map_err(map_err)?;
    jobs.sort_by(|a, b| {
        (a.priority, a.run_after, a.job_id).cmp(&(b.priority, b.run_after, b.job_id))
    });
    Ok(Claimed {
        jobs,
        paused_until: None,
    })
}

/// Fetch one job.
pub async fn get(db: &PgPool, job_id: i64) -> Result<Job, NewsError> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "select {JOB_COLUMNS} from jobs where job_id = $1"
    )))
    .bind(job_id)
    .fetch_optional(db)
    .await
    .map_err(map_err)?
    .ok_or_else(|| NewsError::not_found(format!("job {job_id} not found")))
}

/// Why a running-job update that names `lease` matched nothing.
async fn not_held(db: &PgPool, job_id: i64, lease: Lease<'_>) -> NewsError {
    match get(db, job_id).await {
        Err(e) => e,
        Ok(job) if job.state != "running" => {
            NewsError::conflict(format!("job {job_id} is {}, not running", job.state))
                .with_hint("its lease may have expired; the rescuer made it retryable")
                .with_details(json!({"state": job.state}))
        }
        Ok(job) if job.locked_by.as_deref() != Some(lease.worker) => NewsError::conflict(format!(
            "job {job_id} is leased to {}, not {}",
            job.locked_by.as_deref().unwrap_or("nobody"),
            lease.worker
        )),
        Ok(job) if job.lease_id == Some(lease.lease_id) => NewsError::conflict(format!(
            "job {job_id}: lease {} has expired",
            lease.lease_id
        ))
        .with_hint("an expired lease is not renewed; drop this claim"),
        Ok(_) => NewsError::conflict(format!(
            "job {job_id} is held under another lease, not {}",
            lease.lease_id
        ))
        .with_hint("your lease expired and the job was claimed again; drop this claim"),
    }
}

/// Whether `lease_id` still holds `job_id`: running under that lease, and not yet expired. One
/// primary-key lookup; handlers reach it through `JobLease::ensure_lease` before external writes.
pub async fn holds_lease(db: &PgPool, job_id: i64, lease_id: Uuid) -> Result<bool, NewsError> {
    sqlx::query_scalar(
        "select exists (select 1 from jobs where job_id = $1 and state = 'running'
                          and lease_id = $2 and lease_until > clock_timestamp())",
    )
    .bind(job_id)
    .bind(lease_id)
    .fetch_one(db)
    .await
    .map_err(map_err)
}

/// Extend a running job's lease; `lease` must hold it and must not have expired (the rule of
/// [`holds_lease`]), so a lease that `ensure_lease` already saw as lost is never brought back. When
/// the job belongs to a run, the run's `last_seen_at` moves too, so Chief's stall check sees a live
/// job (§5.5).
pub async fn heartbeat(
    db: &PgPool,
    job_id: i64,
    lease: Lease<'_>,
    extend: Duration,
) -> Result<DateTime<Utc>, NewsError> {
    // Two statements, no shared transaction (ADR 0008 §4.4): `jobs` then `runs` in one would invert the
    // lock order of a pause's finish (`runs` then `jobs`) and could deadlock.
    let row: Option<(DateTime<Utc>, Option<Uuid>)> = sqlx::query_as(
        "update jobs set lease_until = clock_timestamp() + make_interval(secs => $4)
          where job_id = $1 and state = 'running' and locked_by = $2 and lease_id = $3
            and lease_until > clock_timestamp()
          returning lease_until, run_id",
    )
    .bind(job_id)
    .bind(lease.worker)
    .bind(lease.lease_id)
    .bind(extend.as_secs_f64())
    .fetch_optional(db)
    .await
    .map_err(map_err)?;
    match row {
        Some((until, run_id)) => {
            if let Some(run_id) = run_id {
                sqlx::query("update runs set last_seen_at = now() where run_id = $1")
                    .bind(run_id)
                    .execute(db)
                    .await
                    .map_err(map_err)?;
            }
            Ok(until)
        }
        None => Err(not_held(db, job_id, lease).await),
    }
}

/// Mark a running job completed with its result; `lease` must hold it.
pub async fn complete(
    db: &PgPool,
    job_id: i64,
    lease: Lease<'_>,
    result: &Value,
) -> Result<(), NewsError> {
    let done = sqlx::query(
        "update jobs set state = 'completed', result = $4, finished_at = now(),
                lease_until = null, locked_by = null, lease_id = null
          where job_id = $1 and state = 'running' and locked_by = $2 and lease_id = $3",
    )
    .bind(job_id)
    .bind(lease.worker)
    .bind(lease.lease_id)
    .bind(result)
    .execute(db)
    .await
    .map_err(map_err)?;
    if done.rows_affected() == 1 {
        Ok(())
    } else {
        Err(not_held(db, job_id, lease).await)
    }
}

/// What happened to a failed job.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Failed {
    pub job_id: i64,
    pub state: String,
    pub attempt: i16,
    pub run_after: DateTime<Utc>,
    /// Set when the failure paused the pool (`ai_usage_limit`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool_paused_until: Option<DateTime<Utc>>,
}

/// Record a failure and apply the class's policy (§5.4): retry with full-jitter backoff, snooze
/// without spending the attempt, pause the pool, or discard. `lease` must hold the job.
///
/// Lock order is pool row, then job row, the same as the claim and the rescuer, so a usage-limit
/// failure (which updates the pool) can never deadlock against them.
pub async fn fail(
    db: &PgPool,
    job_id: i64,
    lease: Lease<'_>,
    failure: &JobFailure,
) -> Result<Failed, NewsError> {
    let class = failure.class;
    let mut tx = db.begin().await.map_err(map_err)?;
    if class.policy() == Policy::PausePool {
        // A job's pool never changes, so it can be read before any lock is taken.
        let pool_name: Option<String> =
            sqlx::query_scalar("select pool from jobs where job_id = $1")
                .bind(job_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(map_err)?;
        if let Some(p) = pool_name {
            sqlx::query("select pool from pools where pool = $1 for update")
                .bind(p)
                .execute(&mut *tx)
                .await
                .map_err(map_err)?;
        }
    }
    let current: Option<(i16, i16, String)> = sqlx::query_as(
        "select attempt, max_attempts, pool from jobs
          where job_id = $1 and state = 'running' and locked_by = $2 and lease_id = $3
          for update",
    )
    .bind(job_id)
    .bind(lease.worker)
    .bind(lease.lease_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_err)?;
    let Some((attempt, max_attempts, pool_name)) = current else {
        drop(tx);
        return Err(not_held(db, job_id, lease).await);
    };

    let mut pool_paused_until = None;
    // (state, delay, attempt delta, snooze delta, finished)
    let (state, delay, attempt_delta, snooze_delta) = match class.policy() {
        Policy::Retry if attempt >= max_attempts => ("discarded", Duration::ZERO, 0i16, 0i16),
        Policy::Retry => ("retryable", backoff::full_jitter(attempt), 0, 0),
        Policy::Snooze => (
            "snoozed",
            failure.retry_after.unwrap_or(DEFAULT_SNOOZE),
            -1,
            1,
        ),
        Policy::PausePool => {
            let pause = failure.retry_after.unwrap_or(DEFAULT_USAGE_PAUSE);
            let until: DateTime<Utc> = sqlx::query_scalar(
                "update pools set paused_until = greatest(coalesce(paused_until, now()), now() + make_interval(secs => $2))
                  where pool = $1 returning paused_until",
            )
            .bind(&pool_name)
            .bind(pause.as_secs_f64())
            .fetch_one(&mut *tx)
            .await
            .map_err(map_err)?;
            pool_paused_until = Some(until);
            ("snoozed", pause, -1, 1)
        }
        Policy::Discard => ("discarded", Duration::ZERO, 0, 0),
    };

    let row = sqlx::query(
        "update jobs set
            state = $2,
            attempt = attempt + $3,
            snoozes = snoozes + $4,
            run_after = now() + make_interval(secs => $5),
            lease_until = null, locked_by = null, lease_id = null,
            error_class = $6,
            errors = jsonb_path_query_array(
                jsonb_build_array(jsonb_build_object('class', $6::text, 'message', $7::text, 'at', now(), 'attempt', attempt))
                  || errors,
                '$[0 to 4]'),
            finished_at = case when $2 = 'discarded' then now() else null end
          where job_id = $1
          returning attempt, run_after",
    )
    .bind(job_id)
    .bind(state)
    .bind(attempt_delta)
    .bind(snooze_delta)
    .bind(delay.as_secs_f64())
    .bind(class.as_str())
    .bind(truncate(&failure.message, 2000))
    .fetch_one(&mut *tx)
    .await
    .map_err(map_err)?;
    tx.commit().await.map_err(map_err)?;
    Ok(Failed {
        job_id,
        state: state.to_owned(),
        attempt: row.get("attempt"),
        run_after: row.get("run_after"),
        pool_paused_until,
    })
}

fn truncate(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// Re-enqueue one finished job (Salvage): back to `available` with a fresh set of attempts.
pub async fn retry(db: &PgPool, job_id: i64) -> Result<i64, NewsError> {
    let res = sqlx::query(
        "update jobs set state = 'available', attempt = 0, run_after = now(), finished_at = null,
                error_class = null, lease_until = null, locked_by = null, lease_id = null
          where job_id = $1 and state in ('discarded', 'cancelled', 'retryable', 'snoozed')",
    )
    .bind(job_id)
    .execute(db)
    .await;
    match res {
        Ok(r) if r.rows_affected() == 1 => Ok(job_id),
        Ok(_) => {
            let job = get(db, job_id).await?;
            Err(NewsError::conflict(format!(
                "job {job_id} is {}; only discarded, cancelled, retryable or snoozed jobs can be retried",
                job.state
            )))
        }
        Err(e) => {
            let mapped = map_err(e);
            if mapped.code == news_core::ErrorCode::Conflict {
                Err(NewsError::conflict(format!(
                    "job {job_id}: a live job with the same unique key exists"
                )))
            } else {
                Err(mapped)
            }
        }
    }
}

/// Discarded job ids with this error class, oldest first.
pub async fn dead_ids(db: &PgPool, class: ErrorClass) -> Result<Vec<i64>, NewsError> {
    sqlx::query_scalar(
        "select job_id from jobs where state = 'discarded' and error_class = $1 order by job_id",
    )
    .bind(class.as_str())
    .fetch_all(db)
    .await
    .map_err(map_err)
}

/// Pause a pool until a time.
pub async fn pause(
    db: &PgPool,
    pool_name: &str,
    until: DateTime<Utc>,
) -> Result<DateTime<Utc>, NewsError> {
    sqlx::query_scalar("update pools set paused_until = $2 where pool = $1 returning paused_until")
        .bind(pool_name)
        .bind(until)
        .fetch_optional(db)
        .await
        .map_err(map_err)?
        .ok_or_else(|| NewsError::not_found(format!("pool {pool_name:?} does not exist")))
}

pub async fn unpause(db: &PgPool, pool_name: &str) -> Result<(), NewsError> {
    let r = sqlx::query("update pools set paused_until = null where pool = $1")
        .bind(pool_name)
        .execute(db)
        .await
        .map_err(map_err)?;
    if r.rows_affected() == 1 {
        Ok(())
    } else {
        Err(NewsError::not_found(format!(
            "pool {pool_name:?} does not exist"
        )))
    }
}

/// Result of one rescuer pass.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Rescued {
    /// False when no lease had expired or another process held the rescuer lock; nothing was done.
    pub ran: bool,
    pub retryable: Vec<i64>,
    pub discarded: Vec<i64>,
}

/// Move `running` jobs whose lease has expired back to `retryable` (or `discarded` when their attempts
/// are spent). Runs only in the process that gets advisory lock (42, schema), and takes every pool's row lock
/// first, in pool order, so an expired job's slot and a new claim never race. Every lock order in the
/// queue is pool rows (in pool order) before job rows; claims and `fail` follow it too.
///
/// Called before every claim, so it starts with a lock-free check and returns at once when no lease
/// has expired (`ran` is then false).
pub async fn rescue(db: &PgPool) -> Result<Rescued, NewsError> {
    let expired: bool = sqlx::query_scalar(
        "select exists (select 1 from jobs where state = 'running' and lease_until < now())",
    )
    .fetch_one(db)
    .await
    .map_err(map_err)?;
    if !expired {
        return Ok(Rescued::default());
    }
    let mut tx = db.begin().await.map_err(map_err)?;
    let got: bool = sqlx::query_scalar(crate::TRY_SCHEMA_LOCK)
        .bind(crate::RESCUER_LOCK)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_err)?;
    if !got {
        return Ok(Rescued::default());
    }
    sqlx::query("select pool from pools order by pool for update")
        .fetch_all(&mut *tx)
        .await
        .map_err(map_err)?;
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "update jobs set
            state = case when attempt >= max_attempts then 'discarded' else 'retryable' end,
            run_after = now(), lease_until = null, locked_by = null, lease_id = null,
            error_class = 'transient',
            errors = jsonb_path_query_array(
                jsonb_build_array(jsonb_build_object('class', 'transient', 'message',
                    'lease expired (worker ' || coalesce(locked_by, '?') || ' stopped heartbeating)',
                    'at', now(), 'attempt', attempt)) || errors,
                '$[0 to 4]'),
            finished_at = case when attempt >= max_attempts then now() else null end
          where state = 'running' and lease_until < now()
          returning job_id, state",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(map_err)?;
    tx.commit().await.map_err(map_err)?;
    let mut out = Rescued {
        ran: true,
        ..Rescued::default()
    };
    for (id, state) in rows {
        if state == "discarded" {
            out.discarded.push(id);
        } else {
            out.retryable.push(id);
        }
    }
    out.retryable.sort_unstable();
    out.discarded.sort_unstable();
    Ok(out)
}

/// Filters for [`list`].
#[derive(Debug, Clone, Default)]
pub struct ListFilter {
    pub state: Option<String>,
    pub pool: Option<String>,
    pub run_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Count {
    pub pool: String,
    pub state: String,
    pub count: i64,
}

/// Counts by pool and state, and up to `max` jobs, newest first.
pub async fn list(
    db: &PgPool,
    f: &ListFilter,
    max: i64,
) -> Result<(Vec<Count>, Vec<Job>), NewsError> {
    let filter = "($1::text is null or state = $1) and ($2::text is null or pool = $2) and ($3::uuid is null or run_id = $3)";
    let counts: Vec<(String, String, i64)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "select pool, state, count(*) from jobs where {filter} group by pool, state order by pool, state"
    )))
    .bind(&f.state)
    .bind(&f.pool)
    .bind(f.run_id)
    .fetch_all(db)
    .await
    .map_err(map_err)?;
    let jobs = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "select {JOB_COLUMNS} from jobs where {filter} order by job_id desc limit $4"
    )))
    .bind(&f.state)
    .bind(&f.pool)
    .bind(f.run_id)
    .bind(max)
    .fetch_all(db)
    .await
    .map_err(map_err)?;
    Ok((
        counts
            .into_iter()
            .map(|(pool, state, count)| Count { pool, state, count })
            .collect(),
        jobs,
    ))
}

/// A group on the dead list.
#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct DeadGroup {
    pub error_class: String,
    /// The latest error message with digits replaced by `#`, lower-cased, first 120 characters.
    pub fingerprint: String,
    pub count: i64,
    /// Up to five job ids, oldest first.
    pub job_ids: Vec<i64>,
    pub last_at: Option<DateTime<Utc>>,
}

/// Discarded jobs grouped by error class and message fingerprint, biggest group first.
pub async fn dead(db: &PgPool, class: Option<ErrorClass>) -> Result<Vec<DeadGroup>, NewsError> {
    sqlx::query_as(
        "select coalesce(error_class, 'unknown') as error_class,
                left(regexp_replace(lower(coalesce(errors -> 0 ->> 'message', '')), '[0-9]+', '#', 'g'), 120) as fingerprint,
                count(*) as count,
                (array_agg(job_id order by job_id))[1:5] as job_ids,
                max(finished_at) as last_at
           from jobs
          where state = 'discarded' and ($1::text is null or error_class = $1)
          group by 1, 2
          order by count desc, 1, 2",
    )
    .bind(class.map(ErrorClass::as_str))
    .fetch_all(db)
    .await
    .map_err(map_err)
}

/// Set a pool's `max_running`. The service applies Standing's `pools.ai.max_running` (or, in reduced
/// quiet hours, `quiet_hours.ai_slots`) to `ai` with it.
pub async fn set_max_running(db: &PgPool, pool_name: &str, max: i16) -> Result<(), NewsError> {
    sqlx::query("update pools set max_running = $2 where pool = $1")
        .bind(pool_name)
        .bind(max)
        .execute(db)
        .await
        .map_err(map_err)?;
    Ok(())
}

/// Move a run's `running` jobs back to `retryable` with no holder (ADR 0008 §4.1 step 3), so a late
/// heartbeat, completion or failure names a lease that no longer exists and gets `conflict`. The
/// attempt is given back: the job did not fail. Call it in the caller's transaction after locking the
/// run row (lock order: runs, then jobs).
pub async fn revoke_run_leases(
    conn: &mut PgConnection,
    run_id: Uuid,
) -> Result<Vec<i64>, NewsError> {
    sqlx::query_scalar(
        "update jobs set state = 'retryable', run_after = now(), lease_until = null, locked_by = null,
                lease_id = null, attempt = greatest(attempt - 1, 0)
          where run_id = $1 and state = 'running'
          returning job_id",
    )
    .bind(run_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(map_err)
}

/// Every `running` job, of any run, back to `retryable` with no holder: the takeover fence (ADR 0008
/// §9 step 4 and the #10 review notes). A service that has just taken the singleton lock calls it in
/// recovery before anything claims, so a late report from an instance that lost the lock is refused.
pub async fn revoke_all_leases(db: &PgPool) -> Result<Vec<i64>, NewsError> {
    let mut tx = db.begin().await.map_err(map_err)?;
    // Pool rows first, as the claim and the rescuer take them.
    sqlx::query("select pool from pools order by pool for update")
        .fetch_all(&mut *tx)
        .await
        .map_err(map_err)?;
    let ids: Vec<i64> = sqlx::query_scalar(
        "update jobs set state = 'retryable', run_after = now(), lease_until = null, locked_by = null,
                lease_id = null, attempt = greatest(attempt - 1, 0)
          where state = 'running'
          returning job_id",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(map_err)?;
    tx.commit().await.map_err(map_err)?;
    Ok(ids)
}

/// Cancel a run's live jobs (of one pool, when given), recording `class`; in the caller's transaction.
pub async fn cancel_run_jobs(
    conn: &mut PgConnection,
    run_id: Uuid,
    class: Option<ErrorClass>,
    pool: Option<&str>,
) -> Result<Vec<i64>, NewsError> {
    sqlx::query_scalar(
        "update jobs set state = 'cancelled', finished_at = now(), lease_until = null, locked_by = null,
                lease_id = null, error_class = coalesce($2, error_class)
          where run_id = $1 and state in ('available', 'scheduled', 'running', 'retryable', 'snoozed')
            and ($3::text is null or pool = $3)
          returning job_id",
    )
    .bind(run_id)
    .bind(class.map(ErrorClass::as_str))
    .bind(pool)
    .fetch_all(&mut *conn)
    .await
    .map_err(map_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_respects_char_boundaries() {
        assert_eq!(truncate("héllo", 2), "hé");
        assert_eq!(truncate("abc", 10), "abc");
    }

    #[test]
    fn job_columns_match_the_struct() {
        assert_eq!(JOB_COLUMNS.split(", ").count(), 22);
    }
}
