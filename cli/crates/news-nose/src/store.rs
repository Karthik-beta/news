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
    pub status: String,
    pub current_step: Option<String>,
    pub settings_version: i32,
    /// When the run started (or was created, if it has not).
    pub started: DateTime<Utc>,
}

pub async fn run_info(db: &PgPool, run_id: Uuid) -> Result<Option<RunInfo>, sqlx::Error> {
    let row = sqlx::query(
        "select args, status, current_step, settings_version, coalesce(started_at, created_at) as started from runs where run_id = $1",
    )
    .bind(run_id)
    .fetch_optional(db)
    .await?;
    row.map(|r| {
        Ok(RunInfo {
            args: r.try_get("args")?,
            status: r.try_get("status")?,
            current_step: r.try_get("current_step")?,
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

/// Why `save` did not store.
#[derive(Debug)]
pub enum SaveError {
    /// The Budget can no longer change: the run has moved on or an approval covers it.
    Locked(String),
    Db(sqlx::Error),
}

impl From<sqlx::Error> for SaveError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e)
    }
}

/// A Budget row: story, rank, tier, bucket, reason, action, and `adjusted` (stored to 2 places).
type PlanRow = (i64, i32, i16, String, String, String, f64);

fn plan_rows(plan: &Plan) -> Vec<PlanRow> {
    let mut rows: Vec<PlanRow> = plan
        .picks
        .iter()
        .map(|p| {
            (
                p.story_id,
                p.rank as i32,
                i16::from(p.tier),
                p.bucket.clone(),
                p.reason.clone(),
                p.action.as_str().to_owned(),
                p.adjusted,
            )
        })
        .collect();
    rows.sort_by_key(|r| r.0);
    rows
}

/// The stored rows are the plan's: `adjusted` is stored rounded to 2 places, so it matches within half a cent.
fn same_budget(stored: &[PlanRow], plan: &[PlanRow]) -> bool {
    stored.len() == plan.len()
        && stored.iter().zip(plan).all(|(a, b)| {
            (&a.0, &a.1, &a.2, &a.3, &a.4, &a.5) == (&b.0, &b.1, &b.2, &b.3, &b.4, &b.5)
                && (a.6 - b.6).abs() <= 0.005 + 1e-9
        })
}

/// Store the run's scores and its Budget in one transaction. A score row is keyed by story, hour and
/// scorer version, so a re-run inserts nothing new. The run's `run_plans` rows are written once: the
/// same rows again are a no-op, and different ones replace them only while the run is `queued` or
/// `running` at its `plan` step with no approval and no later step started, because an approval's
/// hash covers the Budget (Bureau §5.5). Returns true when new score rows were inserted (log them).
pub async fn save(
    db: &PgPool,
    run_id: Uuid,
    computed_at: DateTime<Utc>,
    scores: &[(i64, &Score)],
    plan: &Plan,
) -> Result<bool, SaveError> {
    let mut tx = db.begin().await?;
    let run: Option<(String, Option<String>)> =
        sqlx::query_as("select status, current_step from runs where run_id = $1 for update")
            .bind(run_id)
            .fetch_optional(&mut *tx)
            .await?;
    let (status, step) = run.ok_or_else(|| SaveError::Locked(format!("no run {run_id}")))?;
    let mut inserted = 0;
    for (story_id, s) in scores {
        inserted += sqlx::query(
            "insert into scores (story_id, computed_at, scorer_version, components, total, explain)
             values ($1, $2, $3, $4, round($5::numeric, 2), $6)
             on conflict (story_id, computed_at, scorer_version) do nothing",
        )
        .bind(story_id)
        .bind(computed_at)
        .bind(&s.scorer_version)
        .bind(sqlx::types::Json(&s.components))
        .bind(s.total)
        .bind(&s.pitch)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    }
    let stored: Vec<PlanRow> = sqlx::query_as(
        "select story_id, rank, tier, bucket, reason, action, adjusted::float8 from run_plans
          where run_id = $1 order by story_id",
    )
    .bind(run_id)
    .fetch_all(&mut *tx)
    .await?;
    if !same_budget(&stored, &plan_rows(plan)) {
        let moved_on: bool = sqlx::query_scalar(
            "select exists (select 1 from approvals where run_id = $1)
                 or exists (select 1 from run_steps l
                             where l.run_id = $1 and l.status <> 'pending'
                               and l.seq > (select seq from run_steps where run_id = $1 and step = 'plan'))",
        )
        .bind(run_id)
        .fetch_one(&mut *tx)
        .await?;
        if !matches!(status.as_str(), "queued" | "running")
            || step.as_deref().is_some_and(|s| s != "plan")
            || moved_on
        {
            return Err(SaveError::Locked(format!(
                "run {run_id} is {status} at step {}, past planning, or its plan is approved: \
                 the Budget cannot change",
                step.as_deref().unwrap_or("none")
            )));
        }
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
    }
    tx.commit().await?;
    Ok(inserted > 0)
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
           from scores where story_id = $1 order by computed_at desc, scorer_version desc limit 1",
    )
    .bind(story_id)
    .fetch_optional(db)
    .await
}
