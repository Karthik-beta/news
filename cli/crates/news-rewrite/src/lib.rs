//! Rewrite, revision handling (Bureau Design §3.11, §8.9): keep published pages correct when sources
//! revise or restate data.
//!
//! * [`find`]: published pages whose data points a later vintage supersedes, and the derived points built
//!   from them, recomputed with the Ledger's arithmetic.
//! * [`classify`] and [`plan`]: value only, restatement or direction flip per page; the 100-page `bulk`
//!   threshold and Edition's budget (step 4) and publish (step 10) selection.
//! * [`patch`] and [`apply`]: the re-render by code, on `data.json`, the stored Copy and `page.json`,
//!   with the "restated" note for a restatement. No AI, and no number typed by code or a model.
//! * [`update`]: the A1 update job for a flip, and `news update <page>`'s output.
//!
//! Rendering (Plate), `news check` and publishing (Press) are the steps after this crate's: Followup's
//! `rerender` and `publish` steps call them. The `page.revise` job does the by-code half of a revision for
//! one page (`args.page_id`): it finds the page's revised points, classifies the change and writes the
//! revised `data.json`, Copy and `page.json` into the story's page folder ([`apply::apply`]), ready for
//! render and check. It needs the Bureau service's context ([`install`]): a database pool and the work
//! directory. A direction flip is not its job (the A1 update is): it fails as `invalid_data`.

use news_core::job::{ErrorClass, Handler, JobFailure, JobFuture, JobInput};
use news_proof::work::Work;
use news_store::PgPool;
use std::sync::OnceLock;

pub mod apply;
pub mod classify;
pub mod find;
pub mod patch;
pub mod plan;
pub mod update;

/// The issue that builds this subsystem.
pub const ISSUE: &str = "#20";

/// Job kinds this subsystem handles (see `news_dispatch::KINDS`).
pub const KINDS: &[&str] = &["page.revise"];

/// What `page.revise` needs from the Bureau service.
pub struct Context {
    pub db: PgPool,
    /// `<root>/work`.
    pub work: Work,
}

static CONTEXT: OnceLock<Context> = OnceLock::new();

/// Install the context for [`handle`]; `false` if one is already installed.
pub fn install(c: Context) -> bool {
    CONTEXT.set(c).is_ok()
}

fn invalid(m: impl Into<String>) -> JobFailure {
    JobFailure::new(ErrorClass::InvalidData, m)
}

/// Revise one page by code. `Ok` says what was written; a page with no revised data is `revised: false`.
pub async fn revise(
    db: &PgPool,
    work: &Work,
    page_id: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<serde_json::Value, JobFailure> {
    let to_failure = |e: news_core::NewsError| {
        if e.retryable {
            JobFailure::new(ErrorClass::Transient, e.message)
        } else {
            invalid(e.message)
        }
    };
    let Some(change) = find::for_page(db, page_id).await.map_err(to_failure)? else {
        return Ok(serde_json::json!({"page_id": page_id, "revised": false}));
    };
    let planned = plan::plan(vec![change]).remove(0);
    if !planned.by_code() {
        return Err(invalid(format!(
            "page {page_id} is a direction flip: it needs the A1 update, not a re-render"
        )));
    }
    let story = planned
        .change
        .story_id
        .ok_or_else(|| invalid(format!("page {page_id} has no story")))?;
    apply::store_derived(db, &planned.change)
        .await
        .map_err(to_failure)?;
    let first: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("select published_at from pages where page_id = $1")
            .bind(page_id)
            .fetch_one(db)
            .await
            .map_err(|e| to_failure(news_store::map_err(e)))?;
    let done = apply::apply(
        work,
        &story.to_string(),
        &planned,
        first.unwrap_or(now),
        now,
    )
    .map_err(to_failure)?;
    serde_json::to_value(&done).map_err(|e| invalid(e.to_string()))
}

/// Run one job.
pub fn handle(input: JobInput) -> JobFuture {
    Box::pin(async move {
        let Some(c) = CONTEXT.get() else {
            return Err(JobFailure::new(
                ErrorClass::Bug,
                format!(
                    "handler for {} needs the Bureau service context ({ISSUE})",
                    input.kind
                ),
            ));
        };
        let page = input.args["page_id"]
            .as_str()
            .map(str::to_owned)
            .or(input.item_ref.clone())
            .ok_or_else(|| invalid("page.revise needs args.page_id"))?;
        revise(&c.db, &c.work, &page, chrono::Utc::now()).await
    })
}

pub const HANDLER: Handler = handle;
