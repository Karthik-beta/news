//! PostgreSQL for Nose and Desk: story inputs, coverage, `scores` and `run_plans`.

use crate::config::Shares;
use crate::desk::Plan;
use crate::score::Score;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Row};
use std::collections::BTreeMap;
use uuid::Uuid;

/// What `news plan` needs from `runs`.
#[derive(Debug, Clone)]
pub struct RunInfo {
    pub args: Value,
    pub settings_version: i32,
    /// When the run started (or was created, if it has not).
    pub started: DateTime<Utc>,
}

pub async fn run_info(db: &PgPool, run_id: Uuid) -> Result<Option<RunInfo>, sqlx::Error> {
    let row = sqlx::query(
        "select args, settings_version, coalesce(started_at, created_at) as started from runs where run_id = $1",
    )
    .bind(run_id)
    .fetch_optional(db)
    .await?;
    row.map(|r| {
        Ok(RunInfo {
            args: r.try_get("args")?,
            settings_version: r.try_get("settings_version")?,
            started: r.try_get("started")?,
        })
    })
    .transpose()
}

/// Open stories that have scoring inputs, by id.
pub async fn open_inputs(db: &PgPool) -> Result<Vec<(i64, Value)>, sqlx::Error> {
    sqlx::query_as(
        "select story_id, score_input from stories
          where status = 'open' and score_input is not null order by story_id",
    )
    .fetch_all(db)
    .await
}

/// Actual shares of the pages published in the 7 days before `as_of`, by the primary category,
/// primary region and page type of each page's story. A page with no story counts in the total only.
pub async fn coverage(db: &PgPool, as_of: DateTime<Utc>) -> Result<Shares, sqlx::Error> {
    type Buckets = (Option<String>, Option<String>, Option<String>);
    let rows: Vec<Buckets> = sqlx::query_as(
        "select s.score_input->'categories'->>0, s.score_input->'regions'->>0, s.score_input->'page'->>'type'
           from pages p left join stories s on s.story_id = p.story_id
          where p.status = 'published' and p.published_at >= $1 - interval '7 days' and p.published_at < $1",
    )
    .bind(as_of)
    .fetch_all(db)
    .await?;
    let total = rows.len() as f64;
    let share = |pick: fn(&Buckets) -> &Option<String>| {
        let mut counts: BTreeMap<String, u32> = BTreeMap::new();
        for k in rows.iter().filter_map(|r| pick(r).as_ref()) {
            *counts.entry(k.clone()).or_default() += 1;
        }
        counts
            .into_iter()
            .map(|(k, n)| (k, f64::from(n) / total))
            .collect()
    };
    Ok(Shares {
        category: share(|r| &r.0),
        region: share(|r| &r.1),
        page_type: share(|r| &r.2),
    })
}

/// Store the run's scores and its Budget in one transaction. A score row is keyed by story, hour and
/// scorer version, so a re-run inserts nothing new; the run's `run_plans` rows are replaced by the
/// same rows. Returns true when the run had no plan before (the first plan of the run).
pub async fn save(
    db: &PgPool,
    run_id: Uuid,
    computed_at: DateTime<Utc>,
    scores: &[(i64, &Score)],
    plan: &Plan,
) -> Result<bool, sqlx::Error> {
    let mut tx = db.begin().await?;
    for (story_id, s) in scores {
        sqlx::query(
            "insert into scores (story_id, computed_at, scorer_version, components, total, explain)
             values ($1, $2, $3, $4, round($5::numeric, 2), $6)
             on conflict (story_id, computed_at) do nothing",
        )
        .bind(story_id)
        .bind(computed_at)
        .bind(&s.scorer_version)
        .bind(sqlx::types::Json(&s.components))
        .bind(s.total)
        .bind(&s.pitch)
        .execute(&mut *tx)
        .await?;
    }
    let before: i64 = sqlx::query_scalar("select count(*) from run_plans where run_id = $1")
        .bind(run_id)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query("delete from run_plans where run_id = $1")
        .bind(run_id)
        .execute(&mut *tx)
        .await?;
    for p in &plan.picks {
        sqlx::query(
            "insert into run_plans (run_id, story_id, rank, tier, bucket, adjusted, reason, action)
             values ($1, $2, $3, $4, $5, round($6::numeric, 2), $7, $8)",
        )
        .bind(run_id)
        .bind(p.story_id)
        .bind(p.rank as i32)
        .bind(i16::from(p.tier))
        .bind(&p.bucket)
        .bind(p.adjusted)
        .bind(&p.reason)
        .bind(p.action.as_str())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(before == 0)
}

/// A stored score row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ScoreRow {
    pub story_id: i64,
    pub computed_at: DateTime<Utc>,
    pub scorer_version: String,
    pub components: Value,
    pub total: f64,
    pub explain: String,
}

/// The latest score of a story.
pub async fn latest_score(db: &PgPool, story_id: i64) -> Result<Option<ScoreRow>, sqlx::Error> {
    sqlx::query_as(
        "select story_id, computed_at, scorer_version, components, total::float8 as total, explain
           from scores where story_id = $1 order by computed_at desc limit 1",
    )
    .bind(story_id)
    .fetch_optional(db)
    .await
}
