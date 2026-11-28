//! Huddle's steps (Bureau §4.7, §8.12), bound to Chief by [`register`]: the Debrief, the calibration
//! form Karthik answers in Board, the fit, one approval per proposed change, and adoption under the
//! G-tier rule. No model is involved anywhere.

use crate::audit::step_err;
use crate::debrief;
use news_chief::BoxFut;
use news_chief::driver::{Handlers, StepCx, StepError, StepHandler, StepOutcome};
use news_chief::gates::{self, GRequest, GateOpen};
use news_chief::presets::GCommand;
use news_core::ErrorCode;
use news_nose::config::Weights;
use news_nose::huddle::{self, Proposal};
use news_store::map_err;
use serde_json::{Value, json};
use std::fmt::Write as _;
use std::sync::Arc;

#[derive(Clone, Copy)]
enum Step {
    Debrief,
    Calibration,
    Propose,
    Review,
    Apply,
}

const STEPS: [(&str, Step); 5] = [
    ("huddle.debrief", Step::Debrief),
    ("huddle.calibration", Step::Calibration),
    ("huddle.propose", Step::Propose),
    ("huddle.review", Step::Review),
    ("huddle.apply", Step::Apply),
];

struct Huddle(Step);

/// Bind the Huddle preset's steps.
pub fn register(h: Handlers) -> Handlers {
    STEPS
        .into_iter()
        .fold(h, |h, (name, step)| h.with(name, Arc::new(Huddle(step))))
}

impl StepHandler for Huddle {
    fn run<'a>(&'a self, cx: &'a mut StepCx) -> BoxFut<'a, Result<StepOutcome, StepError>> {
        Box::pin(async move {
            match self.0 {
                Step::Debrief => {
                    let d = debrief::weekly(&cx.db, cx.now).await.map_err(step_err)?;
                    Ok(StepOutcome::Done(json!({"summary": d})))
                }
                Step::Calibration => calibration(cx).await,
                Step::Propose => match huddle::propose(&cx.db).await {
                    Ok(p) => Ok(StepOutcome::Done(
                        json!({"proposal": p, "summary": {"text": proposal_text(&p)}}),
                    )),
                    // Too few labels since the live version: nothing to approve this week.
                    Err(e) if e.code == ErrorCode::Precondition => Ok(StepOutcome::Done(
                        json!({"proposal": null, "summary": {"text": e.message}}),
                    )),
                    Err(e) => Err(step_err(e)),
                },
                Step::Review => {
                    let out = &cx.outputs["propose"];
                    let p = &out["proposal"];
                    let Some(version) = p["version"].as_str() else {
                        return Ok(StepOutcome::Skip("no change was proposed".into()));
                    };
                    // The approval binds the row's exact weights; `apply` adopts only those.
                    let weights: Value = sqlx::query_scalar(
                        "select weights from scorer_versions where version = $1 and status = 'proposed'",
                    )
                    .bind(version)
                    .fetch_one(&cx.db)
                    .await
                    .map_err(|e| step_err(map_err(e)))?;
                    Ok(StepOutcome::OpenGates(vec![GateOpen {
                        gate: format!("change:{version}"),
                        // One weight change, approved as a whole.
                        whole_subject: true,
                        subject: json!({"change": "weights", "version": version,
                                        "parent": p["parent"], "diff": p["diff"], "weights": weights}),
                        summary: json!({"text": out["summary"]["text"], "labels": p["labels"],
                                        "agreement_before": p["agreement_before"],
                                        "agreement_after": p["agreement_after"], "weights": weights}),
                        preview: None,
                    }]))
                }
                Step::Apply => apply(cx).await,
            }
        })
    }
}

/// The first entry opens the form: the pairwise questions chosen by code (none when no story was scored
/// this week; the Search Console status and the reading sample are still asked). Chief enters the step
/// again once `form.submit` has kept the answer; the labels are then stored as `news scores label` does.
async fn calibration(cx: &StepCx) -> Result<StepOutcome, StepError> {
    let Some(answer) = cx.previous.as_ref().and_then(|p| p.get("answer")) else {
        let qs = huddle::questions(&cx.db, cx.now, huddle::QUESTIONS)
            .await
            .map_err(step_err)?;
        let text = match qs.len() {
            0 => "No two stories to compare this week: the Search Console status and the reading sample only"
                .to_owned(),
            n => format!("{n} pairwise questions: which story deserved a page first?"),
        };
        return Ok(StepOutcome::OpenForm(
            json!({"questions": qs, "text": text}),
        ));
    };
    let stored = if answer["labels"].as_array().is_some_and(|l| !l.is_empty()) {
        let labels = huddle::parse_labels(answer).map_err(step_err)?;
        huddle::store_labels(&cx.db, &cx.run_id.to_string(), &labels)
            .await
            .map_err(step_err)?
    } else {
        0
    };
    Ok(StepOutcome::Done(json!({
        "answer": answer, "labels": stored,
        "summary": {"text": format!("{stored} answers stored"), "labels": stored},
    })))
}

