//! The remaining "Done when" tests of issue #10 (ADR 0008 §13), on top of `chief.rs`:
//!
//! 1. a usage-limit reply from a fake `claude` pauses the run, and it resumes on its own at the reset;
//! 2. Rota's quiet hours: "don't start" holds a heavy run, "reduced" starts it with fewer `ai` slots;
//! 3. a stalled-run pause ends the AI job, cancels the run's workers in every pool and fences its
//!    publishes before the slot is freed;
//! 4. `lineup.cancel`.
//!
//! Chief, Rota and the Lineup read time from a `ManualClock`. PostgreSQL's own `now()` (the pool pause,
//! a job's `run_after`, a lease) cannot be faked, so the usage-limit tests set the virtual clock from the
//! wall clock and keep the pause to a few real seconds.

use super::fixture::Fixture;
use chrono::{DateTime, Duration, Utc};
use news_chief::board::ServiceState;
use news_chief::state::Status;
use news_chief::{BoxFut, Chief, IfBusy, RunStopper, StartPolicy, StartReq, Started};
use news_core::ErrorCode;
use news_core::clock::{Clock, ManualClock};
use news_core::job::{ErrorClass, Handler, JobFailure, JobFuture, JobInput};
use news_dispatch::queue::{self, Lease, NewJob};
use news_dispatch::worker::{self, ActiveJobs, Resolver, WorkOptions};
use news_ghost::claude::{ClaudeOutcome, classify, last_json};
use news_hotline::{Bus, Peer, Service};
use serde_json::{Map, Value, json};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;
use uuid::Uuid;

fn t(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}

/// Thu 12 Nov 2026, 13:40 UTC.
fn clock() -> ManualClock {
    ManualClock::new(t("2026-11-12T13:40:00Z"))
}

fn chief(f: &Fixture, c: &ManualClock) -> Chief {
    Chief::new(f.pool.clone(), Arc::new(c.clone()))
}

async fn status(f: &Fixture, id: Uuid) -> String {
    f.scalar_str(&format!("select status from runs where run_id = '{id}'"))
        .await
}

async fn board_start(
    ch: &Chief,
    preset: &str,
    if_busy: IfBusy,
) -> Result<Started, news_core::NewsError> {
    ch.start(StartReq::board(preset, Map::new(), if_busy)).await
}

fn reprint_args() -> Map<String, Value> {
    let mut args = Map::new();
    args.insert("renderer".into(), json!("1.2.0"));
    args
}

/// Chief's tick, as the service runs it every second.
async fn settle(ch: &Chief) {
    ch.tick().await.unwrap();
}

/// Poll `f` every 100 ms for 20 s; panic naming `what` if it never holds.
async fn eventually<Fut: Future<Output = bool>>(what: &str, mut f: impl FnMut() -> Fut) {
    for _ in 0..200 {
        if f().await {
            return;
        }
        tokio::time::sleep(StdDuration::from_millis(100)).await;
    }
    panic!("never happened: {what}");
}

async fn standing_set(f: &Fixture, base: i32, changes: &[(&str, Value)]) -> i32 {
    let ok = |_: &news_standing::Schedule| Ok(());
    let changes: Vec<(String, Value)> = changes
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect();
    news_standing::set(&f.pool, base, &changes, None, &ok)
        .await
        .unwrap()
        .version
}

/// Make every Standing version date from the virtual clock, so no slot before it ever fires.
async fn stamp_standing(f: &Fixture, c: &ManualClock) {
    sqlx::query("update standing set created_at = $1")
        .bind(c.now())
        .execute(&f.pool)
        .await
        .unwrap();
}

/// A background worker over `pools`, with its own stop switch.
struct Worker {
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    join: tokio::task::JoinHandle<Result<worker::WorkReport, news_core::NewsError>>,
}

impl Worker {
    fn spawn(f: &Fixture, pools: &[&str], resolve: Resolver, active: ActiveJobs) -> Self {
        let (stop, rx) = tokio::sync::oneshot::channel::<()>();
        let mut opts = WorkOptions::new(pools.iter().map(|p| (*p).to_owned()).collect(), "gaps-w");
        opts.poll = StdDuration::from_millis(100);
        opts.maintenance = false;
        opts.active = active;
        let join = tokio::spawn(worker::run(f.pool.clone(), opts, resolve, async move {
            let _ = rx.await;
        }));
        Self {
            stop: Some(stop),
            join,
        }
    }

    async fn stop(mut self) {
        if let Some(s) = self.stop.take() {
            let _ = s.send(());
        }
        tokio::time::timeout(StdDuration::from_secs(20), self.join)
            .await
            .expect("the worker stops")
            .unwrap()
            .unwrap();
    }
}

// ---- 1. Usage limits, with a fake `claude` ------------------------------------------------------

