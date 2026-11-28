//! Frontpage's steps (Bureau §8.6, ADR 0005), over `news-home`: candidates, the curator's draft (A8,
//! through Ghost), save and check with up to two fix rounds, the preview, the approval in Board, and the
//! gated publish.
//!
//! The curator draft is an AI batch of one: code builds the input file, Ghost runs `claude -p` with no
//! tools and the `home-draft.v1` schema, and code validates what comes back (`news home save`, then
//! `check`). A failed check calls the curator again with the errors, twice at most, so the draft step
//! owns the fix rounds and a re-entered step recomputes where it was from the jobs and drafts of the run.
//! Request changes returns the run to this step; Karthik's note goes into the next round's input.

use super::{Ctx, Unset, db_err, step_err};
use news_chief::BoxFut;
use news_chief::driver::{Handlers, StepCx, StepError, StepHandler, StepOutcome};
use news_chief::gates::{self, GRequest, GateOpen};
use news_chief::presets::GCommand;
use news_core::job::ErrorClass;
use news_dispatch::queue::NewJob;
use news_home::{candidates, ops, store};
use serde_json::{Value, json};
use std::sync::Arc;

/// A failed check calls the curator again with the errors, at most this many times (Bureau §8.6).
pub const FIX_ROUNDS: u32 = 2;

/// The schema Ghost holds the curator's output to.
const SCHEMA: &str = "contracts/cli/home-draft.v1.schema.json";

#[derive(Clone, Copy)]
enum Step {
    Candidates,
    Draft,
    Check,
    Preview,
    Review,
    Publish,
}

const STEPS: [(&str, Step); 6] = [
    ("frontpage.candidates", Step::Candidates),
    ("frontpage.draft", Step::Draft),
    ("frontpage.check", Step::Check),
    ("frontpage.preview", Step::Preview),
    ("frontpage.review", Step::Review),
    ("frontpage.publish", Step::Publish),
];

struct Frontpage {
    ctx: Arc<Ctx>,
    step: Step,
}

pub fn register(h: Handlers, ctx: Arc<Ctx>) -> Handlers {
    STEPS.into_iter().fold(h, |h, (name, step)| {
        h.with(
            name,
            Arc::new(Frontpage {
                ctx: ctx.clone(),
                step,
            }),
        )
    })
}

pub(super) fn register_unset(h: Handlers, unset: Arc<Unset>) -> Handlers {
    STEPS
        .into_iter()
        .fold(h, |h, (name, _)| h.with(name, unset.clone()))
}

fn draft_id(cx: &StepCx) -> Result<i64, StepError> {
    cx.outputs
        .get("draft")
        .and_then(|d| d["draft_id"].as_i64())
        .ok_or_else(|| StepError::Fail {
            class: ErrorClass::Bug,
            message: "the curator draft step recorded no draft".into(),
        })
}

fn invalid(message: String) -> StepError {
    StepError::Fail {
        class: ErrorClass::InvalidData,
        message,
    }
}

impl StepHandler for Frontpage {
    fn run<'a>(&'a self, cx: &'a mut StepCx) -> BoxFut<'a, Result<StepOutcome, StepError>> {
        Box::pin(async move {
            match self.step {
                Step::Candidates => {
                    let (data, warnings) =
                        candidates::run(&cx.db, cx.now).await.map_err(step_err)?;
                    let n = data["count"].as_i64().unwrap_or(0);
                    Ok(StepOutcome::Done(json!({
                        "candidates": data, "warnings": warnings, "wrap": {"candidates": n},
                    })))
                }
                Step::Draft => draft(&self.ctx, cx).await,
                Step::Check => {
                    let id = draft_id(cx)?;
                    let checked = ops::check(&cx.db, id, cx.now).await.map_err(step_err)?;
                    if checked["pass"] != json!(true) {
                        return Err(invalid(format!(
                            "draft {id} no longer passes the checks: {}; the live curation stays",
                            first_failures(&checked)
                        )));
                    }
                    Ok(StepOutcome::Done(json!({"draft_id": id, "pass": true})))
                }
                Step::Preview => {
                    let id = draft_id(cx)?;
                    let p = ops::preview(&cx.db, id, &self.ctx.preview_base)
                        .await
                        .map_err(step_err)?;
                    Ok(StepOutcome::Done(json!({"draft_id": id, "preview": p})))
                }
                Step::Review => review(cx).await,
                Step::Publish => publish(&self.ctx, cx).await,
            }
        })
    }
}

