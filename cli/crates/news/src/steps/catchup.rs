//! Catchup (Bureau §8.5): backfill the current year. Enumerate the historical events, score and select,
//! one approval of the whole plan, then each chunk of stories is briefed, written, rendered, checked
//! and published in turn (Edition steps 6 to 10 per chunk). The approval's subject lists the story pages,
//! so it authorises the publishing of every chunk and nothing else.
//!
//! The chunk step is a checkpoint loop: after each chunk it yields, so a pause takes effect between
//! chunks and the next unfinished chunk continues on Resume. Backfill jobs use priority 4.

use super::edition::{fan_out, preflight};
use super::pipeline::{self, Ph, fail};
use super::{Ctx, Unset, db_err, plan, step_err};
use chrono::NaiveDate;
use news_chief::BoxFut;
use news_chief::driver::{Handlers, StepCx, StepError, StepHandler, StepOutcome};
use news_chief::gates::GateOpen;
use news_core::job::ErrorClass;
use news_dispatch::queue::NewJob;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Clone, Copy)]
enum Step {
    Preflight,
    Enumerate,
    Select,
    Review,
    Chunks,
}

const STEPS: [(&str, Step); 5] = [
    ("catchup.preflight", Step::Preflight),
    ("catchup.enumerate", Step::Enumerate),
    ("catchup.select", Step::Select),
    ("catchup.review", Step::Review),
    ("catchup.chunks", Step::Chunks),
];

struct Catchup(Step, Arc<Ctx>);

pub fn register(h: Handlers, ctx: Arc<Ctx>) -> Handlers {
    STEPS.into_iter().fold(h, |h, (name, step)| {
        h.with(name, Arc::new(Catchup(step, ctx.clone())))
    })
}

pub(super) fn register_unset(h: Handlers, unset: Arc<Unset>) -> Handlers {
    STEPS
        .into_iter()
        .fold(h, |h, (name, _)| h.with(name, unset.clone()))
}

impl StepHandler for Catchup {
    fn run<'a>(&'a self, cx: &'a mut StepCx) -> BoxFut<'a, Result<StepOutcome, StepError>> {
        Box::pin(async move { run(&self.1, self.0, cx).await })
    }
}

fn list(cx: &StepCx, key: &str) -> Vec<String> {
    cx.args[key]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}

/// The release dates the run covers: `from` (default `run.backfill_from`, else 1 January of this year)
/// through `to` (default today).
async fn window(cx: &StepCx) -> Result<(NaiveDate, NaiveDate), StepError> {
    let parse = |v: &Value| v.as_str().and_then(|s| s.parse::<NaiveDate>().ok());
    let today = cx.now.date_naive();
    let from = match parse(&cx.args["from"]) {
        Some(d) => d,
        None => {
            let set = news_standing::current(&cx.db).await.map_err(step_err)?;
            parse(&set.get("run.backfill_from")).unwrap_or_else(|| {
                NaiveDate::from_ymd_opt(chrono::Datelike::year(&today), 1, 1).unwrap_or(today)
            })
        }
    };
    let to = parse(&cx.args["to"]).unwrap_or(today);
    if from > to {
        return Err(fail(
            ErrorClass::InvalidData,
            format!("from {from} is after to {to}"),
        ));
    }
    Ok((from, to))
}

async fn run(ctx: &Ctx, step: Step, cx: &StepCx) -> Result<StepOutcome, StepError> {
    match step {
        Step::Preflight => preflight(ctx, cx).await,
        Step::Enumerate => {
            let (from, to) = window(cx).await?;
            let only = list(cx, "sources");
            let sources: Vec<String> = sqlx::query_scalar(
                "select source_id from sources where enabled and health <> 'red'
                    and (cardinality($1::text[]) = 0 or source_id = any($1)) order by source_id",
            )
            .bind(&only)
            .fetch_all(&cx.db)
            .await
            .map_err(db_err)?;
            let jobs = sources
                .iter()
                .map(|s| {
                    NewJob::new("source.discover")
                        .unique_key(format!("enumerate:{s}"))
                        .priority(4)
                        .args(json!({"source_id": s, "from": from, "to": to, "backfill": true}))
                })
                .collect();
            Ok(match fan_out(cx, jobs).await? {
                None => StepOutcome::Wait,
                Some((n, failed)) => StepOutcome::Done(json!({
                    "from": from, "to": to, "queued": n, "failed": failed,
                    "wrap": {"enumerate_jobs": n, "enumerate_failed": failed},
                })),
            })
        }
        Step::Select => {
            let (from, to) = window(cx).await?;
            let types = list(cx, "page_types");
            let stories: Vec<i64> = sqlx::query_scalar(
                "select distinct s.story_id from stories s join candidates c on c.story_id = s.story_id
                  where s.status = 'open' and c.published_at::date between $1 and $2
                    and (cardinality($3::text[]) = 0 or c.page_type = any($3)) order by s.story_id",
            )
            .bind(from)
            .bind(to)
            .bind(&types)
            .fetch_all(&cx.db)
            .await
            .map_err(db_err)?;
            let (planned, summary) = plan::plan_run(cx, Some(&stories), Some(1000))
                .await
                .map_err(step_err)?;
            let pages = plan::page_ids(&cx.db, &planned).await.map_err(step_err)?;
            plan::set_budget(cx, &pages, json!({})).await?;
            let chunk = cx.args["chunk"].as_u64().unwrap_or(50).max(1) as usize;
            Ok(StepOutcome::Done(json!({
                "stories": planned, "pages": pages, "plan": summary, "chunk": chunk,
                "chunks": planned.len().div_ceil(chunk),
                "wrap": {"planned": planned.len()},
            })))
        }
        Step::Review => review(cx).await,
        Step::Chunks => chunks(ctx, cx).await,
    }
}

