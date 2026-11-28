//! Huddle against a real database (#19): labels, the fit and the clamp through the CLI, the G-tier gate on
//! `adopt`, adoption in process (what Chief's `huddle.apply` step calls), the questions chosen by code,
//! the Debrief's golden text from a fixture database, and the preset end to end through Chief with a
//! test Board client answering its form over the Hotline.

use super::fixture::Fixture;
use chrono::{DateTime, TimeDelta, Utc};
use news_chief::Chief;
use news_chief::driver::Handlers;
use news_core::ErrorCode;
use news_hotline::{Bus, Peer, Service};
use news_nose::huddle;
use news_service::handlers::Bureau;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::Arc;

const DEBRIEF_GOLDEN: &str = include_str!("../../../news-critic/tests/golden/debrief-fixture.txt");

fn components(i: i64) -> Value {
    let x = |m: i64| f64::from(((i * m) % 100) as i32) / 100.0;
    json!({"I": x(37), "E": 0.5, "P": 0.5, "T": 0.5, "M": x(53), "F": x(71), "D": x(29), "N": x(17), "G": x(13), "Q": 1.0})
}

/// `n` stories with a score each, an hour before `as_of`, and a running run `r_hud1`.
async fn seed(f: &Fixture, n: i64, as_of: DateTime<Utc>) {
    seed_stories(f, n, as_of).await;
    sqlx::query(
        "insert into runs (run_id, short_id, preset, args, heavy, origin, settings_version, status, config,
                           budget, started_at, last_seen_at)
         values ('00000000-0000-4000-8000-000000000019', 'r_hud1', 'huddle', '{}', false, 'board',
                 (select max(version) from standing), 'running', '{}', '{}', $1, $1)",
    )
    .bind(as_of)
    .execute(&f.pool)
    .await
    .unwrap();
}

/// `n` stories with a score each, an hour before `as_of`.
async fn seed_stories(f: &Fixture, n: i64, as_of: DateTime<Utc>) {
    for i in 1..=n {
        let id: i64 = sqlx::query_scalar(
            "insert into stories (title, first_seen, last_seen) values ($1, $2, $2) returning story_id",
        )
        .bind(format!("Story {i}"))
        .bind(as_of - TimeDelta::days(1))
        .fetch_one(&f.pool)
        .await
        .unwrap();
        assert_eq!(id, i);
        sqlx::query(
            "insert into scores (story_id, computed_at, scorer_version, components, total, explain)
             values ($1, $2, 'v1', $3, $4::numeric, 'pitch')",
        )
        .bind(id)
        .bind(as_of - TimeDelta::hours(1))
        .bind(components(i))
        .bind(10.0 + (i % 50) as f64)
        .execute(&f.pool)
        .await
        .unwrap();
    }
}

