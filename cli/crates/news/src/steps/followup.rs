//! Followup's find, classify, bulk-approval and update steps (Bureau §8.9), over `news-rewrite`.
//!
//! Built: finding the published pages a revision touches, classifying each (value only, direction flip,
//! restatement), the `bulk` stop above 100 pages, storing the derived points the revision re-keys to
//! (`news_rewrite::apply::store_derived`, once, before any re-render), and the A1 update jobs for
//! direction flips.
//!
//! `rerender` re-renders the value-only and restated pages by code from the story's page folder (which
//! `news prune` leaves its inputs in), checks them and registers them as drafts (`news_press::register`);
//! `publish` finishes the direction flips whose A1 update came back, then stages every draft under its
//! own G-tier check and sends the outbox. Both skip when Board rejected the bulk change.
//!
//! The steps pass state in their outputs and re-find from PostgreSQL when they need the changes
//! themselves, restricted to the pages the earlier step named, so a revision that arrives mid-run is
//! never half-applied: it waits for the next Followup.

use super::pipeline;
use super::{Ctx, Unset, db_err, plan as plan_step, revise, step_err};
use news_chief::BoxFut;
use news_chief::driver::{Handlers, StepCx, StepError, StepHandler, StepOutcome};
use news_chief::gates::GateOpen;
use news_core::job::ErrorClass;
use news_rewrite::classify::Kind;
use news_rewrite::plan::{self, BULK, PagePlan};
use news_rewrite::{apply, find, update};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::Arc;

#[derive(Clone, Copy)]
enum Step {
    Find,
    Classify,
    BulkReview,
    Rerender,
    Update,
    Publish,
}

struct Followup(Step, Option<Arc<Ctx>>);

const STEPS: [(&str, Step); 6] = [
    ("followup.find", Step::Find),
    ("followup.classify", Step::Classify),
    ("followup.bulk_review", Step::BulkReview),
    ("followup.rerender", Step::Rerender),
    ("followup.update", Step::Update),
    ("followup.publish", Step::Publish),
];

pub fn register(h: Handlers, ctx: Arc<Ctx>) -> Handlers {
    STEPS.into_iter().fold(h, |h, (name, step)| {
        h.with(name, Arc::new(Followup(step, Some(ctx.clone()))))
    })
}

/// Without a repository root the steps that re-render or publish say why they cannot.
pub(super) fn register_unset(h: Handlers, unset: Arc<Unset>) -> Handlers {
    STEPS.into_iter().fold(h, |h, (name, step)| match step {
        Step::Rerender | Step::Publish => h.with(name, unset.clone()),
        _ => h.with(name, Arc::new(Followup(step, None))),
    })
}

/// The pages the `source` argument selects that a revision touches now, classified, in page order.
async fn plans(cx: &StepCx, only: Option<&BTreeSet<String>>) -> Result<Vec<PagePlan>, StepError> {
    let source = cx.args["source"].as_str();
    let found = find::find(&cx.db, source).await.map_err(step_err)?;
    let found = found
        .into_iter()
        .filter(|c| only.is_none_or(|o| o.contains(&c.page_id)))
        .collect();
    Ok(plan::plan(found))
}

/// The page ids the earlier step named, from its output.
fn named(cx: &StepCx, step: &str) -> BTreeSet<String> {
    cx.outputs
        .get(step)
        .and_then(|o| o["pages"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|p| p["page_id"].as_str().map(str::to_owned))
        .collect()
}

/// What a plan looks like in a step's output and on the bulk gate: each page with its kind, never a
/// figure (the figures are the Ledger's and stay in PostgreSQL).
fn listing(pages: &[PagePlan]) -> Value {
    Value::Array(
        pages
            .iter()
            .map(|p| {
                json!({"page_id": p.change.page_id, "version": p.change.version,
                       "story_id": p.change.story_id, "kind": p.kind.as_str()})
            })
            .collect(),
    )
}

/// The `bulk` gate: above [`BULK`] pages the run asks for approval, and the approval covers exactly
/// the pages listed.
pub fn bulk_gate(pages: &Value, counts: &Value) -> GateOpen {
    let items: Vec<Value> = pages
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| p["page_id"].clone())
        .collect();
    let n = items.len();
    GateOpen {
        gate: "bulk".into(),
        whole_subject: false,
        subject: json!({"items": items, "pages": pages}),
        summary: json!({"text": format!("{n} pages would change at once (the limit without approval is {BULK})"),
                        "counts": counts}),
        preview: None,
        whole_subject: false,
    }
}

