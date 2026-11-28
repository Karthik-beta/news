//! The AI path as `news service run` installs it (#20): `registry::install_ai` puts the real Ghost, with
//! `NewsAccept` and `PgStore`, behind `registry::resolve`, and a fake `claude` at the job boundary
//! answers with a Copy. Ghost and Rewrite are installed once per process, so each phase runs in a child
//! process of this test binary (as the sentinel test in `dispatch.rs` does), with a fake Plate.
//!
//! Unix only: the fake is a shell script. Ghost's Windows spawn (Low integrity, in the job) has its own
//! tests in news-ghost, with a compiled fake.

use super::fixture::{Fixture, url};
use news::steps::pipeline::{PAGE_SCHEMA, prefix};
use news_dispatch::queue::{self, NewJob};
use news_dispatch::worker::{self, WorkOptions};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Set in the child: `<schema>|<root>`.
const CHILD: &str = "NEWS_WIRE_AI_CHILD";

fn copy_in(from: &Path, to: &Path) {
    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
    std::fs::copy(from, to).unwrap();
}

/// A repository root with the schema, a brief, the component index and a fake `claude` whose reply is
/// the fixture Copy as its structured output.
fn root_for(f: &Fixture) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("wire-ai-{}", f.schema));
    let _ = std::fs::remove_dir_all(&root);
    copy_in(
        &here.join("../../..").join(PAGE_SCHEMA),
        &root.join(PAGE_SCHEMA),
    );
    let brief = here.join("../news-proof/tests/fixtures/brief.json");
    copy_in(&brief, &root.join("work/briefs/81234.json"));
    copy_in(&brief, &root.join("work/audit/g1/c1.json"));
    copy_in(
        &here.join("tests/fixtures/component-index.json"),
        &root.join(news_fixer::components::INDEX_REL),
    );
    std::fs::create_dir_all(root.join("pipeline/claude")).unwrap();
    std::fs::write(root.join("pipeline/claude/page-call.settings.json"), "{}").unwrap();
    let copy: Value = serde_json::from_str(
        &std::fs::read_to_string(here.join("../news-proof/tests/fixtures/copy.json")).unwrap(),
    )
    .unwrap();
    let reply = json!({"type": "result", "subtype": "success", "is_error": false, "num_turns": 1,
                       "result": "", "structured_output": copy});
    std::fs::write(root.join("reply.json"), reply.to_string()).unwrap();
    let bin = root.join("claude");
    let script = format!(
        "#!/bin/sh\nexec /bin/cat '{}'\n",
        root.join("reply.json").display()
    );
    std::fs::write(&bin, script).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    root
}

/// Run the child half: the installed handlers work the `ai` pool until no job is left. Returns its
/// output, for a failure message.
fn run_child(f: &Fixture, root: &Path, plate: &str) -> String {
    let name = module_path!().split_once("::").unwrap().1;
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &format!(
                "{name}::the_installed_ghost_stores_a_preset_copy_and_only_validates_an_audit_one"
            ),
        ])
        .args(["--ignored", "--nocapture", "--test-threads=1"])
        .env(CHILD, format!("{}|{}", f.schema, root.display()))
        .env("NEWS_PLATE_URL", plate)
        .output()
        .unwrap();
    let text = format!(
        "child stdout:\n{}\nchild stderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{text}");
    text
}

