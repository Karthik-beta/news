//! Audit's steps (Bureau §8.11), bound to Chief by [`register`]: load the golden sets, score the
//! candidate's stored outputs, compare it with the live version, ask for the promote approval, and
//! promote in process under the G-tier rule.
//!
//! A version is audited from the outputs stored under `pipeline/eval/<set>/outputs/<version>.jsonl`.
//! When the writer suite's (G1) are missing, step 3 makes them: one Ghost `ai.write` job per case with
//! the candidate's prefix, then the calls are written there for `report` to score. Other suites still
//! need their outputs stored. No A12 judge has been checked against Karthik's labels yet, so step 4 is
//! skipped and the report rests on the deterministic metrics alone (the fallback in Bureau §9.1).

use crate::eval;
use crate::golden;
use news_chief::BoxFut;
use news_chief::driver::{Handlers, StepCx, StepError, StepHandler, StepOutcome};
use news_chief::gates::{self, GRequest, GateOpen};
use news_chief::presets::GCommand;
use news_core::job::ErrorClass;
use news_core::{ErrorCode, NewsError};
use news_dispatch::queue::NewJob;
use news_store::map_err;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The output schema of an A1 call, relative to the repository root.
const WRITER_SCHEMA: &str = "contracts/cli/ai/page-draft.v1.schema.json";

#[derive(Clone, Copy)]
enum Step {
    Load,
    Metrics,
    Candidate,
    Judges,
    Report,
    Review,
    Promote,
}

struct Audit {
    root: Arc<PathBuf>,
    step: Step,
}

const STEPS: [(&str, Step); 7] = [
    ("audit.load", Step::Load),
    ("audit.metrics", Step::Metrics),
    ("audit.candidate", Step::Candidate),
    ("audit.judges", Step::Judges),
    ("audit.report", Step::Report),
    ("audit.review", Step::Review),
    ("audit.promote", Step::Promote),
];

/// Bind the Audit preset's steps, reading the golden sets under `root` (the repository).
pub fn register(h: Handlers, root: PathBuf) -> Handlers {
    let root = Arc::new(root);
    STEPS.into_iter().fold(h, |h, (name, step)| {
        h.with(
            name,
            Arc::new(Audit {
                root: root.clone(),
                step,
            }),
        )
    })
}

/// Every Audit step fails with `message` (the repository root is not set), so a run says why instead
/// of ending at a step that is "not built".
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

/// Bind the Audit preset's steps to fail with `message`.
pub fn register_unset(h: Handlers, message: String) -> Handlers {
    let unset = Arc::new(Unset(message));
    STEPS
        .into_iter()
        .fold(h, |h, (name, _)| h.with(name, unset.clone()))
}

/// A retryable error (a busy database) is re-entered; anything else (an unknown suite, a bad set, a
/// missing output) fails the run, because re-entering would meet it again.
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

fn arg<'a>(cx: &'a StepCx, name: &str) -> Result<&'a str, StepError> {
    cx.args[name].as_str().ok_or_else(|| StepError::Fail {
        class: ErrorClass::InvalidData,
        message: format!("the run has no {name}"),
    })
}

/// The sets of `suite` that have no stored outputs for `version`. A version that could name a path is
/// refused first; a stored file that is broken is not "missing" (the report fails on it instead).
fn missing(root: &Path, suite: &str, version: &str) -> Result<Vec<String>, StepError> {
    golden::outputs_path(root, "g1", version).map_err(step_err)?;
    Ok(golden::sets_of(suite)
        .map_err(step_err)?
        .iter()
        .filter(|set| {
            matches!(golden::outputs(root, set, version), Err(e) if e.code == ErrorCode::NotFound)
        })
        .map(|s| (*s).to_owned())
        .collect())
}