#[tokio::test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn labels_fit_clamp_and_adopt_end_to_end() {
    let f = Fixture::migrated().await;
    let now = Utc::now();
    seed(&f, 40, now).await;

    // Twenty answers, every one preferring the story with the higher I: the fit wants more I.
    let labels: Vec<Value> = (0..20)
        .map(|k| {
            let (a, b) = (2 * k + 1, 2 * k + 2);
            let winner = if components(a)["I"].as_f64() > components(b)["I"].as_f64() {
                "a"
            } else {
                "b"
            };
            json!({"a": a, "b": b, "winner": winner})
        })
        .collect();
    let file = std::env::temp_dir().join(format!("huddle-labels-{}.json", std::process::id()));
    std::fs::write(&file, json!({"labels": labels}).to_string()).unwrap();
    let path = file.to_str().unwrap();
    let (env, code, _, err) = f.call(&["scores", "label", "--run", "r_hud1", path]);
    assert_eq!(
        (code, &env["data"]["stored"]),
        (0, &json!(20)),
        "{env} {err}"
    );
    // Answering again replaces, so the count is the same and no row is added.
    let (_, code, _, _) = f.call(&["scores", "label", "--run", "r_hud1", path]);
    assert_eq!(code, 0);
    assert_eq!(f.scalar_i64("select count(*) from score_labels").await, 20);
    let (env, code, _, _) = f.call(&["scores", "label", "--run", "r_nope", path]);
    assert_eq!(
        (code, env["error"]["code"].as_str()),
        (3, Some("not_found"))
    );
    std::fs::remove_file(&file).ok();

    // The fit: v2, I up, no weight moved by more than 0.05, the sum still 1.
    let (env, code, _, err) = f.call(&["scores", "propose", "--from-labels"]);
    assert_eq!(code, 0, "{env} {err}");
    let d = &env["data"];
    assert_eq!(
        (d["version"].as_str(), d["parent"].as_str()),
        (Some("v2"), Some("v1"))
    );
    let diff = d["diff"].as_array().unwrap();
    let (mut sum, mut i_up) = (0.0, false);
    for c in diff {
        let (from, to) = (c["from"].as_f64().unwrap(), c["to"].as_f64().unwrap());
        assert!((to - from).abs() <= 0.05 + 1e-3, "{c}");
        assert!(to >= 0.0, "{c}");
        sum += to;
        i_up |= c["weight"] == "I" && to > from;
    }
    assert!((sum - 1.0).abs() < 1e-3, "{sum}");
    assert!(i_up, "{diff:?}");
    // Nothing is live until adopted: the plan still scores with v1.
    assert_eq!(f.call(&["scores", "weights"]).0["data"]["version"], "v1");

    // adopt is G tier: refused from the CLI, even with a real proposal waiting.
    let (env, code, _, _) = f.call(&["scores", "adopt", "v2"]);
    assert_eq!(
        (code, env["error"]["code"].as_str()),
        (7, Some("gated")),
        "{env}"
    );
    assert_eq!(f.call(&["scores", "weights"]).0["data"]["version"], "v1");

    // Chief adopts in process after the approval; a second adopt is a no-op.
    let a = huddle::adopt(&f.pool, "v2").await.unwrap();
    assert!(!a.already && a.parent == "v1");
    assert!(huddle::adopt(&f.pool, "v2").await.unwrap().already);
    assert_eq!(
        huddle::adopt(&f.pool, "v9").await.unwrap_err().code,
        news_core::ErrorCode::NotFound
    );
    let w = f.call(&["scores", "weights"]).0;
    assert_eq!(w["data"]["version"], "v2");
    assert!(w["data"]["weights"]["I"].as_f64().unwrap() > 0.30);

    // Labels count towards the next version only once it is live: none yet.
    let (env, code, _, _) = f.call(&["scores", "propose", "--from-labels"]);
    assert_eq!(
        (code, env["error"]["code"].as_str()),
        (4, Some("precondition")),
        "{env}"
    );
}

