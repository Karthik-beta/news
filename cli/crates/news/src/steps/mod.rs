//! Preset step bodies (Bureau §8.3–§8.13), bound to Chief by [`register`]. Each step is idempotent and
//! keyed (ADR 0008 §6.4): a step re-entered after a restart finds its own earlier work in PostgreSQL and
//! goes on from there.
//!
//! Built here (#20): Edition's `review` stop (`review_plan`), the Frontpage steps, and Followup's find,
//! classify and bulk-approval steps. Every other step of every preset is still [`NotBuilt`]
//! (`news_chief::driver::NotBuilt`), so such a run ends `failed` at that step and says so.

pub mod edition;
pub mod followup;
pub mod frontpage;

use chrono::{DateTime, Utc};
use news_chief::BoxFut;
use news_chief::driver::{Handlers, StepCx, StepError, StepHandler, StepOutcome};
use news_core::job::ErrorClass;
use news_core::{ErrorCode, NewsError, NewsResult};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

/// Publishes an approved home curation (`news home publish`, G tier): production sends it to the write
/// API; tests send it to a fake. The G-tier check has already passed when this is called.
pub trait HomePublisher: Send + Sync {
    fn publish<'a>(
        &'a self,
        db: &'a news_store::PgPool,
        draft_id: i64,
        now: DateTime<Utc>,
    ) -> BoxFut<'a, NewsResult<Value>>;
}

/// The write API over HTTPS: the origin is `NEWS_API_ORIGIN`, the key is read only here, at the moment of
/// sending (`news_press::api::write_key`).
pub struct HttpHome;

impl HomePublisher for HttpHome {
    fn publish<'a>(
        &'a self,
        db: &'a news_store::PgPool,
        draft_id: i64,
        now: DateTime<Utc>,
    ) -> BoxFut<'a, NewsResult<Value>> {
        Box::pin(async move {
            let origin = std::env::var(news_press::api::ORIGIN_VAR).map_err(|_| {
                NewsError::precondition(format!("{} is not set", news_press::api::ORIGIN_VAR))
                    .with_hint("the write API's origin, for example https://api.example.com")
            })?;
            let api = news_home::api::HttpWriteApi::new(&origin)?;
            news_home::api::publish(
                db,
                draft_id,
                now,
                &api,
                news_press::api::write_key,
                news_home::api::Retry::default(),
            )
            .await
        })
    }
}

/// What the step bodies need from the service.
#[derive(Clone)]
pub struct Ctx {
    /// The repository root: AI job paths are relative to it, and Ghost confines them to `work/`.
    pub root: PathBuf,
    /// The home page preview server (`NEWS_PREVIEW_URL`).
    pub preview_base: String,
    pub home: Arc<dyn HomePublisher>,
}

/// Bind the steps built so far.
pub fn register(h: Handlers, ctx: Ctx) -> Handlers {
    let ctx = Arc::new(ctx);
    let h = edition::register(h);
    let h = followup::register(h);
    frontpage::register(h, ctx)
}

/// Every step that needs the service context fails with `message` (no repository root), so a run says
/// why instead of ending at a step that is "not built".
struct Unset(String);

impl StepHandler for Unset {
    fn run<'a>(&'a self, _: &'a mut StepCx) -> BoxFut<'a, Result<StepOutcome, StepError>> {
        Box::pin(async move {
            Err(StepError::Fail {
                class: ErrorClass::InvalidData,
                message: self.0.clone(),
            })
        })
    }
}

/// Bind the steps that need [`Ctx`] to fail with `message`; the steps that do not are bound as usual.
pub fn register_unset(h: Handlers, message: String) -> Handlers {
    let h = edition::register(h);
    let h = followup::register(h);
    frontpage::register_unset(h, Arc::new(Unset(message)))
}

/// A retryable error (a busy database) is re-entered; anything else fails the run, because re-entering
/// would meet it again. A missing thing is the run's data, not a bug; only `internal` is a bug.
pub(crate) fn step_err(e: NewsError) -> StepError {
    match e.code {
        _ if e.retryable => StepError::Retry(e.message),
        ErrorCode::Internal => StepError::Fail {
            class: ErrorClass::Bug,
            message: e.message,
        },
        _ => StepError::Fail {
            class: ErrorClass::InvalidData,
            message: e.message,
        },
    }
}

pub(crate) fn db_err(e: sqlx::Error) -> StepError {
    step_err(news_store::map_err(e))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_busy_database_is_retried_and_a_bug_is_not_the_runs_data() {
        let busy = NewsError::upstream("db busy");
        assert!(busy.retryable);
        assert!(matches!(step_err(busy), StepError::Retry(_)));
        let bug = step_err(NewsError::internal("x"));
        assert!(matches!(
            bug,
            StepError::Fail {
                class: ErrorClass::Bug,
                ..
            }
        ));
        let data = step_err(NewsError::validation("x"));
        assert!(matches!(
            data,
            StepError::Fail {
                class: ErrorClass::InvalidData,
                ..
            }
        ));
    }
}