impl StepHandler for Followup {
    fn run<'a>(&'a self, cx: &'a mut StepCx) -> BoxFut<'a, Result<StepOutcome, StepError>> {
        Box::pin(async move {
            match self.0 {
                Step::Find => {
                    let pages = plans(cx, None).await?;
                    Ok(StepOutcome::Done(json!({
                        "pages": listing(&pages),
                        "wrap": {"revisions_found": pages.len()},
                    })))
                }
                Step::Classify => {
                    let only = named(cx, "find");
                    let pages = plans(cx, Some(&only)).await?;
                    let mut derived = 0;
                    for p in pages.iter().filter(|p| p.by_code()) {
                        derived += apply::store_derived(&cx.db, &p.change)
                            .await
                            .map_err(step_err)?;
                    }
                    let counts = plan::counts(&pages);
                    // A standalone run's Budget is the pages it found: the G-tier check covers them.
                    let ids: Vec<String> = pages.iter().map(|p| p.change.page_id.clone()).collect();
                    plan_step::set_budget(cx, &ids, json!({})).await?;
                    Ok(StepOutcome::Done(json!({
                        "pages": listing(&pages),
                        "counts": counts,
                        "derived_stored": derived,
                        "wrap": {"value_only": counts["value_only"], "restatement": counts["restatement"],
                                 "direction_flip": counts["direction_flip"]},
                    })))
                }
                Step::BulkReview => {
                    let classified = cx.outputs.get("classify").unwrap_or(&Value::Null);
                    let n = classified["pages"].as_array().map_or(0, Vec::len);
                    if n <= BULK {
                        return Ok(StepOutcome::Skip(format!(
                            "{n} pages, within the {BULK}-page limit: the start authorises publishing"
                        )));
                    }
                    Ok(StepOutcome::OpenGates(vec![bulk_gate(
                        &classified["pages"],
                        &classified["counts"],
                    )]))
                }
                Step::Rerender => rerender(self.1.as_deref(), cx).await,
                Step::Update => update_flips(cx).await,
                Step::Publish => publish(self.1.as_deref(), cx).await,
            }
        })
    }
}

/// Whether Board rejected the change at the `bulk` gate. Every step after it that changes a page
/// (`rerender`, `update` and `publish`) skips when it is.
fn bulk_rejected(cx: &StepCx) -> bool {
    cx.outputs
        .get("bulk_review")
        .and_then(|r| r["decisions"].as_array())
        .is_some_and(|d| d.iter().any(|x| x["decision"] == "rejected"))
}

const NO_ROOT: &str = "the repository root is not set";

/// Value-only and restated pages, re-rendered by code, checked and registered as drafts.
async fn rerender(ctx: Option<&Ctx>, cx: &StepCx) -> Result<StepOutcome, StepError> {
    if bulk_rejected(cx) {
        return Ok(StepOutcome::Skip(
            "the bulk change was rejected in Board; no page is re-rendered".into(),
        ));
    }
    let ctx = ctx.ok_or_else(|| pipeline::fail(ErrorClass::InvalidData, NO_ROOT))?;
    let only = named(cx, "classify");
    let by_code: Vec<PagePlan> = plans(cx, Some(&only))
        .await?
        .into_iter()
        .filter(PagePlan::by_code)
        .collect();
    let (ok, dead) = revise::by_code(cx, ctx, &by_code).await?;
    Ok(StepOutcome::Done(json!({
        "registered": ok.iter().map(|r| r.page_id.clone()).collect::<Vec<_>>(), "dead": dead,
        "wrap": {"rerendered": ok.len(), "rerender_failed": dead.len()},
    })))
}