const FAKE_SH: &str = "#!/bin/sh\nd=$(dirname \"$0\")\necho call >> \"$d/calls\"\ncat \"$d/reply.json\"\nexit \"$(cat \"$d/exit\")\"\n";
const FAKE_CMD: &str = "@echo off\r\necho call>>\"%~dp0calls\"\r\ntype \"%~dp0reply.json\"\r\nset /p code=<\"%~dp0exit\"\r\nexit /b %code%\r\n";

/// A stand-in for the `claude` binary: prints `reply.json`, exits with the code in `exit`, and notes each
/// call in `calls`. It is a script in a temporary directory; the real CLI is never started.
struct FakeClaude {
    dir: PathBuf,
}

impl FakeClaude {
    fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("news-fake-claude-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let name = if cfg!(windows) {
            "claude.cmd"
        } else {
            "claude"
        };
        let bin = dir.join(name);
        std::fs::write(&bin, if cfg!(windows) { FAKE_CMD } else { FAKE_SH }).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        Self { dir }
    }

    fn bin(&self) -> String {
        let name = if cfg!(windows) {
            "claude.cmd"
        } else {
            "claude"
        };
        self.dir.join(name).to_string_lossy().into_owned()
    }

    /// The next replies: a recorded fixture (with `{{RESET_EPOCH}}` filled in) and the exit code.
    fn reply(&self, fixture: &str, reset_epoch: i64, exit: i32) {
        let src = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/claude")
            .join(fixture);
        let body = std::fs::read_to_string(src)
            .unwrap()
            .replace("{{RESET_EPOCH}}", &reset_epoch.to_string());
        std::fs::write(self.dir.join("reply.json"), body).unwrap();
        std::fs::write(self.dir.join("exit"), exit.to_string()).unwrap();
    }

    fn calls(&self) -> usize {
        std::fs::read_to_string(self.dir.join("calls")).map_or(0, |s| s.lines().count())
    }
}