impl StepHandler for Audit {
    fn run<'a>(&'a self, cx: &'a mut StepCx) -> BoxFut<'a, Result<StepOutcome, StepError>> {
        Box::pin(async move {
            let suite = arg(cx, "suite")?.to_owned();
            let version = arg(cx, "prompt_version")?.to_owned();
            let root = self.root.as_path();
            match self.step {
                Step::Load => {
                    let mut sets = Vec::new();
                    for set in golden::sets_of(&suite).map_err(step_err)? {
                        let s = golden::load(root, set).map_err(step_err)?;
                        sets.push(json!({"set": set, "version": s.manifest.version,
                                         "cases": s.cases.len(), "problems": s.problems.len()}));
                    }
                    Ok(StepOutcome::Done(json!({"sets": sets})))
                }
                Step::Metrics => {
                    let missing = missing(root, &suite, &version)?;
                    Ok(StepOutcome::Done(
                        json!({"stored": missing.is_empty(), "missing": missing}),
                    ))
                }
                Step::Candidate => {
                    let missing = missing(root, &suite, &version)?;
                    if missing.is_empty() {
                        return Ok(StepOutcome::Skip(format!(
                            "the outputs of {version} are stored; no fresh calls"
                        )));
                    }
                    if missing.iter().any(|s| s != "g1") {
                        return Err(StepError::Fail {
                            class: ErrorClass::Bug,
                            message: format!(
                                "fresh candidate calls are built for the writer suite (G1) only (#19); store {version}'s outputs for {} under pipeline/eval/<set>/outputs/",
                                missing.join(", ")
                            ),
                        });
                    }
                    fresh_writer(cx, root, &version).await
                }
                Step::Judges => Ok(StepOutcome::Skip(
                    "no A12 judge has been checked against Karthik's labels yet; deterministic metrics only".into(),
                )),
                Step::Report => report(cx, root, &suite, &version).await,
                Step::Review => {
                    let r = &cx.outputs["report"];
                    Ok(StepOutcome::OpenGates(vec![GateOpen {
                        gate: "promote".into(),
                        subject: json!({"suite": suite, "prompt_version": version,
                                        "eval_run": r["eval_run"], "steps": r["steps"]}),
                        summary: json!({"text": r["text"], "passed": r["passed"],
                                        "failed": r["failed"], "regressions": r["regressions"]}),
                        preview: None,
                    }]))
                }
                Step::Promote => promote(cx, &version).await,
            }
        })
    }
}

