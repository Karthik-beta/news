//! `news jobs …`: the Dispatch tools (Bureau §6.4).

use super::{Reply, database, read_json};
use crate::cli::{Holder, JobsCmd};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use news_core::job::{ErrorClass, JobFailure};
use news_core::{NewsError, NewsResult, Shape, tier};
use news_dispatch::queue::{self, Job, Lease, ListFilter};
use news_dispatch::{STATES, is_pool, kinds};
use serde_json::{Value, json};
use std::fmt::Write as _;
use std::time::Duration;

/// Above this many jobs, `jobs retry --class` is Karthik's call (G or H tier, §6.4).
pub const RETRY_BATCH_LIMIT: usize = 50;

pub async fn run(cmd: JobsCmd, shape: &Shape) -> NewsResult<Reply> {
    // Validate arguments before touching the database, so usage errors never depend on it.
    match &cmd {
        JobsCmd::List { state, pool, run } => {
            check_state(state.as_deref())?;
            check_pool(pool.as_deref())?;
            run.as_deref().map(parse_uuid).transpose()?;
        }
        JobsCmd::Dead { class } => {
            class.as_deref().map(parse_class).transpose()?;
        }
        JobsCmd::Claim { pool, .. } => {
            check_pool(Some(pool))?;
            // --limit is how many to claim. Paging or trimming a claim would hide leases from the caller.
            if shape.cursor.is_some() || shape.fields.is_some() {
                return Err(NewsError::usage(
                    "jobs claim takes no --cursor or --fields: it returns every job it claimed",
                )
                .with_hint("use --limit to claim fewer"));
            }
        }
        JobsCmd::Fail { class, .. } => {
            parse_class(class)?;
        }
        JobsCmd::Retry { class, .. } => {
            class.as_deref().map(parse_class).transpose()?;
        }
        JobsCmd::Pause { pool, until } => {
            check_pool(Some(pool))?;
            parse_until(until, Utc::now())?;
            tier::require_human("jobs pause")?;
        }
        JobsCmd::Unpause { pool } => {
            check_pool(Some(pool))?;
            tier::require_human("jobs unpause")?;
        }
        JobsCmd::Heartbeat { .. } | JobsCmd::Complete { .. } => {}
    }

    let (db, _) = database(2).await?;
    match cmd {
        JobsCmd::List { state, pool, run } => {
            let filter = ListFilter {
                state,
                pool,
                run_id: run.as_deref().map(parse_uuid).transpose()?,
            };
            let want = shape.offset() + shape.limit.unwrap_or(news_core::shape::DEFAULT_LIMIT) + 1;
            let (counts, jobs) =
                queue::list(&db, &filter, i64::try_from(want).unwrap_or(i64::MAX)).await?;
            let mut text = String::new();
            for c in &counts {
                let _ = writeln!(text, "{:<8} {:<10} {}", c.pool, c.state, c.count);
            }
            Ok(Reply::new(json!({
                "counts": counts,
                "items": jobs.iter().map(summary).collect::<Vec<_>>(),
                "text": text.trim_end(),
            })))
        }
        JobsCmd::Dead { class } => {
            let groups = queue::dead(&db, class.as_deref().map(parse_class).transpose()?).await?;
            let mut text = String::new();
            for g in &groups {
                let _ = writeln!(
                    text,
                    "{:>5}  {:<18} {}",
                    g.count, g.error_class, g.fingerprint
                );
            }
            Ok(Reply::new(json!({
                "items": groups,
                "text": text.trim_end(),
            })))
        }
        JobsCmd::Claim {
            pool,
            worker,
            lease_seconds,
        } => {
            let limit = i64::try_from(shape.limit.unwrap_or(1)).unwrap_or(1);
            let lease = lease_seconds.map_or_else(|| kinds::pool_lease(&pool), Duration::from_secs);
            let claimed = queue::claim(&db, &pool, limit, lease, &worker).await?;
            Ok(Reply::new(json!({
                "pool": pool,
                "paused_until": claimed.paused_until,
                "items": claimed.jobs.iter().map(claimed_job).collect::<Vec<_>>(),
            })))
        }
        JobsCmd::Heartbeat {
            job,
            holder,
            lease_seconds,
        } => {
            let current = queue::get(&db, job).await?;
            let lease = lease_seconds.map_or_else(
                || {
                    Duration::from_secs(u64::from(
                        kinds::spec(&current.kind).map_or(120, |s| s.lease_secs),
                    ))
                },
                Duration::from_secs,
            );
            let until = queue::heartbeat(&db, job, lease_of(&holder), lease).await?;
            Ok(Reply::new(json!({ "job_id": job, "lease_until": until })))
        }
        JobsCmd::Complete {
            job,
            result,
            holder,
        } => {
            let value = read_json(&result)?;
            queue::complete(&db, job, lease_of(&holder), &value).await?;
            Ok(Reply::new(json!({ "job_id": job, "state": "completed" })))
        }
        JobsCmd::Fail {
            job,
            class,
            message,
            retry_after,
            holder,
        } => {
            let mut failure = JobFailure::new(parse_class(&class)?, message);
            if let Some(s) = retry_after {
                failure = failure.with_retry_after(Duration::from_secs(s));
            }
            let failed = queue::fail(&db, job, lease_of(&holder), &failure).await?;
            Ok(Reply::new(
                serde_json::to_value(failed).unwrap_or(Value::Null),
            ))
        }
        JobsCmd::Retry { job, class } => {
            let ids = match (job, class) {
                (Some(id), _) => vec![id],
                (None, Some(c)) => {
                    let ids = queue::dead_ids(&db, parse_class(&c)?).await?;
                    if ids.len() > RETRY_BATCH_LIMIT {
                        tier::require_human("jobs retry").map_err(|e| {
                            e.with_details(
                                json!({ "matched": ids.len(), "limit": RETRY_BATCH_LIMIT }),
                            )
                        })?;
                    }
                    ids
                }
                (None, None) => return Err(NewsError::usage("give a job id or --class")),
            };
            let single = ids.len() == 1 && job.is_some();
            let mut requeued = Vec::new();
            let mut skipped = Vec::new();
            for id in ids {
                match queue::retry(&db, id).await {
                    Ok(id) => requeued.push(id),
                    Err(e) if single => return Err(e),
                    Err(e) => skipped.push(json!({ "job_id": id, "reason": e.message })),
                }
            }
            Ok(Reply::new(
                json!({ "requeued": requeued, "skipped": skipped }),
            ))
        }
        JobsCmd::Pause { pool, until } => {
            let until = parse_until(&until, Utc::now())?;
            let paused = queue::pause(&db, &pool, until).await?;
            Ok(Reply::new(json!({ "pool": pool, "paused_until": paused })))
        }
        JobsCmd::Unpause { pool } => {
            queue::unpause(&db, &pool).await?;
            Ok(Reply::new(json!({ "pool": pool, "paused_until": null })))
        }
    }
}

