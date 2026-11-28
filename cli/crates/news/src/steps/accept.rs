//! What Ghost hands each model output to (`news_ghost::AcceptSink`), by the step the job runs:
//!
//! * A1 and A2 (page Copy): the schema, Plate's parse and the Copy validators against the story's brief,
//!   then stored (`news ai accept`'s own rules). In update mode (a direction flip) the Copy uses the
//!   diff's new refs, which the story's brief has not seen, so the brief's reference check is left to
//!   Proof's C2 against the patched `data.json`. An Audit call (`args.audit`, #19) is checked against
//!   the brief it was given (`args.input`) and never stored: Ghost keeps the output file, which the job's
//!   result names, and Audit scores it.
//! * A7 (spot check): the verdict and issues shape; the step reads the file Ghost kept.
//! * A8 (the curator's home draft): `home-draft.v1`, parsed by `news_home::ops::parse_draft`; the Frontpage
//!   step saves and checks it.
//! * Every other step: nothing to validate here (not stored).
//!
//! The Bureau service installs Ghost with this as its sink (`registry::install_ai`).

use news_chief::BoxFut;
use news_core::job::{ErrorClass, JobFailure};
use news_ghost::{AcceptSink, Problem, Verdict};
use news_proof::plate::Plate;
use news_proof::validate::{RuleError, schema_errors, validate};
use news_proof::work::{self, Work, pretty, write_atomic};
use news_store::PgPool;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub struct NewsAccept {
    pub db: PgPool,
    /// The repository root: job argument paths are relative to it; the work folder is `<root>/work`.
    pub root: PathBuf,
}

fn bug(m: impl Into<String>) -> JobFailure {
    JobFailure::new(ErrorClass::Bug, m)
}

fn problem(rule: &str, message: impl Into<String>) -> Problem {
    Problem {
        rule: rule.into(),
        at: "/".into(),
        message: message.into(),
    }
}

fn invalid(errors: Vec<Problem>) -> Verdict {
    Verdict {
        valid: false,
        errors,
        stored: false,
    }
}

fn valid(stored: bool) -> Verdict {
    Verdict {
        valid: true,
        errors: vec![],
        stored,
    }
}

/// The verdict on an A8 output: a home draft, or why not.
pub fn home_draft(v: Value) -> Verdict {
    match news_home::ops::parse_draft(v) {
        Ok(_) => valid(false),
        Err(e) => invalid(vec![problem("schema", e.message)]),
    }
}

/// The verdict on an A7 output.
pub fn spotcheck(v: &Value) -> Verdict {
    let ok = matches!(v["verdict"].as_str(), Some("pass" | "flag")) && v["issues"].is_array();
    if ok {
        valid(false)
    } else {
        invalid(vec![problem(
            "schema",
            "a spot check has a verdict (pass or flag) and a list of issues",
        )])
    }
}

fn read_brief(path: &Path) -> Option<news_core::brief::Brief> {
    work::read_json(path)
        .ok()
        .and_then(|b| serde_json::from_value(b).ok())
}

/// The schema, then Plate's parse and the Copy validators (against `brief` when given).
async fn copy_errors(
    copy: &Value,
    brief: Option<&news_core::brief::Brief>,
) -> Result<Vec<RuleError>, JobFailure> {
    let errors = schema_errors(copy);
    if !errors.is_empty() {
        return Ok(errors);
    }
    let oui = copy["oui"].as_str().unwrap_or_default();
    let plate = Plate::connect()
        .await
        .map_err(|e| JobFailure::new(ErrorClass::Transient, e.message.clone()))?
        .check(oui)
        .await
        .map_err(|e| JobFailure::new(ErrorClass::Transient, e.message.clone()))?;
    Ok(validate(copy, brief, Some(&plate)))
}

impl NewsAccept {
    fn work(&self) -> Work {
        Work::new(self.root.join("work"))
    }