/// Fresh A1 calls for the candidate on G1 (Bureau §8.11 step 3): one Ghost `ai.write` job per case, the
/// case's brief as input and the candidate's prefix for its page type
/// (`work/prefix/<version>/<page_type>.md`) as the system prompt. Jobs are keyed by run and case, so a
/// re-entered step queues only what is missing. When none is live, each case's output and `ai_calls`
/// rows become `pipeline/eval/g1/outputs/<version>.jsonl`, which `report` scores.
async fn fresh_writer(cx: &StepCx, root: &Path, version: &str) -> Result<StepOutcome, StepError> {
    let set = golden::load(root, "g1").map_err(step_err)?;
    let key = |case: &str| format!("audit:{}:g1:{case}", cx.short_id);
    let mut jobs = Vec::new();
    let mut absent = BTreeSet::new();
    for case in &set.cases {
        let id = case["id"].as_str().unwrap_or_default();
        let prefix = format!(
            "work/prefix/{version}/{}.md",
            case["page_type"].as_str().unwrap_or_default()
        );
        if !root.join(&prefix).is_file() {
            absent.insert(prefix);
            continue;
        }
        let input = format!("work/audit/{}/g1/{id}.json", cx.short_id);
        jobs.push((id, prefix, input, &case["brief"]));
    }
    if !absent.is_empty() {
        return Err(StepError::Fail {
            class: ErrorClass::InvalidData,
            message: format!(
                "the candidate {version} has no prefix {}: write its prefixes under work/prefix/{version}/",
                absent.into_iter().collect::<Vec<_>>().join(", ")
            ),
        });
    }
    // Calls that share a prefix run back to back, so the prompt cache stays warm.
    jobs.sort_by(|a, b| a.1.cmp(&b.1));
    let db_err = |e: sqlx::Error| step_err(map_err(e));
    for (id, prefix, input, brief) in &jobs {
        // A job of this run that already ran, even one that failed, is not queued again.
        let known: bool = sqlx::query_scalar(
            "select exists (select 1 from jobs where run_id = $1 and unique_key = $2)",
        )
        .bind(cx.run_id)
        .bind(key(id))
        .fetch_one(&cx.db)
        .await
        .map_err(db_err)?;
        if known {
            continue;
        }
        let file = root.join(input);
        file.parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&file, brief.to_string()))
            .map_err(|e| StepError::Retry(format!("cannot write {input}: {e}")))?;
        // `audit` tells the accept step to validate only: an Audit output is never a page.
        cx.enqueue(NewJob::new("ai.write").unique_key(key(id)).args(
            json!({"step": "A1", "input": input, "schema": WRITER_SCHEMA, "prefix": prefix, "audit": true}),
        ))
        .await
        .map_err(step_err)?;
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
    let done: Vec<(String, i64, String, Option<Value>)> = sqlx::query_as(
        "select unique_key, job_id, state, result from jobs where run_id = $1 and step_seq = $2",
    )
    .bind(cx.run_id)
    .bind(cx.seq)
    .fetch_all(&cx.db)
    .await
    .map_err(db_err)?;
    let done: HashMap<String, (i64, String, Option<Value>)> = done
        .into_iter()
        .map(|(k, id, state, result)| (k, (id, state, result)))
        .collect();
    // Only calls that reached a verdict count; a usage-limit or timed-out call is not a try.
    let rows: Vec<(i64, String, String, i16, i64)> = sqlx::query_as(
        "select job_id, step, validation, repair_round, tokens from ai_calls
          where run_id = $1 and validation <> '' order by call_id",
    )
    .bind(cx.run_id)
    .fetch_all(&cx.db)
    .await
    .map_err(db_err)?;
    let mut lines = String::new();
    let mut failed = 0;
    for (id, ..) in &jobs {
        let (job, state, result) = done.get(&key(id)).cloned().unwrap_or_default();
        let output = result
            .as_ref()
            .filter(|_| state == "completed")
            .and_then(|r| r["output"].as_str())
            .and_then(|f| std::fs::read_to_string(f).ok())
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .filter(Value::is_object);
        failed += usize::from(output.is_none());
        let calls: Vec<Value> = rows
            .iter()
            .filter(|r| r.0 == job)
            .map(|(_, step, validation, round, tokens)| {
                json!({"step": step, "validation": validation, "repair_round": round, "tokens": tokens})
            })
            .collect();
        let _ = writeln!(
            lines,
            "{}",
            json!({"case": id, "output": output, "calls": calls})
        );
    }
    let path = golden::outputs_path(root, "g1", version).map_err(step_err)?;
    let tmp = path.with_extension("jsonl.tmp");
    path.parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&tmp, &lines))
        .and_then(|()| std::fs::rename(&tmp, &path))
        .map_err(|e| StepError::Retry(format!("cannot write {}: {e}", path.display())))?;
    Ok(StepOutcome::Done(json!({
        "calls": jobs.len(), "failed": failed,
        "outputs": format!("pipeline/eval/g1/outputs/{version}.jsonl"),
        "summary": {"text": format!("{} fresh A1 calls for {version}: {failed} without an output", jobs.len())},
    })))
}