/// The plan approval: how many pages, how many chunks, and what the writer calls will probably cost
/// (the average of the A1 calls so far; a figure the Ledger has, not one typed here).
async fn review(cx: &StepCx) -> Result<StepOutcome, StepError> {
    let sel = cx.outputs.get("select").cloned().unwrap_or(Value::Null);
    let avg: Option<f64> = sqlx::query_scalar(
        "select avg(tokens_in + tokens_out)::float8 from ai_calls where step = 'A1' and repair_round = 0",
    )
    .fetch_one(&cx.db)
    .await
    .map_err(db_err)?;
    let n = sel["stories"].as_array().map_or(0, Vec::len);
    let est = avg.map(|a| (a * n as f64).round() as i64);
    Ok(StepOutcome::OpenGates(vec![GateOpen {
        gate: "plan".into(),
        whole_subject: false,
        subject: json!({"items": sel["pages"], "stories": sel["stories"], "chunk": sel["chunk"]}),
        summary: json!({
            "text": format!("Catchup: {n} pages in {} chunks of {}", sel["chunks"], sel["chunk"]),
            "items": n, "chunks": sel["chunks"], "estimated_tokens": est, "subject_items": sel["pages"],
        }),
        preview: None,
    }]))
}

/// One chunk per entry: brief, write, render and check, publish; then yield. The state is the chunk
/// number and the running totals, saved with each yield.
async fn chunks(ctx: &Ctx, cx: &StepCx) -> Result<StepOutcome, StepError> {
    let rejected = cx
        .outputs
        .get("review")
        .and_then(|r| r["decisions"].as_array())
        .is_some_and(|d| d.iter().any(|x| x["decision"] == "rejected"));
    if rejected {
        return Ok(StepOutcome::Skip(
            "the plan was rejected in Board; nothing is written or published".into(),
        ));
    }
    let sel = cx.outputs.get("select").cloned().unwrap_or(Value::Null);
    let all = pipeline::ids(&sel["stories"]);
    let size = sel["chunk"].as_u64().unwrap_or(50).max(1) as usize;
    let state = cx.previous.clone().unwrap_or_else(|| json!({}));
    let i = state["chunk"].as_u64().unwrap_or(0) as usize;
    let total = |k: &str| state["totals"][k].as_i64().unwrap_or(0);
    if i * size >= all.len() {
        // Counted from the outbox: a chunk that ran twice after a restart reported nothing the second time.
        let published: i64 = sqlx::query_scalar(
            "select count(*) from publish_outbox where run_id = $1 and op = 'put_page' and sent_at is not null",
        )
        .bind(cx.run_id)
        .fetch_one(&cx.db)
        .await
        .map_err(db_err)?;
        let failed = total("failed");
        return Ok(StepOutcome::Done(json!({
            "chunks": i, "published": published, "failed": failed,
            "wrap": {"published": published, "failed": failed},
        })));
    }
    let chunk = &all[i * size..all.len().min((i + 1) * size)];
    // A chunk re-entered after a restart (published, not yet checkpointed) skips what this run already sent:
    // briefing a published page again would build a needless next version.
    let sent: Vec<i64> = sqlx::query_scalar(
        "select distinct p.story_id from pages p join publish_outbox o on o.page_id = p.page_id
          where o.run_id = $1 and o.op = 'put_page' and o.sent_at is not null and p.story_id = any($2)",
    )
    .bind(cx.run_id)
    .bind(chunk)
    .fetch_all(&cx.db)
    .await
    .map_err(db_err)?;
    let slice: Vec<i64> = chunk
        .iter()
        .copied()
        .filter(|s| !sent.contains(s))
        .collect();
    let slice = &slice[..];
    let (briefed, mut dead) = pipeline::brief(cx, ctx, slice).await?;
    let briefed: Vec<i64> = briefed
        .iter()
        .filter_map(|w| w.story.parse().ok())
        .collect();
    let Ph::Done((written, dead_w)) = pipeline::write(cx, &briefed).await? else {
        return Ok(StepOutcome::Wait);
    };
    dead.extend(dead_w);
    let Ph::Done((registered, dead_r)) = pipeline::render(cx, ctx, &written).await? else {
        return Ok(StepOutcome::Wait);
    };
    dead.extend(dead_r);
    let (_, sent, dead_rows) = pipeline::publish(cx, ctx, &written).await?;
    let failed = dead.len() as i64 + i64::from(dead_rows);
    let _ = registered;
    Ok(StepOutcome::Yield(json!({
        "chunk": i + 1,
        "totals": {"published": total("published") + i64::from(sent), "failed": total("failed") + failed},
    })))
}