async fn child(v: &str) {
    let (schema, root) = v.split_once('|').unwrap();
    let root = Path::new(root);
    let cfg = news_store::DbConfig::new(url(), schema).unwrap();
    let pool = news_store::connect(&cfg, 5).await.unwrap();
    let installed = news::registry::install_ai(&pool, root, Some(root.join("claude")), None);
    assert_eq!(installed, (true, true), "(ghost, rewrite)");
    let mut opts = WorkOptions::new(vec!["ai".into()], "wire-ai");
    opts.poll = Duration::from_millis(100);
    opts.maintenance = false;
    let (stop, rx) = tokio::sync::oneshot::channel::<()>();
    let join = tokio::spawn(worker::run(
        pool.clone(),
        opts,
        news::registry::resolve,
        async move {
            let _ = rx.await;
        },
    ));
    let mut left = 1;
    for _ in 0..300 {
        left = sqlx::query_scalar::<_, i64>(
            "select count(*) from jobs where state not in ('completed', 'discarded', 'cancelled')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        if left == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let _ = stop.send(());
    join.await.unwrap().unwrap();
    assert_eq!(left, 0, "the ai jobs never finished");
}

async fn enqueue(f: &Fixture, args: Value, story: Option<&str>) -> i64 {
    let new = NewJob {
        item_ref: story.map(str::to_owned),
        ..NewJob::new("ai.write").args(args)
    };
    queue::enqueue(&f.pool, &new).await.unwrap().job_id
}

/// The job's state and result, and why it failed: its error class, attempts and errors, and the
/// `ai_calls` rows of its calls.
async fn result(f: &Fixture, job: i64) -> (String, Value, String) {
    let (state, result, class, attempt, errors): (String, Value, Option<String>, i16, Value) =
        sqlx::query_as(
            "select state, coalesce(result, 'null'::jsonb), error_class, attempt, errors
               from jobs where job_id = $1",
        )
        .bind(job)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    let calls: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "select step, subtype, validation, output_file from ai_calls where job_id = $1",
    )
    .bind(job)
    .fetch_all(&f.pool)
    .await
    .unwrap();
    let why = format!(
        "job {job} {state}: class {class:?}, attempt {attempt}, errors {errors}, calls {calls:?}, result {result}"
    );
    (state, result, why)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn the_installed_ghost_stores_a_preset_copy_and_only_validates_an_audit_one() {
    if let Ok(v) = std::env::var(CHILD) {
        return child(&v).await;
    }
    let f = Fixture::migrated().await;
    let root = root_for(&f);
    let plate = super::ai::fake_plate();
    let prefix = prefix(&root, "macro-release").unwrap();

    // An Audit call (#19): validated against the brief it was given, never stored as a page.
    let audit = enqueue(
        &f,
        json!({"step": "A1", "input": "work/audit/g1/c1.json", "schema": PAGE_SCHEMA,
               "prefix": prefix, "audit": true}),
        None,
    )
    .await;
    let out = run_child(&f, &root, &plate);
    let (state, r, why) = result(&f, audit).await;
    assert_eq!(state, "completed", "{why}\n{out}");
    assert_eq!(
        (r["valid"].as_bool(), r["stored"].as_bool()),
        (Some(true), Some(false)),
        "{r}"
    );
    assert!(
        Path::new(r["output"].as_str().unwrap()).is_file(),
        "Audit reads the kept output"
    );
    assert!(
        !root.join("work/pages").exists(),
        "an Audit output is never a page"
    );

    // A preset's A1: validated against the story's brief and stored.
    let write = enqueue(
        &f,
        json!({"step": "A1", "story": "81234", "input": "work/briefs/81234.json",
               "schema": PAGE_SCHEMA, "prefix": prefix}),
        Some("81234"),
    )
    .await;
    let out = run_child(&f, &root, &plate);
    let (state, r, why) = result(&f, write).await;
    assert_eq!(state, "completed", "{why}\n{out}");
    assert_eq!(r["stored"], true, "{r}");
    let stored: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("work/pages/81234/copy.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(stored["job"], write);

    // Each call is an `ai_calls` row on the service's pool, named by its prefix file.
    let name = prefix.rsplit('/').next().unwrap();
    assert_eq!(
        f.scalar_i64(&format!(
            "select count(*) from ai_calls where validation = 'valid' and prompt_version = '{name}'"
        ))
        .await,
        2
    );

    // A direction flip's A1 update (#20): its input composed by code, run by the installed Ghost, and
    // stored by NewsAccept as the story's next Copy.
    use news_rewrite::find::{PageChange, PointMove};
    let flip = news_rewrite::plan::plan(vec![PageChange {
        page_id: "p-flip".into(),
        version: 1,
        story_id: Some(81234),
        points: vec![PointMove {
            old_id: "dp_old".into(),
            new_id: "dp_new".into(),
            old_text: "-0.2".into(),
            new_text: "0.1".into(),
            new_numeric: Some("0.1".into()),
            new_locator: json!({"kind": "json"}),
            ..Default::default()
        }],
        derived: vec![],
    }])
    .remove(0);
    let new = news::steps::pipeline::update_job(&root, &flip, "81234").unwrap();
    let update = queue::enqueue(&f.pool, &new).await.unwrap().job_id;
    // The rewritten Copy differs from the first: `work::save` keeps an identical Copy as it was stored.
    let reply_path = root.join("reply.json");
    let mut reply: Value =
        serde_json::from_str(&std::fs::read_to_string(&reply_path).unwrap()).unwrap();
    reply["structured_output"]["fly_title"] = json!("America's prices");
    std::fs::write(&reply_path, reply.to_string()).unwrap();
    let out = run_child(&f, &root, &plate);
    let (state, r, why) = result(&f, update).await;
    assert_eq!(state, "completed", "{why}\n{out}");
    assert_eq!(r["stored"], true, "{r}");
    let stored: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("work/pages/81234/copy.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(stored["job"], update);
    assert_eq!(stored["copy"]["fly_title"], "America's prices");
    let _ = std::fs::remove_dir_all(&root);
}