fn lease_of(h: &Holder) -> Lease<'_> {
    Lease::new(&h.worker, h.lease)
}

fn summary(j: &Job) -> Value {
    json!({
        "job_id": j.job_id,
        "kind": j.kind,
        "pool": j.pool,
        "state": j.state,
        "attempt": j.attempt,
        "max_attempts": j.max_attempts,
        "priority": j.priority,
        "run_after": j.run_after,
        "run_id": j.run_id,
        "item_ref": j.item_ref,
        "error_class": j.error_class,
    })
}

fn claimed_job(j: &Job) -> Value {
    json!({
        "job_id": j.job_id,
        "kind": j.kind,
        "pool": j.pool,
        "attempt": j.attempt,
        // With the worker name, this names the lease for heartbeat, complete and fail.
        "lease_id": j.lease_id,
        "run_id": j.run_id,
        "item_ref": j.item_ref,
        "args": j.args,
        // Input files are written by the enqueuing step and named in args (Ghost, #18).
        "input_path": j.args.get("input_path"),
        "lease_until": j.lease_until,
    })
}

fn check_pool(p: Option<&str>) -> NewsResult<()> {
    match p {
        Some(p) if !is_pool(p) => Err(NewsError::usage(format!(
            "unknown pool {p:?}; one of {}",
            news_dispatch::POOLS.join(", ")
        ))),
        _ => Ok(()),
    }
}

fn check_state(s: Option<&str>) -> NewsResult<()> {
    match s {
        Some(s) if !STATES.contains(&s) => Err(NewsError::usage(format!(
            "unknown state {s:?}; one of {}",
            STATES.join(", ")
        ))),
        _ => Ok(()),
    }
}

fn parse_class(s: &str) -> NewsResult<ErrorClass> {
    s.parse().map_err(NewsError::usage)
}

fn parse_uuid(s: &str) -> NewsResult<uuid::Uuid> {
    s.parse()
        .map_err(|_| NewsError::usage(format!("--run must be a run UUID, got {s:?}")))
}

/// RFC 3339, or `+<n>m`, `+<n>h`, `+<n>d` from `now`.
pub fn parse_until(s: &str, now: DateTime<Utc>) -> NewsResult<DateTime<Utc>> {
    if let Some(rel) = s.strip_prefix('+') {
        let (num, unit) = rel.split_at(rel.len().saturating_sub(1));
        let n: i64 = num
            .parse()
            .map_err(|_| NewsError::usage(format!("bad --until {s:?}")))?;
        let d = match unit {
            "m" => ChronoDuration::minutes(n),
            "h" => ChronoDuration::hours(n),
            "d" => ChronoDuration::days(n),
            _ => {
                return Err(NewsError::usage(format!(
                    "bad --until {s:?}: use m, h or d"
                )));
            }
        };
        return Ok(now + d);
    }
    DateTime::parse_from_rfc3339(s)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|_| NewsError::usage(format!("bad --until {s:?}: RFC 3339 or +30m, +2h, +1d")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn until_accepts_relative_and_absolute_times() {
        let now = Utc.with_ymd_and_hms(2026, 11, 15, 12, 0, 0).unwrap();
        assert_eq!(
            parse_until("+30m", now).unwrap(),
            now + ChronoDuration::minutes(30)
        );
        assert_eq!(
            parse_until("+2h", now).unwrap(),
            now + ChronoDuration::hours(2)
        );
        assert_eq!(
            parse_until("+1d", now).unwrap(),
            now + ChronoDuration::days(1)
        );
        assert_eq!(
            parse_until("2026-11-16T06:00:00Z", now).unwrap(),
            Utc.with_ymd_and_hms(2026, 11, 16, 6, 0, 0).unwrap()
        );
        for bad in ["", "+", "+5", "+5s", "tomorrow", "+xh"] {
            assert!(parse_until(bad, now).is_err(), "{bad}");
        }
    }

    #[test]
    fn unknown_pools_states_and_classes_are_usage_errors() {
        assert_eq!(check_pool(Some("gpu")).unwrap_err().exit_code(), 2);
        assert!(check_pool(Some("ai")).is_ok());
        assert_eq!(check_state(Some("done")).unwrap_err().exit_code(), 2);
        assert_eq!(parse_class("oops").unwrap_err().exit_code(), 2);
    }
}
