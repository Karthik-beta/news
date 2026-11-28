//! Edition (Bureau §8.3): the main production run. Steps 1 to 4 and 6 to 11, with step 5's optional
//! plan approval; step 12 is Chief's Wrap.
//!
//! Every step works from PostgreSQL and the work folder (see [`super::pipeline`]), so a restart or a
//! wait for AI jobs re-enters it and it goes on from there. Per-item failures never stop the run: a
//! story that cannot be briefed, a writer call with no result, a page Proof blocks twice, each is
//! counted in the Wrap and left for Salvage. The run stops early for preflight blockers only.
//!
//! With `review_plan` off the step is skipped, and the Start (or the schedule that fired) covers the
//! publish step (`Cover::GateOrStart("plan")`). With it on, the run stops at `plan`; the approval then
//! covers exactly the items in the Budget the plan step set, because the gate's subject carries them.

use super::pipeline::{self, Ph, fail};
use super::{Ctx, Unset, db_err, plan, revise, step_err};
use news_chief::BoxFut;
use news_chief::driver::{Handlers, StepCx, StepError, StepHandler, StepOutcome};
use news_chief::gates::{self, GRequest, GateOpen};
use news_chief::presets::GCommand;
use news_core::job::ErrorClass;
use news_dispatch::queue::NewJob;
use news_rewrite::{find, plan as rplan, update};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::Arc;

#[derive(Clone, Copy)]
enum Step {
    Preflight,
    Discover,
    Stories,
    Plan,
    Review,
    Brief,
    Write,
    Render,
    Spotcheck,
    Publish,
    Promote,
}

const STEPS: [(&str, Step); 11] = [
    ("edition.preflight", Step::Preflight),
    ("edition.discover", Step::Discover),
    ("edition.stories", Step::Stories),
    ("edition.plan", Step::Plan),
    ("edition.review", Step::Review),
    ("edition.brief", Step::Brief),
    ("edition.write", Step::Write),
    ("edition.render", Step::Render),
    ("edition.spotcheck", Step::Spotcheck),
    ("edition.publish", Step::Publish),
    ("edition.promote", Step::Promote),
];

struct Edition {
    ctx: Arc<Ctx>,
    step: Step,
}

pub fn register(h: Handlers, ctx: Arc<Ctx>) -> Handlers {
    STEPS.into_iter().fold(h, |h, (name, step)| {
        h.with(
            name,
            Arc::new(Edition {
                ctx: ctx.clone(),
                step,
            }),
        )
    })
}

/// Without a repository root only the plan stop can run; the rest say why they cannot.
pub(super) fn register_unset(h: Handlers, unset: Arc<Unset>) -> Handlers {
    STEPS.into_iter().fold(h, |h, (name, step)| {
        if matches!(step, Step::Review) {
            h.with(name, Arc::new(Review))
        } else {
            h.with(name, unset.clone())
        }
    })
}

struct Review;

/// The gate Board shows for a plan: the Budget's items are what the approval covers.
pub fn plan_gate(budget: &Value, plan: &Value) -> GateOpen {
    let items = budget
        .get("items")
        .cloned()
        .filter(Value::is_array)
        .unwrap_or_else(|| json!([]));
    let n = items.as_array().map_or(0, Vec::len);
    GateOpen {
        gate: "plan".into(),
        subject: json!({"items": items, "plan": plan}),
        summary: json!({"text": format!("Plan: {n} items to write and publish"), "items": n}),
        preview: None,
    }
}

impl StepHandler for Review {
    fn run<'a>(&'a self, cx: &'a mut StepCx) -> BoxFut<'a, Result<StepOutcome, StepError>> {
        Box::pin(async move { Ok(review(cx)) })
    }
}

fn review(cx: &StepCx) -> StepOutcome {
    if cx.args["review_plan"] != json!(true) {
        return StepOutcome::Skip("review_plan is off: the start authorises publishing".into());
    }
    // The plan step's output is the plan; `cx.budget` is the Budget it set on the run.
    let plan = cx.outputs.get("plan").cloned().unwrap_or(Value::Null);
    StepOutcome::OpenGates(vec![plan_gate(&cx.budget, &plan)])
}

