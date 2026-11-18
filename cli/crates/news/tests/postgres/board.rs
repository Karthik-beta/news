//! Board's client against the real Bureau service on PostgreSQL (issue #29, "Done when"): Board starts,
//! asks for changes on, approves and cancels fixture runs over the Hotline; the same actions from a
//! terminal-style client are refused; a gate raises a toast that opens Board on that gate; an update
//! waits for the safe point. The server side is the service's own handlers behind
//! `news_hotline::serve_conn`, over an in-memory stream instead of the named pipe.

use super::fixture::Fixture;
use news_board::app::Board;
use news_board::link::{BoxStream, Connect, Link};
use news_board::secrets::Store;
use news_chief::driver::{Handlers, StepCx, StepError, StepHandler, StepOutcome};
use news_chief::gates::GateOpen;
use news_chief::{BoxFut, Chief};
use news_core::clock::SystemClock;
use news_hotline::{Bus, Peer, Service};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

struct Scripted(Box<dyn Fn() -> StepOutcome + Send + Sync>);

impl StepHandler for Scripted {
    fn run<'a>(&'a self, _cx: &'a mut StepCx) -> BoxFut<'a, Result<StepOutcome, StepError>> {
        let o = (self.0)();
        Box::pin(async move { Ok(o) })
    }
}

fn done() -> Arc<dyn StepHandler> {
    Arc::new(Scripted(Box::new(|| {
        StepOutcome::Done(json!({"wrap": {"checked": 1}}))
    })))
}

/// The curation review gate; each time it opens, the draft is a new one (a new hash).
fn review(reviews: Arc<AtomicUsize>) -> Arc<dyn StepHandler> {
    Arc::new(Scripted(Box::new(move || {
        let n = reviews.fetch_add(1, Ordering::SeqCst) + 1;
        StepOutcome::OpenGates(vec![GateOpen {
            gate: "publish".into(),
            subject: json!({"draft": n}),
            summary: json!({"text": format!("Curation draft {n}")}),
            preview: Some(json!({"draft": n})),
        }])
    })))
}

struct Duplex {
    svc: Arc<dyn Service>,
    peer: Peer,
    rt: tokio::runtime::Handle,
}

impl Connect for Duplex {
    fn connect(&self) -> news_hotline::BoxFut<'_, Result<BoxStream, String>> {
        Box::pin(async {
            let (client, server) = tokio::io::duplex(1 << 20);
            self.rt.spawn(news_hotline::serve_conn(
                server,
                self.peer.clone(),
                self.svc.clone(),
            ));
            Ok(Box::new(client) as BoxStream)
        })
    }
}

#[derive(Default)]
struct Mem(Mutex<Vec<String>>);

impl Store for Mem {
    fn write(&self, t: &str, _: &str) -> std::io::Result<()> {
        self.0.lock().unwrap().push(t.to_owned());
        Ok(())
    }
    fn exists(&self, t: &str) -> bool {
        self.0.lock().unwrap().iter().any(|x| x == t)
    }
    fn delete(&self, _: &str) -> std::io::Result<()> {
        Ok(())
    }
}

fn board(svc: &Arc<dyn Service>, peer: Peer) -> Board {
    let conn = Duplex {
        svc: svc.clone(),
        peer,
        rt: tokio::runtime::Handle::current(),
    };
    Board::new(Link::spawn(Arc::new(conn), || {}), Box::<Mem>::default())
}

