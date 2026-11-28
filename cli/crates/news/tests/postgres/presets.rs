//! The preset step bodies (#20) on a fixture database, with a fake `claude` and a fake write API:
//! Edition's `review_plan` stop (a scheduled Edition runs with no gate, and stops at `plan` when the
//! schedule asks), Frontpage end to end (a failed check and a fix round, Request changes with a note,
//! Approve, Reject, all answered as Board answers them: through `Chief::answer`), and Followup's find,
//! classify, bulk stop and A1 update jobs.

use super::fixture::Fixture;
use chrono::{DateTime, Utc};
use news::steps::{Ctx, HomePublisher};
use news_chief::driver::{Handlers, StepCx, StepError, StepHandler, StepOutcome};
use news_chief::gates::{self, Answer, Decision, GRequest};
use news_chief::presets::GCommand;
use news_chief::{BoxFut, Chief, IfBusy, Origin, StartReq};
use news_core::clock::ManualClock;
use news_core::job::{Handler, JobFuture, JobInput};
use news_dispatch::worker::{self, ActiveJobs, Resolver, WorkOptions};
use news_home::api::{ApiError, CurationReply, Retry, WriteApi};
use serde_json::{Map, Value, json};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

fn t0() -> DateTime<Utc> {
    super::home::now()
}

pub(super) fn chief(f: &Fixture, handlers: Handlers) -> Chief {
    Chief::builder(f.pool.clone(), Arc::new(ManualClock::new(t0())))
        .handlers(handlers)
        .build()
}

async fn status(f: &Fixture, id: Uuid) -> String {
    f.scalar_str(&format!("select status from runs where run_id = '{id}'"))
        .await
}

pub(super) async fn wait_status(f: &Fixture, id: Uuid, want: &str) {
    for _ in 0..400 {
        let st = status(f, id).await;
        if st == want {
            return;
        }
        assert!(
            !matches!(st.as_str(), "failed" | "cancelled") || st == want,
            "run {id} is {st}, not {want}: {}",
            f.scalar_str(&format!(
                "select coalesce(string_agg(step || ': ' || coalesce(error::text, ''), '; '), '') from run_steps where run_id = '{id}' and status = 'failed'"
            ))
            .await
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!(
        "run {id} never became {want}; it is {}",
        status(f, id).await
    );
}

/// The one open gate of a run: (name, round, subject hash).
pub(super) async fn open_gate(f: &Fixture, id: Uuid) -> (String, i16, String) {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "select gate, round, encode(subject_sha, 'hex') from gates where run_id = '{id}' and answered_at is null"
    )))
    .fetch_one(&f.pool)
    .await
    .unwrap()
}

pub(super) async fn answer(
    ch: &Chief,
    short: &str,
    g: &(String, i16, String),
    d: Decision,
    note: Option<&str>,
) {
    ch.answer(Answer {
        run: short,
        gate: &g.0,
        round: g.1,
        subject_sha: &g.2,
        decision: d,
        note,
    })
    .await
    .unwrap();
    ch.ensure_drivers().await.unwrap();
}

struct Done;
impl StepHandler for Done {
    fn run<'a>(&'a self, _: &'a mut StepCx) -> BoxFut<'a, Result<StepOutcome, StepError>> {
        Box::pin(async { Ok(StepOutcome::Done(json!({}))) })
    }
}

/// Edition's publish step as far as the G-tier rule goes: authorised by the plan approval, or by the
/// start when the plan stop was skipped.
struct Authorised;
impl StepHandler for Authorised {
    fn run<'a>(&'a self, cx: &'a mut StepCx) -> BoxFut<'a, Result<StepOutcome, StepError>> {
        Box::pin(async move {
            let mut conn = cx.db.acquire().await.unwrap();
            gates::authorize(
                &mut conn,
                GRequest {
                    run_id: cx.run_id,
                    step_seq: cx.seq,
                    command: GCommand::Publish,
                    item: None,
                },
            )
            .await
            .map_err(|e| StepError::Fail {
                class: news_core::job::ErrorClass::Bug,
                message: e.message,
            })?;
            Ok(StepOutcome::Done(json!({"wrap": {"published": 1}})))
        })
    }
}