/// The text Board shows with the proposal, rendered by code.
fn proposal_text(p: &Proposal) -> String {
    let mut t = format!(
        "Scorer {} from {}, fitted on {} labels:",
        p.version, p.parent, p.labels
    );
    for c in &p.diff {
        let _ = write!(t, " {} {:.4} → {:.4};", c.weight, c.from, c.to);
    }
    t.pop();
    let tau = |x: Option<f64>| x.map_or_else(|| "n/a".to_owned(), |v| format!("{v:.2}"));
    let _ = write!(
        t,
        ". Agreement with the {} held-out labels: {} → {}.",
        p.held_out,
        tau(p.agreement_before),
        tau(p.agreement_after)
    );
    t
}

/// Adopt each change Karthik approved, in the transaction its G-tier authorisation was checked in;
/// skipped when none was.
async fn apply(cx: &StepCx) -> Result<StepOutcome, StepError> {
    let approved: Vec<String> = cx.outputs["review"]["decisions"]
        .as_array()
        .map(|d| {
            d.iter()
                .filter(|d| d["decision"] == "approved")
                .filter_map(|d| {
                    d["gate"]
                        .as_str()?
                        .strip_prefix("change:")
                        .map(str::to_owned)
                })
                .collect()
        })
        .unwrap_or_default();
    if approved.is_empty() {
        return Ok(StepOutcome::Skip("no change was approved".into()));
    }
    let mut tx = cx.db.begin().await.map_err(|e| step_err(map_err(e)))?;
    let mut adopted = Vec::new();
    for version in &approved {
        let mut auth = gates::authorize(
            &mut tx,
            GRequest {
                run_id: cx.run_id,
                step_seq: cx.seq,
                command: GCommand::ScoresAdopt,
                item: Some(version),
            },
        )
        .await
        .map_err(step_err)?;
        // The weights the approved gate held (its summary is written with its subject, whose hash
        // the approval names).
        let approved: Value = sqlx::query_scalar(
            "select g.summary->'weights' from gates g
               join approvals a on a.run_id = g.run_id and a.gate = g.gate and a.round = g.round
                               and a.subject_sha = g.subject_sha
              where g.run_id = $1 and g.gate = $2 and a.decision = 'approved'
              order by g.round desc limit 1",
        )
        .bind(cx.run_id)
        .bind(format!("change:{version}"))
        .fetch_one(auth.conn())
        .await
        .map_err(|e| step_err(map_err(e)))?;
        let expected: Weights = serde_json::from_value(approved).map_err(|e| StepError::Fail {
            class: news_core::job::ErrorClass::Bug,
            message: format!("the approval of change:{version} holds no weights: {e}"),
        })?;
        let a = huddle::adopt_in(auth.conn(), version, Some(&expected))
            .await
            .map_err(step_err)?;
        adopted.push(json!(a));
    }
    tx.commit().await.map_err(|e| step_err(map_err(e)))?;
    let names: Vec<&str> = adopted
        .iter()
        .filter_map(|a| a["version"].as_str())
        .collect();
    Ok(StepOutcome::Done(json!({
        "adopted": adopted,
        "summary": {"text": format!("Scorer {} is live", names.join(", "))},
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use news_nose::huddle::Change;

    #[test]
    fn every_huddle_step_is_bound() {
        let p = news_chief::presets::preset("huddle").unwrap_or_else(|| panic!("huddle"));
        let declared: Vec<&str> = p.steps.iter().map(|s| s.handler).collect();
        let bound: Vec<&str> = STEPS.iter().map(|(n, _)| *n).collect();
        assert_eq!(declared, bound);
    }

    #[test]
    fn the_proposal_text_lists_each_weight_and_the_agreement() {
        let p = Proposal {
            version: "v2".into(),
            parent: "v1".into(),
            labels: 20,
            diff: vec![Change {
                weight: "I".into(),
                from: 0.3,
                to: 0.35,
            }],
            held_out: 4,
            agreement_before: Some(0.5),
            agreement_after: None,
        };
        assert_eq!(
            proposal_text(&p),
            "Scorer v2 from v1, fitted on 20 labels: I 0.3000 → 0.3500. Agreement with the 4 held-out labels: 0.50 → n/a."
        );
    }
}
