//! `news eval run|report|promote` (Bureau §6.4): score a suite's golden sets and keep the result.
//!
//! `run` without a prompt version checks the sets alone (schemas, briefs, tokens); with one, it scores
//! that version's stored outputs (`pipeline/eval/<set>/outputs/<version>.jsonl`). Either way the
//! result is an `eval_runs` row, named `e<id>`, and `report` prints it back. [`promote`] is what
//! Audit's G-tier `promote` step runs; the CLI form is always `gated`.

use crate::golden::{self, Manifest};
use crate::metrics::{self, Score};
use news_chief::gates::Authorized;
use news_chief::presets::GCommand;
use news_core::{NewsError, NewsResult};
use news_store::{PgPool, map_err};
use serde_json::{Map, Value, json};
use std::fmt::Write as _;
use std::path::Path;
use uuid::Uuid;

/// How many failed cases a report lists per set.
const LISTED: usize = 10;

/// Score every set of `suite`.
pub fn score(
    root: &Path,
    suite: &str,
    prompt_version: Option<&str>,
) -> NewsResult<Vec<(Manifest, Score)>> {
    golden::sets_of(suite)?
        .iter()
        .map(|set| {
            let loaded = golden::load(root, set)?;
            let outputs = prompt_version
                .map(|v| golden::outputs(root, set, v))
                .transpose()?;
            let s = metrics::score(&loaded, outputs.as_deref());
            Ok((loaded.manifest, s))
        })
        .collect()
}

/// `<set>.<metric>` → value, the shape `eval report` returns.
pub fn flat(results: &[(Manifest, Score)]) -> Map<String, Value> {
    results
        .iter()
        .flat_map(|(m, s)| {
            s.metrics
                .iter()
                .map(move |(k, v)| (format!("{}.{k}", m.set), json!(v)))
        })
        .collect()
}

fn fmt(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.3}")
    }
}

/// The text Karthik reads: per set, cases against the target count, each metric against its target,
/// and the first failed cases.
pub fn render(suite: &str, prompt_version: Option<&str>, results: &[(Manifest, Score)]) -> String {
    let mut t = String::new();
    let what = prompt_version.map_or_else(
        || "golden sets only (no outputs scored)".to_owned(),
        |v| format!("prompt version {v}"),
    );
    let _ = writeln!(t, "Critic: suite {suite}, {what}");
    for (m, s) in results {
        let _ = writeln!(
            t,
            "\n{} {} (v{}, {}): {} passed, {} failed; {} of {} target cases",
            m.set.to_uppercase(),
            m.name,
            m.version,
            m.step,
            s.passed,
            s.failed,
            fmt(s.metrics.get("cases").copied().unwrap_or(0.0)),
            m.target_cases
        );
        for (k, v) in s.metrics.iter().filter(|(k, _)| *k != "cases") {
            let target = m.targets.get(k).map_or_else(String::new, |b| {
                let mut parts = Vec::new();
                if let Some(x) = b.min {
                    parts.push(format!(">= {}", fmt(x)));
                }
                if let Some(x) = b.max {
                    parts.push(format!("<= {}", fmt(x)));
                }
                let mark = if b.met(*v) { "met" } else { "MISSED" };
                format!("  (target {}: {mark})", parts.join(", "))
            });
            let _ = writeln!(t, "  {k}: {}{target}", fmt(*v));
        }
        if prompt_version.is_some() {
            for (k, _) in m
                .targets
                .iter()
                .filter(|(k, _)| !s.metrics.contains_key(*k))
            {
                let _ = writeln!(t, "  {k}: not measured");
            }
        }
        for (id, why) in s.failures.iter().take(LISTED) {
            let _ = writeln!(t, "  failed {id}: {why}");
        }
        if s.failures.len() > LISTED {
            let _ = writeln!(t, "  … and {} more", s.failures.len() - LISTED);
        }
    }
    t
}