fn scheduled_edition(key: &str, review_plan: bool) -> StartReq<'_> {
    let mut args = Map::new();
    args.insert("review_plan".into(), json!(review_plan));
    StartReq {
        preset: "edition",
        args,
        origin: Origin::Schedule,
        schedule: Some((key, t0())),
        if_busy: IfBusy::Queue,
        missed_before: 0,
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn a_scheduled_edition_runs_without_a_gate_and_stops_at_plan_with_review_plan_on() {
    let f = Fixture::migrated().await;
    let handlers = [
        "preflight",
        "discover",
        "stories",
        "plan",
        "brief",
        "write",
        "render",
        "spotcheck",
        "promote",
    ]
    .into_iter()
    .fold(
        news::steps::register_unset(Handlers::default(), "no root".into()),
        |h, s| h.with(&format!("edition.{s}"), Arc::new(Done)),
    )
    .with("edition.publish", Arc::new(Authorised));
    let ch = chief(&f, handlers);

    // review_plan off: no gate; the schedule's start authorises the publish.
    let run = ch
        .start(scheduled_edition("edition-am", false))
        .await
        .unwrap()
        .run()
        .clone();
    ch.ensure_drivers().await.unwrap();
    wait_status(&f, run.run_id, "succeeded").await;
    assert_eq!(
        f.scalar_str(&format!(
            "select status from run_steps where run_id = '{}' and step = 'review'",
            run.run_id
        ))
        .await,
        "skipped"
    );
    assert_eq!(
        f.scalar_i64(&format!(
            "select count(*) from gates where run_id = '{}'",
            run.run_id
        ))
        .await,
        0
    );

    // review_plan on: the run stops at `plan` and publishes nothing until Board approves.
    let run = ch
        .start(scheduled_edition("edition-pm", true))
        .await
        .unwrap()
        .run()
        .clone();
    ch.ensure_drivers().await.unwrap();
    wait_status(&f, run.run_id, "waiting_approval").await;
    let g = open_gate(&f, run.run_id).await;
    assert_eq!(g.0, "plan");
    assert_eq!(
        f.scalar_str(&format!(
            "select status from run_steps where run_id = '{}' and step = 'publish'",
            run.run_id
        ))
        .await,
        "pending"
    );
    answer(&ch, &run.short_id, &g, Decision::Approved, None).await;
    wait_status(&f, run.run_id, "succeeded").await;
}

// ---- Frontpage ------------------------------------------------------------------------------------

/// The repository root of these tests' fake `claude` (one Frontpage test uses it).
static ROOT: Mutex<Option<PathBuf>> = Mutex::new(None);

/// The curator as a fake `claude` that Ghost would run: its answer depends only on its input file. With
/// no errors in the input it answers a draft with a page that does not exist; with errors, the good
/// draft; and with a note from Karthik, the good draft with the side stories swapped.
fn fake_curator(input: JobInput) -> JobFuture {
    Box::pin(async move {
        let root = ROOT.lock().unwrap().clone().unwrap();
        let rel = input.args["input"].as_str().unwrap().to_owned();
        let from: Value =
            serde_json::from_str(&std::fs::read_to_string(root.join(&rel)).unwrap()).unwrap();
        assert_eq!(input.args["step"], "A8");
        assert_eq!(from["skill"], "news-home-curation");
        assert!(from["candidates"].as_array().is_some_and(|c| !c.is_empty()));
        let mut draft = super::home::draft();
        if from["errors"].is_null() {
            draft["slots"]["side"][0] = json!({"page_id": "home-nope"});
        } else if !from["note"].is_null() {
            let side = draft["slots"]["side"].as_array_mut().unwrap();
            side.reverse();
        }
        let out = root.join(format!("work/home/out-{}.json", input.job_id));
        std::fs::write(&out, draft.to_string()).unwrap();
        Ok(json!({"step": "A8", "valid": true, "stored": false, "output": out.to_string_lossy()}))
    })
}

fn resolve_fake_curator() -> Resolver {
    |kind| (kind == "ai.write").then_some(fake_curator as Handler)
}

/// A write API that makes every curation live as the next version.
struct Api(Mutex<i32>);
impl WriteApi for Api {
    async fn put_curation(&self, _: &str, _: &str, _: Vec<u8>) -> Result<CurationReply, ApiError> {
        let mut v = self.0.lock().unwrap();
        *v += 1;
        Ok(CurationReply {
            version: *v,
            live_version: *v,
            purged: vec![],
        })
    }
    async fn post_rollback(&self, _: &str, _: &str, _: Vec<u8>) -> Result<CurationReply, ApiError> {
        unreachable!("no rollback here")
    }
}

#[derive(Default)]
struct FakeHome {
    api: Option<Api>,
    sent: Mutex<Vec<i64>>,
}

impl HomePublisher for FakeHome {
    fn publish<'a>(
        &'a self,
        db: &'a news_store::PgPool,
        draft_id: i64,
        now: DateTime<Utc>,
    ) -> BoxFut<'a, news_core::NewsResult<Value>> {
        Box::pin(async move {
            self.sent.lock().unwrap().push(draft_id);
            let api = self.api.as_ref().unwrap();
            news_home::api::publish(
                db,
                draft_id,
                now,
                api,
                || Ok("test-key".into()),
                Retry {
                    attempts: 1,
                    pause: Duration::ZERO,
                },
            )
            .await
        })
    }
}

