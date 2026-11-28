//! The Wrap (ADR 0008 §6.5): the only writer of a terminal status. Built from PostgreSQL alone, with
//! code-rendered text, in the caller's transaction.

use crate::presets;
use crate::row::RunRow;
use crate::state::Status;
use chrono::{DateTime, Utc};
use news_core::NewsError;
use news_store::map_err;
use serde_json::{Map, Value, json};
use sqlx::{PgConnection, Row};

/// End `run` (locked by the caller) with a terminal `status` and write its Wrap.
pub async fn finish(
    conn: &mut PgConnection,
    run: &RunRow,
    status: Status,
    reason: Option<&str>,
    now: DateTime<Utc>,
) -> Result<Value, NewsError> {
    finish_inner(conn, run, status, reason, now, false).await
}

/// The Wrap of a run inserted as `skipped` in the same transaction (a scheduled run that never got
/// to start: late, preset live, or held by the quota guard).
pub(crate) async fn finish_new_skip(
    conn: &mut PgConnection,
    run: &RunRow,
    reason: &str,
    now: DateTime<Utc>,
) -> Result<Value, NewsError> {
    finish_inner(conn, run, Status::Skipped, Some(reason), now, true).await
}

/// Cancel (Bureau Design, "Cancel"): the run's outbox rows never sent are dropped (dead, `cancelled`), so
/// no later drain publishes them; a row whose send may have landed (`unknown_since`, or attempted) stays
/// to be reconciled. A dropped put takes its page back: a new page to an unowned draft, a revision's
/// pending version deleted (as a dead put does in Press).
async fn drop_unsent(
    conn: &mut PgConnection,
    run_id: uuid::Uuid,
    now: DateTime<Utc>,
) -> Result<(), NewsError> {
    let puts: Vec<(String, i32)> = sqlx::query(
        "update publish_outbox set dead_at = $2, last_error = 'cancelled'
          where run_id = $1 and sent_at is null and dead_at is null and attempts = 0
            and unknown_since is null and op <> 'home_curation'
         returning page_id, version, op",
    )
    .bind(run_id)
    .bind(now)
    .fetch_all(&mut *conn)
    .await
    .map_err(map_err)?
    .iter()
    .filter(|r| r.get::<String, _>("op") == "put_page")
    .map(|r| (r.get("page_id"), r.get("version")))
    .collect();
    for (page, v) in puts {
        for sql in [
            "delete from page_refs where page_id = $1 and version = $2
                and exists (select 1 from page_drafts d where d.page_id = $1 and d.version = $2)",
            "delete from page_drafts where page_id = $1 and version = $2",
            "update pages set status = 'draft', run_id = null
              where page_id = $1 and version = $2 and status = 'ready'",
        ] {
            sqlx::query(sql)
                .bind(&page)
                .bind(v)
                .execute(&mut *conn)
                .await
                .map_err(map_err)?;
        }
    }
    Ok(())
}