/// Finish the flips, then stage and send every draft of the pages found, each under its own check.
async fn publish(ctx: Option<&Ctx>, cx: &StepCx) -> Result<StepOutcome, StepError> {
    if bulk_rejected(cx) {
        return Ok(StepOutcome::Skip(
            "the bulk change was rejected in Board; nothing is published".into(),
        ));
    }
    let ctx = ctx.ok_or_else(|| pipeline::fail(ErrorClass::InvalidData, NO_ROOT))?;
    let only = named(cx, "classify");
    let found = plans(cx, Some(&only)).await?;
    let flips: Vec<PagePlan> = found.iter().filter(|p| !p.by_code()).cloned().collect();
    let (flipped, mut dead) = revise::flips(cx, ctx, &flips).await?;
    // The drafts `rerender` registered (and the flips just finished) are the Budget's pages' pending versions.
    let stories = pipeline::stories_of(cx, &pipeline::budget_ids(cx, "items")).await?;
    let (staged, sent, dead_rows) = pipeline::publish(cx, ctx, &stories).await?;
    if let Some(r) = cx.outputs.get("rerender") {
        dead.extend(r["dead"].as_array().cloned().unwrap_or_default());
    }
    Ok(StepOutcome::Done(json!({
        "staged": staged, "sent": sent, "flips_published": flipped.len(), "dead": dead,
        "wrap": {"published": sent, "failed": dead.len() + dead_rows as usize},
    })))
}

/// A1 update jobs (`ai.write`, mode `update`) for the direction flips, one per page, keyed by page,
/// version and diff, so a re-entered step queues only what is missing. Waits for them to finish.
async fn update_flips(cx: &StepCx) -> Result<StepOutcome, StepError> {
    if bulk_rejected(cx) {
        return Ok(StepOutcome::Skip(
            "the bulk change was rejected in Board; no page is updated".into(),
        ));
    }
    let only = named(cx, "classify");
    let flips: Vec<PagePlan> = plans(cx, Some(&only))
        .await?
        .into_iter()
        .filter(|p| p.kind == Kind::DirectionFlip)
        .collect();
    if flips.is_empty() {
        return Ok(StepOutcome::Skip("no direction flips".into()));
    }
    // The first entry queues the jobs and waits; the next finds them done.
    let mut queued = 0;
    for p in &flips {
        let Some(story) = p.change.story_id else {
            return Err(StepError::Fail {
                class: news_core::job::ErrorClass::InvalidData,
                message: format!("page {} has no story to rewrite", p.change.page_id),
            });
        };
        let job = update::update_job(p, &story.to_string());
        // A job of this run that already ran, even one that failed or was cancelled, is not queued
        // again: a failed update goes to the dead list for Salvage, it does not loop here.
        let known: bool = sqlx::query_scalar(
            "select exists (select 1 from jobs where run_id = $1 and unique_key = $2)",
        )
        .bind(cx.run_id)
        .bind(&job.unique_key)
        .fetch_one(&cx.db)
        .await
        .map_err(db_err)?;
        if !known {
            cx.enqueue(job).await.map_err(step_err)?;
            queued += 1;
        }
    }
    let live: i64 = sqlx::query_scalar(
        "select count(*) from jobs where run_id = $1 and step_seq = $2
            and state in ('available', 'scheduled', 'running', 'retryable', 'snoozed')",
    )
    .bind(cx.run_id)
    .bind(cx.seq)
    .fetch_one(&cx.db)
    .await
    .map_err(db_err)?;
    if live > 0 {
        return Ok(StepOutcome::Wait);
    }
    // A cancelled update (Chief cancels `ai` jobs at the token cap) is as dead as a discarded one.
    let dead: i64 = sqlx::query_scalar(
        "select count(*) from jobs where run_id = $1 and step_seq = $2
            and state in ('discarded', 'cancelled')",
    )
    .bind(cx.run_id)
    .bind(cx.seq)
    .fetch_one(&cx.db)
    .await
    .map_err(db_err)?;
    Ok(StepOutcome::Done(json!({
        "flips": flips.len(), "queued": queued, "dead": dead,
        "wrap": {"updates_queued": flips.len(), "updates_failed": dead},
    })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_bulk_gate_covers_exactly_the_pages_listed() {
        let pages = json!([{"page_id": "a", "kind": "value_only"}, {"page_id": "b", "kind": "direction_flip"}]);
        let g = bulk_gate(&pages, &json!({"pages": 2}));
        assert_eq!(g.gate, "bulk");
        assert_eq!(g.subject["items"], json!(["a", "b"]));
        assert!(g.summary["text"].as_str().unwrap().starts_with("2 pages"));
    }
}