    async fn page(
        &self,
        job_id: i64,
        story: &str,
        update: bool,
        copy: Value,
    ) -> Result<Verdict, JobFailure> {
        let w = self.work();
        let brief = read_brief(&w.brief(story));
        let against = if update { None } else { brief.as_ref() };
        let errors = copy_errors(&copy, against).await?;
        if errors.is_empty() {
            work::save(
                &w,
                story,
                copy,
                vec![],
                "ai",
                Some(job_id),
                chrono::Utc::now(),
            )
            .map_err(|e| bug(e.message))?;
            return Ok(valid(true));
        }
        let rejected = serde_json::json!({"job": job_id, "copy": copy, "errors": errors});
        write_atomic(&w.rejected(story), &pretty(&rejected)).map_err(|e| bug(e.message))?;
        Ok(invalid(errors.into_iter().map(from_rule).collect()))
    }

    /// An Audit call's Copy: validated against the brief it was given, never stored as a page.
    async fn audit(&self, input: &str, copy: &Value) -> Result<Verdict, JobFailure> {
        // Ghost confined `input` to `work/` before the call.
        let brief = read_brief(&self.root.join(input))
            .ok_or_else(|| bug(format!("the Audit call's brief {input} cannot be read")))?;
        let errors = copy_errors(copy, Some(&brief)).await?;
        Ok(if errors.is_empty() {
            valid(false)
        } else {
            invalid(errors.into_iter().map(from_rule).collect())
        })
    }
}

fn from_rule(e: RuleError) -> Problem {
    Problem {
        rule: e.rule,
        at: e.at,
        message: e.message,
    }
}

impl AcceptSink for NewsAccept {
    fn accept<'a>(
        &'a self,
        job_id: i64,
        file: &'a Path,
    ) -> BoxFut<'a, Result<Verdict, JobFailure>> {
        Box::pin(async move {
            let job = news_dispatch::queue::get(&self.db, job_id)
                .await
                .map_err(|e| bug(e.message))?;
            let step = job.args["step"]
                .as_str()
                .or_else(|| news_ghost::default_step(&job.kind))
                .unwrap_or_default()
                .to_owned();
            let value: Value = match std::fs::read_to_string(file)
                .map_err(|e| e.to_string())
                .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
            {
                Ok(v) => v,
                Err(e) => return Ok(invalid(vec![problem("schema", format!("not JSON: {e}"))])),
            };
            match step.as_str() {
                "A8" => Ok(home_draft(value)),
                "A7" => Ok(spotcheck(&value)),
                // TODO(#103): Audit's fresh G1 calls queue these; whoever merges second checks that its
                // `critic::audit_makes_fresh_a1_calls...` test runs through this branch (the real sink).
                "A1" | "A2" if job.args["audit"] == true => {
                    self.audit(job.args["input"].as_str().unwrap_or_default(), &value)
                        .await
                }
                "A1" | "A2" => {
                    let story = match &job.args["story"] {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => n.to_string(),
                        _ => job.item_ref.clone().unwrap_or_default(),
                    };
                    work::check_story(&story).map_err(|e| bug(e.message))?;
                    self.page(job_id, &story, job.args["mode"] == "update", value)
                        .await
                }
                _ => Ok(valid(false)),
            }
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_home_draft_is_accepted_and_anything_else_is_refused_with_the_reason() {
        let bad = home_draft(json!({"slots": 1}));
        assert!(!bad.valid);
        assert!(
            bad.errors[0].message.contains("not a home draft"),
            "{:?}",
            bad.errors
        );
    }

    #[test]
    fn a_spot_check_needs_a_verdict_and_issues() {
        assert!(spotcheck(&json!({"verdict": "pass", "issues": []})).valid);
        assert!(!spotcheck(&json!({"verdict": "maybe", "issues": []})).valid);
        assert!(!spotcheck(&json!({"verdict": "flag"})).valid);
    }
}
