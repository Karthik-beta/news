//! What Ghost reads and writes besides the call itself: the step's model and the run's token ceiling
//! (Standing), the run's tokens so far, and one `ai_calls` row per call.

use chrono::{DateTime, Utc};
use news_core::job::{ErrorClass, JobFailure};
use serde::Serialize;
use sqlx::PgPool;
use std::future::Future;
use std::pin::Pin;
use uuid::Uuid;

pub type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// What a call may use, read just before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    /// `claude -p --model` for this step.
    pub model: String,
    /// `quota.ai_run_tokens`; `None` is no ceiling.
    pub ceiling: Option<u64>,
    /// Input-equivalent tokens the run's calls have used so far.
    pub spent: u64,
}

/// One `claude -p` call: the columns of ClickHouse `news.ai_calls` (`infra/clickhouse/003_ticker.sql`)
/// plus the input-equivalent total and the files Board shows.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AiCall {
    pub ts: DateTime<Utc>,
    pub run_id: Option<Uuid>,
    pub job_id: i64,
    pub step: String,
    pub model: String,
    pub prompt_version: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cache_read: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    pub turns: u32,
    pub duration_ms: u64,
    pub subtype: String,
    /// `valid`, `invalid:<rule>`, or empty when nothing was validated.
    pub validation: String,
    pub repair_round: u8,
    pub est_cost_usd: f64,
    /// Input-equivalent tokens (`news_rota::tokens`), what the run ceiling counts.
    pub tokens: u64,
    pub input_file: String,
    pub output_file: Option<String>,
}

pub trait Store: Send + Sync {
    fn limits<'a>(
        &'a self,
        run: Option<Uuid>,
        step: &'a str,
    ) -> BoxFut<'a, Result<Limits, JobFailure>>;
    /// Record a call. Never fails the job: a call that happened is recorded as well as possible.
    fn record<'a>(&'a self, call: &'a AiCall) -> BoxFut<'a, ()>;
}

/// The Standing key holding the model for a step (its `doc` in news-standing names the steps).
pub fn model_key(step: &str) -> &'static str {
    match step {
        "A1" | "A2" | "A11" | "A12" => "models.main",
        "A8" => "models.curator",
        _ => "models.light",
    }
}

/// PostgreSQL `ai_calls` (the run ceiling and Board's records), mirrored to ClickHouse `news.ai_calls`
/// (the quota guard's Bureau usage and dashboards) when telemetry is on.
#[derive(Debug, Clone)]
pub struct PgStore {
    pub db: PgPool,
    pub clickhouse: Option<news_telemetry::ClickHouse>,
}

fn transient(what: &str, e: impl std::fmt::Display) -> JobFailure {
    JobFailure::new(ErrorClass::Transient, format!("{what}: {e}"))
}

impl Store for PgStore {
    fn limits<'a>(
        &'a self,
        run: Option<Uuid>,
        step: &'a str,
    ) -> BoxFut<'a, Result<Limits, JobFailure>> {
        Box::pin(async move {
            let s = news_standing::current(&self.db)
                .await
                .map_err(|e| transient("cannot read Standing", e))?;
            let model = s
                .text(model_key(step))
                .filter(|m| news_standing::is_model_name(m))
                .ok_or_else(|| {
                    JobFailure::new(
                        ErrorClass::Bug,
                        format!("no valid model in {}", model_key(step)),
                    )
                })?;
            let spent: i64 = match run {
                Some(r) => sqlx::query_scalar(
                    "select coalesce(sum(tokens), 0)::bigint from ai_calls where run_id = $1",
                )
                .bind(r)
                .fetch_one(&self.db)
                .await
                .map_err(|e| transient("cannot read the run's tokens", e))?,
                None => 0,
            };
            Ok(Limits {
                model,
                ceiling: s
                    .opt_int("quota.ai_run_tokens")
                    .and_then(|c| u64::try_from(c).ok()),
                spent: u64::try_from(spent).unwrap_or(0),
            })
        })
    }

    fn record<'a>(&'a self, c: &'a AiCall) -> BoxFut<'a, ()> {
        Box::pin(async move {
            let big = |n: u64| i64::try_from(n).unwrap_or(i64::MAX);
            let r = sqlx::query(
                "insert into ai_calls (at, run_id, job_id, step, model, prompt_version, tokens_in,
                    tokens_out, cache_read, cache_write_5m, cache_write_1h, tokens, turns, duration_ms,
                    subtype, validation, repair_round, est_cost_usd, input_file, output_file)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17,
                    $18, $19, $20)",
            )
            .bind(c.ts)
            .bind(c.run_id)
            .bind(c.job_id)
            .bind(&c.step)
            .bind(&c.model)
            .bind(&c.prompt_version)
            .bind(big(c.tokens_in))
            .bind(big(c.tokens_out))
            .bind(big(c.cache_read))
            .bind(big(c.cache_write_5m))
            .bind(big(c.cache_write_1h))
            .bind(big(c.tokens))
            .bind(i32::try_from(c.turns).unwrap_or(i32::MAX))
            .bind(big(c.duration_ms))
            .bind(&c.subtype)
            .bind(&c.validation)
            .bind(i16::from(c.repair_round))
            .bind(c.est_cost_usd)
            .bind(&c.input_file)
            .bind(&c.output_file)
            .execute(&self.db)
            .await;
            if let Err(e) = r {
                tracing::warn!(job_id = c.job_id, error = %e, "ai_calls row not written");
            }
            if let Some(ch) = self.clickhouse.clone() {
                let row = c.clone();
                let sent = tokio::task::spawn_blocking(move || {
                    ch.insert_rows("news.ai_calls", std::slice::from_ref(&row))
                })
                .await;
                if let Ok(Err(e)) = sent {
                    tracing::debug!(error = %e, "ai_calls row not mirrored to ClickHouse");
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_take_their_model_role() {
        for s in ["A1", "A2", "A11", "A12"] {
            assert_eq!(model_key(s), "models.main");
        }
        assert_eq!(model_key("A8"), "models.curator");
        for s in ["A3", "A4", "A5", "A6", "A7", "A9", "A10"] {
            assert_eq!(model_key(s), "models.light");
        }
        // Every key Ghost reads is registered.
        for k in [
            "models.main",
            "models.light",
            "models.curator",
            "quota.ai_run_tokens",
        ] {
            assert!(news_standing::decl(k).is_some(), "{k}");
        }
    }
}