/// `news eval run`: score, store, and return `{eval_run, passed, failed}`.
pub async fn run(
    db: &PgPool,
    root: &Path,
    suite: &str,
    prompt_version: Option<&str>,
) -> NewsResult<Value> {
    let results = score(root, suite, prompt_version)?;
    let text = render(suite, prompt_version, &results);
    let id = store(db, suite, prompt_version, None, &results, &text).await?;
    let passed: u32 = results.iter().map(|(_, s)| s.passed).sum();
    let failed: u32 = results.iter().map(|(_, s)| s.failed).sum();
    Ok(json!({"eval_run": format!("e{id}"), "passed": passed, "failed": failed}))
}

/// Keep a scored suite as an `eval_runs` row (`run_id` is the Audit run, if any); returns its id.
pub async fn store(
    db: &PgPool,
    suite: &str,
    prompt_version: Option<&str>,
    run_id: Option<Uuid>,
    results: &[(Manifest, Score)],
    text: &str,
) -> NewsResult<i64> {
    let passed: u32 = results.iter().map(|(_, s)| s.passed).sum();
    let failed: u32 = results.iter().map(|(_, s)| s.failed).sum();
    sqlx::query_scalar(
        "insert into eval_runs (suite, prompt_version, run_id, passed, failed, metrics, report)
         values ($1, $2, $3, $4, $5, $6, $7) returning eval_run_id",
    )
    .bind(suite)
    .bind(prompt_version.unwrap_or_default())
    .bind(run_id)
    .bind(passed as i32)
    .bind(failed as i32)
    .bind(Value::Object(flat(results)))
    .bind(text)
    .fetch_one(db)
    .await
    .map_err(map_err)
}

/// The AI steps a suite's prompts belong to (what `news eval promote <step>` names).
pub fn steps_of(suite: &str) -> NewsResult<Vec<&'static str>> {
    golden::sets_of(suite)?
        .iter()
        .map(|set| match *set {
            "g1" => Ok("A1"),
            "g2" => Ok("A3"),
            "g5" => Ok("A8"),
            "g6" => Ok("A7"),
            other => Err(NewsError::internal(format!("set {other} has no step"))),
        })
        .collect()
}

/// The live prompt version of a step: its latest promotion, if any.
pub async fn live(db: &PgPool, step: &str) -> NewsResult<Option<String>> {
    sqlx::query_scalar(
        "select prompt_version from prompt_promotions where step = $1
          order by promoted_at desc, promotion_id desc limit 1",
    )
    .bind(step)
    .fetch_optional(db)
    .await
    .map_err(map_err)
}

/// The flat metrics of the latest run that scored `prompt_version` on `set` (by any suite).
pub async fn latest(
    db: &PgPool,
    set: &str,
    prompt_version: &str,
) -> NewsResult<Option<Map<String, Value>>> {
    let m: Option<Value> = sqlx::query_scalar(
        "select metrics from eval_runs where prompt_version = $1 and metrics ? $2
          order by eval_run_id desc limit 1",
    )
    .bind(prompt_version)
    .bind(format!("{set}.cases"))
    .fetch_optional(db)
    .await
    .map_err(map_err)?;
    Ok(m.map(|m| m.as_object().cloned().unwrap_or_default()))
}

/// The metrics where the candidate is worse than the live version, judged by each target's direction
/// (a `min` target regresses when the value falls, a `max` one when it rises): `(metric, candidate,
/// live)`. Metrics without a target have no direction and are not compared.
pub fn regressions(
    results: &[(Manifest, Score)],
    live: &Map<String, Value>,
) -> Vec<(String, f64, f64)> {
    let mut out = Vec::new();
    for (m, s) in results {
        for (k, b) in &m.targets {
            let key = format!("{}.{k}", m.set);
            let (Some(&c), Some(l)) = (s.metrics.get(k), live.get(&key).and_then(Value::as_f64))
            else {
                continue;
            };
            if (b.min.is_some() && c < l) || (b.max.is_some() && c > l) {
                out.push((key, c, l));
            }
        }
    }
    out
}