impl Drop for FakeClaude {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The `ai` job handler of these tests: runs the fake `claude` and turns its reply into the job's
/// result or failure with Ghost's own classifier, so the test follows the shape the service reads.
fn ask_claude(input: JobInput) -> JobFuture {
    Box::pin(async move {
        let bin = input.args["claude"].as_str().unwrap_or_default().to_owned();
        let now: DateTime<Utc> = input.args["now"]
            .as_str()
            .unwrap_or_default()
            .parse()
            .unwrap();
        let out = tokio::task::spawn_blocking(move || {
            std::process::Command::new(bin)
                .args(["-p", "--output-format", "json", "write the page"])
                .output()
        })
        .await
        .map_err(|e| JobFailure::new(ErrorClass::Bug, e.to_string()))?
        .map_err(|e| {
            JobFailure::new(
                ErrorClass::Bug,
                format!("cannot start the fake claude: {e}"),
            )
        })?;
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        match classify(out.status.code(), &stdout) {
            ClaudeOutcome::Ok => last_json(&stdout)
                .ok_or_else(|| JobFailure::new(ErrorClass::AiInvalidOutput, stdout.clone())),
            ClaudeOutcome::UsageLimit {
                resets_at: Some(at),
            } => Err(
                JobFailure::new(ErrorClass::AiUsageLimit, "usage limit reached")
                    .with_retry_after((at - now).to_std().unwrap_or_default()),
            ),
            ClaudeOutcome::UsageLimit { resets_at: None } => Err(JobFailure::new(
                ErrorClass::AiUsageLimit,
                "usage limit reached, no reset time",
            )),
            ClaudeOutcome::Failed => Err(JobFailure::new(ErrorClass::AiInvalidOutput, stdout)),
        }
    })
}

fn ask_claude_resolver() -> Resolver {
    |_| Some(ask_claude as Handler)
}

/// Whole seconds, so a reset time survives the epoch in the fake's reply.
fn now_secs() -> DateTime<Utc> {
    DateTime::from_timestamp(Utc::now().timestamp(), 0).unwrap()
}

type Events = Arc<Mutex<Vec<(String, Value)>>>;

fn chief_with_events(f: &Fixture, c: &ManualClock) -> (Chief, Events) {
    let events: Events = Arc::default();
    let sink = events.clone();
    let ch = Chief::builder(f.pool.clone(), Arc::new(c.clone()))
        .emit(Arc::new(move |topic: &str, data: Value| {
            sink.lock().unwrap().push((topic.to_owned(), data));
        }))
        .build();
    (ch, events)
}

fn usage_kinds(events: &Events) -> Vec<String> {
    events
        .lock()
        .unwrap()
        .iter()
        .filter(|(topic, _)| topic == "usage")
        .map(|(_, d)| d["kind"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn a_usage_limit_reply_from_the_fake_claude_pauses_the_run_and_it_resumes_at_the_reset() {
    let f = Fixture::migrated().await;
    let base = now_secs();
    let c = ManualClock::new(base);
    let (ch, events) = chief_with_events(&f, &c);
    let run = board_start(&ch, "edition", IfBusy::Offer)
        .await
        .unwrap()
        .run()
        .clone();
    let reset = base + Duration::seconds(6);
    let claude = FakeClaude::new("limit");
    claude.reply("usage_limit.json", reset.timestamp(), 1);
    let job = queue::enqueue(
        &f.pool,
        &NewJob::new("ai.write")
            .for_step(run.run_id, 7)
            .args(json!({"claude": claude.bin(), "now": base})),
    )
    .await
    .unwrap()
    .job_id;
    let w = Worker::spawn(&f, &["ai"], ask_claude_resolver(), ActiveJobs::default());

    // The fake says the limit is reached: the job is snoozed without spending its attempt, and the
    // `ai` pool is paused until the reset.
    let (fx, cl) = (&f, &claude);
    eventually("the first claude call and the pool pause", || async move {
        cl.calls() == 1
            && fx
                .scalar_i64("select count(*) from pools where pool = 'ai' and paused_until > now()")
                .await
                == 1
    })
    .await;
    let (state, attempt, snoozes): (String, i16, i16) =
        sqlx::query_as("select state, attempt, snoozes from jobs where job_id = $1")
            .bind(job)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!((state.as_str(), attempt, snoozes), ("snoozed", 0, 1));

    // Chief pauses the run until the reset, with its reason and time on the row.
    settle(&ch).await;
    settle(&ch).await;
    assert_eq!(status(&f, run.run_id).await, "paused");
    let (reason, at): (String, Option<DateTime<Utc>>) =
        sqlx::query_as("select paused_reason, resume_at from runs where run_id = $1")
            .bind(run.run_id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(reason, "usage_limit");
    let at = at.expect("resume_at is the reset time");
    assert!(
        at >= reset && at < reset + Duration::seconds(30),
        "resume_at {at} is not the reset {reset}"
    );
    let board = news_chief::board::status(&f.pool, ServiceState::Up, None, None, c.now())
        .await
        .unwrap();
    assert_eq!(board["waiting"][0]["kind"], "paused");
    assert_eq!(board["waiting"][0]["resume_at"], json!(at));
    assert!(
        board["text"]
            .as_str()
            .unwrap()
            .contains("Paused for the usage limit; resumes on its own at"),
        "{}",
        board["text"]
    );

    // A second before the reset nothing moves, and nothing calls `claude`.
    c.set(at - Duration::seconds(1));
    settle(&ch).await;
    assert_eq!(status(&f, run.run_id).await, "paused");
    assert_eq!(claude.calls(), 1);

    // Past the reset Chief resumes the run by itself; the job then runs again and succeeds.
    claude.reply("ok.json", 0, 0);
    c.set(at + Duration::seconds(1));
    settle(&ch).await;
    assert_eq!(status(&f, run.run_id).await, "running");
    eventually("the retried job completes", || async move {
        fx.scalar_str(&format!("select state from jobs where job_id = {job}"))
            .await
            == "completed"
    })
    .await;
    assert_eq!(claude.calls(), 2);
    assert_eq!(
        usage_kinds(&events),
        ["limit_reached", "limit_reset"],
        "one event each way, none while paused"
    );
    let reached = events
        .lock()
        .unwrap()
        .iter()
        .find(|(_, d)| d["kind"] == "limit_reached")
        .map(|(_, d)| d.clone())
        .unwrap();
    assert_eq!(reached["resets_at"], json!(at));
    news_hotline::contracts()
        .check_output(
            "run.show",
            &news_chief::board::run_show(&f.pool, &run.short_id, c.now(), None)
                .await
                .unwrap(),
        )
        .unwrap();
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn a_usage_limit_without_a_reset_time_pauses_for_the_default_hour() {
    // The provisional classifier (ADR 0008 §1) gives no reset time: Dispatch pauses `ai` for an hour.
    let f = Fixture::migrated().await;
    let base = now_secs();
    let c = ManualClock::new(base);
    let ch = chief(&f, &c);
    let run = board_start(&ch, "edition", IfBusy::Offer)
        .await
        .unwrap()
        .run()
        .clone();
    let claude = FakeClaude::new("noreset");
    claude.reply("usage_limit_no_reset.json", 0, 1);
    queue::enqueue(
        &f.pool,
        &NewJob::new("ai.write")
            .for_step(run.run_id, 7)
            .args(json!({"claude": claude.bin(), "now": base})),
    )
    .await
    .unwrap();
    let w = Worker::spawn(&f, &["ai"], ask_claude_resolver(), ActiveJobs::default());
    let cl = &claude;
    eventually("the claude call", || async move { cl.calls() == 1 }).await;
    let fx = &f;
    eventually("the pool pause", || async move {
        fx.scalar_i64("select count(*) from pools where pool = 'ai' and paused_until > now()")
            .await
            == 1
    })
    .await;
    w.stop().await;

    settle(&ch).await;
    settle(&ch).await;
    assert_eq!(status(&f, run.run_id).await, "paused");
    let at: DateTime<Utc> = sqlx::query_scalar("select resume_at from runs where run_id = $1")
        .bind(run.run_id)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert!(
        at >= base + Duration::minutes(59) && at <= base + Duration::minutes(61),
        "resume_at {at} is not an hour after {base}"
    );
    c.set(at - Duration::seconds(1));
    settle(&ch).await;
    assert_eq!(status(&f, run.run_id).await, "paused");
    c.set(at + Duration::seconds(1));
    settle(&ch).await;
    assert_eq!(status(&f, run.run_id).await, "running");
    assert_eq!(claude.calls(), 1, "no call while paused");
}

// ---- 2. Rota: quiet hours -----------------------------------------------------------------------

/// Standing for the quiet-hours tests: UTC, quiet hours `start`..`end` in `mode`, one daily Edition
/// schedule `cron`, all dated from the clock's start.
async fn quiet_standing(f: &Fixture, c: &ManualClock, quiet: (&str, &str), mode: &str, cron: &str) {
    stamp_standing(f, c).await;
    standing_set(
        f,
        1,
        &[
            ("rota.time_zone", json!("UTC")),
            ("quiet_hours.enabled", json!(true)),
            ("quiet_hours.start", json!(quiet.0)),
            ("quiet_hours.end", json!(quiet.1)),
            ("quiet_hours.mode", json!(mode)),
            ("quiet_hours.ai_slots", json!(1)),
            (
                "schedules.am",
                json!({"preset": "edition", "args": {}, "cron": cron, "enabled": true}),
            ),
        ],
    )
    .await;
    stamp_standing(f, c).await;
}

fn quiet_rota(f: &Fixture, c: &ManualClock) -> (Chief, news_rota::Rota, news_rota::RotaPolicy) {
    let policy = news_rota::RotaPolicy::default();
    let ch = Chief::builder(f.pool.clone(), Arc::new(c.clone()))
        .policy(Arc::new(policy.clone()))
        .build();
    let rota = news_rota::Rota::new(
        f.pool.clone(),
        ch.clone(),
        Arc::new(c.clone()),
        policy.clone(),
        Arc::new(news_rota::Unknown),
        Arc::new(|_, _| {}),
    );
    (ch, rota, policy)
}

/// The one fire of a tick, as a queued run.
fn queued_fire(fires: &[(String, news_rota::Fire)]) -> (&Started, DateTime<Utc>) {
    match fires {
        [(key, news_rota::Fire::Fired { slot, outcome, .. })] if key == "schedules.am" => {
            (outcome, *slot)
        }
        other => panic!("expected one fire of schedules.am, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn quiet_hours_in_dont_start_mode_hold_a_heavy_run_until_they_end() {
    let f = Fixture::migrated().await;
    let c = ManualClock::new(t("2026-11-12T05:30:00Z"));
    // Quiet 23:00-07:00 UTC; the Edition slot is 06:00, inside them.
    quiet_standing(&f, &c, ("23:00", "07:00"), "dont_start", "0 6 * * *").await;
    let (ch, rota, policy) = quiet_rota(&f, &c);
    c.set(t("2026-11-12T06:00:10Z"));
    let fires = rota.tick().await.unwrap();
    let (outcome, slot) = queued_fire(&fires);
    assert_eq!(slot, t("2026-11-12T06:00:00Z"));
    let Started::Queued { run, position } = outcome else {
        panic!("a heavy run in quiet hours waits: {outcome:?}")
    };
    assert_eq!(*position, 1);
    assert_eq!(
        run.start_by,
        Some(t("2026-11-12T08:00:00Z")),
        "the late limit (2 h) counts from the slot"
    );
    assert_eq!(status(&f, run.run_id).await, "queued");
    assert_eq!(
        f.scalar_str("select outcome from rota_fires where schedule_key = 'schedules.am'")
            .await,
        "queued"
    );

    // Quiet hours hold starts, never light runs; a manual heavy start is refused with the reason.
    assert!(matches!(
        board_start(&ch, "frontpage", IfBusy::Offer).await.unwrap(),
        Started::Running(_)
    ));
    let e = board_start(&ch, "catchup", IfBusy::Offer)
        .await
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Precondition);
    assert_eq!(e.details.unwrap()["reason"], "quiet_hours");

    // The Lineup says why it waits, and the tick does not start it.
    let lineup = news_chief::board::lineup(&f.pool, Some(&policy as &dyn StartPolicy), c.now())
        .await
        .unwrap();
    assert_eq!(lineup["items"][0]["held_by"], "quiet_hours");
    c.set(t("2026-11-12T06:59:59Z"));
    settle(&ch).await;
    assert_eq!(status(&f, run.run_id).await, "queued");

    // The first instant after quiet hours it starts, and `start_by` is cleared.
    c.set(t("2026-11-12T07:00:00Z"));
    settle(&ch).await;
    assert_eq!(status(&f, run.run_id).await, "running");
    let start_by: Option<DateTime<Utc>> =
        sqlx::query_scalar("select start_by from runs where run_id = $1")
            .bind(run.run_id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(start_by, None);
    // The slot is not fired twice.
    assert!(rota.tick().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn a_run_held_by_quiet_hours_past_the_late_limit_ends_skipped_with_a_wrap() {
    let f = Fixture::migrated().await;
    let c = ManualClock::new(t("2026-11-12T05:30:00Z"));
    // Quiet 06:00-10:00 outlasts the 2 h late limit of the 06:00 slot.
    quiet_standing(&f, &c, ("06:00", "10:00"), "dont_start", "0 6 * * *").await;
    let (ch, rota, _) = quiet_rota(&f, &c);
    c.set(t("2026-11-12T06:00:10Z"));
    let fires = rota.tick().await.unwrap();
    let (outcome, _) = queued_fire(&fires);
    let id = outcome.run().run_id;
    assert_eq!(status(&f, id).await, "queued");
    c.set(t("2026-11-12T07:59:59Z"));
    settle(&ch).await;
    assert_eq!(
        status(&f, id).await,
        "queued",
        "still inside the late limit"
    );
    c.set(t("2026-11-12T08:00:01Z"));
    settle(&ch).await;
    assert_eq!(status(&f, id).await, "skipped");
    let wrap: Value = sqlx::query_scalar("select summary from runs where run_id = $1")
        .bind(id)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(wrap["reason"], "late");
    assert!(wrap["text"].as_str().unwrap().contains("skipped"));
    // Quiet hours end later; the skipped run stays skipped and its slot is not fired again.
    c.set(t("2026-11-12T10:00:30Z"));
    settle(&ch).await;
    assert_eq!(status(&f, id).await, "skipped");
    assert!(rota.tick().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn quiet_hours_in_reduced_mode_start_a_heavy_run_on_time_with_fewer_ai_slots() {
    let f = Fixture::migrated().await;
    let c = ManualClock::new(t("2026-11-12T05:30:00Z"));
    quiet_standing(&f, &c, ("23:00", "07:00"), "reduced", "0 6 * * *").await;
    let (_, rota, _) = quiet_rota(&f, &c);
    c.set(t("2026-11-12T06:00:10Z"));
    let fires = rota.tick().await.unwrap();
    let (outcome, _) = queued_fire(&fires);
    let Started::Running(run) = outcome else {
        panic!("reduced mode starts the heavy run on time: {outcome:?}")
    };
    assert_eq!(
        f.scalar_i64("select max_running::bigint from pools where pool = 'ai'")
            .await,
        1,
        "quiet hours' ai_slots"
    );
    for _ in 0..3 {
        queue::enqueue(&f.pool, &NewJob::new("ai.write").for_step(run.run_id, 1))
            .await
            .unwrap();
    }
    let lease = StdDuration::from_secs(60);
    let first = queue::claim(&f.pool, "ai", 5, lease, "w").await.unwrap();
    assert_eq!(first.jobs.len(), 1, "one slot in quiet hours");

    // When quiet hours end the pool is back to Standing's size and the rest are claimed.
    c.set(t("2026-11-12T07:00:30Z"));
    assert!(rota.tick().await.unwrap().is_empty());
    assert_eq!(
        f.scalar_i64("select max_running::bigint from pools where pool = 'ai'")
            .await,
        3
    );
    let rest = queue::claim(&f.pool, "ai", 5, lease, "w").await.unwrap();
    assert_eq!(rest.jobs.len(), 2, "three slots again, one already taken");
}

// ---- 3. A stalled run's pause --------------------------------------------------------------------

static HANG_STARTED: AtomicUsize = AtomicUsize::new(0);
static HANG_ENDED: AtomicUsize = AtomicUsize::new(0);

struct Ended;

impl Drop for Ended {
    fn drop(&mut self) {
        HANG_ENDED.fetch_add(1, Ordering::SeqCst);
    }
}

/// A job that never finishes by itself. Used by one test only (the counters are global).
fn hang(_: JobInput) -> JobFuture {
    Box::pin(async {
        HANG_STARTED.fetch_add(1, Ordering::SeqCst);
        let _end = Ended;
        tokio::time::sleep(StdDuration::from_secs(300)).await;
        Ok(json!({}))
    })
}

/// The service's real stopper, held at the door until the test lets it in.
struct Gated {
    inner: news_service::host::ServiceStopper,
    entered: Arc<AtomicBool>,
    release: Arc<tokio::sync::Notify>,
}

impl RunStopper for Gated {
    fn stop<'a>(&'a self, run: Uuid) -> BoxFut<'a, ()> {
        Box::pin(async move {
            self.entered.store(true, Ordering::SeqCst);
            self.release.notified().await;
            self.inner.stop(run).await;
        })
    }

    fn stopping(&self, run: Uuid) -> (Vec<i64>, Option<DateTime<Utc>>) {
        self.inner.stopping(run)
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn pausing_a_stalled_run_stops_its_work_in_every_pool_and_fences_its_publishes_before_the_slot_frees()
 {
    let f = Fixture::migrated().await;
    let c = clock();
    let active = ActiveJobs::default();
    let entered = Arc::new(AtomicBool::new(false));
    let release = Arc::new(tokio::sync::Notify::new());
    let ch = Chief::builder(f.pool.clone(), Arc::new(c.clone()))
        .stopper(Arc::new(Gated {
            inner: news_service::host::ServiceStopper {
                active: active.clone(),
            },
            entered: entered.clone(),
            release: release.clone(),
        }))
        .build();
    let run = board_start(&ch, "catchup", IfBusy::Offer)
        .await
        .unwrap()
        .run()
        .clone();

    // One job in each of four pools: the AI job, two code workers, and a publish.
    for pool in ["ai", "cpu", "net", "publish"] {
        queue::enqueue(
            &f.pool,
            &NewJob::new("test.hang").pool(pool).for_step(run.run_id, 1),
        )
        .await
        .unwrap();
    }
    let resolve: Resolver = |_| Some(hang as Handler);
    let w = Worker::spawn(
        &f,
        &["ai", "cpu", "net", "publish"],
        resolve,
        active.clone(),
    );
    eventually("four handlers running", || async {
        HANG_STARTED.load(Ordering::SeqCst) == 4
    })
    .await;
    let held: Vec<(i64, String, Uuid)> = sqlx::query_as(
        "select job_id, locked_by, lease_id from jobs where run_id = $1 and state = 'running' order by job_id",
    )
    .bind(run.run_id)
    .fetch_all(&f.pool)
    .await
    .unwrap();
    assert_eq!(held.len(), 4);
    // Publishes of the run: one sent without an answer, one never tried, one answered.
    for (key, attempts, sent) in [
        ("k-sent-unanswered", 1, false),
        ("k-never-tried", 0, false),
        ("k-answered", 1, true),
    ] {
        sqlx::query(
            "insert into publish_outbox (page_id, version, bundle_sha, idempotency_key, op, run_id, attempts, sent_at)
             values ($1, 1, $2, $3, 'put_page', $4, $5, case when $6 then now() else null end)",
        )
        .bind(key)
        .bind(vec![0u8; 32])
        .bind(key)
        .bind(run.run_id)
        .bind(attempts)
        .bind(sent)
        .execute(&f.pool)
        .await
        .unwrap();
    }

    // Fifteen minutes of silence is not yet a stall; one second more is.
    let board = |now| news_chief::board::status(&f.pool, ServiceState::Up, None, None, now);
    c.advance(Duration::minutes(15));
    let b = board(c.now()).await.unwrap();
    assert_eq!(b["running"][0]["stalled"], false);
    assert_ne!(b["next"]["rule"], "stalled_heavy");
    c.advance(Duration::seconds(1));
    let b = board(c.now()).await.unwrap();
    assert_eq!(b["running"][0]["stalled"], true);
    assert_eq!(b["next"]["rule"], "stalled_heavy");
    assert_eq!(b["next"]["action"], "run.pause");

    // Pausing it stops it now, not after its current step.
    let (_, mode, stalled) = ch.pause(&run.short_id).await.unwrap();
    assert_eq!((mode.as_str(), stalled), ("now", true));
    let (st, stop_mode, why): (String, String, String) =
        sqlx::query_as("select status, stop_mode, paused_reason from runs where run_id = $1")
            .bind(run.run_id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(
        (st.as_str(), stop_mode.as_str(), why.as_str()),
        ("pausing", "now", "karthik")
    );
    settle(&ch).await;
    eventually("the stopper is called", || async {
        entered.load(Ordering::SeqCst)
    })
    .await;

    // While the stopper has not returned the slot is held: the work is still there, a heavy start is
    // offered only the queue, and nothing is fenced yet.
    assert_eq!(status(&f, run.run_id).await, "pausing");
    assert_eq!(
        f.scalar_i64(&format!(
            "select count(*) from jobs where run_id = '{}' and state = 'running'",
            run.run_id
        ))
        .await,
        4
    );
    let e = ch
        .start(StartReq::board("reprint", reprint_args(), IfBusy::Offer))
        .await
        .unwrap_err();
    assert_eq!(e.details.unwrap()["offers"], json!(["queue"]));
    let queued = ch
        .start(StartReq::board("reprint", reprint_args(), IfBusy::Queue))
        .await
        .unwrap()
        .run()
        .clone();
    settle(&ch).await;
    assert_eq!(status(&f, queued.run_id).await, "queued");
    assert_eq!(
        f.scalar_i64("select count(*) from publish_outbox where unknown_since is not null")
            .await,
        0,
        "publishes are fenced when the work has stopped, not before"
    );
    let show = news_chief::board::run_show(&f.pool, &run.short_id, c.now(), Some(&**ch.stopper()))
        .await
        .unwrap();
    assert_eq!(show["run"]["stopping"]["tasks_left"], 4);

    // Let the stopper in: every task ends, then the leases are revoked and the publish fenced in the
    // same transaction that frees the slot.
    release.notify_one();
    eventually("the run is paused", || async {
        status(&f, run.run_id).await == "paused"
    })
    .await;
    assert_eq!(
        HANG_ENDED.load(Ordering::SeqCst),
        4,
        "every handler had ended before the slot was freed"
    );
    assert!(active.is_empty());
    assert_eq!(
        f.scalar_i64(&format!(
            "select count(*) from jobs where run_id = '{}' and state = 'retryable' and lease_id is null and locked_by is null",
            run.run_id
        ))
        .await,
        4
    );
    let unknown: Vec<(String, DateTime<Utc>)> = sqlx::query_as(
        "select idempotency_key, unknown_since from publish_outbox where unknown_since is not null",
    )
    .fetch_all(&f.pool)
    .await
    .unwrap();
    assert_eq!(unknown, vec![("k-sent-unanswered".to_owned(), c.now())]);
    // A late completion from any of the four revoked leases is refused.
    for (job_id, worker, lease) in &held {
        let e = queue::complete(&f.pool, *job_id, Lease::new(worker, *lease), &json!({}))
            .await
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::Conflict, "job {job_id}");
    }
    // The slot is free: the queued heavy run starts, and the paused run's jobs are left alone.
    settle(&ch).await;
    assert_eq!(status(&f, queued.run_id).await, "running");
    tokio::time::sleep(StdDuration::from_millis(500)).await;
    assert_eq!(
        f.scalar_i64(&format!(
            "select count(*) from jobs where run_id = '{}' and state = 'retryable'",
            run.run_id
        ))
        .await,
        4,
        "the workers do not claim a paused run's jobs"
    );
    assert_eq!(HANG_STARTED.load(Ordering::SeqCst), 4);
    // Only a running run can be paused; a resume (queued: the slot is taken) hands the publish back.
    assert_eq!(
        ch.pause(&run.short_id).await.unwrap_err().code,
        ErrorCode::Precondition
    );
    let (_, to, _) = ch.resume(&run.short_id).await.unwrap();
    assert_eq!(to, Status::Queued);
    assert_eq!(
        f.scalar_i64("select count(*) from publish_outbox where unknown_since is not null")
            .await,
        0
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn a_draining_pause_that_stalls_is_escalated_to_stop_now() {
    let f = Fixture::migrated().await;
    let c = clock();
    let ch = chief(&f, &c);
    let run = board_start(&ch, "catchup", IfBusy::Offer)
        .await
        .unwrap()
        .run()
        .clone();
    let job = queue::enqueue(
        &f.pool,
        &NewJob::new("test.fetch")
            .pool("cpu")
            .for_step(run.run_id, 1),
    )
    .await
    .unwrap()
    .job_id;
    let claimed = queue::claim(&f.pool, "cpu", 1, StdDuration::from_secs(600), "w")
        .await
        .unwrap();
    assert_eq!(claimed.jobs.len(), 1);
    let (_, mode, stalled) = ch.pause(&run.short_id).await.unwrap();
    assert_eq!((mode.as_str(), stalled), ("drain", false));
    // Asking again while it is not stalled changes nothing.
    let (_, mode, _) = ch.pause(&run.short_id).await.unwrap();
    assert_eq!(mode.as_str(), "drain");
    settle(&ch).await;
    assert_eq!(
        status(&f, run.run_id).await,
        "pausing",
        "the drain waits for the job"
    );

    // The job never returns: after the stall limit the same pause stops it now.
    c.advance(Duration::minutes(16));
    let (_, mode, stalled) = ch.pause(&run.short_id).await.unwrap();
    assert_eq!((mode.as_str(), stalled), ("now", true));
    assert_eq!(
        f.scalar_str(&format!(
            "select stop_mode from runs where run_id = '{}'",
            run.run_id
        ))
        .await,
        "now"
    );
    settle(&ch).await;
    eventually("the pause completes", || async {
        status(&f, run.run_id).await == "paused"
    })
    .await;
    let j = &claimed.jobs[0];
    let e = queue::complete(&f.pool, job, j.lease("w"), &json!({}))
        .await
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Conflict, "the revoked lease is refused");
}

// ---- 4. lineup.cancel -----------------------------------------------------------------------------

fn bureau(f: &Fixture, ch: Chief) -> Arc<news_service::handlers::Bureau> {
    let state = Arc::new(news_service::State::default());
    state.set(ServiceState::Up);
    let bureau = Arc::new(news_service::handlers::Bureau::new(
        state,
        Arc::new(Bus::default()),
        Arc::new(news_service::log::LogRing::default()),
    ));
    bureau.set_core(Some(Arc::new(news_service::handlers::Core {
        db: f.pool.clone(),
        chief: ch,
        policy: Arc::new(news_chief::AllowAll),
        stopper: Arc::new(news_chief::NoStopper),
    })));
    bureau
}

/// One Hotline call from `peer` into the service's handlers; the reply envelope.
async fn hotline(
    bureau: &Arc<news_service::handlers::Bureau>,
    peer: Peer,
    cmd: &str,
    args: Value,
) -> Value {
    let svc: Arc<dyn Service> = bureau.clone();
    let (mut client, server) = tokio::io::duplex(1 << 20);
    let h = tokio::spawn(news_hotline::serve_conn(server, peer, svc));
    let env = news_hotline::call(&mut client, "t-1", cmd, args)
        .await
        .unwrap();
    drop(client);
    let _ = h.await;
    env
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn lineup_cancel_removes_a_queued_run_with_a_wrap_and_only_from_board() {
    let f = Fixture::migrated().await;
    let c = clock();
    let ch = chief(&f, &c);
    let bureau = bureau(&f, ch.clone());
    let holder = board_start(&ch, "catchup", IfBusy::Offer)
        .await
        .unwrap()
        .run()
        .clone();
    // Two heavy runs wait: a scheduled Edition, then a Reprint from Board a minute later.
    let first = ch
        .start(StartReq {
            preset: "edition",
            args: Map::new(),
            origin: news_chief::Origin::Schedule,
            schedule: Some(("schedules.am", c.now())),
            if_busy: IfBusy::Queue,
            missed_before: 0,
        })
        .await
        .unwrap();
    let Started::Queued { run: a, position } = &first else {
        panic!("{first:?}")
    };
    assert_eq!(*position, 1);
    let a = a.clone();
    c.advance(Duration::minutes(1));
    let second = ch
        .start(StartReq::board("reprint", reprint_args(), IfBusy::Queue))
        .await
        .unwrap();
    let Started::Queued { run: b, position } = &second else {
        panic!("{second:?}")
    };
    assert_eq!(*position, 2);
    let b = b.clone();

    // Anything but Board is gated and the run stays queued.
    let terminal = Peer {
        argv1_is_board: false,
        ..Peer::board()
    };
    let env = hotline(
        &bureau,
        terminal,
        "lineup.cancel",
        json!({"run": a.short_id}),
    )
    .await;
    assert_eq!(env["error"]["code"], "gated", "{env}");
    assert_eq!(status(&f, a.run_id).await, "queued");

    // Board removes it: a Wrap says so, the finish time is set, and the next run moves up.
    let env = hotline(
        &bureau,
        Peer::board(),
        "lineup.cancel",
        json!({"run": a.short_id}),
    )
    .await;
    assert_eq!(env["ok"], true, "{env}");
    news_hotline::contracts()
        .check_output("lineup.cancel", &env["data"])
        .unwrap();
    assert_eq!(env["data"]["status"], "cancelled");
    assert!(
        env["data"]["wrap"]["text"]
            .as_str()
            .unwrap()
            .contains("Lineup")
    );
    assert_eq!(status(&f, a.run_id).await, "cancelled");
    assert_eq!(
        f.scalar_i64(&format!(
            "select count(*) from runs where run_id = '{}' and finished_at is not null and summary is not null",
            a.run_id
        ))
        .await,
        1
    );
    let lineup = news_chief::board::lineup(&f.pool, None, c.now())
        .await
        .unwrap();
    let items = lineup["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["short_id"], json!(b.short_id));
    assert_eq!(items[0]["position"], 1);

    // Asking again, for a run that is not queued, or for no run at all, changes nothing.
    for (run, want) in [
        (a.short_id.as_str(), "precondition"),
        (holder.short_id.as_str(), "precondition"),
        ("r_nosuchrun", "not_found"),
    ] {
        let env = hotline(&bureau, Peer::board(), "lineup.cancel", json!({"run": run})).await;
        assert_eq!(env["error"]["code"], want, "{run}: {env}");
    }
    assert_eq!(status(&f, holder.run_id).await, "running");

    // A cancelled run is never promoted or skipped later, and its preset is free again.
    c.advance(Duration::hours(3));
    settle(&ch).await;
    assert_eq!(status(&f, a.run_id).await, "cancelled");
    assert_eq!(status(&f, b.run_id).await, "queued");
    assert!(matches!(
        board_start(&ch, "edition", IfBusy::Queue).await.unwrap(),
        Started::Queued { .. }
    ));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn cancelling_the_run_queued_for_a_pause_gives_the_slot_back_to_the_paused_holder() {
    let f = Fixture::migrated().await;
    let c = clock();
    let ch = chief(&f, &c);
    let holder = board_start(&ch, "catchup", IfBusy::Offer)
        .await
        .unwrap()
        .run()
        .clone();
    // "Pause the running one and start this": the new Edition waits at rank 0 for the slot.
    let (_, new, _) = ch
        .start_after_pause("edition", Map::new(), &holder.short_id)
        .await
        .unwrap();
    assert_eq!(status(&f, new.run_id).await, "queued");
    let (_, wrap) = ch.lineup_cancel(&new.short_id).await.unwrap();
    assert_eq!(wrap["status"], "cancelled");
    settle(&ch).await;
    assert_eq!(status(&f, holder.run_id).await, "paused");
    settle(&ch).await;
    assert_eq!(
        status(&f, new.run_id).await,
        "cancelled",
        "the cancelled run does not take the slot"
    );
    // Nothing is queued for the slot any more, so the holder resumes straight into it.
    let (_, to, pos) = ch.resume(&holder.short_id).await.unwrap();
    assert_eq!((to, pos), (Status::Running, None));
}
