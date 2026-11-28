//! Flash (Bureau §8.4): one to ten named stories, written and published at once: capture now, brief,
//! write, render and check, publish. Discovery and selection are skipped; the tier comes from the
//! score. The start authorises publishing, so the Budget is the stories named.

use super::edition::fan_out;
use super::pipeline::{self, Ph, fail};
use super::{Ctx, Unset, db_err, plan};
use news_chief::BoxFut;
use news_chief::driver::{Handlers, StepCx, StepError, StepHandler, StepOutcome};
use news_core::job::ErrorClass;
use news_dispatch::queue::NewJob;
use serde_json::json;
use std::sync::Arc;

#[derive(Clone, Copy)]
enum Step {
    Preflight,
    Capture,
    Brief,
    Write,
    Render,
    Publish,
}

const STEPS: [(&str, Step); 6] = [
    ("flash.preflight", Step::Preflight),
    ("flash.capture", Step::Capture),
    ("flash.brief", Step::Brief),
    ("flash.write", Step::Write),
    ("flash.render", Step::Render),
    ("flash.publish", Step::Publish),
];

struct Flash(Step, Arc<Ctx>);

pub fn register(h: Handlers, ctx: Arc<Ctx>) -> Handlers {
    STEPS.into_iter().fold(h, |h, (name, step)| {
        h.with(name, Arc::new(Flash(step, ctx.clone())))
    })
}

pub(super) fn register_unset(h: Handlers, unset: Arc<Unset>) -> Handlers {
    STEPS
        .into_iter()
        .fold(h, |h, (name, _)| h.with(name, unset.clone()))
}

/// The story ids a step recorded under `stories`.
fn recorded(cx: &StepCx, step: &str) -> Vec<i64> {
    cx.outputs
        .get(step)
        .map_or_else(Vec::new, |o| pipeline::ids(&o["stories"]))
}

impl StepHandler for Flash {
    fn run<'a>(&'a self, cx: &'a mut StepCx) -> BoxFut<'a, Result<StepOutcome, StepError>> {
        Box::pin(async move { run(&self.1, self.0, cx).await })
    }
}

async fn run(ctx: &Ctx, step: Step, cx: &StepCx) -> Result<StepOutcome, StepError> {
    match step {
        Step::Preflight => {
            let keys = cx.args["keys"].as_array().map_or(0, Vec::len);
            if !(1..=10).contains(&keys) {
                return Err(fail(
                    ErrorClass::InvalidData,
                    format!("Flash takes 1 to 10 keys, not {keys}"),
                ));
            }
            std::fs::create_dir_all(ctx.root.join("work/ai")).map_err(|e| {
                fail(
                    ErrorClass::InvalidData,
                    format!("cannot write the work folder: {e}"),
                )
            })?;
            Ok(StepOutcome::Done(json!({"pass": true, "keys": keys})))
        }
        Step::Capture => capture(cx).await,
        Step::Brief => {
            let ids = recorded(cx, "capture");
            let (ok, dead) = pipeline::brief(cx, ctx, &ids).await?;
            Ok(StepOutcome::Done(json!({
                "stories": ok.iter().filter_map(|w| w.story.parse::<i64>().ok()).collect::<Vec<_>>(),
                "dead": dead, "wrap": {"briefed": ok.len(), "failed": dead.len()},
            })))
        }
        Step::Write => Ok(
            match pipeline::write(cx, ctx, &recorded(cx, "brief")).await? {
                Ph::Wait => StepOutcome::Wait,
                Ph::Done((ok, dead)) => StepOutcome::Done(json!({
                    "stories": ok, "dead": dead, "wrap": {"written": ok.len(), "failed": dead.len()},
                })),
            },
        ),
        Step::Render => Ok(
            match pipeline::render(cx, ctx, &recorded(cx, "write")).await? {
                Ph::Wait => StepOutcome::Wait,
                Ph::Done((ok, dead)) => StepOutcome::Done(json!({
                    "pages": ok.iter().map(|r| r.page_id.clone()).collect::<Vec<_>>(),
                    "dead": dead, "wrap": {"rendered": ok.len(), "failed": dead.len()},
                })),
            },
        ),
        Step::Publish => {
            let (staged, sent, dead) = pipeline::publish(cx, ctx, &recorded(cx, "write")).await?;
            Ok(StepOutcome::Done(json!({
                "staged": staged, "sent": sent, "wrap": {"published": sent, "failed": dead},
            })))
        }
    }
}

/// Resolve the keys to stories (an event key, or a story id), capture the calendar events among them
/// now, score just these stories and set the Budget to their pages.
async fn capture(cx: &StepCx) -> Result<StepOutcome, StepError> {
    let keys: Vec<String> = cx.args["keys"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|k| k.as_str().map_or_else(|| k.to_string(), str::to_owned))
        .collect();
    let mut ids = Vec::new();
    let mut unknown = Vec::new();
    for k in &keys {
        let id: Option<i64> = sqlx::query_scalar(
            "select story_id from stories where event_key = $1 or story_id::text = $1 limit 1",
        )
        .bind(k)
        .fetch_optional(&cx.db)
        .await
        .map_err(db_err)?;
        match id {
            Some(i) => ids.push(i),
            None => unknown.push(k.clone()),
        }
    }
    if !unknown.is_empty() {
        return Err(fail(
            ErrorClass::InvalidData,
            format!("no story for: {}", unknown.join(", ")),
        ));
    }
    let events: Vec<String> = sqlx::query_scalar(
        "select event_key from calendar_events where event_key = any($1) and status in ('expected', 'late')",
    )
    .bind(&keys)
    .fetch_all(&cx.db)
    .await
    .map_err(db_err)?;
    let jobs = events
        .iter()
        .map(|e| {
            NewJob::new("calendar.capture")
                .unique_key(format!("capture:{e}"))
                .args(json!({"event_key": e}))
        })
        .collect();
    if fan_out(cx, jobs).await?.is_none() {
        return Ok(StepOutcome::Wait);
    }
    let (planned, summary) = plan::plan_run(cx, Some(&ids), None)
        .await
        .map_err(super::step_err)?;
    if let Some(missing) = ids.iter().find(|i| !planned.contains(i)) {
        return Err(fail(
            ErrorClass::InvalidData,
            format!("story {missing} has no scoring inputs yet"),
        ));
    }
    let pages = plan::page_ids(&cx.db, &planned)
        .await
        .map_err(super::step_err)?;
    plan::set_budget(cx, &pages, json!({})).await?;
    Ok(StepOutcome::Done(json!({
        "stories": planned, "plan": summary, "wrap": {"planned": planned.len()},
    })))
}
