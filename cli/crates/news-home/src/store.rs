//! `home_curations` (TD "Data model"): drafts, their check results and the published versions.
//! A draft is `draft` until a check passes (`checked`), `approved` once its approval time is fixed for
//! publishing, then `published` (with the API's version) or `superseded` by a later one.

use crate::check::{Report, RuleResult};
use crate::draft::{Draft, Slots, ts};
use chrono::{DateTime, Utc};
use news_core::{NewsError, NewsResult};
use news_store::{PgPool, map_err};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Stored {
    pub draft_id: i64,
    pub status: String,
    pub version: Option<i32>,
    pub draft: Draft,
    pub check_results: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub approved_at: Option<DateTime<Utc>>,
    pub published_at: Option<DateTime<Utc>>,
}

impl Stored {
    /// The rule results of the last check, the ones a published body carries.
    pub fn results(&self) -> Vec<RuleResult> {
        self.check_results
            .as_ref()
            .and_then(|v| serde_json::from_value(v["results"].clone()).ok())
            .unwrap_or_default()
    }

    pub fn passed(&self) -> bool {
        self.check_results
            .as_ref()
            .and_then(|v| v["passed"].as_bool())
            == Some(true)
    }
}

#[derive(sqlx::FromRow)]
struct Row {
    draft_id: i64,
    status: String,
    version: Option<i32>,
    slots: Value,
    curator_model: Option<String>,
    rationale: Option<String>,
    check_results: Option<Value>,
    created_at: DateTime<Utc>,
    approved_at: Option<DateTime<Utc>>,
    published_at: Option<DateTime<Utc>>,
}

macro_rules! select_curations {
    ($($tail:literal),*) => {
        concat!(
            "select draft_id, status, version, slots, curator_model, rationale, check_results,
                    created_at, approved_at, published_at
               from home_curations ",
            $($tail),*
        )
    };
}

impl TryFrom<Row> for Stored {
    type Error = NewsError;
    fn try_from(r: Row) -> NewsResult<Self> {
        let slots: Slots = serde_json::from_value(r.slots).map_err(|e| {
            NewsError::internal(format!("draft {} has unreadable slots: {e}", r.draft_id))
        })?;
        Ok(Self {
            draft_id: r.draft_id,
            status: r.status,
            version: r.version,
            draft: Draft {
                curator_model: r.curator_model.unwrap_or_default(),
                slots,
                rationale: r.rationale.and_then(|t| serde_json::from_str(&t).ok()),
            },
            check_results: r.check_results,
            created_at: r.created_at,
            approved_at: r.approved_at,
            published_at: r.published_at,
        })
    }
}

/// Store a draft. The same draft saved again while it is still open returns the same id.
pub async fn save(db: &PgPool, d: &Draft, run_id: Option<Uuid>) -> NewsResult<(i64, bool)> {
    let slots = serde_json::to_value(&d.slots).map_err(|e| NewsError::internal(e.to_string()))?;
    let rationale = d
        .rationale
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| NewsError::internal(e.to_string()))?;
    let open: Option<i64> = sqlx::query_scalar(
        "select draft_id from home_curations
          where status in ('draft', 'checked') and slots = $1 and curator_model = $2
            and rationale is not distinct from $3
          order by draft_id limit 1",
    )
    .bind(&slots)
    .bind(&d.curator_model)
    .bind(&rationale)
    .fetch_optional(db)
    .await
    .map_err(map_err)?;
    if let Some(id) = open {
        return Ok((id, false));
    }
    let id = sqlx::query_scalar(
        "insert into home_curations (run_id, slots, figures, rationale, curator_model)
         values ($1, $2, $3, $4, $5) returning draft_id",
    )
    .bind(run_id)
    .bind(&slots)
    .bind(d.figure_ids())
    .bind(&rationale)
    .bind(&d.curator_model)
    .fetch_one(db)
    .await
    .map_err(map_err)?;
    Ok((id, true))
}

pub async fn get(db: &PgPool, draft_id: i64) -> NewsResult<Stored> {
    sqlx::query_as::<_, Row>(select_curations!("where draft_id = $1"))
        .bind(draft_id)
        .fetch_optional(db)
        .await
        .map_err(map_err)?
        .ok_or_else(|| NewsError::not_found(format!("no draft curation {draft_id}")))?
        .try_into()
}

/// What a check stores: `{passed, checked_at, results, failures}`.
pub fn check_json(report: &Report, now: DateTime<Utc>) -> Value {
    json!({"passed": report.pass, "checked_at": ts(now), "results": report.results, "failures": report.failures})
}

/// Keep a check's outcome. A passing draft becomes `checked`, a failing one goes back to `draft`;
/// an approved, published or closed draft keeps its status (its results do not change either).
pub async fn store_check(
    db: &PgPool,
    draft_id: i64,
    report: &Report,
    now: DateTime<Utc>,
) -> NewsResult<()> {
    sqlx::query(
        "update home_curations
            set check_results = $2, status = case when $3 then 'checked' else 'draft' end
          where draft_id = $1 and status in ('draft', 'checked')",
    )
    .bind(draft_id)
    .bind(check_json(report, now))
    .bind(report.pass)
    .execute(db)
    .await
    .map_err(map_err)?;
    Ok(())
}

/// Fix the approval time and the results a publish will carry, once: a resend after a crash reads
/// them back, so its body, and with it the idempotency key, is the same.
pub async fn approve(
    db: &PgPool,
    draft_id: i64,
    approved_at: DateTime<Utc>,
    report: &Report,
    now: DateTime<Utc>,
) -> NewsResult<()> {
    sqlx::query(
        "update home_curations
            set check_results = $3, approved_at = $2, status = 'approved'
          where draft_id = $1 and status in ('draft', 'checked') and approved_at is null",
    )
    .bind(draft_id)
    .bind(approved_at)
    .bind(check_json(report, now))
    .execute(db)
    .await
    .map_err(map_err)?;
    Ok(())
}

/// The write API has the curation live as `version`: this draft is published and the one it replaced
/// is superseded.
pub async fn mark_published(
    db: &PgPool,
    draft_id: i64,
    version: i32,
    now: DateTime<Utc>,
) -> NewsResult<()> {
    let mut tx = db.begin().await.map_err(map_err)?;
    // Only an approved draft goes live; one in any other state (already published, superseded) is left
    // alone, and so is the live curation.
    let done = sqlx::query("update home_curations set status = 'published', version = $2, published_at = $3 where draft_id = $1 and status = 'approved'")
        .bind(draft_id)
        .bind(version)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;
    if done.rows_affected() == 1 {
        sqlx::query("update home_curations set status = 'superseded' where status = 'published' and draft_id <> $1")
            .bind(draft_id)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
    }
    tx.commit().await.map_err(map_err)
}

/// The newest published curation, if any.
pub async fn live(db: &PgPool) -> NewsResult<Option<Stored>> {
    sqlx::query_as::<_, Row>(select_curations!(
        "where status = 'published' order by version desc nulls last, draft_id desc limit 1"
    ))
    .fetch_optional(db)
    .await
    .map_err(map_err)?
    .map(Stored::try_from)
    .transpose()
}
