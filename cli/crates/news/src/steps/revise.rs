//! Re-rendering a published page after its data revised (Bureau §3.11): the code path for value-only
//! changes and restatements, and the finish of a direction flip once the A1 update stored its new Copy.
//! Used by Followup's `rerender` and `publish` and by Edition's publish step (step 10).

use super::pipeline::{self, Job, Rendered, job, work};
use super::{Ctx, db_err, step_err};
use chrono::{DateTime, Utc};
use news_chief::driver::{StepCx, StepError};
use news_press::register::Registered;
use news_rewrite::apply;
use news_rewrite::plan::PagePlan;
use news_rewrite::update;
use serde_json::{Value, json};

async fn first_published(cx: &StepCx, page_id: &str) -> Result<DateTime<Utc>, StepError> {
    let t: Option<DateTime<Utc>> =
        sqlx::query_scalar("select published_at from pages where page_id = $1")
            .bind(page_id)
            .fetch_one(&cx.db)
            .await
            .map_err(db_err)?;
    Ok(t.unwrap_or(cx.now))
}

/// Re-render each page by code (no tokens), check it and register it as the next version. A page that
/// cannot be revised (its page folder is gone, the check blocks it) is dead, not the run.
pub async fn by_code(
    cx: &StepCx,
    ctx: &Ctx,
    plans: &[PagePlan],
) -> Result<(Vec<Registered>, Vec<Value>), StepError> {
    finish(cx, ctx, plans, false).await
}

/// Finish the direction flips whose A1 update completed: patch the page's data and version around the
/// Copy the writer stored, render, check, register. A flip whose update has no result is dead.
pub async fn flips(
    cx: &StepCx,
    ctx: &Ctx,
    plans: &[PagePlan],
) -> Result<(Vec<Registered>, Vec<Value>), StepError> {
    finish(cx, ctx, plans, true).await
}

async fn finish(
    cx: &StepCx,
    ctx: &Ctx,
    plans: &[PagePlan],
    flip: bool,
) -> Result<(Vec<Registered>, Vec<Value>), StepError> {
    let w = work(ctx);
    let (mut ok, mut dead) = (Vec::new(), Vec::new());
    for p in plans.iter().filter(|p| p.by_code() != flip) {
        let page = &p.change.page_id;
        let Some(story) = p.change.story_id else {
            dead.push(json!({"page_id": page, "error": "the page has no story"}));
            continue;
        };
        if flip {
            let key = update::update_job(p, &story.to_string())
                .unique_key
                .unwrap_or_default();
            match job(cx, &key).await? {
                Some(Job::Completed(_)) => {}
                other => {
                    dead.push(json!({"page_id": page, "error": format!("the update call has no result ({other:?})")}));
                    continue;
                }
            }
        }
        let first = first_published(cx, page).await?;
        let applied = if flip {
            apply::apply_flip(&w, &story.to_string(), p, first, cx.now)
        } else {
            apply::apply(&w, &story.to_string(), p, first, cx.now)
        };
        if let Err(e) = applied {
            if e.retryable {
                return Err(step_err(e));
            }
            dead.push(json!({"page_id": page, "error": e.message}));
            continue;
        }
        match pipeline::render_one(cx, ctx, story).await? {
            Rendered::Registered(r) => ok.push(r),
            Rendered::Failed(f) => dead.push(json!({"page_id": page, "failures": f})),
        }
    }
    Ok((ok, dead))
}
