//! A row of `runs`, and finding one by id or short id.

use crate::state::Status;
use chrono::{DateTime, Utc};
use news_core::NewsError;
use news_store::map_err;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

pub const RUN_COLUMNS: &str = "run_id, short_id, preset, args, heavy, origin, schedule_key, slot_at, settings_version, \
    status, lineup_rank, queued_at, start_by, stop_mode, paused_reason, resume_at, current_step, config, budget, \
    created_at, started_at, last_seen_at, finished_at, summary";

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct RunRow {
    pub run_id: Uuid,
    pub short_id: String,
    pub preset: String,
    pub args: Value,
    pub heavy: bool,
    pub origin: String,
    pub schedule_key: Option<String>,
    pub slot_at: Option<DateTime<Utc>>,
    pub settings_version: i32,
    pub status: String,
    pub lineup_rank: i16,
    pub queued_at: Option<DateTime<Utc>>,
    pub start_by: Option<DateTime<Utc>>,
    pub stop_mode: Option<String>,
    pub paused_reason: Option<String>,
    pub resume_at: Option<DateTime<Utc>>,
    pub current_step: Option<String>,
    pub config: Value,
    pub budget: Value,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub summary: Option<Value>,
}

impl RunRow {
    pub fn status(&self) -> Status {
        Status::parse(&self.status).unwrap_or(Status::Failed)
    }
}

/// Load a run. `lock`: `for update` (every control action locks the run row first, ADR 0008 §4.2).
pub async fn load(conn: &mut PgConnection, run_id: Uuid, lock: bool) -> Result<RunRow, NewsError> {
    let sql = format!(
        "select {RUN_COLUMNS} from runs where run_id = $1{}",
        if lock { " for update" } else { "" }
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(run_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_err)?
        .ok_or_else(|| NewsError::not_found(format!("run {run_id} not found")))
}

/// A run id from a uuid or a short id (`r_…`).
pub async fn resolve(conn: &mut PgConnection, run_ref: &str) -> Result<Uuid, NewsError> {
    if let Ok(id) = Uuid::parse_str(run_ref) {
        return Ok(id);
    }
    sqlx::query_scalar("select run_id from runs where short_id = $1")
        .bind(run_ref)
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_err)?
        .ok_or_else(|| NewsError::not_found(format!("no run {run_ref:?}")))
}

/// A fresh short id: `r_` and eight lower-case hex digits.
pub fn new_short_id() -> String {
    format!("r_{}", &Uuid::new_v4().simple().to_string()[..8])
}