/// The jobs of a fan-out step (`discover`, `stories`): queue each once and wait for them. `None` while
/// any is live, else `(queued, failed)`; failures are the Wrap's, not the run's.
pub(super) async fn fan_out(
    cx: &StepCx,
    jobs: Vec<NewJob>,
) -> Result<Option<(usize, usize)>, StepError> {
    let (mut live, mut failed, n) = (false, 0, jobs.len());
    for j in jobs {
        match pipeline::queue_once(cx, j).await? {
            pipeline::Job::Live => live = true,
            pipeline::Job::Dead(_) => failed += 1,
            pipeline::Job::Completed(_) => {}
        }
    }
    Ok((!live).then_some((n, failed)))
}

/// Story ids a step's output lists under `stories`.
fn stories(cx: &StepCx, step: &str) -> Vec<i64> {
    cx.outputs
        .get(step)
        .map_or_else(Vec::new, |o| pipeline::ids(&o["stories"]))
}

impl StepHandler for Edition {
    fn run<'a>(&'a self, cx: &'a mut StepCx) -> BoxFut<'a, Result<StepOutcome, StepError>> {
        Box::pin(async move { run(&self.ctx, self.step, cx).await })
    }
}

async fn run(ctx: &Ctx, step: Step, cx: &StepCx) -> Result<StepOutcome, StepError> {
    match step {
        Step::Review => Ok(review(cx)),
        Step::Preflight => preflight(ctx, cx).await,
        Step::Discover => discover(cx).await,
        Step::Stories => {
            let job = NewJob::new("story.detect")
                .unique_key("detect")
                .args(json!({}));
            Ok(match fan_out(cx, vec![job]).await? {
                None => StepOutcome::Wait,
                // A failed detection leaves the stories there already for the plan.
                Some((_, failed)) => StepOutcome::Done(json!({
                    "failed": failed, "wrap": {"detect_failed": failed},
                })),
            })
        }
        Step::Plan => plan::edition(cx).await.map(StepOutcome::Done),
        Step::Brief => {
            let ids: Vec<i64> = pipeline::run_items(cx)
                .await?
                .iter()
                .map(|i| i.story_id)
                .collect();
            let (ok, dead) = pipeline::brief(cx, ctx, &ids).await?;
            Ok(StepOutcome::Done(json!({
                "stories": ok.iter().filter_map(|w| w.story.parse::<i64>().ok()).collect::<Vec<_>>(),
                "dead": dead,
                "wrap": {"briefed": ok.len(), "failed": dead.len()},
            })))
        }
        Step::Write => write(cx).await,
        Step::Render => {
            let written = stories(cx, "write");
            Ok(match pipeline::render(cx, ctx, &written).await? {
                Ph::Wait => StepOutcome::Wait,
                Ph::Done((ok, dead)) => StepOutcome::Done(json!({
                    "pages": ok.iter().map(|r| r.page_id.clone()).collect::<Vec<_>>(),
                    "dead": dead, "wrap": {"rendered": ok.len(), "failed": dead.len()},
                })),
            })
        }
        Step::Spotcheck => {
            let written = stories(cx, "write");
            Ok(match pipeline::spotcheck(cx, ctx, &written).await? {
                Ph::Wait => StepOutcome::Wait,
                Ph::Done((checked, blocked, unchecked)) => StepOutcome::Done(json!({
                    "checked": checked, "blocked": blocked, "unchecked": unchecked,
                    "wrap": {"spot_checked": checked, "spot_blocked": blocked},
                })),
            })
        }
        Step::Publish => publish(ctx, cx).await,
        Step::Promote => promote(ctx, cx).await,
    }
}