fn first_failures(checked: &Value) -> String {
    let list = checked["failures"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    let shown: Vec<String> = list
        .iter()
        .take(3)
        .map(|f| {
            format!(
                "{}: {}",
                f["code"].as_str().unwrap_or("?"),
                f["message"].as_str().unwrap_or("")
            )
        })
        .collect();
    format!("{} failure(s): {}", list.len(), shown.join("; "))
}

/// How many times Karthik has asked for changes on this run, and his latest note.
async fn changes(cx: &StepCx) -> Result<(u32, Option<String>), StepError> {
    let notes: Vec<Option<String>> = sqlx::query_scalar(
        "select note from approvals where run_id = $1 and gate = 'publish' and decision = 'changes'
          order by round",
    )
    .bind(cx.run_id)
    .fetch_all(&cx.db)
    .await
    .map_err(db_err)?;
    Ok((
        u32::try_from(notes.len()).unwrap_or(u32::MAX),
        notes.into_iter().last().flatten(),
    ))
}

/// The curator's output: the file Ghost kept, read back as a draft. A reply that is not a draft is a
/// failed round with the reason, so the next round tells the curator what was wrong. `curator_model`
/// is the model Ghost called (its job result), never what the model says about itself.
fn read_draft(result: &Value, model: &str) -> Result<news_home::draft::Draft, String> {
    let path = result["output"]
        .as_str()
        .ok_or("the curator call left no output file")?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let mut value: Value = serde_json::from_str(&text).map_err(|e| format!("not JSON: {e}"))?;
    if let Some(d) = value.as_object_mut() {
        d.insert("curator_model".into(), json!(model));
    }
    ops::parse_draft(value).map_err(|e| e.message)
}

async fn draft(ctx: &Ctx, cx: &StepCx) -> Result<StepOutcome, StepError> {
    let (round, note) = changes(cx).await?;
    let mut errors = Value::Null;
    let mut previous = Value::Null;
    for fix in 0..=FIX_ROUNDS {
        let key = format!("curator:{round}:{fix}");
        let job: Option<(String, Option<Value>, Option<String>)> = sqlx::query_as(
            "select state, result, error_class from jobs where run_id = $1 and unique_key = $2
              order by job_id desc limit 1",
        )
        .bind(cx.run_id)
        .bind(&key)
        .fetch_optional(&cx.db)
        .await
        .map_err(db_err)?;
        let Some((state, result, class)) = job else {
            return queue(ctx, cx, &key, &note, &previous, &errors).await;
        };
        match state.as_str() {
            "completed" => {}
            // A cancelled call (Chief cancels `ai` jobs at the token cap) has no result either;
            // waiting on it would re-enter this step forever.
            "discarded" | "cancelled" => {
                let fallback = if state == "cancelled" {
                    ErrorClass::AiUsageLimit
                } else {
                    ErrorClass::Bug
                };
                return Err(StepError::Fail {
                    class: class
                        .as_deref()
                        .and_then(|c| c.parse().ok())
                        .unwrap_or(fallback),
                    message: format!("the curator call was {state} ({key})"),
                });
            }
            _ => return Ok(StepOutcome::Wait),
        }
        let result = result.unwrap_or(Value::Null);
        let model = result["model"].as_str().unwrap_or_default().to_owned();
        if model.is_empty() {
            return Err(StepError::Fail {
                class: ErrorClass::Bug,
                message: format!("the curator call ({key}) did not record the model it ran"),
            });
        }
        let outcome = match read_draft(&result, &model) {
            Ok(d) => {
                let (id, _) = store::save(&cx.db, &d, Some(cx.run_id))
                    .await
                    .map_err(step_err)?;
                let checked = ops::check(&cx.db, id, cx.now).await.map_err(step_err)?;
                if checked["pass"] == json!(true) {
                    return Ok(StepOutcome::Done(json!({
                        "draft_id": id, "round": round, "fix_rounds": fix,
                        "wrap": {"drafts": 1, "fix_rounds": fix},
                    })));
                }
                previous = serde_json::to_value(&d).unwrap_or(Value::Null);
                checked["failures"].clone()
            }
            Err(why) => json!([{"code": "schema", "message": why}]),
        };
        errors = outcome;
    }
    Err(invalid(format!(
        "the curation still fails the checks after {FIX_ROUNDS} fix rounds: {}; the live curation stays",
        first_failures(&json!({"failures": errors}))
    )))
}

/// Write the curator's input (candidates built by code; on a fix round the errors and the draft that
/// failed; on a changes round Karthik's note) and queue the call.
async fn queue(
    ctx: &Ctx,
    cx: &StepCx,
    key: &str,
    note: &Option<String>,
    previous: &Value,
    errors: &Value,
) -> Result<StepOutcome, StepError> {
    let items = cx
        .outputs
        .get("candidates")
        .map_or(Value::Null, |c| c["candidates"]["items"].clone());
    let input = json!({
        "step": "A8", "skill": "news-home-curation", "candidates": items,
        "note": note, "previous": previous, "errors": errors,
    });
    let rel = format!("work/home/{}/{}.json", cx.short_id, key.replace(':', "-"));
    let file = ctx.root.join(&rel);
    let write = file
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&file, input.to_string()));
    write.map_err(|e| StepError::Retry(format!("cannot write {rel}: {e}")))?;
    cx.enqueue(
        NewJob::new("ai.write")
            .unique_key(key)
            .args(json!({"step": "A8", "input": rel, "schema": SCHEMA})),
    )
    .await
    .map_err(step_err)?;
    Ok(StepOutcome::Wait)
}

