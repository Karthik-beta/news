//! Recurring jobs (§5.6): the `schedules` table, cron expressions in UTC, evaluated by the Dispatch
//! worker under advisory lock (43, schema).
//!
//! A schedule keeps only its latest due run: each due schedule enqueues one job with the unique key
//! `schedule:<id>` (a no-op while the previous one is still live) and moves `next_at` to the first
//! occurrence after now, skipping every occurrence missed while the laptop slept.
//!
//! Writes go through [`upsert`], which checks the cron expression and the kind's pool. A row that is
//! bad anyway (hand-edited) is disabled with a note and a warning; it never stops the other schedules.

use crate::kinds;
use crate::queue::{NewJob, enqueue_on};
use chrono::{DateTime, Utc};
use news_core::NewsError;
use news_store::map_err;
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;

/// Parse a 5-field cron expression (UTC).
pub fn parse(expr: &str) -> Result<croner::Cron, NewsError> {
    expr.parse::<croner::Cron>()
        .map_err(|e| NewsError::validation(format!("invalid cron expression {expr:?}: {e}")))
}

/// The first occurrence strictly after `after`.
pub fn next_after(expr: &str, after: DateTime<Utc>) -> Result<DateTime<Utc>, NewsError> {
    parse(expr)?
        .find_next_occurrence(&after, false)
        .map_err(|e| NewsError::validation(format!("cron {expr:?} has no next occurrence: {e}")))
}

/// A schedule to create or replace. `pool` defaults to the kind's pool from the registry.
#[derive(Debug, Clone)]
pub struct NewSchedule {
    pub schedule_id: String,
    pub kind: String,
    pub pool: Option<String>,
    pub cron: String,
    pub args: Value,
    pub enabled: bool,
    pub note: Option<String>,
}

impl NewSchedule {
    pub fn new(id: impl Into<String>, kind: impl Into<String>, cron: impl Into<String>) -> Self {
        Self {
            schedule_id: id.into(),
            kind: kind.into(),
            pool: None,
            cron: cron.into(),
            args: Value::Object(serde_json::Map::new()),
            enabled: true,
            note: None,
        }
    }
    pub fn pool(mut self, pool: impl Into<String>) -> Self {
        self.pool = Some(pool.into());
        self
    }
}

/// The pool a schedule's jobs go to: the given one must match a registered kind's pool.
fn schedule_pool(kind: &str, pool: Option<&str>) -> Result<String, NewsError> {
    match (pool, kinds::spec(kind)) {
        (Some(p), Some(spec)) if p != spec.pool => Err(NewsError::usage(format!(
            "job kind {kind:?} runs in pool {:?}, not {p:?}",
            spec.pool
        ))),
        (Some(p), _) if crate::is_pool(p) => Ok(p.to_owned()),
        (Some(p), _) => Err(NewsError::usage(format!("unknown pool {p:?}"))),
        (None, Some(spec)) => Ok(spec.pool.to_owned()),
        (None, None) => Err(NewsError::usage(format!(
            "unknown job kind {kind:?} and no pool given"
        ))),
    }
}

