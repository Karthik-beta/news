//! Critic against a real database (#19): `news eval run` checks the golden sets or scores stored
//! outputs, stores an `eval_runs` row, and `news eval report` prints it back; the Audit preset runs end
//! to end through Chief, on stored outputs and on fresh A1 calls answered by a fake `claude`, and
//! promotes only after its approval.

use super::fixture::{Fixture, call};
use chrono::Utc;
use news_chief::driver::Handlers;
use news_chief::gates::{self, Answer, Decision};
use news_chief::{Chief, IfBusy, StartReq};
use news_core::clock::ManualClock;
use news_core::job::{ErrorClass, Handler, JobFailure, JobFuture, JobInput};
use news_ghost::store::{AiCall, PgStore, Store};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dest = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &dest);
        } else {
            std::fs::copy(e.path(), dest).unwrap();
        }
    }
}

/// A copy of the golden sets (and the brief schema they check against) in a scratch folder under the
/// build's own temporary directory, so a test can add stored outputs without touching the repository.
/// One folder per test: they run in parallel.
fn scratch_root(tag: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("news-critic-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    copy_dir(&repo().join("pipeline/eval"), &root.join("pipeline/eval"));
    let ai = root.join("contracts/cli/ai");
    std::fs::create_dir_all(&ai).unwrap();
    std::fs::copy(
        repo().join("contracts/cli/ai/brief.v1.schema.json"),
        ai.join("brief.v1.schema.json"),
    )
    .unwrap();
    root
}

#[tokio::test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn eval_run_checks_sets_scores_outputs_and_reports() {
    let f = Fixture::migrated().await;

    // The sets alone: every case is valid.
    let (env, code, _, err) = f.call(&["eval", "run", "all"]);
    assert_eq!(code, 0, "{env} {err}");
    assert_eq!(env["data"]["failed"], 0, "{env}");
    assert!(env["data"]["passed"].as_i64().unwrap() > 0);
    let id = env["data"]["eval_run"].as_str().unwrap().to_owned();

    let (env, code, _, _) = f.call(&["eval", "report", &id]);
    assert_eq!(code, 0, "{env}");
    let text = env["data"]["text"].as_str().unwrap();
    assert!(text.contains("golden sets only"), "{text}");
    assert!(env["data"]["metrics"]["g1.cases"].as_f64().unwrap() > 0.0);

    // Stored outputs of a version: G2 answered exactly as labelled, so every target is met.
    let root = scratch_root("eval");
    let g2 = root.join("pipeline/eval/g2");
    let mut lines = String::new();
    for e in std::fs::read_dir(g2.join("cases")).unwrap() {
        let case: Value =
            serde_json::from_str(&std::fs::read_to_string(e.unwrap().path()).unwrap()).unwrap();
        let mut output = case["label"].clone();
        output["confidence"] = json!("high");
        lines += &format!("{}\n", json!({"case": case["id"], "output": output}));
    }
    std::fs::create_dir_all(g2.join("outputs")).unwrap();
    std::fs::write(g2.join("outputs/test-v1.jsonl"), lines).unwrap();

    let mut cmd = f.news();
    cmd.current_dir(&root);
    let args = ["eval", "run", "classify", "--prompt-version", "test-v1"];
    let (env, code, _, err) = call(&mut cmd, &args);
    assert_eq!(code, 0, "{env} {err}");
    assert_eq!(env["data"]["failed"], 0, "{env}");
    let id = env["data"]["eval_run"].as_str().unwrap().to_owned();
    let (env, _, _, _) = f.call(&["eval", "report", &id]);
    assert_eq!(env["data"]["metrics"]["g2.page_type_accuracy"], 1.0);
    assert!(
        env["data"]["text"]
            .as_str()
            .unwrap()
            .contains("page_type_accuracy: 1  (target >= 0.950: met)"),
        "{env}"
    );

    // A version with no stored outputs, an unknown run and an unknown suite.
    let mut cmd = f.news();
    cmd.current_dir(&root);
    let (env, code, _, _) = call(
        &mut cmd,
        &["eval", "run", "writer", "--prompt-version", "none"],
    );
    assert_eq!(
        (code, env["error"]["code"].as_str()),
        (3, Some("not_found")),
        "{env}"
    );
    let (env, _, _, _) = f.call(&["eval", "report", "e999999"]);
    assert_eq!(env["error"]["code"], "not_found");
    let (env, _, _, _) = f.call(&["eval", "run", "nope"]);
    assert_eq!(env["error"]["code"], "usage");
    assert_eq!(f.scalar_i64("select count(*) from eval_runs").await, 2);
    let _ = std::fs::remove_dir_all(&root);
}