async fn until(b: &mut Board, what: &str, cond: impl Fn(&Board) -> bool) {
    // The service answers in about a second on a Docker database: allow half a minute.
    for i in 0..1500 {
        b.poll();
        if cond(b) {
            return;
        }
        // Events normally refresh the board; a periodic read keeps the test from depending on timing.
        if i % 100 == 99 {
            b.refresh_status();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!(
        "timed out waiting for {what}: {:?}\n{:?}\nin flight {}\nstatus {}",
        b.conn, b.feedback, b.in_flight, b.status
    );
}

async fn run_status(f: &Fixture, short: &str) -> String {
    f.scalar_str(&format!(
        "select status from runs where short_id = '{short}'"
    ))
    .await
}

async fn run_is(f: &Fixture, short: &str, want: &str) {
    for _ in 0..600 {
        if run_status(f, short).await == want {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("{short} is {}, not {want}", run_status(f, short).await);
}

fn open_gate(b: &Board) -> Option<Value> {
    b.status["waiting"]
        .as_array()?
        .iter()
        .find_map(|w| (!w["gate"].is_null()).then(|| w["gate"].clone()))
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn board_starts_asks_for_changes_approves_and_cancels_runs_through_the_service() {
    let f = Fixture::migrated().await;
    let reviews = Arc::new(AtomicUsize::new(0));
    let handlers = Handlers::default()
        .with("frontpage.candidates", done())
        .with("frontpage.draft", done())
        .with("frontpage.check", done())
        .with("frontpage.preview", done())
        .with("frontpage.review", review(reviews.clone()))
        .with("frontpage.publish", done());
    let bus = Arc::new(Bus::default());
    let emit_bus = bus.clone();
    let chief = Chief::builder(f.pool.clone(), Arc::new(SystemClock))
        .handlers(handlers)
        .emit(Arc::new(move |topic, data| emit_bus.emit(topic, data)))
        .build();
    let state = Arc::new(news_service::State::default());
    state.set(news_chief::board::ServiceState::Up);
    let bureau = Arc::new(news_service::handlers::Bureau::new(
        state,
        bus,
        Arc::new(news_service::log::LogRing::default()),
    ));
    bureau.set_core(Some(Arc::new(news_service::handlers::Core {
        db: f.pool.clone(),
        chief: chief.clone(),
        policy: Arc::new(news_chief::AllowAll),
        stopper: Arc::new(news_chief::NoStopper),
    })));
    // The service's ticks, as the host runs them.
    let ticker = chief.clone();
    let ticks = tokio::spawn(async move {
        loop {
            let _ = ticker.tick().await;
            let _ = ticker.ensure_drivers().await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });
    let svc: Arc<dyn Service> = bureau.clone();
    let mut b = board(&svc, Peer::board());
    until(&mut b, "Board online with the presets", |b| {
        b.can_act() && b.preset("frontpage").is_some()
    })
    .await;
    // Let the event connection subscribe before anything happens.
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Start: Board sends `run.start`; the run reaches its review gate.
    b.forms.preset = "frontpage".into();
    b.start_run("frontpage", None);
    until(&mut b, "the first gate", |b| open_gate(b).is_some()).await;
    let gate = open_gate(&b).unwrap();
    let short = gate["short_id"].as_str().unwrap().to_owned();
    assert_eq!(gate["round"], 1);
    assert_eq!(run_status(&f, &short).await, "waiting_approval");

    // The gate raised a toast that opens Board on it.
    until(&mut b, "a toast", |b| !b.toasts.is_empty()).await;
    let toast = b.toasts.remove(0);
    assert_eq!(toast.open.arg(), format!("gate:{short}.publish"));
    b.open(&toast.open);
    assert_eq!(b.selected_run.as_deref(), Some(short.as_str()));
    until(&mut b, "the run detail", |b| b.detail.is_some()).await;

    // A stale answer is refused: the hash must be the one the gate holds now.
    let stale = {
        let mut g = gate.clone();
        g["subject_sha"] = json!("00".repeat(32));
        g
    };
    b.answer_gate(&stale, "approve");
    until(&mut b, "the refusal", |b| {
        b.feedback.as_ref().is_some_and(|f| !f.ok)
    })
    .await;
    assert_eq!(run_status(&f, &short).await, "waiting_approval");

    // Request changes (a note is required), then the new draft comes back with a new hash.
    b.forms
        .gate_notes
        .insert(format!("{short}.publish"), "shorter lead".into());
    b.answer_gate(&gate, "changes");
    until(&mut b, "round 2", |b| {
        open_gate(b).is_some_and(|g| g["round"] == 2 && g["subject_sha"] != gate["subject_sha"])
    })
    .await;
    let gate2 = open_gate(&b).unwrap();
    // The service recorded the answer with its note, origin board, and no approval for the gate yet.
    assert_eq!(
        f.scalar_str(&format!(
            "select a.decision || ':' || a.note || ':' || a.origin from approvals a join runs r using (run_id)              where r.short_id = '{short}' and a.round = 1"
        ))
        .await,
        "changes:shorter lead:board"
    );

    // Approve what Board showed: the run finishes.
    b.answer_gate(&gate2, "approve");
    run_is(&f, &short, "succeeded").await;

    // Cancel another run at its gate (asks twice).
    b.start_run("frontpage", None);
    until(&mut b, "a second gate", |b| {
        open_gate(b).is_some_and(|g| g["short_id"] != short.as_str())
    })
    .await;
    let second = open_gate(&b).unwrap()["short_id"]
        .as_str()
        .unwrap()
        .to_owned();
    b.run_control("run.cancel", &second, false);
    assert_eq!(
        run_status(&f, &second).await,
        "waiting_approval",
        "the first click only asks"
    );
    b.confirmed();
    run_is(&f, &second, "cancelled").await;

    // The same actions from a terminal-style client are refused as `gated` and change nothing.
    let before = f.scalar_i64("select count(*) from runs").await;
    let mut terminal = board(
        &svc,
        Peer {
            argv1_is_board: false,
            ..Peer::board()
        },
    );
    until(&mut terminal, "the terminal online with presets", |b| {
        b.can_act() && b.preset("frontpage").is_some()
    })
    .await;
    terminal.forms.preset = "frontpage".into();
    terminal.start_run("frontpage", None);
    until(&mut terminal, "a refusal", |b| b.feedback.is_some()).await;
    let fb = terminal.feedback.clone().unwrap();
    assert!(
        !fb.ok && fb.text.contains("only be taken in Board"),
        "{fb:?}"
    );
    assert_eq!(f.scalar_i64("select count(*) from runs").await, before);
    assert_eq!(
        f.scalar_i64("select count(*) from hotline_refusals where refusal = 'not_board'")
            .await,
        1
    );
    ticks.abort();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn an_update_waits_for_the_safe_point_then_swaps_the_binary_and_asks_for_a_restart() {
    use news_service::update::{
        UpdateGate, finish, hash_path, in_flight, record_staged_hash, staged_path,
    };
    let f = Fixture::migrated().await;
    // A live run with an AI call in flight.
    let chief = Chief::builder(f.pool.clone(), Arc::new(SystemClock))
        .handlers(Handlers::default().with("rollcall.health", done()))
        .build();
    let run = chief
        .start(news_chief::StartReq::board(
            "rollcall",
            serde_json::Map::new(),
            news_chief::IfBusy::Offer,
        ))
        .await
        .unwrap()
        .run()
        .clone();
    let job: i64 = sqlx::query_scalar(
        "insert into jobs (kind, pool, run_id, state, lease_until, lease_id, locked_by) \
         values ('ai.fixture', 'ai', $1, 'running', now() + interval '1 hour', $2, 'test') \
         returning job_id",
    )
    .bind(run.run_id)
    .bind(Uuid::new_v4())
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let waiting = in_flight(&f.pool).await.unwrap();
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0]["kind"], "ai_call");
    assert_eq!(waiting[0]["short_id"], run.short_id.as_str());

    let dir = std::env::temp_dir().join(format!("news-update-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("news.exe");
    std::fs::write(&exe, "old build").unwrap();
    std::fs::write(staged_path(&exe), "new build").unwrap();
    // `news service stage` records the hash; `finish` checks it at the safe point.
    record_staged_hash(&staged_path(&exe)).unwrap();
    let gate = Arc::new(UpdateGate::default());
    assert!(gate.begin());
    assert!(!gate.allows_claim("ai"), "no new AI claim while updating");
    let (g2, db, e2, s2) = (gate.clone(), f.pool.clone(), exe.clone(), staged_path(&exe));
    let finishing =
        tokio::spawn(async move { finish(&db, &g2, &e2, &s2, Duration::from_millis(20)).await });
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!gate.restart_requested(), "an AI call is still in flight");
    assert_eq!(std::fs::read_to_string(&exe).unwrap(), "old build");

    // The call finishes: the safe point arrives, the binary is swapped and the host is told to restart.
    sqlx::query(
        "update jobs set state = 'completed', lease_until = null, lease_id = null, finished_at = now() \
         where job_id = $1",
    )
    .bind(job)
    .execute(&f.pool)
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(5), finishing)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(gate.restart_requested());
    assert_eq!(std::fs::read_to_string(&exe).unwrap(), "new build");
    assert!(
        !hash_path(&staged_path(&exe)).exists(),
        "the hash is used once"
    );

    // Another staged build that changed after it was staged: refused at the safe point, nothing
    // swapped, and the gate reopens claims.
    std::fs::write(staged_path(&exe), "second build").unwrap();
    record_staged_hash(&staged_path(&exe)).unwrap();
    std::fs::write(staged_path(&exe), "tampered build").unwrap();
    let gate = Arc::new(UpdateGate::default());
    assert!(gate.begin());
    let e = finish(
        &f.pool,
        &gate,
        &exe,
        &staged_path(&exe),
        Duration::from_millis(20),
    )
    .await
    .unwrap_err();
    assert!(e.message.contains("does not match"), "{}", e.message);
    assert!(!gate.updating() && !gate.restart_requested());
    assert_eq!(std::fs::read_to_string(&exe).unwrap(), "new build");
    assert_eq!(
        std::fs::read_to_string(dir.join("news.exe.old")).unwrap(),
        "old build"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
