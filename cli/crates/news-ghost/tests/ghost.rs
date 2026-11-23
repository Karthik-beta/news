//! Ghost end to end against the fake `claude` (src/bin/fake_claude.rs): never the real CLI, no network.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use news_core::job::{ErrorClass, JobFailure, JobInput, JobLease, LeaseCheck};
use news_ghost::claude::{ENV_ALLOW, ENV_SET};
use news_ghost::store::{AiCall, BoxFut, Limits, Store};
use news_ghost::{AcceptSink, Ghost, Problem, Verdict};
use serde_json::{Value, json};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

struct Held;
impl LeaseCheck for Held {
    fn ensure(&self) -> Pin<Box<dyn Future<Output = Result<(), JobFailure>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Default)]
struct MemStore {
    ceiling: Option<u64>,
    spent: u64,
    calls: Mutex<Vec<AiCall>>,
}

impl Store for MemStore {
    fn limits<'a>(
        &'a self,
        _: Option<Uuid>,
        step: &'a str,
    ) -> BoxFut<'a, Result<Limits, JobFailure>> {
        let model = news_ghost::store::model_key(step).trim_start_matches("models.");
        let l = Limits {
            model: model.to_owned(),
            ceiling: self.ceiling,
            spent: self.spent,
        };
        Box::pin(async move { Ok(l) })
    }
    fn record<'a>(&'a self, call: &'a AiCall) -> BoxFut<'a, ()> {
        self.calls.lock().unwrap().push(call.clone());
        Box::pin(async {})
    }
}

/// `news ai accept` double: valid or not, and what it was sent.
struct Sink {
    valid: bool,
    seen: Mutex<Vec<(i64, Value)>>,
}

impl AcceptSink for Sink {
    fn accept<'a>(
        &'a self,
        job_id: i64,
        file: &'a Path,
    ) -> BoxFut<'a, Result<Verdict, JobFailure>> {
        let out: Value = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
        self.seen.lock().unwrap().push((job_id, out));
        let errors = if self.valid {
            vec![]
        } else {
            vec![Problem {
                rule: "refs".into(),
                at: "headline".into(),
                message: "unknown ref dp_x".into(),
            }]
        };
        let v = Verdict {
            valid: self.valid,
            errors,
            stored: self.valid,
        };
        Box::pin(async move { Ok(v) })
    }
}

struct Setup {
    dir: PathBuf,
    ghost: Ghost,
    store: Arc<MemStore>,
    sink: Arc<Sink>,
}