/// Create or replace a schedule after checking its cron expression and pool. Returns its `next_at`.
pub async fn upsert(db: &PgPool, s: &NewSchedule) -> Result<DateTime<Utc>, NewsError> {
    let pool = schedule_pool(&s.kind, s.pool.as_deref())?;
    let next_at = next_after(&s.cron, Utc::now())?;
    sqlx::query(
        "insert into schedules (schedule_id, kind, pool, args, cron, next_at, enabled, note)
         values ($1, $2, $3, $4, $5, $6, $7, $8)
         on conflict (schedule_id) do update set kind = excluded.kind, pool = excluded.pool,
           args = excluded.args, cron = excluded.cron, next_at = excluded.next_at,
           enabled = excluded.enabled, note = excluded.note",
    )
    .bind(&s.schedule_id)
    .bind(&s.kind)
    .bind(&pool)
    .bind(&s.args)
    .bind(&s.cron)
    .bind(next_at)
    .bind(s.enabled)
    .bind(&s.note)
    .execute(db)
    .await
    .map_err(map_err)?;
    Ok(next_at)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Fired {
    pub schedule_id: String,
    pub job_id: i64,
    /// True when the previous run was still live, so nothing new was enqueued.
    pub existing: bool,
    pub next_at: DateTime<Utc>,
}

/// Enqueue every due, enabled schedule once. Returns `None` when another process holds the lock.
/// A row with a bad cron expression or the wrong pool for its kind is disabled (with a note) and
/// skipped; the others still fire.
pub async fn tick(db: &PgPool) -> Result<Option<Vec<Fired>>, NewsError> {
    let mut tx = db.begin().await.map_err(map_err)?;
    let got: bool = sqlx::query_scalar(crate::TRY_SCHEMA_LOCK)
        .bind(crate::SCHEDULER_LOCK)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_err)?;
    if !got {
        return Ok(None);
    }
    let now: DateTime<Utc> = sqlx::query_scalar("select now()")
        .fetch_one(&mut *tx)
        .await
        .map_err(map_err)?;
    let due: Vec<(String, String, String, Value, String)> = sqlx::query_as(
        "select schedule_id, kind, pool, args, cron from schedules
          where enabled and next_at <= now() order by schedule_id for update",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(map_err)?;
    let mut fired = Vec::with_capacity(due.len());
    for (id, kind, pool, args, cron) in due {
        // Both checks run in Rust before any SQL, so a bad row never aborts the transaction.
        let checked = next_after(&cron, now).and_then(|next| {
            schedule_pool(&kind, Some(&pool))?;
            Ok(next)
        });
        let next_at = match checked {
            Ok(next) => next,
            Err(e) => {
                tracing::warn!(schedule = %id, error = %e, "schedule disabled");
                sqlx::query(
                    "update schedules set enabled = false,
                            note = 'disabled: ' || $2 || coalesce(' | ' || note, '')
                      where schedule_id = $1",
                )
                .bind(&id)
                .bind(&e.message)
                .execute(&mut *tx)
                .await
                .map_err(map_err)?;
                continue;
            }
        };
        let job = NewJob::new(kind)
            .pool(pool)
            .args(args)
            .unique_key(format!("schedule:{id}"));
        let e = enqueue_on(&mut tx, &job).await?;
        sqlx::query(
            "update schedules set next_at = $2, last_enqueued_at = now(), last_job_id = $3 where schedule_id = $1",
        )
        .bind(&id)
        .bind(next_at)
        .bind(e.job_id)
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;
        fired.push(Fired {
            schedule_id: id,
            job_id: e.job_id,
            existing: e.existing,
            next_at,
        });
    }
    tx.commit().await.map_err(map_err)?;
    Ok(Some(fired))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn next_occurrence_is_in_utc_and_strictly_later() {
        let t = Utc.with_ymd_and_hms(2026, 11, 15, 6, 0, 0).unwrap();
        assert_eq!(
            next_after("0 6 * * *", t).unwrap(),
            Utc.with_ymd_and_hms(2026, 11, 16, 6, 0, 0).unwrap()
        );
        let before = Utc.with_ymd_and_hms(2026, 11, 15, 5, 59, 0).unwrap();
        assert_eq!(next_after("0 6 * * *", before).unwrap(), t);
    }

    #[test]
    fn a_long_sleep_skips_to_the_next_future_occurrence() {
        // Next_at is computed from now, never from the missed next_at, so missed runs collapse to one.
        let now = Utc.with_ymd_and_hms(2026, 11, 20, 12, 0, 0).unwrap();
        assert_eq!(
            next_after("0 6 * * *", now).unwrap(),
            Utc.with_ymd_and_hms(2026, 11, 21, 6, 0, 0).unwrap()
        );
    }

    #[test]
    fn bad_expressions_are_validation_errors() {
        assert!(parse("not a cron").is_err());
        assert!(parse("61 * * * *").is_err());
    }
}
