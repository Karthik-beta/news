//! Recurring jobs (§5.6): the `schedules` table, cron expressions in UTC, evaluated by the Dispatch
//! worker under advisory lock 43.
//!
//! A schedule keeps only its latest due run: each due schedule enqueues one job with the unique key
//! `schedule:<id>` (a no-op while the previous one is still live) and moves `next_at` to the first
//! occurrence after now, skipping every occurrence missed while the laptop slept.

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

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Fired {
    pub schedule_id: String,
    pub job_id: i64,
    /// True when the previous run was still live, so nothing new was enqueued.
    pub existing: bool,
    pub next_at: DateTime<Utc>,
}

/// Enqueue every due, enabled schedule once. Returns `None` when another process holds the lock.
pub async fn tick(db: &PgPool) -> Result<Option<Vec<Fired>>, NewsError> {
    let mut tx = db.begin().await.map_err(map_err)?;
    let got: bool = sqlx::query_scalar("select pg_try_advisory_xact_lock($1)")
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
        let next_at = next_after(&cron, now)?;
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