fn preview(cx: &StepCx) -> Value {
    cx.outputs
        .get("preview")
        .map_or(Value::Null, |p| p["preview"].clone())
}

/// The subject is the draft and the bytes the preview showed: a different draft is a different hash.
fn subject(id: i64, st: &store::Stored, preview: &Value) -> Value {
    json!({"draft_id": id, "slots": st.draft.slots, "preview_sha": preview["sha256"]})
}

async fn review(cx: &StepCx) -> Result<StepOutcome, StepError> {
    let id = draft_id(cx)?;
    let st = store::get(&cx.db, id).await.map_err(step_err)?;
    let preview = preview(cx);
    let subject = subject(id, &st, &preview);
    let n = st.draft.page_ids().len();
    Ok(StepOutcome::OpenGates(vec![GateOpen {
        gate: "publish".into(),
        subject,
        summary: json!({"text": format!("Curation draft {id}: {n} pages, lead {}", st.draft.slots.lead.page_id),
                        "draft_id": id, "rationale": st.draft.rationale}),
        preview: Some(json!({"draft_id": id, "url": preview["url"], "views": preview["views"]})),
        // Home publish acts on the whole draft, never on one item.
        whole_subject: true,
    }]))
}

async fn publish(ctx: &Ctx, cx: &StepCx) -> Result<StepOutcome, StepError> {
    let rejected = cx
        .outputs
        .get("review")
        .and_then(|r| r["decisions"].as_array())
        .is_some_and(|d| d.iter().any(|x| x["decision"] == "rejected"));
    if rejected {
        return Ok(StepOutcome::Skip(
            "the curation was rejected in Board; the live curation stays".into(),
        ));
    }
    let id = draft_id(cx)?;
    // The G-tier rule: an unused approval of the `publish` gate, for the hash Board showed. The
    // transaction holds the run row (`for share`) until the send is done, so a cancel cannot land between.
    let mut tx = cx.db.begin().await.map_err(db_err)?;
    let auth = gates::authorize(
        &mut tx,
        GRequest {
            run_id: cx.run_id,
            step_seq: cx.seq,
            command: GCommand::HomePublish,
            item: None,
        },
    )
    .await
    .map_err(step_err)?;
    // The draft as stored now must be the subject Board approved.
    let st = store::get(&cx.db, id).await.map_err(step_err)?;
    let now_sha = gates::subject_sha(&subject(id, &st, &preview(cx)));
    if auth.approved.map(|a| a.subject_sha) != Some(now_sha) {
        return Err(step_err(news_core::NewsError::gated(format!(
            "not authorised: curation draft {id} is not the one Board approved"
        ))));
    }
    let sent = ctx
        .home
        .publish(&cx.db, id, cx.now)
        .await
        .map_err(step_err)?;
    tx.commit().await.map_err(db_err)?;
    Ok(StepOutcome::Done(json!({
        "draft_id": id, "published": sent, "wrap": {"published": 1},
    })))
}