/// A loopback preview server: it keeps the draft it was sent and shows its hash on every page.
fn preview_server() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        let mut held = String::new();
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 8192];
            let (end, len) = loop {
                let n = s.read(&mut chunk).unwrap_or(0);
                if n == 0 {
                    break (0, 0);
                }
                buf.extend_from_slice(&chunk[..n]);
                if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&buf[..end]).to_ascii_lowercase();
                    let len = head
                        .lines()
                        .find_map(|l| {
                            l.strip_prefix("content-length:")
                                .and_then(|v| v.trim().parse().ok())
                        })
                        .unwrap_or(0usize);
                    if buf.len() >= end + 4 + len {
                        break (end, len);
                    }
                }
            };
            if end == 0 {
                continue;
            }
            let put = buf.starts_with(b"PUT ");
            let reply = if put {
                held = news_home::preview::sha256_hex(&buf[end + 4..end + 4 + len]);
                "HTTP/1.1 204 No Content\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                    .to_owned()
            } else {
                format!(
                    "HTTP/1.1 200 OK\r\nx-news-preview: {held}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                )
            };
            let _ = s.write_all(reply.as_bytes());
        }
    });
    url
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn frontpage_runs_end_to_end_with_a_fix_round_changes_approval_and_rejection() {
    let f = Fixture::migrated().await;
    super::home::seed(&f).await;
    let root = std::env::temp_dir().join(format!("news-presets-{}", f.schema));
    std::fs::create_dir_all(root.join("work/home")).unwrap();
    *ROOT.lock().unwrap() = Some(root.clone());
    let home = Arc::new(FakeHome {
        api: Some(Api(Mutex::new(0))),
        ..FakeHome::default()
    });
    let ctx = Ctx {
        root: root.clone(),
        preview_base: preview_server(),
        home: home.clone(),
        ..super::pages::ctx(&root, super::pages::tools(), super::pages::api())
    };
    let ch = chief(&f, news::steps::register(Handlers::default(), ctx));
    // The `ai` pool, run by a worker whose handler is the fake curator.
    let (stop, rx) = tokio::sync::oneshot::channel::<()>();
    let mut opts = WorkOptions::new(vec!["ai".into()], "presets-w");
    opts.poll = Duration::from_millis(100);
    opts.maintenance = false;
    opts.active = ActiveJobs::default();
    let pool = f.pool.clone();
    let work = tokio::spawn(worker::run(
        pool,
        opts,
        resolve_fake_curator(),
        async move {
            let _ = rx.await;
        },
    ));

    // Run 1: the first draft fails the check, the fix round fixes it; Request changes with a note
    // brings a new draft and a new hash; Approve publishes it.
    let run = ch
        .start(StartReq::board("frontpage", Map::new(), IfBusy::Offer))
        .await
        .unwrap()
        .run()
        .clone();
    ch.ensure_drivers().await.unwrap();
    wait_status(&f, run.run_id, "waiting_approval").await;
    let g1 = open_gate(&f, run.run_id).await;
    assert_eq!((g1.0.as_str(), g1.1), ("publish", 1));
    assert_eq!(
        f.scalar_i64(&format!(
            "select count(*) from jobs where run_id = '{}' and kind = 'ai.write'",
            run.run_id
        ))
        .await,
        2,
        "one call, then one fix round"
    );
    // Both drafts are kept; only the fixed one passed.
    assert_eq!(
        f.scalar_str("select string_agg(status, ',' order by draft_id) from home_curations")
            .await,
        "draft,checked"
    );
    answer(
        &ch,
        &run.short_id,
        &g1,
        Decision::Changes,
        Some("swap the side stories"),
    )
    .await;
    wait_status(&f, run.run_id, "waiting_approval").await;
    let g2 = open_gate(&f, run.run_id).await;
    assert_eq!(g2.1, 2);
    assert_ne!(g1.2, g2.2, "a new draft is a new hash");
    // The note went into the curator's next input.
    let notes = f
        .scalar_i64(&format!(
            "select count(*) from jobs where run_id = '{}' and unique_key like 'curator:1:%'",
            run.run_id
        ))
        .await;
    assert!(notes >= 1);
    assert!(
        home.sent.lock().unwrap().is_empty(),
        "nothing is sent before the approval"
    );
    answer(&ch, &run.short_id, &g2, Decision::Approved, None).await;
    wait_status(&f, run.run_id, "succeeded").await;
    assert_eq!(home.sent.lock().unwrap().len(), 1);
    assert_eq!(
        f.scalar_str(
            "select status || ':' || version::text from home_curations where status = 'published'"
        )
        .await,
        "published:1"
    );

    // Run 2: rejected in Board. Nothing is published and the live curation stays.
    let run = ch
        .start(StartReq::board("frontpage", Map::new(), IfBusy::Offer))
        .await
        .unwrap()
        .run()
        .clone();
    ch.ensure_drivers().await.unwrap();
    wait_status(&f, run.run_id, "waiting_approval").await;
    let g = open_gate(&f, run.run_id).await;
    answer(&ch, &run.short_id, &g, Decision::Rejected, None).await;
    wait_status(&f, run.run_id, "succeeded").await;
    assert_eq!(
        home.sent.lock().unwrap().len(),
        1,
        "a rejected curation is never sent"
    );
    assert_eq!(
        f.scalar_str(&format!(
            "select status from run_steps where run_id = '{}' and step = 'publish'",
            run.run_id
        ))
        .await,
        "skipped"
    );
    assert_eq!(
        f.scalar_i64("select count(*) from home_curations where status = 'published'")
            .await,
        1
    );

    let _ = stop.send(());
    let _ = work.await;
    let _ = std::fs::remove_dir_all(&root);
}