impl Drop for Setup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn setup(name: &str, brief: Value, store: MemStore, valid: bool) -> Setup {
    let dir = std::env::temp_dir().join(format!(
        "news-ghost-test-{name}-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let root = dir.join("repo");
    for d in [
        "work/briefs",
        "work/prefix",
        "contracts/cli/ai",
        "pipeline/claude",
    ] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    std::fs::write(root.join("work/briefs/81234.json"), brief.to_string()).unwrap();
    std::fs::write(root.join("work/prefix/macro-release.3f9a1c.md"), "# prefix").unwrap();
    std::fs::write(
        root.join("contracts/cli/ai/page-draft.v1.schema.json"),
        "{\n  \"type\": \"object\"\n}\n",
    )
    .unwrap();
    std::fs::write(root.join("pipeline/claude/page-call.settings.json"), "{}").unwrap();
    let store = Arc::new(store);
    let sink = Arc::new(Sink {
        valid,
        seen: Mutex::default(),
    });
    let mut ghost = Ghost::new(
        PathBuf::from(env!("CARGO_BIN_EXE_fake-claude")),
        root,
        sink.clone(),
        store.clone(),
    );
    ghost.work = dir.join("work");
    #[cfg(windows)]
    {
        ghost.job = Some(Arc::new(news_win::Job::root().unwrap()));
    }
    Setup {
        dir,
        ghost,
        store,
        sink,
    }
}

fn job(kind: &str, job_id: i64, run: Uuid) -> JobInput {
    JobInput {
        job_id,
        kind: kind.into(),
        pool: "ai".into(),
        attempt: 1,
        run_id: Some(run),
        item_ref: None,
        args: json!({"input": "work/briefs/81234.json",
                     "schema": "contracts/cli/ai/page-draft.v1.schema.json",
                     "prefix": "work/prefix/macro-release.3f9a1c.md"}),
        lease: JobLease::new(job_id, Some(Uuid::new_v4()), Arc::new(Held)),
    }
}

/// What each fake `claude` recorded for a job, oldest first.
fn records(s: &Setup, job_id: i64) -> Vec<Value> {
    let dir = s.ghost.work.join(job_id.to_string());
    let mut v: Vec<(std::time::SystemTime, Value)> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with("fake-claude-"))
                .map(|e| {
                    (
                        e.metadata().unwrap().modified().unwrap(),
                        serde_json::from_str(&std::fs::read_to_string(e.path()).unwrap()).unwrap(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by_key(|(t, _)| *t);
    v.into_iter().map(|(_, r)| r).collect()
}

fn env_of(record: &Value) -> Vec<(String, String)> {
    serde_json::from_value(record["env"].clone()).unwrap()
}

#[tokio::test]
async fn the_argv_is_the_golden_call_and_a_valid_copy_is_accepted() {
    let s = setup(
        "golden",
        json!({"item": {"story_id": 81234}}),
        MemStore::default(),
        true,
    );
    let run = Uuid::new_v4();
    let out = s.ghost.run(job("ai.write", 11, run)).await.unwrap();
    assert_eq!(out["valid"], true);
    assert_eq!(out["repair_round"], 0);
    let rec = records(&s, 11);
    assert_eq!(rec.len(), 1);
    let root = &s.ghost.root;
    let golden: Vec<String> = [
        "-p",
        "--model",
        "main",
        "--tools",
        "",
        "--strict-mcp-config",
        "--mcp-config",
        r#"{"mcpServers":{}}"#,
        "--settings",
        &root
            .join("pipeline/claude/page-call.settings.json")
            .to_string_lossy(),
        "--disable-slash-commands",
        "--no-session-persistence",
        "--append-system-prompt-file",
        &root
            .join("work/prefix/macro-release.3f9a1c.md")
            .to_string_lossy(),
        "--json-schema",
        r#"{"type":"object"}"#,
        "--max-turns",
        "2",
        "--max-budget-usd",
        "0.60",
        "--output-format",
        "json",
    ]
    .map(str::to_owned)
    .to_vec();
    assert_eq!(rec[0]["argv"], json!(golden));
    assert_eq!(
        rec[0]["stdin"],
        json!({"item": {"story_id": 81234}}).to_string()
    );
    // The output went to `news ai accept`, and the call is one ai_calls row.
    let seen = s.sink.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, 11);
    assert!(seen[0].1["headline"].is_string());
    let calls = s.store.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    let c = &calls[0];
    assert_eq!(
        (c.step.as_str(), c.validation.as_str(), c.repair_round),
        ("A1", "valid", 0)
    );
    assert_eq!((c.run_id, c.job_id), (Some(run), 11));
    assert_eq!(c.prompt_version, "macro-release.3f9a1c.md");
    assert_eq!(
        (c.tokens_in, c.tokens_out, c.cache_read, c.cache_write_1h),
        (1000, 500, 8000, 200)
    );
    assert_eq!(c.tokens, 1700, "cache reads are not counted");
    assert_eq!(out["tokens"], 1700);
}

#[tokio::test]
async fn planted_secrets_never_reach_the_child() {
    let mut s = setup("env", json!({}), MemStore::default(), true);
    let sentinels = [
        ("ANTHROPIC_API_KEY", "SENTINEL-anthropic"),
        ("AWS_ACCESS_KEY_ID", "SENTINEL-aws-id"),
        ("AWS_SECRET_ACCESS_KEY", "SENTINEL-aws-secret"),
        ("CLOUDFLARE_API_TOKEN", "SENTINEL-cloudflare"),
        ("CF_API_TOKEN", "SENTINEL-cf"),
        ("NEWS_WRITE_API_KEY", "SENTINEL-write-api"),
        ("DATABASE_URL", "postgres://news:SENTINEL-pg@127.0.0.1/news"),
        ("NEWS_CH_PASSWORD", "SENTINEL-clickhouse"),
        ("GITHUB_TOKEN", "SENTINEL-github"),
        ("OTEL_EXPORTER_OTLP_HEADERS", "authorization=SENTINEL-otel"),
        ("HTTPS_PROXY", "http://u:SENTINEL-proxy@proxy"),
    ];
    for (k, v) in sentinels {
        s.ghost.parent_env.push((k.into(), v.into()));
    }
    let run = Uuid::new_v4();
    s.ghost.run(job("ai.write", 12, run)).await.unwrap();
    let env = env_of(&records(&s, 12)[0]);
    for (k, v) in &env {
        assert!(!v.contains("SENTINEL"), "{k} leaked");
        let up = k.to_ascii_uppercase();
        assert!(
            ENV_ALLOW.contains(&up.as_str()) || ENV_SET.contains(&up.as_str()),
            "{k} is not on the allow-list"
        );
    }
    assert!(
        !env.iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("ANTHROPIC_API_KEY"))
    );
    let get = |name: &str| env.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
    assert_eq!(get("CLAUDE_CODE_DISABLE_AUTO_MEMORY").as_deref(), Some("1"));
    assert_eq!(get("NEWS_SPAWNED_BY"), Some(format!("{run}:12")));
    assert_eq!(
        get("OTEL_RESOURCE_ATTRIBUTES"),
        Some(format!("news.run_id={run},news.job_id=12,news.step=A1"))
    );
}

#[tokio::test]
async fn one_repair_then_ai_invalid_output() {
    let brief = json!({"item": {"story_id": 7}});
    let s = setup("repair", brief.clone(), MemStore::default(), false);
    let f = s
        .ghost
        .run(job("ai.write", 13, Uuid::new_v4()))
        .await
        .unwrap_err();
    assert_eq!(f.class, ErrorClass::AiInvalidOutput, "{f}");
    assert!(f.message.contains("refs"), "{}", f.message);
    let rec = records(&s, 13);
    assert_eq!(rec.len(), 2, "one A1 call and one A2 repair, no more");
    // The repair call gets the errors, the previous Copy and the brief, on the A2 model and label.
    let repair: Value = serde_json::from_str(rec[1]["stdin"].as_str().unwrap()).unwrap();
    assert_eq!(repair["step"], "A2");
    assert_eq!(repair["errors"][0]["rule"], "refs");
    assert_eq!(repair["errors"][0]["where"], "headline");
    assert!(repair["previous"]["headline"].is_string());
    assert_eq!(repair["brief"], brief);
    assert!(
        env_of(&rec[1])
            .iter()
            .any(|(k, v)| k == "OTEL_RESOURCE_ATTRIBUTES" && v.ends_with("news.step=A2"))
    );
    assert_eq!(s.sink.seen.lock().unwrap().len(), 2);
    let calls = s.store.calls.lock().unwrap().clone();
    let rounds: Vec<(&str, u8, &str)> = calls
        .iter()
        .map(|c| (c.step.as_str(), c.repair_round, c.validation.as_str()))
        .collect();
    assert_eq!(
        rounds,
        [("A1", 0, "invalid:refs"), ("A2", 1, "invalid:refs")]
    );

    // Steps other than A1 get no repair: their fallback is code.
    let f = s
        .ghost
        .run(job("ai.classify", 14, Uuid::new_v4()))
        .await
        .unwrap_err();
    assert_eq!(f.class, ErrorClass::AiInvalidOutput);
    assert_eq!(records(&s, 14).len(), 1);
}

#[tokio::test]
async fn a_usage_limit_reply_pauses_until_the_reset() {
    let reset = chrono::Utc::now().timestamp() + 3600;
    let s = setup(
        "limit",
        json!({"fake": {"mode": "usage_limit", "reset": reset}}),
        MemStore::default(),
        true,
    );
    let f = s
        .ghost
        .run(job("ai.write", 15, Uuid::new_v4()))
        .await
        .unwrap_err();
    // Dispatch pauses the `ai` pool for `retry_after` and Chief pauses the run until then and resumes
    // it on its own (tests/postgres dispatch.rs and chief.rs cover that half).
    assert_eq!(f.class, ErrorClass::AiUsageLimit);
    let wait = f.retry_after.unwrap().as_secs();
    assert!((3590..=3600).contains(&wait), "{wait}");
    assert!(s.sink.seen.lock().unwrap().is_empty());
    let calls = s.store.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].subtype, "usage_limit");
}

#[tokio::test]
async fn the_token_ceiling_stops_new_calls() {
    let store = MemStore {
        ceiling: Some(2_500_000),
        spent: 2_500_000,
        ..MemStore::default()
    };
    let s = setup("ceiling", json!({}), store, true);
    let out = s
        .ghost
        .run(job("ai.write", 16, Uuid::new_v4()))
        .await
        .unwrap();
    assert_eq!(out["skipped"], "token_ceiling");
    assert!(records(&s, 16).is_empty(), "claude was started");
    assert!(s.store.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_failed_call_is_transient() {
    let s = setup(
        "fail",
        json!({"fake": {"mode": "fail"}}),
        MemStore::default(),
        true,
    );
    let f = s
        .ghost
        .run(job("ai.write", 17, Uuid::new_v4()))
        .await
        .unwrap_err();
    assert_eq!(f.class, ErrorClass::Transient, "{f}");
    assert!(f.message.contains("fake claude failed"), "{}", f.message);
}

#[cfg(windows)]
#[tokio::test]
async fn the_call_and_a_process_it_starts_are_both_in_the_ai_job() {
    let s = Arc::new(setup(
        "job",
        json!({"fake": {"mode": "spawn"}}),
        MemStore::default(),
        true,
    ));
    let s2 = s.clone();
    let task = tokio::spawn(async move { s2.ghost.run(job("ai.write", 18, Uuid::new_v4())).await });
    let dir = s.ghost.work.join("18");
    let rec = loop {
        let found = std::fs::read_dir(&dir).ok().and_then(|rd| {
            rd.flatten()
                .find(|e| e.file_name().to_string_lossy().starts_with("fake-claude-"))
                .and_then(|e| std::fs::read_to_string(e.path()).ok())
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        });
        if let Some(r) = found {
            break r;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    };
    let job_object = s.ghost.job.as_ref().unwrap();
    let pid = u32::try_from(rec["pid"].as_u64().unwrap()).unwrap();
    let grandchild = u32::try_from(rec["grandchild"].as_u64().unwrap()).unwrap();
    assert!(
        job_object.contains_pid(pid).unwrap(),
        "claude -p is outside the job"
    );
    assert!(
        job_object.contains_pid(grandchild).unwrap(),
        "its child is outside the job"
    );
    assert!(!job_object.contains_pid(std::process::id()).unwrap());
    std::fs::write(dir.join("go"), "").unwrap();
    assert_eq!(task.await.unwrap().unwrap()["valid"], true);
    job_object.terminate().unwrap();
}

#[cfg(windows)]
#[tokio::test]
async fn without_the_ai_job_no_call_starts() {
    let mut s = setup("nojob", json!({}), MemStore::default(), true);
    s.ghost.job = None;
    let f = s
        .ghost
        .run(job("ai.write", 19, Uuid::new_v4()))
        .await
        .unwrap_err();
    assert_eq!(f.class, ErrorClass::Bug);
    assert!(records(&s, 19).is_empty());
}
