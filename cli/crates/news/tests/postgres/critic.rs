//! Critic against a real database (#19): `news eval run` checks the golden sets or scores stored
//! outputs, stores an `eval_runs` row, and `news eval report` prints it back.

use super::fixture::{Fixture, call};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

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

/// A copy of the golden sets (and the brief schema they check against) in a scratch folder, so a test
/// can add stored outputs without touching the repository.
fn scratch_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("news-critic-{}", std::process::id()));
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
    let root = scratch_root();
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
