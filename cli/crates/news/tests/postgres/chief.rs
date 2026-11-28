//! Chief, the Lineup, Rota, Standing and the Hotline handlers against a real database (ADR 0008 §13).
//! Time is a `ManualClock`; every test has its own schema.

use super::fixture::Fixture;
use chrono::{DateTime, Duration, Utc};
use news_chief::driver::{Handlers, StepCx, StepError, StepHandler, StepOutcome};
use news_chief::gates::{self, Answer, Decision, GRequest, GateOpen};
use news_chief::presets::GCommand;
use news_chief::state::{self, Change, PausedReason, Status, StopMode};
use news_chief::{AiClaim, BoxFut, Chief, IfBusy, Origin, Policy, StartPolicy, StartReq, Started};
use news_core::ErrorCode;
use news_core::clock::{Clock, ManualClock};
use news_dispatch::queue::{self, NewJob};
use serde_json::{Map, Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;
use uuid::Uuid;

const IGNORE: &str = "needs Postgres";

fn t(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}

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

fn scheduled<'a>(preset: &'a str, key: &'a str, slot: DateTime<Utc>) -> StartReq<'a> {
    StartReq {
        preset,
        args: Map::new(),
        origin: Origin::Schedule,
        schedule: Some((key, slot)),
        if_busy: IfBusy::Queue,
        missed_before: 0,
    }
}

fn reprint_args() -> Map<String, Value> {
    let mut args = Map::new();
    args.insert("renderer".into(), json!("1.2.0"));
    args
}

/// Finish a run's pause the way the tick does.
async fn settle(ch: &Chief) {
    ch.tick().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn standing_versions_record_who_when_old_and_new_and_refuse_a_stale_base() {
    let _ = IGNORE;
    let f = Fixture::migrated().await;
    let ok = |_: &news_standing::Schedule| Ok(());
    let r = news_standing::set(
        &f.pool,
        1,
        &[("pools.ai.max_running".into(), json!(5))],
        Some("more"),
        &ok,
    )
    .await
    .unwrap();
    assert_eq!(r.version, 2);
    assert_eq!(
        r.changes,
        vec![json!({"key": "pools.ai.max_running", "old": 3, "new": 5})]
    );
    let (who, note): (String, Option<String>) =
        sqlx::query_as("select who, note from standing where version = 2")
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!((who.as_str(), note.as_deref()), ("board", Some("more")));
    // A stale base is a conflict; an unknown key or a bad value is validation_failed.
    let e = news_standing::set(
        &f.pool,
        1,
        &[("pools.ai.max_running".into(), json!(4))],
        None,
        &ok,
    )
    .await
    .unwrap_err();
    assert_eq!(e.code, ErrorCode::Conflict);
    let e = news_standing::set(&f.pool, 2, &[("no.such".into(), json!(1))], None, &ok)
        .await
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::ValidationFailed);
    // Null resets to the default; history by key shows both changes.
    news_standing::set(
        &f.pool,
        2,
        &[("pools.ai.max_running".into(), Value::Null)],
        None,
        &ok,
    )
    .await
    .unwrap();
    let h = news_standing::history(&f.pool, Some("pools.ai.max_running"), 10, None)
        .await
        .unwrap();
    assert_eq!(h.iter().map(|v| v.version).collect::<Vec<_>>(), [3, 2]);
    // A run records the version it started with.
    let c = clock();
    let ch = chief(&f, &c);
    let s = board_start(&ch, "frontpage", IfBusy::Offer).await.unwrap();
    assert_eq!(s.run().settings_version, 3);
    assert_eq!(s.run().config["pools.ai.max_running"], 3);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn the_lineup_offers_queues_promotes_and_skips_late_runs() {
    let f = Fixture::migrated().await;
    let c = clock();
    let ch = chief(&f, &c);
    let a = board_start(&ch, "catchup", IfBusy::Offer).await.unwrap();
    let Started::Running(holder) = &a else {
        panic!("{a:?}")
    };
    // A manual heavy start while the slot is busy: a queue offer naming the holder.
    let e = ch
        .start(StartReq::board("reprint", reprint_args(), IfBusy::Offer))
        .await
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Conflict);
    assert!(!e.retryable);
    let d = e.details.clone().unwrap();
    assert_eq!(d["reason"], "queue_offer");
    assert_eq!(d["blocking"]["short_id"], holder.short_id);
    assert_eq!(d["offers"], json!(["queue", "pause_running"]));
    news_hotline::contracts()
        .check_output("run.start", &json!({}))
        .unwrap_err();
    // A scheduled heavy run waits with start_by; another (light) starts beside it.
    let s = ch
        .start(scheduled("edition", "schedules.am", c.now()))
        .await
        .unwrap();
    let Started::Queued {
        run: queued,
        position,
    } = &s
    else {
        panic!("{s:?}")
    };
    assert_eq!(*position, 1);
    assert_eq!(queued.start_by, Some(c.now() + Duration::minutes(120)));
    assert!(matches!(
        board_start(&ch, "frontpage", IfBusy::Offer).await.unwrap(),
        Started::Running(_)
    ));
    // The slot frees (the holder is paused): the queued run starts and start_by is cleared.
    ch.pause(&holder.short_id).await.unwrap();
    settle(&ch).await;
    settle(&ch).await;
    assert_eq!(status(&f, holder.run_id).await, "paused");
    assert_eq!(status(&f, queued.run_id).await, "running");
    let sb: Option<DateTime<Utc>> =
        sqlx::query_scalar("select start_by from runs where run_id = $1")
            .bind(queued.run_id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(sb, None);
    // A resume while the slot is busy rejoins the Lineup at rank 1.
    let (_, to, pos) = ch.resume(&holder.short_id).await.unwrap();
    assert_eq!((to, pos), (Status::Queued, Some(1)));
    // A queued run past its start_by ends skipped with a Wrap; lineup.cancel ends one cancelled.
    let late = ch
        .start(scheduled("reprint", "schedules.rp", c.now()))
        .await;
    assert!(late.is_err(), "reprint needs its renderer argument");
    let mut args = Map::new();
    args.insert("renderer".into(), json!("1.2.0"));
    let s = ch
        .start(StartReq {
            args,
            ..scheduled("reprint", "schedules.rp", c.now())
        })
        .await
        .unwrap();
    let id = s.run().run_id;
    c.advance(Duration::minutes(121));
    settle(&ch).await;
    assert_eq!(status(&f, id).await, "skipped");
    let wrap: Value = sqlx::query_scalar("select summary from runs where run_id = $1")
        .bind(id)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(wrap["reason"], "late");
    assert!(wrap["text"].as_str().unwrap().contains("skipped"));
    news_hotline::contracts()
        .check_output(
            "run.cancel",
            &json!({"run_id": id, "short_id": s.run().short_id,
        "status": "cancelled", "wrap": wrap, "text": "x"}),
        )
        .unwrap();
    let (_, w) = ch.lineup_cancel(&holder.short_id).await.unwrap();
    assert_eq!(w["status"], "cancelled");
    assert!(w["text"].as_str().unwrap().contains("Lineup"));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn pause_the_running_one_reserves_the_slot_for_the_new_run() {
    let f = Fixture::migrated().await;
    let c = clock();
    let ch = chief(&f, &c);
    let holder = board_start(&ch, "catchup", IfBusy::Offer)
        .await
        .unwrap()
        .run()
        .clone();
    // A running job of the holder: the drain waits for it.
    let job = queue::enqueue(
        &f.pool,
        &NewJob::new("test.fetch")
            .pool("cpu")
            .for_step(holder.run_id, 1),
    )
    .await
    .unwrap();
    let claimed = queue::claim(&f.pool, "cpu", 1, StdDuration::from_secs(60), "w")
        .await
        .unwrap();
    assert_eq!(claimed.jobs.len(), 1);
    let (held, new, pos) = ch
        .start_after_pause("edition", Map::new(), &holder.short_id)
        .await
        .unwrap();
    assert_eq!(pos, 1);
    assert_eq!(held.run_id, holder.run_id);
    assert_eq!(status(&f, holder.run_id).await, "pausing");
    // While the rank-0 run waits, nothing else takes the slot (review note 1)…
    settle(&ch).await;
    assert_eq!(
        status(&f, holder.run_id).await,
        "pausing",
        "the drain waits for the running job"
    );
    let e = ch
        .start(StartReq::board("reprint", reprint_args(), IfBusy::Offer))
        .await
        .unwrap_err();
    assert_eq!(
        e.details.unwrap()["offers"],
        json!(["queue"]),
        "the holder is already pausing"
    );
    // …the job finishes, the pause completes, and the new run takes the slot.
    let j = &claimed.jobs[0];
    queue::complete(&f.pool, job.job_id, j.lease("w"), &json!({}))
        .await
        .unwrap();
    settle(&ch).await;
    assert_eq!(status(&f, holder.run_id).await, "paused");
    settle(&ch).await;
    assert_eq!(status(&f, new.run_id).await, "running");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn a_queued_rank_0_run_blocks_a_free_slot_with_a_queue_only_offer() {
    // Review note 2: no run holds the slot, but a rank-0 run is queued for it.
    let f = Fixture::migrated().await;
    let c = clock();
    let ch = chief(&f, &c);
    let holder = board_start(&ch, "catchup", IfBusy::Offer)
        .await
        .unwrap()
        .run()
        .clone();
    let (_, new, _) = ch
        .start_after_pause("edition", Map::new(), &holder.short_id)
        .await
        .unwrap();
    // Finish the holder's pause without promoting (as if the tick had not run yet).
    ch.finish_pause(holder.run_id).await.unwrap();
    assert_eq!(status(&f, new.run_id).await, "queued");
    let e2 = ch
        .start(StartReq::board("reprint", reprint_args(), IfBusy::Offer))
        .await
        .unwrap_err();
    let d = e2.details.unwrap();
    assert_eq!(d["blocking"]["status"], "queued");
    assert_eq!(d["blocking"]["short_id"], new.short_id);
    assert_eq!(d["offers"], json!(["queue"]));
    // A resume of a paused heavy run does not take it either.
    let (_, to, _) = ch.resume(&holder.short_id).await.unwrap();
    assert_eq!(to, Status::Queued);
    settle(&ch).await;
    assert_eq!(
        status(&f, new.run_id).await,
        "running",
        "the rank-0 run goes first"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn an_approval_does_not_overtake_a_queued_rank_0_run() {
    // Review note 1, the third way into the slot: the last gate of a heavy run answered while the slot
    // is free but reserved for a rank-0 run.
    let f = Fixture::migrated().await;
    let c = clock();
    let ch = chief(&f, &c);
    // An Edition waiting at its plan gate (set up directly: no step handlers run here).
    let waiting = board_start(&ch, "edition", IfBusy::Offer)
        .await
        .unwrap()
        .run()
        .clone();
    let subject = json!({"plan": 1});
    sqlx::query("update runs set status = 'waiting_approval' where run_id = $1")
        .bind(waiting.run_id)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into gates (run_id, gate, round, step_seq, subject_sha, summary, opened_at)
         values ($1, 'plan', 1, 1, $2, '{}', now())",
    )
    .bind(waiting.run_id)
    .bind(gates::subject_sha(&subject).to_vec())
    .execute(&f.pool)
    .await
    .unwrap();
    // A Catchup takes the slot; "pause it and start this" reserves the slot for a Reprint at rank 0.
    let holder = board_start(&ch, "catchup", IfBusy::Offer)
        .await
        .unwrap()
        .run()
        .clone();
    let (_, reserved, _) = ch
        .start_after_pause("reprint", reprint_args(), &holder.short_id)
        .await
        .unwrap();
    ch.finish_pause(holder.run_id).await.unwrap();
    assert_eq!(status(&f, reserved.run_id).await, "queued");
    // The slot is free, but the approved Edition joins the Lineup behind the rank-0 run.
    let (_, to) = ch
        .answer(Answer {
            run: &waiting.short_id,
            gate: "plan",
            round: 1,
            subject_sha: &gates::hex(&gates::subject_sha(&subject)),
            decision: Decision::Approved,
            note: None,
        })
        .await
        .unwrap();
    assert_eq!(to, Status::Queued);
    settle(&ch).await;
    assert_eq!(status(&f, reserved.run_id).await, "running");
    assert_eq!(status(&f, waiting.run_id).await, "queued");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn a_claim_and_a_pause_race_both_ways_with_real_transactions() {
    let f = Fixture::migrated().await;
    let c = clock();
    let ch = chief(&f, &c);
    let run = board_start(&ch, "catchup", IfBusy::Offer)
        .await
        .unwrap()
        .run()
        .clone();
    for _ in 0..2 {
        queue::enqueue(
            &f.pool,
            &NewJob::new("test.fetch")
                .pool("cpu")
                .for_step(run.run_id, 1),
        )
        .await
        .unwrap();
    }
    // (a) The claim holds the run row FOR SHARE; the pause waits for it, then its jobs are running.
    let mut t1 = f.pool.begin().await.unwrap();
    let got = queue::claim_in_tx(&mut t1, "cpu", 1, StdDuration::from_secs(60), "w")
        .await
        .unwrap();
    assert_eq!(got.jobs.len(), 1);
    let pool = f.pool.clone();
    let id = run.run_id;
    let t2 = tokio::spawn(async move {
        let mut tx = pool.begin().await.unwrap();
        state::transition(
            &mut tx,
            id,
            &[Status::Running],
            Change {
                stop_mode: Some(StopMode::Drain),
                paused_reason: Some(PausedReason::Karthik),
                ..Change::to(Status::Pausing)
            },
            Utc::now(),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    });
    tokio::time::sleep(StdDuration::from_millis(400)).await;
    assert!(
        !t2.is_finished(),
        "the pause waits for the claim's FOR SHARE lock"
    );
    t1.commit().await.unwrap();
    t2.await.unwrap();
    assert_eq!(status(&f, run.run_id).await, "pausing");
    assert_eq!(
        f.scalar_i64(&format!(
            "select count(*) from jobs where run_id = '{id}' and state = 'running'"
        ))
        .await,
        1
    );
    // A claim after the pause takes nothing of the run.
    let none = queue::claim(&f.pool, "cpu", 5, StdDuration::from_secs(60), "w2")
        .await
        .unwrap();
    assert!(none.jobs.is_empty());
    // Step 3 moves the running job to retryable with no holder; a late completion is refused.
    ch.finish_pause(run.run_id).await.unwrap();
    assert_eq!(status(&f, run.run_id).await, "paused");
    let j = &got.jobs[0];
    let late = queue::complete(&f.pool, j.job_id, j.lease("w"), &json!({}))
        .await
        .unwrap_err();
    assert_eq!(late.code, ErrorCode::Conflict);
    // The paused run's retryable job is not claimed; after resume it is.
    assert!(
        queue::claim(&f.pool, "cpu", 5, StdDuration::from_secs(60), "w3")
            .await
            .unwrap()
            .jobs
            .is_empty()
    );
    ch.resume(&run.short_id).await.unwrap();
    assert_eq!(
        queue::claim(&f.pool, "cpu", 5, StdDuration::from_secs(60), "w3")
            .await
            .unwrap()
            .jobs
            .len(),
        2
    );

    // (b) The pause holds the row; the claim waits, then takes nothing of that run.
    let g = Fixture::migrated().await;
    let ch = chief(&g, &c);
    let run = board_start(&ch, "catchup", IfBusy::Offer)
        .await
        .unwrap()
        .run()
        .clone();
    queue::enqueue(
        &g.pool,
        &NewJob::new("test.fetch")
            .pool("cpu")
            .for_step(run.run_id, 1),
    )
    .await
    .unwrap();
    let mut t2 = g.pool.begin().await.unwrap();
    state::transition(
        &mut t2,
        run.run_id,
        &[Status::Running],
        Change {
            stop_mode: Some(StopMode::Drain),
            paused_reason: Some(PausedReason::Karthik),
            ..Change::to(Status::Pausing)
        },
        Utc::now(),
    )
    .await
    .unwrap();
    let pool = g.pool.clone();
    let t1 = tokio::spawn(async move {
        queue::claim(&pool, "cpu", 5, StdDuration::from_secs(60), "w")
            .await
            .unwrap()
    });
    tokio::time::sleep(StdDuration::from_millis(400)).await;
    assert!(
        !t1.is_finished(),
        "the claim waits for the pause's row lock"
    );
    t2.commit().await.unwrap();
    assert!(t1.await.unwrap().jobs.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn takeover_recovery_revokes_leases_and_completes_pauses_before_any_claim() {
    // Review note 3: the old instance's lock went with its session; the new one fences from its side.
    let f = Fixture::migrated().await;
    let c = clock();
    let old = chief(&f, &c);
    let run = board_start(&old, "catchup", IfBusy::Offer)
        .await
        .unwrap()
        .run()
        .clone();
    let pausing = board_start(&old, "frontpage", IfBusy::Offer)
        .await
        .unwrap()
        .run()
        .clone();
    for r in [run.run_id, pausing.run_id] {
        queue::enqueue(
            &f.pool,
            &NewJob::new("test.fetch").pool("cpu").for_step(r, 1),
        )
        .await
        .unwrap();
    }
    let claimed = queue::claim(&f.pool, "cpu", 5, StdDuration::from_secs(600), "old")
        .await
        .unwrap();
    assert_eq!(claimed.jobs.len(), 2);
    old.pause(&pausing.short_id).await.unwrap();
    // A new instance recovers: every lease revoked, the pause completed.
    let new = chief(&f, &c);
    let report = new.recover().await.unwrap();
    assert_eq!(report["revoked"].as_array().unwrap().len(), 2);
    assert_eq!(status(&f, pausing.run_id).await, "paused");
    for j in &claimed.jobs {
        let e = queue::complete(&f.pool, j.job_id, j.lease("old"), &json!({}))
            .await
            .unwrap_err();
        assert_eq!(
            e.code,
            ErrorCode::Conflict,
            "a stale completion fails the lease check"
        );
    }
    assert_eq!(
        f.scalar_i64(&format!(
            "select count(*) from jobs where run_id = '{}' and state = 'retryable'",
            run.run_id
        ))
        .await,
        1
    );
}

/// A step handler that records its calls and returns what it is told.
struct Scripted {
    calls: Arc<AtomicUsize>,
    out: Box<dyn Fn(&StepCx) -> StepOutcome + Send + Sync>,
}

impl StepHandler for Scripted {
    fn run<'a>(&'a self, cx: &'a mut StepCx) -> BoxFut<'a, Result<StepOutcome, StepError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let o = (self.out)(cx);
        Box::pin(async move { Ok(o) })
    }
}

fn done(calls: &Arc<AtomicUsize>) -> Arc<dyn StepHandler> {
    Arc::new(Scripted {
        calls: calls.clone(),
        out: Box::new(|_| StepOutcome::Done(json!({"wrap": {"checked": 1}}))),
    })
}

/// What `authorize()` returned inside the step.
type Seen = Result<Option<gates::Approved>, ErrorCode>;

/// Records the result of `authorize()` from inside the publish step.
struct Publish {
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl StepHandler for Publish {
    fn run<'a>(&'a self, cx: &'a mut StepCx) -> BoxFut<'a, Result<StepOutcome, StepError>> {
        Box::pin(async move {
            let mut tx = cx.db.begin().await.unwrap();
            let r = gates::authorize(
                &mut tx,
                GRequest {
                    run_id: cx.run_id,
                    step_seq: cx.seq,
                    command: GCommand::HomePublish,
                    item: None,
                },
            )
            .await
            .map(|a| a.approved)
            .map_err(|e| e.code);
            self.seen.lock().unwrap().push(r);
            Ok(StepOutcome::Done(json!({"wrap": {"published": 1}})))
        })
    }
}

async fn wait_status(f: &Fixture, id: Uuid, want: &str) {
    for _ in 0..100 {
        if status(f, id).await == want {
            return;
        }
        tokio::time::sleep(StdDuration::from_millis(50)).await;
    }
    panic!(
        "run {id} never became {want}; it is {}",
        status(f, id).await
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn a_gated_run_waits_for_board_binds_the_hash_and_authorises_once() {
    let f = Fixture::migrated().await;
    let c = clock();
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let subject = json!({"draft": 7, "items": []});
    let s2 = subject.clone();
    let handlers = Handlers::default()
        .with("frontpage.candidates", done(&calls))
        .with("frontpage.draft", done(&calls))
        .with("frontpage.check", done(&calls))
        .with("frontpage.preview", done(&calls))
        .with(
            "frontpage.review",
            Arc::new(Scripted {
                calls: calls.clone(),
                out: Box::new(move |_| {
                    StepOutcome::OpenGates(vec![GateOpen {
                        gate: "publish".into(),
                        subject: s2.clone(),
                        summary: json!({"text": "Curation v42"}),
                        preview: None,
                        whole_subject: true,
                    }])
                }),
            }),
        )
        .with(
            "frontpage.publish",
            Arc::new(Publish { seen: seen.clone() }),
        );
    let ch = Chief::builder(f.pool.clone(), Arc::new(c.clone()))
        .handlers(handlers)
        .build();
    let run = board_start(&ch, "frontpage", IfBusy::Offer)
        .await
        .unwrap()
        .run()
        .clone();
    ch.ensure_drivers().await.unwrap();
    wait_status(&f, run.run_id, "waiting_approval").await;
    assert_eq!(calls.load(Ordering::SeqCst), 5);
    // Outside its step, every G-tier command is gated.
    for g in GCommand::ALL {
        let mut tx = f.pool.begin().await.unwrap();
        let e = gates::authorize(
            &mut tx,
            GRequest {
                run_id: run.run_id,
                step_seq: 6,
                command: g,
                item: None,
            },
        )
        .await;
        assert_eq!(e.err().map(|e| e.code), Some(ErrorCode::Gated), "{g:?}");
    }
    // An approval for another hash is refused (gated), and the foreign key refuses a direct insert.
    let wrong = gates::hex(&gates::subject_sha(&json!({"draft": 8})));
    let e = ch
        .answer(Answer {
            run: &run.short_id,
            gate: "publish",
            round: 1,
            subject_sha: &wrong,
            decision: Decision::Approved,
            note: None,
        })
        .await
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Gated);
    let fk = sqlx::query(
        "insert into approvals (run_id, gate, round, decision, decided_at, subject_sha, origin)
                          values ($1, 'publish', 1, 'approved', now(), $2, 'board')",
    )
    .bind(run.run_id)
    .bind(gates::subject_sha(&json!({"draft": 8})).to_vec())
    .execute(&f.pool)
    .await;
    assert!(fk.is_err());
    // The hash binds the subject, the summary and the preview Board shows.
    let right = gates::hex(&gates::gate_sha(&GateOpen {
        gate: "publish".into(),
        subject: subject.clone(),
        summary: json!({"text": "Curation v42"}),
        preview: None,
        whole_subject: true,
    }));
    let (_, st) = ch
        .answer(Answer {
            run: &run.short_id,
            gate: "publish",
            round: 1,
            subject_sha: &right,
            decision: Decision::Approved,
            note: None,
        })
        .await
        .unwrap();
    assert_eq!(st, Status::Running);
    ch.ensure_drivers().await.unwrap();
    wait_status(&f, run.run_id, "succeeded").await;
    let a = seen.lock().unwrap().clone();
    let a = a[0].expect("authorised").expect("by the gate's approval");
    assert_eq!(seen.lock().unwrap().len(), 1);
    // The authorisation names the approved round, the hash Board sent and the subject's own hash.
    assert_eq!(a.round, 1);
    assert_eq!(gates::hex(&a.gate_sha), right);
    assert_eq!(a.subject_sha, gates::subject_sha(&subject));
    // The approval was consumed with the step: it authorises nothing again.
    let consumed: Option<DateTime<Utc>> =
        sqlx::query_scalar("select consumed_at from approvals where run_id = $1")
            .bind(run.run_id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert!(consumed.is_some());
    let wrap: Value = sqlx::query_scalar("select summary from runs where run_id = $1")
        .bind(run.run_id)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(wrap["counts"]["published"], 1);
    assert_eq!(wrap["counts"]["checked"], 4);
    assert_eq!(wrap["approvals"][0]["decision"], "approved");
    let show = news_chief::board::run_show(&f.pool, &run.short_id, c.now(), None)
        .await
        .unwrap();
    news_hotline::contracts()
        .check_output("run.show", &show)
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn a_restarted_driver_resumes_after_the_last_finished_step() {
    let f = Fixture::migrated().await;
    let c = clock();
    let calls = Arc::new(AtomicUsize::new(0));
    let ch = Chief::builder(f.pool.clone(), Arc::new(c.clone()))
        .handlers(Handlers::default().with("rollcall.health", done(&calls)))
        .build();
    let run = board_start(&ch, "rollcall", IfBusy::Offer)
        .await
        .unwrap()
        .run()
        .clone();
    ch.ensure_drivers().await.unwrap();
    // Step 2 is not built: the run fails there with a Wrap, after step 1 ran once.
    wait_status(&f, run.run_id, "failed").await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let steps: Vec<(i16, String)> =
        sqlx::query_as("select seq, status from run_steps where run_id = $1 order by seq")
            .bind(run.run_id)
            .fetch_all(&f.pool)
            .await
            .unwrap();
    assert_eq!(steps[0], (1, "done".into()));
    assert_eq!(steps[1], (2, "failed".into()));
    let wrap: Value = sqlx::query_scalar("select summary from runs where run_id = $1")
        .bind(run.run_id)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert!(wrap["reason"].as_str().unwrap().contains("not built yet"));

    // A run killed after step 1: a new Chief re-enters at step 2 and never repeats step 1.
    let g = Fixture::migrated().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let first = Chief::builder(g.pool.clone(), Arc::new(c.clone()))
        .handlers(
            Handlers::default()
                .with("frontpage.candidates", done(&calls))
                .with(
                    "frontpage.draft",
                    Arc::new(Scripted {
                        calls: calls.clone(),
                        out: Box::new(|_| StepOutcome::Wait),
                    }),
                ),
        )
        .build();
    let run = board_start(&first, "frontpage", IfBusy::Offer)
        .await
        .unwrap()
        .run()
        .clone();
    first.ensure_drivers().await.unwrap();
    for _ in 0..100 {
        if calls.load(Ordering::SeqCst) >= 2 {
            break;
        }
        tokio::time::sleep(StdDuration::from_millis(50)).await;
    }
    first.stop_drivers().await;
    let calls2 = Arc::new(AtomicUsize::new(0));
    let second = Chief::builder(g.pool.clone(), Arc::new(c.clone()))
        .handlers(
            Handlers::default()
                .with("frontpage.candidates", done(&calls2))
                .with("frontpage.draft", done(&calls2))
                .with("frontpage.check", done(&calls2))
                .with("frontpage.preview", done(&calls2)),
        )
        .build();
    second.recover().await.unwrap();
    second.ensure_drivers().await.unwrap();
    // It stops at the unbuilt review step (fails), having run draft, check and preview once each.
    wait_status(&g, run.run_id, "failed").await;
    assert_eq!(
        calls2.load(Ordering::SeqCst),
        3,
        "candidates was not run again"
    );
}

/// A policy whose answers the test sets.
#[derive(Default)]
struct Knobs {
    claim: Mutex<Option<AiClaim>>,
}

impl StartPolicy for Knobs {
    fn check(&self, _: &news_chief::presets::PresetDecl, _: Origin, _: DateTime<Utc>) -> Policy {
        match *self.claim.lock().unwrap() {
            Some(AiClaim::Cap) | Some(AiClaim::Reserve) => Policy::Refuse {
                reason: "quota".into(),
                text: "quota".into(),
            },
            _ => Policy::Allow,
        }
    }
    fn ai_claim(&self, _: DateTime<Utc>) -> AiClaim {
        self.claim.lock().unwrap().unwrap_or(AiClaim::Allow)
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn usage_limits_and_the_quota_reserve_pause_runs_that_resume_on_their_own() {
    let f = Fixture::migrated().await;
    let c = clock();
    let knobs = Arc::new(Knobs::default());
    let ch = Chief::builder(f.pool.clone(), Arc::new(c.clone()))
        .policy(knobs.clone())
        .build();
    let run = board_start(&ch, "edition", IfBusy::Offer)
        .await
        .unwrap()
        .run()
        .clone();
    queue::enqueue(&f.pool, &NewJob::new("ai.write").for_step(run.run_id, 7))
        .await
        .unwrap();
    // A usage-limit failure paused the `ai` pool: the run pauses until the reset, then resumes.
    let reset = c.now() + Duration::hours(1);
    queue::pause(&f.pool, "ai", reset).await.unwrap();
    settle(&ch).await;
    settle(&ch).await;
    assert_eq!(status(&f, run.run_id).await, "paused");
    let (reason, at): (String, Option<DateTime<Utc>>) =
        sqlx::query_as("select paused_reason, resume_at from runs where run_id = $1")
            .bind(run.run_id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!((reason.as_str(), at), ("usage_limit", Some(reset)));
    queue::unpause(&f.pool, "ai").await.unwrap();
    c.set(reset + Duration::seconds(1));
    settle(&ch).await;
    assert_eq!(status(&f, run.run_id).await, "running");
    // The reserve pauses it (quota); new starts are refused; it resumes when usage falls back.
    *knobs.claim.lock().unwrap() = Some(AiClaim::Reserve);
    settle(&ch).await;
    settle(&ch).await;
    assert_eq!(status(&f, run.run_id).await, "paused");
    assert_eq!(
        board_start(&ch, "frontpage", IfBusy::Offer)
            .await
            .unwrap_err()
            .code,
        ErrorCode::Precondition
    );
    *knobs.claim.lock().unwrap() = None;
    settle(&ch).await;
    assert_eq!(status(&f, run.run_id).await, "running");
    // At the cap, the run's live ai jobs are cancelled.
    *knobs.claim.lock().unwrap() = Some(AiClaim::Cap);
    settle(&ch).await;
    assert_eq!(
        f.scalar_i64(&format!(
            "select count(*) from jobs where run_id = '{}' and state = 'cancelled'",
            run.run_id
        ))
        .await,
        1
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn rota_drops_missed_slots_fires_the_latest_and_skips_past_the_late_limit() {
    let f = Fixture::migrated().await;
    let ok = |_: &news_standing::Schedule| Ok(());
    // Schedules change at the clock's start; slots before the change never fire.
    let c = ManualClock::new(t("2026-11-12T05:00:00Z"));
    sqlx::query("update standing set created_at = $1")
        .bind(c.now())
        .execute(&f.pool)
        .await
        .unwrap();
    news_standing::set(
        &f.pool,
        1,
        &[
            ("rota.time_zone".into(), json!("UTC")),
            (
                "schedules.fp".into(),
                json!({"preset": "frontpage", "args": {}, "cron": "0 6,18 * * *", "enabled": true}),
            ),
            (
                "schedules.hourly".into(),
                json!({"preset": "rollcall", "args": {}, "cron": "0 * * * *", "enabled": true}),
            ),
        ],
        None,
        &ok,
    )
    .await
    .unwrap();
    sqlx::query("update standing set created_at = $1")
        .bind(c.now())
        .execute(&f.pool)
        .await
        .unwrap();
    let ch = chief(&f, &c);
    let rota = news_rota::Rota::new(
        f.pool.clone(),
        ch.clone(),
        Arc::new(c.clone()),
        news_rota::RotaPolicy::default(),
        Arc::new(news_rota::Unknown),
        Arc::new(|_, _| {}),
    );
    // Laptop asleep from 05:00 to 07:30 (no ticks). At the first tick after: the hourly schedule's latest slot (07:00)
    // is late with the next one 30 minutes away: missed. The 06:00 Frontpage slot is late, but the next
    // is 10.5 h away and it is within the late limit: it fires.
    c.set(t("2026-11-12T07:30:00Z"));
    let fires = rota.tick().await.unwrap();
    let fp = fires.iter().find(|(k, _)| k == "schedules.fp").unwrap();
    assert!(
        matches!(
            &fp.1,
            news_rota::Fire::Fired {
                outcome: Started::Running(_),
                ..
            }
        ),
        "{fp:?}"
    );
    let hr = fires.iter().find(|(k, _)| k == "schedules.hourly").unwrap();
    assert!(
        matches!(&hr.1, news_rota::Fire::Missed { slot, missed_before: 1 } if *slot == t("2026-11-12T07:00:00Z")),
        "{hr:?}"
    );
    assert_eq!(
        f.scalar_i64("select count(*) from rota_fires where outcome = 'missed' and run_id is null")
            .await,
        1
    );
    // Another tick fires nothing twice.
    assert!(rota.tick().await.unwrap().is_empty());
    // Asleep again until 20:30: the 18:00 slot is past the late limit (2 h): skipped with a Wrap.
    let fp_run = sqlx::query_scalar::<_, Uuid>(
        "select run_id from rota_fires where schedule_key = 'schedules.fp'",
    )
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let mut tx = f.pool.begin().await.unwrap();
    let r = news_chief::row::load(&mut tx, fp_run, true).await.unwrap();
    news_chief::wrap::finish(&mut tx, &r, Status::Succeeded, None, c.now())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    c.set(t("2026-11-12T20:30:00Z"));
    let fires = rota.tick().await.unwrap();
    let fp = fires.iter().find(|(k, _)| k == "schedules.fp").unwrap();
    match &fp.1 {
        news_rota::Fire::Fired {
            outcome: Started::Skipped { reason, run },
            ..
        } => {
            assert_eq!(reason, "late");
            assert_eq!(status(&f, run.run_id).await, "skipped");
        }
        other => panic!("{other:?}"),
    }
    // The ai pool follows Standing.
    news_standing::set(
        &f.pool,
        2,
        &[("pools.ai.max_running".into(), json!(5))],
        None,
        &ok,
    )
    .await
    .unwrap();
    rota.tick().await.unwrap();
    assert_eq!(
        f.scalar_i64("select max_running::bigint from pools where pool = 'ai'")
            .await,
        5
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn the_hotline_gates_h_commands_from_anything_but_board_and_answers_reads() {
    use news_hotline::{Bus, Peer, Service};
    let f = Fixture::migrated().await;
    let c = clock();
    let state = Arc::new(news_service::State::default());
    state.set(news_chief::board::ServiceState::Up);
    let bureau = Arc::new(news_service::handlers::Bureau::new(
        state,
        Arc::new(Bus::default()),
        Arc::new(news_service::log::LogRing::default()),
    ));
    bureau.set_core(Some(Arc::new(news_service::handlers::Core {
        db: f.pool.clone(),
        chief: chief(&f, &c),
        policy: Arc::new(news_chief::AllowAll),
        stopper: Arc::new(news_chief::NoStopper),
    })));
    let call = |peer: Peer, cmd: &'static str, args: Value| {
        let svc: Arc<dyn Service> = bureau.clone();
        async move {
            let (mut client, server) = tokio::io::duplex(1 << 20);
            let h = tokio::spawn(news_hotline::serve_conn(server, peer, svc));
            let env = news_hotline::call(&mut client, "t-1", cmd, args)
                .await
                .unwrap();
            drop(client);
            let _ = h.await;
            env
        }
    };
    let terminal = Peer {
        argv1_is_board: false,
        ..Peer::board()
    };
    let env = call(
        terminal.clone(),
        "run.start",
        json!({"preset": "frontpage", "args": {}}),
    )
    .await;
    assert_eq!(env["error"]["code"], "gated", "{env}");
    assert_eq!(
        f.scalar_i64("select count(*) from hotline_refusals where refusal = 'not_board'")
            .await,
        1
    );
    let env = call(
        Peer::board(),
        "run.start",
        json!({"preset": "frontpage", "args": {}}),
    )
    .await;
    assert_eq!(env["ok"], true, "{env}");
    news_hotline::contracts()
        .check_output("run.start", &env["data"])
        .unwrap();
    for (cmd, args) in [
        ("status", json!({})),
        ("run.presets", json!({})),
        ("run.list", json!({})),
        ("lineup.list", json!({})),
        ("standing.get", json!({})),
        ("standing.history", json!({})),
        ("service.ping", json!({})),
        ("service.log", json!({})),
    ] {
        let env = call(terminal.clone(), cmd, args).await;
        assert_eq!(env["ok"], true, "{cmd}: {env}");
        news_hotline::contracts()
            .check_output(cmd, &env["data"])
            .unwrap_or_else(|e| panic!("{cmd}: {e}\n{env}"));
    }
    let env = call(
        Peer::board(),
        "standing.set",
        json!({"base_version": 1, "changes": [{"key": "chief.stall_minutes", "value": 20}]}),
    )
    .await;
    news_hotline::contracts()
        .check_output("standing.set", &env["data"])
        .unwrap();
    let env = call(
        Peer::board(),
        "standing.set",
        json!({"base_version": 1, "changes": [{"key": "chief.stall_minutes", "value": 25}]}),
    )
    .await;
    assert_eq!(env["error"]["code"], "conflict");
    // The terminal's `news run status` gives the same shape in-process.
    let (env, code, _, _) = f.call(&["run", "status"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["service"]["state"], "down");
    let (env, code, _, _) = f.call(&["standing", "show", "pools."]);
    assert_eq!(code, 0, "{env}");
    let (env, code, _, _) = f.call(&["run", "lineup"]);
    assert_eq!(code, 0, "{env}");
    let (env, code, _, _) = f.call(&["run", "list"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["items"].as_array().unwrap().len(), 1);
}

/// A slow inline step: counts entries and concurrent runs, marks `ended` when its task stops (by
/// returning or by being aborted), and `finished` only when it returns on its own.
struct Slow {
    entered: Arc<AtomicUsize>,
    active: Arc<AtomicUsize>,
    max_active: Arc<AtomicUsize>,
    ended: Arc<AtomicUsize>,
    finished: Arc<AtomicUsize>,
    secs: u64,
}

struct Ended(Arc<AtomicUsize>, Arc<AtomicUsize>);
impl Drop for Ended {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
        self.1.fetch_add(1, Ordering::SeqCst);
    }
}

impl StepHandler for Slow {
    fn run<'a>(&'a self, _cx: &'a mut StepCx) -> BoxFut<'a, Result<StepOutcome, StepError>> {
        Box::pin(async move {
            self.entered.fetch_add(1, Ordering::SeqCst);
            let now = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(now, Ordering::SeqCst);
            let _guard = Ended(self.active.clone(), self.ended.clone());
            tokio::time::sleep(StdDuration::from_secs(self.secs)).await;
            self.finished.fetch_add(1, Ordering::SeqCst);
            Ok(StepOutcome::Done(json!({})))
        })
    }
}

fn slow(secs: u64) -> (Arc<Slow>, [Arc<AtomicUsize>; 5]) {
    let c: [Arc<AtomicUsize>; 5] = std::array::from_fn(|_| Arc::new(AtomicUsize::new(0)));
    let s = Arc::new(Slow {
        entered: c[0].clone(),
        active: c[1].clone(),
        max_active: c[2].clone(),
        ended: c[3].clone(),
        finished: c[4].clone(),
        secs,
    });
    (s, c)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn a_cancel_mid_inline_step_frees_the_slot_only_after_the_step_task_ended() {
    let f = Fixture::migrated().await;
    let c = clock();
    let (h, [entered, _, _, ended, finished]) = slow(30);
    let ch = Chief::builder(f.pool.clone(), Arc::new(c.clone()))
        .handlers(Handlers::default().with("catchup.preflight", h))
        .build();
    let run = board_start(&ch, "catchup", IfBusy::Offer)
        .await
        .unwrap()
        .run()
        .clone();
    ch.ensure_drivers().await.unwrap();
    for _ in 0..100 {
        if entered.load(Ordering::SeqCst) == 1 {
            break;
        }
        tokio::time::sleep(StdDuration::from_millis(50)).await;
    }
    // A heavy run waits in the Lineup behind it.
    let queued = ch
        .start(StartReq::board("reprint", reprint_args(), IfBusy::Queue))
        .await
        .unwrap()
        .run()
        .clone();
    ch.cancel(&run.short_id).await.unwrap();
    // The tick stops the step task, waits for it, then cancels the run and frees the slot.
    settle(&ch).await;
    for _ in 0..100 {
        if status(&f, run.run_id).await == "cancelled" {
            break;
        }
        tokio::time::sleep(StdDuration::from_millis(50)).await;
    }
    assert_eq!(status(&f, run.run_id).await, "cancelled");
    assert_eq!(
        ended.load(Ordering::SeqCst),
        1,
        "the step task had ended before the slot was freed"
    );
    assert_eq!(finished.load(Ordering::SeqCst), 0);
    settle(&ch).await;
    assert_eq!(status(&f, queued.run_id).await, "running");
    // The stopped step wrote nothing: not done.
    let s: String =
        sqlx::query_scalar("select status from run_steps where run_id = $1 and seq = 1")
            .bind(run.run_id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_ne!(s, "done");
    ch.stop_drivers().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn a_lost_lock_never_runs_a_step_twice_at_once() {
    let f = Fixture::migrated().await;
    let c = clock();
    let (h, [entered, active, max_active, ended, finished]) = slow(1);
    let handlers = Handlers::default().with("rollcall.health", h);
    let old = Chief::builder(f.pool.clone(), Arc::new(c.clone()))
        .handlers(handlers.clone())
        .build();
    let run = board_start(&old, "rollcall", IfBusy::Offer)
        .await
        .unwrap()
        .run()
        .clone();
    old.ensure_drivers().await.unwrap();
    for _ in 0..100 {
        if entered.load(Ordering::SeqCst) == 1 {
            break;
        }
        tokio::time::sleep(StdDuration::from_millis(20)).await;
    }
    // The fence: stopping the drivers also stops and awaits their step tasks.
    old.stop_drivers().await;
    assert_eq!(active.load(Ordering::SeqCst), 0);
    assert_eq!(ended.load(Ordering::SeqCst), 1);
    // A new instance re-enters the step: it runs once more, never beside the old one.
    let new = Chief::builder(f.pool.clone(), Arc::new(c.clone()))
        .handlers(handlers)
        .build();
    new.recover().await.unwrap();
    new.ensure_drivers().await.unwrap();
    wait_status(&f, run.run_id, "failed").await; // step 2 is not built
    assert_eq!(entered.load(Ordering::SeqCst), 2);
    assert_eq!(finished.load(Ordering::SeqCst), 1);
    assert_eq!(max_active.load(Ordering::SeqCst), 1);
}