/// Score the stored outputs, keep the `eval_runs` row (once per run), and compare each set with the
/// live version of its step.
async fn report(
    cx: &StepCx,
    root: &Path,
    suite: &str,
    version: &str,
) -> Result<StepOutcome, StepError> {
    let results = eval::score(root, suite, Some(version)).map_err(step_err)?;
    let steps = eval::steps_of(suite).map_err(step_err)?;
    let mut text = eval::render(suite, Some(version), &results);
    let mut regressions = Vec::new();
    let _ = writeln!(text, "\nAgainst the live versions:");
    for ((m, _), step) in results.iter().zip(&steps) {
        let live = eval::live(&cx.db, step).await.map_err(step_err)?;
        let metrics = match &live {
            Some(v) => eval::latest(&cx.db, &m.set, v).await.map_err(step_err)?,
            None => None,
        };
        match (&live, metrics) {
            (None, _) => {
                let _ = writeln!(text, "  {step}: no live version yet");
            }
            (Some(v), None) => {
                let _ = writeln!(text, "  {step}: live {v} has no scored run of {}", m.set);
            }
            (Some(v), Some(live_m)) => {
                let one =
                    std::slice::from_ref(results.iter().find(|(x, _)| x.set == m.set).ok_or_else(
                        || StepError::Fail {
                            class: ErrorClass::Bug,
                            message: "set vanished".into(),
                        },
                    )?);
                let r = eval::regressions(one, &live_m);
                if r.is_empty() {
                    let _ = writeln!(text, "  {step}: no regression against {v}");
                }
                for (k, c, l) in r {
                    let _ = writeln!(text, "  {step}: REGRESSION {k} {c} (live {v}: {l})");
                    regressions
                        .push(json!({"metric": k, "candidate": c, "live": l, "live_version": v}));
                }
            }
        }
    }
    // A re-entered step reuses this run's row instead of adding a second one.
    let existing: Option<i64> = sqlx::query_scalar(
        "select eval_run_id from eval_runs where run_id = $1 order by eval_run_id limit 1",
    )
    .bind(cx.run_id)
    .fetch_optional(&cx.db)
    .await
    .map_err(|e| step_err(map_err(e)))?;
    let id = match existing {
        Some(id) => id,
        None => eval::store(
            &cx.db,
            suite,
            Some(version),
            Some(cx.run_id),
            &results,
            &text,
        )
        .await
        .map_err(step_err)?,
    };
    let passed: u32 = results.iter().map(|(_, s)| s.passed).sum();
    let failed: u32 = results.iter().map(|(_, s)| s.failed).sum();
    Ok(StepOutcome::Done(json!({
        "eval_run": format!("e{id}"), "eval_run_id": id, "steps": steps,
        "passed": passed, "failed": failed, "regressions": regressions, "text": text,
    })))
}

/// Promote every step of the suite, in the step's authorised transaction; skipped unless the gate was
/// approved.
async fn promote(cx: &StepCx, version: &str) -> Result<StepOutcome, StepError> {
    let approved = cx.outputs["review"]["decisions"]
        .as_array()
        .is_some_and(|d| {
            d.iter()
                .any(|d| d["gate"] == "promote" && d["decision"] == "approved")
        });
    if !approved {
        return Ok(StepOutcome::Skip("the promotion was not approved".into()));
    }
    let report = &cx.outputs["report"];
    let id = report["eval_run_id"]
        .as_i64()
        .ok_or_else(|| StepError::Fail {
            class: ErrorClass::Bug,
            message: "the report step recorded no evaluation run".into(),
        })?;
    let steps: Vec<String> = report["steps"]
        .as_array()
        .map(|s| {
            s.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let mut tx = cx.db.begin().await.map_err(|e| step_err(map_err(e)))?;
    let mut auth = gates::authorize(
        &mut tx,
        GRequest {
            run_id: cx.run_id,
            step_seq: cx.seq,
            command: GCommand::EvalPromote,
            item: None,
        },
    )
    .await
    .map_err(step_err)?;
    let mut promoted = Vec::new();
    for step in &steps {
        eval::promote(&mut auth, step, version, id)
            .await
            .map_err(step_err)?;
        promoted.push(json!({"step": step, "prompt_version": version}));
    }
    tx.commit().await.map_err(|e| step_err(map_err(e)))?;
    Ok(StepOutcome::Done(
        json!({"promoted": promoted, "eval_run": report["eval_run"]}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_map_to_retry_or_fail() {
        assert!(matches!(
            step_err(NewsError::not_found("x")),
            StepError::Fail {
                class: ErrorClass::InvalidData,
                ..
            }
        ));
        assert!(matches!(
            step_err(NewsError::internal("x")),
            StepError::Fail {
                class: ErrorClass::Bug,
                ..
            }
        ));
    }

    #[test]
    fn every_audit_step_is_bound() {
        let p = news_chief::presets::preset("audit").unwrap_or_else(|| panic!("audit"));
        let declared: Vec<&str> = p.steps.iter().map(|s| s.handler).collect();
        let bound: Vec<&str> = STEPS.iter().map(|(n, _)| *n).collect();
        assert_eq!(declared, bound);
    }
}