/// Step 2: a discovery job for every enabled source that is not red, and a capture for every calendar
/// event whose day has come.
async fn discover(cx: &StepCx) -> Result<StepOutcome, StepError> {
    let sources: Vec<String> = sqlx::query_scalar(
        "select source_id from sources where enabled and health <> 'red' order by source_id",
    )
    .fetch_all(&cx.db)
    .await
    .map_err(db_err)?;
    let events: Vec<String> = sqlx::query_scalar(
        "select event_key from calendar_events
          where status in ('expected', 'late') and expected_day <= $1::timestamptz::date order by event_key",
    )
    .bind(cx.now)
    .fetch_all(&cx.db)
    .await
    .map_err(db_err)?;
    let mut jobs: Vec<NewJob> = sources
        .iter()
        .map(|s| {
            NewJob::new("source.discover")
                .unique_key(format!("discover:{s}"))
                .args(json!({"source_id": s}))
        })
        .collect();
    jobs.extend(events.iter().map(|e| {
        NewJob::new("calendar.capture")
            .unique_key(format!("capture:{e}"))
            .args(json!({"event_key": e}))
    }));
    Ok(match fan_out(cx, jobs).await? {
        None => StepOutcome::Wait,
        Some((n, failed)) => StepOutcome::Done(json!({
            "queued": n, "failed": failed,
            "wrap": {"discover_jobs": n, "discover_failed": failed},
        })),
    })
}

/// Step 1: blockers stop the run. Services, quiet hours and the quota guard are Chief's, at the start;
/// what is checked here is what this run needs: a work folder, and a source to read.
pub(super) async fn preflight(ctx: &Ctx, cx: &StepCx) -> Result<StepOutcome, StepError> {
    let (enabled, red): (i64, i64) = sqlx::query_as(
        "select count(*) filter (where enabled), count(*) filter (where enabled and health = 'red') from sources",
    )
    .fetch_one(&cx.db)
    .await
    .map_err(db_err)?;
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();
    if enabled == 0 {
        blockers.push("no source is enabled".to_owned());
    } else if red == enabled {
        blockers.push("every enabled source is red in the Sitrep".to_owned());
    } else if red > 0 {
        warnings.push(format!("{red} enabled source(s) are red and are skipped"));
    }
    if let Err(e) = std::fs::create_dir_all(ctx.root.join("work/ai")) {
        blockers.push(format!(
            "cannot write {}: {e}",
            ctx.root.join("work").display()
        ));
    }
    if !blockers.is_empty() {
        return Err(fail(
            ErrorClass::InvalidData,
            format!("preflight blockers: {}", blockers.join("; ")),
        ));
    }
    Ok(StepOutcome::Done(
        json!({"pass": true, "warnings": warnings}),
    ))
}

/// Step 7: A1 for the briefed stories, and the A1 update for each direction flip planned at step 4.
async fn write(cx: &StepCx) -> Result<StepOutcome, StepError> {
    let briefed = stories(cx, "brief");
    let mut wait = false;
    let mut flip_dead = 0;
    for p in planned_updates(cx).await? {
        let Some(story) = p.change.story_id else {
            continue;
        };
        match pipeline::queue_once(cx, update::update_job(&p, &story.to_string())).await? {
            pipeline::Job::Live => wait = true,
            pipeline::Job::Dead(_) => flip_dead += 1,
            pipeline::Job::Completed(_) => {}
        }
    }
    match pipeline::write(cx, &briefed).await? {
        Ph::Wait => Ok(StepOutcome::Wait),
        Ph::Done(_) if wait => Ok(StepOutcome::Wait),
        Ph::Done((ok, dead)) => Ok(StepOutcome::Done(json!({
            "stories": ok, "dead": dead, "flips_dead": flip_dead,
            "wrap": {"written": ok.len(), "failed": dead.len() + flip_dead},
        }))),
    }
}

/// The direction flips the plan step budgeted, as Rewrite sees them now.
async fn planned_updates(cx: &StepCx) -> Result<Vec<rplan::PagePlan>, StepError> {
    let ids = budgeted(cx, "update");
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(
        rplan::plan(find::find(&cx.db, None).await.map_err(step_err)?)
            .into_iter()
            .filter(|p| !p.by_code() && ids.contains(&p.change.page_id))
            .collect(),
    )
}