/// Stored outputs of `version` for G2, every case answered as labelled.
fn store_g2_outputs(root: &Path, version: &str) {
    let g2 = root.join("pipeline/eval/g2");
    let mut lines = String::new();
    for e in std::fs::read_dir(g2.join("cases")).unwrap() {
        let case: Value =
            serde_json::from_str(&std::fs::read_to_string(e.unwrap().path()).unwrap()).unwrap();
        let mut output = case["label"].clone();
        output["confidence"] = json!("high");
        lines += &format!("{}\n", json!({"case": case["id"], "output": output}));
    }
    std::fs::create_dir_all(g2.join("outputs")).unwrap();
    std::fs::write(g2.join(format!("outputs/{version}.jsonl")), lines).unwrap();
}

async fn audit_run(ch: &Chief, version: &str) -> uuid::Uuid {
    audit_suite(ch, "classify", version).await
}

async fn audit_suite(ch: &Chief, suite: &str, version: &str) -> uuid::Uuid {
    let mut args = Map::new();
    args.insert("suite".into(), json!(suite));
    args.insert("prompt_version".into(), json!(version));
    let run = ch
        .start(StartReq::board("audit", args, IfBusy::Offer))
        .await
        .unwrap()
        .run()
        .clone();
    ch.ensure_drivers().await.unwrap();
    run.run_id
}