#[tokio::test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn questions_skip_labelled_pairs_and_ask_about_a_story_at_most_twice() {
    let f = Fixture::migrated().await;
    let now = Utc::now();
    seed(&f, 40, now).await;
    let before = huddle::questions(&f.pool, now, 30).await.unwrap();
    assert!(!before.is_empty() && before.len() <= 30);
    let q = &before[0];
    let labels = vec![huddle::LabelIn {
        a: q.a.story_id,
        b: q.b.story_id,
        winner: "a".into(),
    }];
    assert_eq!(
        huddle::store_labels(&f.pool, "r_hud1", &labels)
            .await
            .unwrap(),
        1
    );
    let after = huddle::questions(&f.pool, now, 30).await.unwrap();
    let key = |q: &huddle::Question| {
        (
            q.a.story_id.min(q.b.story_id),
            q.a.story_id.max(q.b.story_id),
        )
    };
    let asked: HashSet<_> = after.iter().map(key).collect();
    assert!(!asked.contains(&key(q)));
    let mut seen = std::collections::HashMap::<i64, u32>::new();
    for q in &after {
        *seen.entry(q.a.story_id).or_default() += 1;
        *seen.entry(q.b.story_id).or_default() += 1;
    }
    assert!(seen.values().all(|c| *c <= 2));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn huddle_answers_go_over_the_hotline_from_board_only_and_questions_to_anyone() {
    let f = Fixture::migrated().await;
    seed(&f, 40, Utc::now()).await;
    let state = Arc::new(news_service::State::default());
    state.set(news_chief::board::ServiceState::Up);
    let bureau = Arc::new(news_service::handlers::Bureau::new(
        state,
        Arc::new(Bus::default()),
        Arc::new(news_service::log::LogRing::default()),
    ));
    bureau.set_core(Some(Arc::new(news_service::handlers::Core {
        db: f.pool.clone(),
        chief: news_chief::Chief::builder(f.pool.clone(), Arc::new(news_core::clock::SystemClock))
            .build(),
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

    // `huddle.questions` is a read: both peers get the same questions.
    let from_terminal = call(terminal.clone(), "huddle.questions", json!({})).await;
    assert_eq!(from_terminal["ok"], true, "{from_terminal}");
    let qs = from_terminal["data"]["questions"]
        .as_array()
        .unwrap()
        .clone();
    assert!(!qs.is_empty() && qs.len() <= 30);
    let from_board = call(Peer::board(), "huddle.questions", json!({})).await;
    assert_eq!(
        from_board["data"]["questions"],
        from_terminal["data"]["questions"]
    );

    // `huddle.answer` is H tier: a terminal is refused as `gated`, nothing is stored, the refusal is logged.
    let labels: Vec<Value> = qs
        .iter()
        .take(3)
        .map(|q| json!({"a": q["a"]["story_id"], "b": q["b"]["story_id"], "winner": "a"}))
        .collect();
    let answer = json!({"run": "r_hud1", "labels": labels});
    let env = call(terminal, "huddle.answer", answer.clone()).await;
    assert_eq!(env["error"]["code"], "gated", "{env}");
    assert_eq!(f.scalar_i64("select count(*) from score_labels").await, 0);
    assert_eq!(
        f.scalar_i64(
            "select count(*) from hotline_refusals where cmd = 'huddle.answer' and refusal = 'not_board'"
        )
        .await,
        1
    );

    // Board's answer is stored, one row per label.
    let env = call(Peer::board(), "huddle.answer", answer).await;
    assert_eq!(
        (&env["ok"], &env["data"]["stored"]),
        (&json!(true), &json!(3)),
        "{env}"
    );
    assert_eq!(f.scalar_i64("select count(*) from score_labels").await, 3);
}

#[tokio::test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn the_debrief_of_a_fixture_database_is_the_golden_text() {
    let f = Fixture::migrated().await;
    let as_of: DateTime<Utc> = "2026-11-23T12:00:00Z".parse().unwrap();
    let day = |d: i64, h: i64| as_of - TimeDelta::days(d) - TimeDelta::hours(h);
    for (title, input) in [
        (
            "US CPI",
            json!({"categories": ["economy"], "regions": ["north-america"], "page": {"type": "macro"}}),
        ),
        (
            "Apple results",
            json!({"categories": ["finance"], "regions": ["india"], "page": {"type": "earnings"}}),
        ),
        (
            "Quiet story",
            json!({"categories": ["tech"], "regions": ["europe"], "page": {"type": "news"}}),
        ),
    ] {
        sqlx::query("insert into stories (title, first_seen, last_seen, score_input) values ($1, $2, $2, $3)")
            .bind(title)
            .bind(day(3, 0))
            .bind(input)
            .execute(&f.pool)
            .await
            .unwrap();
    }
    // Pages: three in the window (one indexed), one eight days old that must not count.
    for (id, story, ptype, tier, index, at) in [
        (
            "2026/11/us-cpi",
            Some(1_i64),
            "macro",
            1,
            "index",
            day(3, 0),
        ),
        (
            "2026/11/apple",
            Some(2),
            "earnings",
            2,
            "noindex",
            day(2, 0),
        ),
        ("2026/11/us-ppi", Some(1), "macro", 2, "noindex", day(1, 0)),
        ("2026/11/old", Some(3), "news", 2, "index", day(8, 0)),
    ] {
        sqlx::query(
            "insert into pages (page_id, slug, story_id, page_type, tier, version, status, published_at, index_status)
             values ($1, $1, $2, $3, $4::smallint, 1, 'published', $5, $6)",
        )
        .bind(id)
        .bind(story)
        .bind(ptype)
        .bind(tier)
        .bind(at)
        .bind(index)
        .execute(&f.pool)
        .await
        .unwrap();
    }
    for class in ["parse", "parse", "upstream"] {
        sqlx::query("insert into jobs (kind, pool, state, error_class, finished_at) values ('doc.fetch', 'net', 'discarded', $1, $2)")
            .bind(class)
            .bind(day(1, 0))
            .execute(&f.pool)
            .await
            .unwrap();
    }
    sqlx::query(
        "insert into home_curations (status, slots, published_at) values ('published', '{}', $1)",
    )
    .bind(day(3, 3))
    .execute(&f.pool)
    .await
    .unwrap();
    for (m, status) in [
        ("acme corp", "open"),
        ("globex", "open"),
        ("initech", "dismissed"),
    ] {
        sqlx::query("insert into unresolved_mentions (mention_norm, mention, reason, status) values ($1, $1, 'no_match', $2)")
            .bind(m)
            .bind(status)
            .execute(&f.pool)
            .await
            .unwrap();
    }
    // Scores in the window: story 1 was planned, stories 2 and 3 were not.
    for (id, total) in [(1_i64, 75.2_f64), (2, 61.0), (3, 52.5)] {
        sqlx::query(
            "insert into scores (story_id, computed_at, scorer_version, components, total, explain)
             values ($1, $2, 'v1', '{}', $3::numeric, 'pitch')",
        )
        .bind(id)
        .bind(as_of - TimeDelta::hours(2))
        .bind(total)
        .execute(&f.pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "insert into run_plans (run_id, story_id, rank, tier, bucket, adjusted, reason, action)
         values ('00000000-0000-4000-8000-000000000019', 1, 1, 1, 'economy', 75.2, 'first', 'new')",
    )
    .execute(&f.pool)
    .await
    .unwrap();

    let out = news_critic::debrief::weekly(&f.pool, as_of).await.unwrap();
    assert_eq!(out["text"].as_str().unwrap(), DEBRIEF_GOLDEN);
    assert_eq!(out["pages"]["total"], 3);
    assert_eq!(out["failures"]["jobs_discarded"]["parse"], 2);
    assert_eq!(out["index_budget"]["indexed"], 1);
}

/// The Bureau service's Hotline side over `chief`, up and connected to the fixture database.
fn bureau(f: &Fixture, chief: Chief) -> Arc<Bureau> {
    let state = Arc::new(news_service::State::default());
    state.set(news_chief::board::ServiceState::Up);
    let bureau = Arc::new(Bureau::new(
        state,
        Arc::new(Bus::default()),
        Arc::new(news_service::log::LogRing::default()),
    ));
    bureau.set_core(Some(Arc::new(news_service::handlers::Core {
        db: f.pool.clone(),
        chief,
        policy: Arc::new(news_chief::AllowAll),
        stopper: Arc::new(news_chief::NoStopper),
    })));
    bureau
}

/// One Hotline request from `peer`; a reply that succeeded is checked against its contract.
async fn hotline(bureau: &Arc<Bureau>, peer: Peer, cmd: &str, args: Value) -> Value {
    let svc: Arc<dyn Service> = bureau.clone();
    let (mut client, server) = tokio::io::duplex(1 << 20);
    let h = tokio::spawn(news_hotline::serve_conn(server, peer, svc));
    let env = news_hotline::call(&mut client, "t-1", cmd, args)
        .await
        .unwrap();
    drop(client);
    let _ = h.await;
    if env["ok"] == true {
        news_hotline::contracts()
            .check_output(cmd, &env["data"])
            .unwrap_or_else(|e| panic!("{cmd} reply breaks its contract: {e}"));
    }
    env
}

/// Poll `f` every 50 ms for 30 s.
async fn eventually<T, Fut: std::future::Future<Output = Option<T>>>(
    what: &str,
    mut f: impl FnMut() -> Fut,
) -> T {
    for _ in 0..600 {
        if let Some(v) = f().await {
            return v;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("never happened: {what}");
}

/// Start Huddle from Board over the Hotline, answer its form as Board (checking the refusals on the
/// way) and return when the `change:v2` gate waits, with nothing adopted yet: the service side, Chief
/// and a test Board client.
async fn to_change_gate(f: &Fixture) -> (Arc<Bureau>, Chief, String, Value) {
    seed_stories(f, 40, Utc::now()).await;
    let ch = Chief::builder(f.pool.clone(), Arc::new(news_core::clock::SystemClock))
        .handlers(news_critic::huddle::register(Handlers::default()))
        .build();
    let b = bureau(f, ch.clone());
    let terminal = Peer {
        argv1_is_board: false,
        ..Peer::board()
    };

    // Board starts Huddle; the Debrief runs and the form opens with the questions chosen by code.
    let env = hotline(
        &b,
        Peer::board(),
        "run.start",
        json!({"preset": "huddle", "args": {}}),
    )
    .await;
    assert_eq!(env["ok"], true, "{env}");
    let short = env["data"]["short_id"].as_str().unwrap().to_owned();
    ch.ensure_drivers().await.unwrap();
    let form = eventually("the calibration form opens", || {
        let (b, short) = (&b, &short);
        async move {
            let env = hotline(b, Peer::board(), "run.show", json!({"run": short})).await;
            let step = env["data"]["steps"]
                .as_array()?
                .iter()
                .find(|s| s["id"] == "calibration")?
                .clone();
            (env["data"]["run"]["status"] == "waiting_approval"
                && step["summary"]["form_sha"].is_string())
            .then(|| step["summary"].clone())
        }
    })
    .await;
    assert_eq!(form["round"], 1);
    let questions = form["form"]["questions"].as_array().unwrap().clone();
    assert!(questions.len() >= 10 && questions.len() <= 30, "{form}");
    assert_eq!(
        f.scalar_str(&format!(
            "select s.status from run_steps s join runs r using (run_id) where r.short_id = '{short}' and s.step = 'debrief'"
        ))
        .await,
        "done"
    );

    // Karthik prefers the story with the higher I every time, so the fit wants more I.
    let labels: Vec<Value> = questions
        .iter()
        .map(|q| {
            let (a, b) = (
                q["a"]["story_id"].as_i64().unwrap(),
                q["b"]["story_id"].as_i64().unwrap(),
            );
            let winner = if components(a)["I"].as_f64() > components(b)["I"].as_f64() {
                "a"
            } else {
                "b"
            };
            json!({"a": a, "b": b, "winner": winner})
        })
        .collect();
    let args = json!({"run": short, "form": "huddle.calibration", "round": 1, "form_sha": form["form_sha"],
                      "answer": {"labels": labels, "search_console": "No coverage problems this week",
                                 "reading": [{"page": "2026/11/us-cpi", "note": "Reads well"}]}});

    // From a terminal the answer is refused (gated, exit 7): nothing stored, the run still waits.
    let env = hotline(&b, terminal, "form.submit", args.clone()).await;
    assert_eq!(env["error"]["details"]["refusal"], "not_board", "{env}");
    assert_eq!(
        news_hotline::error_code(&env).map(ErrorCode::exit_code),
        Some(7)
    );
    // From Board, a form that is not the one shown is gated, and another round is a precondition.
    let mut stale = args.clone();
    stale["form_sha"] = json!("0".repeat(64));
    let env = hotline(&b, Peer::board(), "form.submit", stale).await;
    assert_eq!(env["error"]["code"], "gated", "{env}");
    let mut later = args.clone();
    later["round"] = json!(2);
    let env = hotline(&b, Peer::board(), "form.submit", later).await;
    assert_eq!(env["error"]["code"], "precondition", "{env}");
    // A label for a pair the form did not ask is refused at once.
    let mut unasked = args.clone();
    unasked["answer"]["labels"][0] = json!({"a": 1, "b": 1, "winner": "a"});
    let env = hotline(&b, Peer::board(), "form.submit", unasked).await;
    assert_eq!(env["error"]["code"], "validation_failed", "{env}");
    assert_eq!(f.scalar_i64("select count(*) from score_labels").await, 0);

    // Board's answer is kept and the run goes on to the fit.
    let env = hotline(&b, Peer::board(), "form.submit", args.clone()).await;
    assert_eq!(env["data"]["run_status"], "running", "{env}");
    let env = hotline(&b, Peer::board(), "form.submit", args).await;
    assert_eq!(
        env["error"]["code"], "precondition",
        "an answered form is not answered twice: {env}"
    );
    ch.ensure_drivers().await.unwrap();

    // The proposal waits for its approval: nothing is adopted before it.
    let gate = eventually("the change gate opens", || {
        let (b, short) = (&b, &short);
        async move {
            let env = hotline(b, Peer::board(), "run.show", json!({"run": short})).await;
            env["data"]["gates"]
                .as_array()?
                .iter()
                .find(|g| g["answered_at"].is_null())
                .cloned()
        }
    })
    .await;
    assert_eq!(gate["gate"], "change:v2", "{gate}");
    assert_eq!(
        f.scalar_i64("select count(*) from score_labels").await,
        labels.len() as i64
    );
    assert_eq!(
        f.scalar_str(&format!(
            "select s.output->'answer'->>'search_console' from run_steps s join runs r using (run_id)
              where r.short_id = '{short}' and s.step = 'calibration'"
        ))
        .await,
        "No coverage problems this week"
    );
    assert_eq!(huddle::live(&f.pool).await.unwrap().version, "v1");
    assert_eq!(f.call(&["scores", "weights"]).0["data"]["version"], "v1");
    (b, ch, short, gate)
}

/// Poll the run until it has ended; its final status.
async fn ended(f: &Fixture, short: &str) -> String {
    eventually("the run ends", || async move {
        let st = f
            .scalar_str(&format!(
                "select status from runs where short_id = '{short}'"
            ))
            .await;
        matches!(st.as_str(), "succeeded" | "failed" | "cancelled").then_some(st)
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn huddle_runs_end_to_end_with_board_answering_the_form_and_adopts_only_after_approval() {
    let f = Fixture::migrated().await;
    let (b, ch, short, gate) = to_change_gate(&f).await;

    // Board approves the change; the apply step adopts it.
    let env = hotline(
        &b,
        Peer::board(),
        "gate.approve",
        json!({"run": short, "gate": "change:v2", "round": gate["round"], "subject_sha": gate["subject_sha"]}),
    )
    .await;
    assert_eq!(env["ok"], true, "{env}");
    ch.ensure_drivers().await.unwrap();
    assert_eq!(ended(&f, &short).await, "succeeded");
    let live = huddle::live(&f.pool).await.unwrap();
    assert_eq!(live.version, "v2");
    assert!(live.weights.i > 0.30, "{:?}", live.weights);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn a_proposal_changed_after_its_approval_is_not_adopted() {
    let f = Fixture::migrated().await;
    let (b, ch, short, gate) = to_change_gate(&f).await;
    // While the gate waits, the terminal cannot replace the proposal under it.
    let (env, code, _, _) = f.call(&["scores", "propose", "--from-labels"]);
    assert_eq!(env["error"]["code"], "conflict", "{env}");
    assert_ne!(code, 0);
    let env = hotline(
        &b,
        Peer::board(),
        "gate.approve",
        json!({"run": short, "gate": "change:v2", "round": gate["round"], "subject_sha": gate["subject_sha"]}),
    )
    .await;
    assert_eq!(env["ok"], true, "{env}");
    // Approved, and still not adopted: the terminal is refused again, and a row changed behind the
    // gate's back (as an older `propose` would have done) is refused by `apply`.
    let (env, _, _, _) = f.call(&["scores", "propose", "--from-labels"]);
    assert_eq!(env["error"]["code"], "conflict", "{env}");
    sqlx::query(
        "update scorer_versions set weights = jsonb_set(weights, '{I}', to_jsonb(0.5)) where version = 'v2'",
    )
    .execute(&f.pool)
    .await
    .unwrap();
    ch.ensure_drivers().await.unwrap();
    assert_eq!(ended(&f, &short).await, "failed");
    let err = f
        .scalar_str(&format!(
            "select s.error->>'message' from run_steps s join runs r using (run_id)
              where r.short_id = '{short}' and s.step = 'apply' and s.status = 'failed'"
        ))
        .await;
    assert!(err.contains("not the ones approved"), "{err}");
    assert_eq!(huddle::live(&f.pool).await.unwrap().version, "v1");
    assert_eq!(f.call(&["scores", "weights"]).0["data"]["version"], "v1");
}
