//! Audit's steps (Bureau §8.11), bound to Chief by [`register`]: load the golden sets, score the
//! candidate's stored outputs, compare it with the live version, ask for the promote approval, and
//! promote in process under the G-tier rule.
//!
//! Fresh candidate calls (step 3) are not built: a version is audited from the outputs stored under
//! `pipeline/eval/<set>/outputs/<version>.jsonl`, and the step fails when any are missing. No A12
//! judge has been checked against Karthik's labels yet, so step 4 is skipped and the report rests on
//! the deterministic metrics alone (the fallback in Bureau §9.1).

use crate::eval;
use crate::golden;
use news_chief::BoxFut;
use news_chief::driver::{Handlers, StepCx, StepError, StepHandler, StepOutcome};
use news_chief::gates::{self, GRequest, GateOpen};
use news_chief::presets::GCommand;
use news_core::job::ErrorClass;
use news_core::{ErrorCode, NewsError};
use news_store::map_err;
use serde_json::json;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

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
fn step_err(e: NewsError) -> StepError {
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

/// The sets of `suite` that have no stored outputs for `version`.
fn missing(root: &std::path::Path, suite: &str, version: &str) -> Result<Vec<String>, StepError> {
    Ok(golden::sets_of(suite)
        .map_err(step_err)?
        .iter()
        .filter(|set| golden::outputs(root, set, version).is_err())
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
                    Err(StepError::Fail {
                        class: ErrorClass::Bug,
                        message: format!(
                            "fresh candidate calls are not built yet (#19); store {version}'s outputs for {} under pipeline/eval/<set>/outputs/",
                            missing.join(", ")
                        ),
                    })
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
                        // Promotion acts on the whole suite, not on items.
                        whole_subject: true,
                    }]))
                }
                Step::Promote => promote(cx, &version).await,
            }
        })
    }
}

/// Score the stored outputs, keep the `eval_runs` row (once per run), and compare each set with the
/// live version of its step.
async fn report(
    cx: &StepCx,
    root: &std::path::Path,
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