async fn wait_for(f: &Fixture, run_id: uuid::Uuid, want: &str) {
    for _ in 0..600 {
        let st = f
            .scalar_str(&format!(
                "select status from runs where run_id = '{run_id}'"
            ))
            .await;
        if st == want {
            return;
        }
        assert!(
            !matches!(st.as_str(), "failed" | "succeeded" | "cancelled"),
            "the run ended {st}, not {want}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the run never became {want}");
}

/// Answer the run's promote gate; the subject is rebuilt from what the report step stored.
async fn answer(f: &Fixture, ch: &Chief, run_id: uuid::Uuid, version: &str, d: Decision) {
    answer_suite(f, ch, run_id, ("classify", "A3"), version, d).await;
}

async fn answer_suite(
    f: &Fixture,
    ch: &Chief,
    run_id: uuid::Uuid,
    (suite, step): (&str, &str),
    version: &str,
    d: Decision,
) {
    let (short, eval_run): (String, i64) = sqlx::query_as(
        "select r.short_id, e.eval_run_id from runs r join eval_runs e using (run_id) where r.run_id = $1",
    )
    .bind(run_id)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let subject = json!({"suite": suite, "prompt_version": version,
                         "eval_run": format!("e{eval_run}"), "steps": [step]});
    // Board's hash: of the summary and preview it shows; the summary carries the subject's own hash.
    let (summary, preview): (Value, Option<Value>) = sqlx::query_as(
        "select summary, preview from gates where run_id = $1 and gate = 'promote' and round = 1",
    )
    .bind(run_id)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        summary["subject_hash"],
        gates::hex(&gates::subject_sha(&subject))
    );
    let sha = gates::hex(&news_core::tier::gate_sha(&summary, preview.as_ref()));
    ch.answer(Answer {
        run: &short,
        gate: "promote",
        round: 1,
        subject_sha: &sha,
        decision: d,
        note: None,
    })
    .await
    .unwrap();
    ch.ensure_drivers().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn audit_scores_stored_outputs_waits_for_approval_and_promotes() {
    let f = Fixture::migrated().await;
    let root = scratch_root("audit");
    store_g2_outputs(&root, "cand-v1");
    store_g2_outputs(&root, "cand-v2");
    let ch = Chief::builder(f.pool.clone(), Arc::new(ManualClock::new(Utc::now())))
        .handlers(news_critic::audit::register(
            Handlers::default(),
            root.clone(),
        ))
        .build();

    // Approved: the report is kept with the run, then A3's live version is cand-v1.
    let run = audit_run(&ch, "cand-v1").await;
    wait_for(&f, run, "waiting_approval").await;
    let text = f
        .scalar_str(&format!(
            "select report from eval_runs where run_id = '{run}'"
        ))
        .await;
    assert!(text.contains("A3: no live version yet"), "{text}");
    assert_eq!(
        f.scalar_str(&format!(
            "select status from run_steps where run_id = '{run}' and step = 'judges'"
        ))
        .await,
        "skipped"
    );
    answer(&f, &ch, run, "cand-v1", Decision::Approved).await;
    wait_for(&f, run, "succeeded").await;
    assert_eq!(
        news_critic::eval::live(&f.pool, "A3")
            .await
            .unwrap()
            .as_deref(),
        Some("cand-v1")
    );

    // Rejected: compared with cand-v1 (no regression), nothing promoted.
    let run = audit_run(&ch, "cand-v2").await;
    wait_for(&f, run, "waiting_approval").await;
    let text = f
        .scalar_str(&format!(
            "select report from eval_runs where run_id = '{run}'"
        ))
        .await;
    assert!(text.contains("A3: no regression against cand-v1"), "{text}");
    answer(&f, &ch, run, "cand-v2", Decision::Rejected).await;
    for _ in 0..200 {
        let st = f
            .scalar_str(&format!("select status from runs where run_id = '{run}'"))
            .await;
        if matches!(st.as_str(), "succeeded" | "failed" | "cancelled") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        news_critic::eval::live(&f.pool, "A3")
            .await
            .unwrap()
            .as_deref(),
        Some("cand-v1")
    );
    assert_eq!(
        f.scalar_i64("select count(*) from prompt_promotions").await,
        1
    );

    // A version with no stored outputs fails at the candidate step; the CLI form stays gated.
    let run = audit_run(&ch, "missing").await;
    for _ in 0..200 {
        let st = f
            .scalar_str(&format!("select status from runs where run_id = '{run}'"))
            .await;
        if st == "failed" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        f.scalar_str(&format!(
            "select status from run_steps where run_id = '{run}' and step = 'candidate'"
        ))
        .await,
        "failed"
    );
    let (env, code, _, _) = f.call(&["eval", "promote", "A3", "cand-v2"]);
    assert_eq!((code, env["error"]["code"].as_str()), (7, Some("gated")));
    let _ = std::fs::remove_dir_all(&root);
}

/// The fake `claude`'s pool and output folder (one test uses it: a job handler is a plain `fn`).
static FAKE: std::sync::OnceLock<(news_store::PgPool, PathBuf)> = std::sync::OnceLock::new();

/// Ghost with a fake `claude` behind it, at the `ai` job boundary: each `ai.write` call answers with a
/// Copy, records its `ai_calls` rows through Ghost's own store and returns what Ghost returns. A brief
/// with an injection fails C4 on the first try and passes after the A2 repair; the rest pass at once.
/// For the candidate `broken-v1`, `claude` crashes on those briefs instead and records no call.
fn fake_claude(input: JobInput) -> JobFuture {
    Box::pin(async move {
        let (db, dir) = FAKE
            .get()
            .cloned()
            .ok_or_else(|| JobFailure::new(ErrorClass::Bug, "the fake claude is not set up"))?;
        let brief = input.args["input"].as_str().unwrap_or_default().to_owned();
        let prefix = input.args["prefix"].as_str().unwrap_or_default();
        if prefix.contains("/broken-v1/") && brief.contains("injection") {
            return Err(JobFailure::new(
                ErrorClass::Bug,
                "claude -p failed (exit 2)",
            ));
        }
        let file = dir.join(format!("{}.out.json", input.job_id));
        std::fs::write(
            &file,
            json!({"headline": "Annual inflation fell to {dp_7kq2}", "oui": "Page()"}).to_string(),
        )
        .unwrap();
        let rounds: &[(&str, &str)] = if brief.contains("injection") {
            &[("A1", "invalid:c4"), ("A2", "valid")]
        } else {
            &[("A1", "valid")]
        };
        let store = PgStore {
            db,
            clickhouse: None,
        };
        for (round, (step, validation)) in rounds.iter().enumerate() {
            store
                .record(&AiCall {
                    ts: Utc::now(),
                    run_id: input.run_id,
                    job_id: input.job_id,
                    step: (*step).into(),
                    model: "main".into(),
                    prompt_version: prefix.rsplit('/').next().unwrap_or_default().into(),
                    tokens_in: 1000,
                    tokens_out: 500,
                    cache_read: 0,
                    cache_write_5m: 0,
                    cache_write_1h: 0,
                    turns: 1,
                    duration_ms: 10,
                    subtype: "success".into(),
                    validation: (*validation).into(),
                    repair_round: round as u8,
                    est_cost_usd: 0.0,
                    tokens: 1500,
                    input_file: brief.clone(),
                    output_file: Some(file.to_string_lossy().into_owned()),
                })
                .await;
        }
        Ok(
            json!({"step": "A1", "valid": true, "stored": false, "repair_round": rounds.len() - 1,
                  "output": file.to_string_lossy(), "tokens": 1500 * rounds.len()}),
        )
    })
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn audit_makes_fresh_a1_calls_with_a_fake_claude_and_promotes_only_after_approval() {
    let f = Fixture::migrated().await;
    let root = scratch_root("fresh");
    let g1 = root.join("pipeline/eval/g1/cases");
    let cases = std::fs::read_dir(&g1).unwrap().count();
    let injections = std::fs::read_dir(&g1)
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("injection")
        })
        .count();
    // The candidate's prompt: one prefix per page type in G1.
    for v in ["fresh-v1", "broken-v1"] {
        let prefixes = root.join("work/prefix").join(v);
        std::fs::create_dir_all(&prefixes).unwrap();
        for t in [
            "earnings",
            "macro-release",
            "major-report",
            "news-story",
            "policy-decision",
            "science",
        ] {
            std::fs::write(prefixes.join(format!("{t}.md")), "# candidate prefix").unwrap();
        }
    }
    let out = root.join("work/fake-claude");
    std::fs::create_dir_all(&out).unwrap();
    assert!(FAKE.set((f.pool.clone(), out)).is_ok());
    let ch = Chief::builder(f.pool.clone(), Arc::new(ManualClock::new(Utc::now())))
        .handlers(news_critic::audit::register(
            Handlers::default(),
            root.clone(),
        ))
        .build();
    let (stop, rx) = tokio::sync::oneshot::channel::<()>();
    let mut opts = news_dispatch::worker::WorkOptions::new(vec!["ai".into()], "audit-w");
    opts.poll = Duration::from_millis(100);
    opts.maintenance = false;
    let resolve: news_dispatch::worker::Resolver =
        |kind| (kind == "ai.write").then_some(fake_claude as Handler);
    let worker = tokio::spawn(news_dispatch::worker::run(
        f.pool.clone(),
        opts,
        resolve,
        async move {
            let _ = rx.await;
        },
    ));

    let run = audit_suite(&ch, "writer", "fresh-v1").await;
    wait_for(&f, run, "waiting_approval").await;
    // One job per case, each with the candidate's prefix, and the calls kept as G1's stored outputs.
    assert_eq!(
        f.scalar_i64(&format!(
            "select count(*) from jobs where run_id = '{run}' and kind = 'ai.write' and state = 'completed'
                and args->>'prefix' like 'work/prefix/fresh-v1/%' and args->>'audit' = 'true'"
        ))
        .await,
        cases as i64
    );
    let stored =
        std::fs::read_to_string(root.join("pipeline/eval/g1/outputs/fresh-v1.jsonl")).unwrap();
    assert_eq!(stored.lines().count(), cases);
    let metric = |k: &'static str| {
        let f = &f;
        async move {
            f.scalar_str(&format!(
                "select metrics->>'g1.{k}' from eval_runs where run_id = '{run}'"
            ))
            .await
            .parse::<f64>()
            .unwrap()
        }
    };
    let first = (cases - injections) as f64 / cases as f64;
    assert!((metric("valid_first_try").await - first).abs() < 1e-9);
    assert_eq!(metric("valid_after_repair").await, 1.0);
    assert_eq!(metric("c4_c5_after_repair").await, 0.0);
    let text = f
        .scalar_str(&format!(
            "select report from eval_runs where run_id = '{run}'"
        ))
        .await;
    assert!(text.contains("A1: no live version yet"), "{text}");
    // Nothing is promoted while the approval waits.
    assert_eq!(news_critic::eval::live(&f.pool, "A1").await.unwrap(), None);

    answer_suite(
        &f,
        &ch,
        run,
        ("writer", "A1"),
        "fresh-v1",
        Decision::Approved,
    )
    .await;
    wait_for(&f, run, "succeeded").await;
    assert_eq!(
        news_critic::eval::live(&f.pool, "A1")
            .await
            .unwrap()
            .as_deref(),
        Some("fresh-v1")
    );

    // Calls that crashed measured nothing: the step fails, naming them, and stores no outputs, so the
    // next Audit of that version makes its calls again.
    let run = audit_suite(&ch, "writer", "broken-v1").await;
    let mut status = String::new();
    for _ in 0..600 {
        status = f
            .scalar_str(&format!("select status from runs where run_id = '{run}'"))
            .await;
        if matches!(status.as_str(), "succeeded" | "failed" | "cancelled") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(status, "failed");
    let err = f
        .scalar_str(&format!(
            "select error->>'message' from run_steps where run_id = '{run}' and step = 'candidate'"
        ))
        .await;
    assert!(
        err.contains(&format!("{injections} of {cases} fresh A1 calls")),
        "{err}"
    );
    assert!(err.contains("claude -p failed"), "{err}");
    assert!(
        !root
            .join("pipeline/eval/g1/outputs/broken-v1.jsonl")
            .exists()
    );
    assert_eq!(
        f.scalar_i64(&format!(
            "select count(*) from jobs where run_id = '{run}' and state = 'discarded'"
        ))
        .await,
        injections as i64
    );
    assert_eq!(
        news_critic::eval::live(&f.pool, "A1")
            .await
            .unwrap()
            .as_deref(),
        Some("fresh-v1")
    );
    let _ = stop.send(());
    let _ = worker.await;
    let _ = std::fs::remove_dir_all(&root);
}