/// `news eval promote <step> <prompt_version>` as Chief runs it in process, inside the authorised
/// transaction of Audit's `promote` step (G tier, Bureau §5.5). `false` when this run had already
/// promoted it (a re-entered step).
pub async fn promote(
    auth: &mut Authorized<'_>,
    step: &str,
    prompt_version: &str,
    eval_run_id: i64,
) -> NewsResult<bool> {
    if auth.command != GCommand::EvalPromote {
        return Err(NewsError::gated(
            "the authorisation is not for news eval promote",
        ));
    }
    let run_id = auth.run_id;
    let n = sqlx::query(
        "insert into prompt_promotions (step, prompt_version, eval_run_id, run_id)
         select $1, $2, $3, $4 from eval_runs where eval_run_id = $3 and prompt_version = $2
         on conflict (run_id, step) do nothing",
    )
    .bind(step)
    .bind(prompt_version)
    .bind(eval_run_id)
    .bind(run_id)
    .execute(auth.conn())
    .await
    .map_err(map_err)?
    .rows_affected();
    if n > 0 {
        return Ok(true);
    }
    let mine: Option<String> = sqlx::query_scalar(
        "select prompt_version from prompt_promotions where run_id = $1 and step = $2",
    )
    .bind(run_id)
    .bind(step)
    .fetch_optional(auth.conn())
    .await
    .map_err(map_err)?;
    match mine {
        Some(v) if v == prompt_version => Ok(false),
        Some(v) => Err(NewsError::conflict(format!(
            "this run already promoted {v} for {step}"
        ))),
        None => Err(NewsError::not_found(format!(
            "evaluation run e{eval_run_id} did not score {prompt_version}"
        ))),
    }
}

/// `news eval report <eval_run>`.
pub async fn report(db: &PgPool, eval_run: &str) -> NewsResult<Value> {
    let id: i64 = eval_run
        .strip_prefix('e')
        .and_then(|n| n.parse().ok())
        .filter(|n| *n > 0)
        .ok_or_else(|| {
            NewsError::usage(format!("{eval_run:?} is not an evaluation run (e<number>)"))
        })?;
    let row: Option<(Value, String)> =
        sqlx::query_as("select metrics, report from eval_runs where eval_run_id = $1")
            .bind(id)
            .fetch_optional(db)
            .await
            .map_err(map_err)?;
    let (metrics, text) =
        row.ok_or_else(|| NewsError::not_found(format!("no evaluation run {eval_run}")))?;
    Ok(json!({"eval_run": eval_run, "metrics": metrics, "text": text}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::golden::Bound;
    use std::collections::BTreeMap;

    #[test]
    fn report_text_marks_targets_and_lists_failures() {
        let m = Manifest {
            set: "g6".into(),
            name: "Seeded faults".into(),
            suite: "spotcheck".into(),
            version: 1,
            step: "A7".into(),
            target_cases: 50,
            targets: BTreeMap::from([
                (
                    "a7_recall".into(),
                    Bound {
                        min: Some(0.7),
                        max: None,
                    },
                ),
                (
                    "false_flag_rate".into(),
                    Bound {
                        min: None,
                        max: Some(0.1),
                    },
                ),
            ]),
            notes: None,
        };
        let s = Score {
            metrics: BTreeMap::from([
                ("cases".into(), 52.0),
                ("a7_recall".into(), 0.75),
                ("false_flag_rate".into(), 0.2),
            ]),
            passed: 40,
            failed: 12,
            failures: (0..12)
                .map(|i| (format!("g6-{i}"), "missed".into()))
                .collect(),
        };
        let r = [(m, s)];
        let t = render("spotcheck", Some("v2"), &r);
        assert!(
            t.contains("G6 Seeded faults (v1, A7): 40 passed, 12 failed; 52 of 50 target cases")
        );
        assert!(t.contains("a7_recall: 0.750  (target >= 0.700: met)"));
        assert!(t.contains("false_flag_rate: 0.200  (target <= 0.100: MISSED)"));
        assert!(t.contains("… and 2 more"));
        assert_eq!(flat(&r)["g6.a7_recall"], json!(0.75));
    }
}