async fn finish_inner(
    conn: &mut PgConnection,
    run: &RunRow,
    status: Status,
    reason: Option<&str>,
    now: DateTime<Utc>,
    fresh_skip: bool,
) -> Result<Value, NewsError> {
    if !status.is_terminal() {
        return Err(NewsError::internal("wrap::finish needs a terminal status"));
    }
    if status == Status::Cancelled {
        drop_unsent(conn, run.run_id, now).await?;
    }
    let steps: Vec<Value> = sqlx::query(
        "select seq, step, status, output from run_steps where run_id = $1 order by seq",
    )
    .bind(run.run_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(map_err)?
    .iter()
    .map(|r| {
        json!({"seq": r.get::<i16, _>("seq"), "step": r.get::<String, _>("step"),
               "status": r.get::<String, _>("status")})
    })
    .collect();
    // Counts the steps report in their output's `wrap` object, summed by code, plus jobs by state.
    let mut counts: Map<String, Value> = Map::new();
    let outputs: Vec<Option<Value>> =
        sqlx::query_scalar("select output -> 'wrap' from run_steps where run_id = $1")
            .bind(run.run_id)
            .fetch_all(&mut *conn)
            .await
            .map_err(map_err)?;
    for o in outputs.into_iter().flatten() {
        if let Value::Object(m) = o {
            for (k, v) in m {
                if let Some(n) = v.as_i64().filter(|n| *n >= 0) {
                    let cur = counts.get(&k).and_then(Value::as_i64).unwrap_or(0);
                    counts.insert(k, json!(cur + n));
                }
            }
        }
    }
    let jobs: Vec<(String, i64)> = sqlx::query_as(
        "select state, count(*) from jobs where run_id = $1 group by state order by state",
    )
    .bind(run.run_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(map_err)?;
    for (state, n) in jobs {
        counts.insert(format!("jobs_{state}"), json!(n));
    }
    let approvals: Vec<Value> = sqlx::query(
        "select gate, round, decision, decided_at from approvals where run_id = $1 order by decided_at, gate",
    )
    .bind(run.run_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(map_err)?
    .iter()
    .map(|r| {
        json!({"gate": r.get::<String, _>("gate"), "round": r.get::<i16, _>("round"),
               "decision": r.get::<String, _>("decision"),
               "decided_at": r.get::<DateTime<Utc>, _>("decided_at"), "origin": "board"})
    })
    .collect();
    let unresolved: Vec<Value> = sqlx::query(
        "select page_id, case when unknown_since is not null then 'sent, no answer' else 'not sent' end as reason
           from publish_outbox where run_id = $1 and sent_at is null order by outbox_id",
    )
    .bind(run.run_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(map_err)?
    .iter()
    .map(|r| json!({"page_id": r.get::<String, _>("page_id"), "reason": r.get::<String, _>("reason")}))
    .collect();

    let name = presets::preset(&run.preset).map_or(run.preset.as_str(), |p| p.name);
    let mut text = format!("{name} {} {}", run.short_id, status.as_str());
    if let Some(r) = reason {
        text.push_str(&format!(": {r}"));
    }
    let done = steps.iter().filter(|s| s["status"] == "done").count();
    text.push_str(&format!(". Steps done {done} of {}.", steps.len()));
    let mut figures: Vec<String> = counts
        .iter()
        .filter_map(|(k, v)| Some(format!("{k} {}", v.as_i64()?)))
        .collect();
    figures.sort();
    if !figures.is_empty() {
        text.push_str(&format!(" {}.", figures.join(", ")));
    }
    if !unresolved.is_empty() {
        text.push_str(&format!(" {} publishes unresolved.", unresolved.len()));
    }

    let wrap = json!({
        "run_id": run.run_id,
        "short_id": run.short_id,
        "preset": run.preset,
        "status": status.as_str(),
        "origin": run.origin,
        "schedule_key": run.schedule_key,
        "settings_version": run.settings_version,
        "queued_at": run.queued_at,
        "started_at": run.started_at,
        "finished_at": now,
        "reason": reason,
        "steps": steps,
        "counts": counts,
        "approvals": approvals,
        "tokens": Value::Null,
        "unresolved_publishes": unresolved,
        "text": text,
    });
    let done = sqlx::query(
        "update runs set status = $2, summary = $3, finished_at = $4,
                stop_mode = null, paused_reason = null, resume_at = null
          where run_id = $1 and (status <> all('{succeeded,failed,cancelled,skipped}') or ($5 and status = 'skipped'))",
    )
    .bind(run.run_id)
    .bind(status.as_str())
    .bind(&wrap)
    .bind(now)
    .bind(fresh_skip)
    .execute(&mut *conn)
    .await
    .map_err(map_err)?;
    if done.rows_affected() != 1 {
        return Err(NewsError::precondition(format!(
            "run {} has already finished",
            run.short_id
        )));
    }
    Ok(wrap)
}
