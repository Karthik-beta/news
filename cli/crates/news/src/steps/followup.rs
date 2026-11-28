//! Followup's find, classify, bulk-approval and update steps (Bureau §8.9), over `news-rewrite`.
//!
//! Built: finding the published pages a revision touches, classifying each (value only, direction flip,
//! restatement), the `bulk` stop above 100 pages, storing the derived points the revision re-keys to
//! (`news_rewrite::apply::store_derived`, once, before any re-render), and the A1 update jobs for
//! direction flips.
//!
//! Not built: `rerender` and `publish`. Re-rendering needs the story's page folder under `work/`, which
//! `news prune` removes once a page is published, and publishing needs the rendered bundle in Press's
//! layout (`work/pages/<page_id>/v<n>/`) plus a `pages` row at the new version; neither handoff is
//! specified yet. Those two steps stay `NotBuilt`.
//!
//! The steps pass state in their outputs and re-find from PostgreSQL when they need the changes
//! themselves, restricted to the pages the earlier step named, so a revision that arrives mid-run is
//! never half-applied: it waits for the next Followup.

use super::{db_err, step_err};
use news_chief::BoxFut;
use news_chief::driver::{Handlers, StepCx, StepError, StepHandler, StepOutcome};
use news_chief::gates::GateOpen;
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
    Update,
}

struct Followup(Step);

pub fn register(h: Handlers) -> Handlers {
    [
        ("followup.find", Step::Find),
        ("followup.classify", Step::Classify),
        ("followup.bulk_review", Step::BulkReview),
        ("followup.update", Step::Update),
    ]
    .into_iter()
    .fold(h, |h, (name, step)| h.with(name, Arc::new(Followup(step))))
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
                Step::Update => update_flips(cx).await,
            }
        })
    }
}

/// Whether Board rejected the change at the `bulk` gate. Every step after it that changes a page
/// (`update`, and `rerender` and `publish` once built) skips when it is.
fn bulk_rejected(cx: &StepCx) -> bool {
    cx.outputs
        .get("bulk_review")
        .and_then(|r| r["decisions"].as_array())
        .is_some_and(|d| d.iter().any(|x| x["decision"] == "rejected"))
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