/// The page ids the plan step recorded in the Budget under `key` (`revise` or `update`).
fn budgeted(cx: &StepCx, key: &str) -> BTreeSet<String> {
    cx.budget[key]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}

/// Step 10: the revisions planned at step 4 (value-only re-renders by code, direction flips whose
/// update came back) join the new pages; every page is staged under its own G-tier check and the
/// outbox is sent. Revisions found since step 4, or beyond the `bulk` limit, are deferred.
async fn publish(ctx: &Ctx, cx: &StepCx) -> Result<StepOutcome, StepError> {
    let mut story_ids = stories(cx, "write");
    let planned = budgeted(cx, "revise");
    let found = rplan::plan(find::find(&cx.db, None).await.map_err(step_err)?);
    let (now, deferred) = rplan::due_at_publish(found, &planned);
    let (by_code, mut dead) = revise::by_code(cx, ctx, &now).await?;
    let flips = planned_updates(cx).await?;
    let (flipped, dead_flips) = revise::flips(cx, ctx, &flips).await?;
    dead.extend(dead_flips);
    for p in now.iter().chain(&flips) {
        story_ids.extend(p.change.story_id);
    }
    let revised = by_code.len() + flipped.len();
    let (staged, sent, dead_rows) = pipeline::publish(cx, ctx, &story_ids).await?;
    let tier1: i64 = sqlx::query_scalar(
        "select count(*) from pages where story_id = any($1) and tier = 1 and status = 'published'",
    )
    .bind(&story_ids)
    .fetch_one(&cx.db)
    .await
    .map_err(db_err)?;
    Ok(StepOutcome::Done(json!({
        "staged": staged, "sent": sent, "revised": revised, "dead": dead, "deferred_revisions": deferred,
        "wrap": {"published": sent, "tier1": tier1, "failed": dead.len() + dead_rows as usize,
                 "revisions_published": revised, "queued_next": deferred},
    })))
}

/// Step 11: the index budget (published `noindex` pages by priority, up to `publish.index_per_day`
/// today), sent; then the working files of published pages go.
async fn promote(ctx: &Ctx, cx: &StepCx) -> Result<StepOutcome, StepError> {
    let per_day = news_standing::current(&cx.db)
        .await
        .map_err(step_err)?
        .int("publish.index_per_day");
    let mut tx = cx.db.begin().await.map_err(db_err)?;
    let picks = {
        let mut auth = gates::authorize(
            &mut tx,
            GRequest {
                run_id: cx.run_id,
                step_seq: cx.seq,
                command: GCommand::Publish,
                item: None,
            },
        )
        .await
        .map_err(step_err)?;
        news_press::store::stage_index_promotions(&mut auth, per_day, cx.now)
            .await
            .map_err(step_err)?
    };
    tx.commit().await.map_err(db_err)?;
    let (sent, dead) = pipeline::send(cx, ctx).await?;
    let (files, bytes) = news_press::prune::prune(&cx.db, &ctx.root.join("work"))
        .await
        .map_err(step_err)?;
    Ok(StepOutcome::Done(json!({
        "promoted": picks.len(), "sent": sent, "dead": dead, "pruned_files": files, "pruned_bytes": bytes,
        "wrap": {"index_promoted": picks.len()},
    })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_plan_gate_carries_the_budget_items_the_approval_covers() {
        let g = plan_gate(
            &json!({"items": ["a-page", 7]}),
            &json!({"counts": {"t1": 1}}),
        );
        assert_eq!(g.gate, "plan");
        assert_eq!(g.subject["items"], json!(["a-page", 7]));
        assert_eq!(g.summary["items"], 2);
        // No Budget yet: an empty plan, still a gate that covers nothing.
        let none = plan_gate(&json!({}), &Value::Null);
        assert_eq!(none.subject["items"], json!([]));
    }
}