// ---- Followup -------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn followup_finds_classifies_and_queues_the_a1_update_for_the_direction_flip() {
    let f = Fixture::migrated().await;
    super::rewrite::seed(&f).await;
    // The flip page needs a story for its update job.
    let story: i64 = sqlx::query_scalar(
        "insert into stories (event_key, first_seen, last_seen) values ('e-flip', now(), now()) returning story_id",
    )
    .fetch_one(&f.pool)
    .await
    .unwrap();
    sqlx::query("update pages set story_id = $1 where page_id = 'p-flip'")
        .bind(story)
        .execute(&f.pool)
        .await
        .unwrap();
    // `rerender` and `publish` are not built; they are stood in for here so the run reaches `update`.
    let handlers = news::steps::register_unset(Handlers::default(), "no root".into())
        .with("followup.rerender", Arc::new(Done))
        .with("followup.publish", Arc::new(Done));
    let ch = chief(&f, handlers);
    let run = ch
        .start(StartReq::board("followup", Map::new(), IfBusy::Offer))
        .await
        .unwrap()
        .run()
        .clone();
    ch.ensure_drivers().await.unwrap();
    // The flip waits for its A1 update job (no worker runs the `ai` pool here).
    wait_status(&f, run.run_id, "waiting_ai").await;
    let find: Value = sqlx::query_scalar::<_, Value>(sqlx::AssertSqlSafe(format!(
        "select output from run_steps where run_id = '{}' and step = 'classify'",
        run.run_id
    )))
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let kinds: Vec<(String, String)> = find["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["page_id"].as_str().unwrap().into(),
                p["kind"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(
        kinds,
        [
            ("p-chain".to_owned(), "value_only".to_owned()),
            ("p-fact".into(), "restatement".into()),
            ("p-flip".into(), "direction_flip".into()),
            ("p-value".into(), "value_only".into()),
        ]
    );
    // The derived point the value-only page re-keys to was stored (once, by the Ledger's arithmetic).
    assert_eq!(
        f.scalar_i64("select count(*) from derived_points where value = 0.2")
            .await,
        1
    );
    // One A1 update job, for the flip, keyed by page, next version and diff.
    assert_eq!(
        f.scalar_str(&format!(
            "select string_agg(unique_key, ',') from jobs where run_id = '{}' and kind = 'ai.write'",
            run.run_id
        ))
        .await
        .split(':')
        .take(3)
        .collect::<Vec<_>>(),
        ["update", "p-flip", "2"]
    );
    // Four pages: within the bulk limit, so no gate.
    assert_eq!(
        f.scalar_str(&format!(
            "select status from run_steps where run_id = '{}' and step = 'bulk_review'",
            run.run_id
        ))
        .await,
        "skipped"
    );

    // Chief cancels the `ai` jobs at the token cap: the update is dead, not waited on or queued again.
    let mut conn = f.pool.acquire().await.unwrap();
    news_dispatch::queue::cancel_run_jobs(&mut conn, run.run_id, None, Some("ai"))
        .await
        .unwrap();
    drop(conn);
    wait_status(&f, run.run_id, "succeeded").await;
    let update: Value = sqlx::query_scalar::<_, Value>(sqlx::AssertSqlSafe(format!(
        "select output from run_steps where run_id = '{}' and step = 'update'",
        run.run_id
    )))
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(update["dead"], 1, "a cancelled update counts as dead");
    assert_eq!(
        f.scalar_i64(&format!(
            "select count(*) from jobs where run_id = '{}' and kind = 'ai.write'",
            run.run_id
        ))
        .await,
        1,
        "the cancelled update is not queued again"
    );
}

/// Opens the `bulk` gate whatever the page count, so a test needs no 101 pages.
struct BulkGate;
impl StepHandler for BulkGate {
    fn run<'a>(&'a self, cx: &'a mut StepCx) -> BoxFut<'a, Result<StepOutcome, StepError>> {
        Box::pin(async move {
            let c = cx.outputs.get("classify").cloned().unwrap_or_default();
            Ok(StepOutcome::OpenGates(vec![
                news::steps::followup::bulk_gate(&c["pages"], &c["counts"]),
            ]))
        })
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn followup_rejected_at_the_bulk_gate_queues_no_a1_update() {
    let f = Fixture::migrated().await;
    super::rewrite::seed(&f).await;
    let story: i64 = sqlx::query_scalar(
        "insert into stories (event_key, first_seen, last_seen) values ('e-flip', now(), now()) returning story_id",
    )
    .fetch_one(&f.pool)
    .await
    .unwrap();
    sqlx::query("update pages set story_id = $1 where page_id = 'p-flip'")
        .bind(story)
        .execute(&f.pool)
        .await
        .unwrap();
    let handlers = news::steps::register_unset(Handlers::default(), "no root".into())
        .with("followup.bulk_review", Arc::new(BulkGate))
        .with("followup.rerender", Arc::new(Done))
        .with("followup.publish", Arc::new(Done));
    let ch = chief(&f, handlers);
    let run = ch
        .start(StartReq::board("followup", Map::new(), IfBusy::Offer))
        .await
        .unwrap()
        .run()
        .clone();
    ch.ensure_drivers().await.unwrap();
    wait_status(&f, run.run_id, "waiting_approval").await;
    let g = open_gate(&f, run.run_id).await;
    assert_eq!(g.0, "bulk");
    answer(&ch, &run.short_id, &g, Decision::Rejected, None).await;
    wait_status(&f, run.run_id, "succeeded").await;
    assert_eq!(
        f.scalar_i64(&format!(
            "select count(*) from jobs where run_id = '{}' and kind = 'ai.write'",
            run.run_id
        ))
        .await,
        0,
        "a rejected bulk change queues no A1 update"
    );
    assert_eq!(
        f.scalar_str(&format!(
            "select status from run_steps where run_id = '{}' and step = 'update'",
            run.run_id
        ))
        .await,
        "skipped"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn frontpage_fails_when_the_curator_call_is_cancelled_at_the_cap() {
    let f = Fixture::migrated().await;
    super::home::seed(&f).await;
    let root = std::env::temp_dir().join(format!("news-presets-{}", f.schema));
    std::fs::create_dir_all(root.join("work/home")).unwrap();
    let ctx = Ctx {
        root: root.clone(),
        preview_base: "http://127.0.0.1:9".into(),
        home: Arc::new(FakeHome::default()),
        ..super::pages::ctx(&root, super::pages::tools(), super::pages::api())
    };
    let ch = chief(&f, news::steps::register(Handlers::default(), ctx));
    // No worker runs the `ai` pool: the curator call stays queued until Chief cancels it.
    let run = ch
        .start(StartReq::board("frontpage", Map::new(), IfBusy::Offer))
        .await
        .unwrap()
        .run()
        .clone();
    ch.ensure_drivers().await.unwrap();
    wait_status(&f, run.run_id, "waiting_ai").await;
    let mut conn = f.pool.acquire().await.unwrap();
    news_dispatch::queue::cancel_run_jobs(&mut conn, run.run_id, None, Some("ai"))
        .await
        .unwrap();
    drop(conn);
    wait_status(&f, run.run_id, "failed").await;
    assert_eq!(
        f.scalar_str(&format!(
            "select status from run_steps where run_id = '{}' and step = 'draft'",
            run.run_id
        ))
        .await,
        "failed"
    );
    assert_eq!(
        f.scalar_i64(&format!(
            "select count(*) from jobs where run_id = '{}' and kind = 'ai.write'",
            run.run_id
        ))
        .await,
        1,
        "the cancelled call is not queued again"
    );
    assert_eq!(
        f.scalar_i64("select count(*) from home_curations").await,
        0,
        "no draft was saved"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn followup_rejected_at_the_bulk_gate_skips_rerender_update_and_publish() {
    let f = Fixture::migrated().await;
    super::rewrite::seed(&f).await;
    let root = std::env::temp_dir().join(format!("news-presets-{}", f.schema));
    std::fs::create_dir_all(root.join("work")).unwrap();
    // The real steps, with the bulk gate forced open: a rejection must stop all three.
    let ctx = super::pages::ctx(&root, super::pages::tools(), super::pages::api());
    let handlers = news::steps::register(Handlers::default(), ctx)
        .with("followup.bulk_review", Arc::new(BulkGate));
    let ch = chief(&f, handlers);
    let run = ch
        .start(StartReq::board("followup", Map::new(), IfBusy::Offer))
        .await
        .unwrap()
        .run()
        .clone();
    ch.ensure_drivers().await.unwrap();
    wait_status(&f, run.run_id, "waiting_approval").await;
    let g = open_gate(&f, run.run_id).await;
    answer(&ch, &run.short_id, &g, Decision::Rejected, None).await;
    wait_status(&f, run.run_id, "succeeded").await;
    for step in ["rerender", "update", "publish"] {
        assert_eq!(
            f.scalar_str(&format!(
                "select status from run_steps where run_id = '{}' and step = '{step}'",
                run.run_id
            ))
            .await,
            "skipped",
            "{step}"
        );
    }
    assert_eq!(f.scalar_i64("select count(*) from publish_outbox").await, 0);
    let _ = std::fs::remove_dir_all(&root);
}
