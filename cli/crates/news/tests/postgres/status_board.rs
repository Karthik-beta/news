//! The status board of Bureau §8.1 ("Example board") from a fixture database: Thu 12 Nov 2026,
//! 08:40 in New York, settings v12, Catchup waiting at its plan gate, three schedules, a Huddle 8 days
//! ago, a Rollcall 2 days ago, seven dead jobs since Salvage r_29c0 and 0.4M Bureau tokens today.
//!
//! The golden text is what Chief's code renders for that database (`tests/golden/status_board_8_1.txt`).
//! It is not byte for byte the example in the design document, because Chief does not read everything
//! the example shows yet. What differs, and why:
//!
//! | In §8.1's example                         | In the rendered board                      | Why |
//! | ----------------------------------------- | ------------------------------------------ | --- |
//! | `Catchup r_51d2 … (news run status …)`     | `catchup r_51d2 gate plan since …`          | the text names the preset by id and omits the hint line |
//! | `Edition today 11:00 · Frontpage Fri 09:00` | `edition Thu 12 Nov 11:00 · frontpage Fri 13 Nov 09:00` | schedules print preset id and local date |
//! | `Diary today …` (3 events), `Stories waiting`, `Frontpage` age | absent; `due.diary` is `[]`, `due.stories_waiting` and `due.frontpage` are `null` | `board::status` queries none of `calendar_events`, `stories`, `scores` or `home_curations` |
//! | `Sources ons red … bea amber`              | absent; `problems.sources` is `[]`          | it queries no source health either |
//! | `Huddle … 8 days ago` / `Rollcall … Sitrep` | `last r_3e11, 8.0 days ago`                 | the age prints as a decimal, and Rollcall names the last run, not a Sitrep |
//! | `Usage … 0.4M tokens · cap and reserve not set` | `Bureau today 400000 tokens`            | the usage line prints tokens as is |
//! | `NEXT Start Edition` (rule 2, `tier1_waiting`) | `r_51d2 waits for your answer at gate plan` (rule 3) | rules 2, 4, 5, 6 and 8 are `fire: \|_\| None` in `board::RULES`, so the first rule that can match is the open gate |
//!
//! The tables those rules would read exist now (`calendar_events`, `stories`, `scores`, `sources`,
//! `home_curations`); what is missing is the reader in `board.rs`, not fixture rows, so no rows are built
//! for them here. When the reader lands, this test gains those rows and the golden file the missing lines.

use super::fixture::Fixture;
use chrono::{DateTime, Utc};
use news_chief::StartPolicy;
use news_chief::board::ServiceState;
use news_chief::gates;
use news_chief::presets;
use serde_json::{Value, json};
use uuid::Uuid;

fn t(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}

/// A run row as the service would have left it, with its `run_steps` when it is still live.
async fn insert_run(
    f: &Fixture,
    short_id: &str,
    preset: &str,
    status: &str,
    started: DateTime<Utc>,
    finished: Option<DateTime<Utc>>,
) -> Uuid {
    let p = presets::preset(preset).unwrap();
    let id = Uuid::new_v4();
    let summary = finished.map(|_| json!({"text": format!("{} {short_id} {status}", p.name)}));
    sqlx::query(
        "insert into runs (run_id, short_id, preset, args, heavy, origin, settings_version, status, current_step,
                           config, budget, created_at, started_at, last_seen_at, finished_at, summary)
         values ($1, $2, $3, '{}', $4, 'board', 1, $5, $6, '{}', '{}', $7, $7, $8, $9, $10)",
    )
    .bind(id)
    .bind(short_id)
    .bind(preset)
    .bind(p.heavy)
    .bind(status)
    .bind(if finished.is_none() { Some("review") } else { None })
    .bind(started)
    .bind(finished.unwrap_or(started))
    .bind(finished)
    .bind(summary)
    .execute(&f.pool)
    .await
    .unwrap();
    id
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

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn the_status_board_of_the_bureau_8_1_example_day_is_rendered_from_a_fixture_database() {
    let f = Fixture::migrated().await;
    let now = t("2026-11-12T13:40:00Z");

    // Settings v12: New York, the three example schedules, then nine small edits to reach the version.
    let mut version = standing_set(
        &f,
        1,
        &[
            ("rota.time_zone", json!("America/New_York")),
            (
                "schedules.edition",
                json!({"preset": "edition", "args": {}, "cron": "0 11 * * *", "enabled": true}),
            ),
            (
                "schedules.frontpage",
                json!({"preset": "frontpage", "args": {}, "cron": "0 9 * * 5", "enabled": true}),
            ),
            (
                "schedules.rollcall",
                json!({"preset": "rollcall", "args": {}, "cron": "0 7 * * 1", "enabled": true}),
            ),
        ],
    )
    .await;
    while version < 12 {
        let size = 3 + ((version + 1) % 2);
        version = standing_set(&f, version, &[("pools.ai.max_running", json!(size))]).await;
    }
    assert_eq!(version, 12);

    // Finished runs: the last Edition, Huddle, Rollcall and Salvage.
    for (short, preset, finished) in [
        ("r_6b90", "edition", "2026-11-11T23:10:00Z"),
        ("r_3e11", "huddle", "2026-11-04T13:40:00Z"),
        ("r_44aa", "rollcall", "2026-11-10T13:40:00Z"),
        ("r_29c0", "salvage", "2026-11-09T16:00:00Z"),
    ] {
        let end = t(finished);
        insert_run(
            &f,
            short,
            preset,
            "succeeded",
            end - chrono::Duration::hours(1),
            Some(end),
        )
        .await;
    }

    // Catchup r_51d2 waits at its plan gate since Wed 21:05 New York time.
    let opened = t("2026-11-12T02:05:00Z");
    let catchup = insert_run(&f, "r_51d2", "catchup", "waiting_approval", opened, None).await;
    let p = presets::preset("catchup").unwrap();
    let (gate_seq, _) = p.step("review").unwrap();
    for (i, s) in p.steps.iter().enumerate() {
        let seq = i as i16 + 1;
        let step_status = match seq.cmp(&gate_seq) {
            std::cmp::Ordering::Less => "done",
            std::cmp::Ordering::Equal => "running",
            std::cmp::Ordering::Greater => "pending",
        };
        sqlx::query(
            "insert into run_steps (run_id, seq, step, kind, status) values ($1, $2, $3, $4, $5)",
        )
        .bind(catchup)
        .bind(seq)
        .bind(s.id)
        .bind(s.kind.as_str())
        .bind(step_status)
        .execute(&f.pool)
        .await
        .unwrap();
    }
    let summary = json!({"text": "340 items in 7 chunks, estimated 4.1M input tokens, about 6 h"});
    let plan = json!({"items": 340, "chunks": 7});
    sqlx::query(
        "insert into gates (run_id, gate, round, step_seq, subject_sha, summary, opened_at)
         values ($1, 'plan', 1, $2, $3, $4, $5)",
    )
    .bind(catchup)
    .bind(gate_seq)
    .bind(gates::subject_sha(&plan).to_vec())
    .bind(&summary)
    .bind(opened)
    .execute(&f.pool)
    .await
    .unwrap();

    // Dead jobs: seven since Salvage r_29c0 (transient 4, ai_invalid_output 3) and two before it.
    for (class, n, finished) in [
        ("transient", 4, "2026-11-10T09:00:00Z"),
        ("ai_invalid_output", 3, "2026-11-11T09:00:00Z"),
        ("bug", 2, "2026-11-08T09:00:00Z"),
    ] {
        for _ in 0..n {
            sqlx::query(
                "insert into jobs (kind, pool, state, attempt, max_attempts, error_class, finished_at)
                 values ('doc.fetch', 'net', 'discarded', 6, 6, $1, $2)",
            )
            .bind(class)
            .bind(t(finished))
            .execute(&f.pool)
            .await
            .unwrap();
        }
    }

    // Bureau usage today: 0.4M tokens, no cap or reserve set.
    let policy = news_rota::RotaPolicy::default();
    policy.set_usage(
        Some(news_rota::Usage {
            bureau_today: 400_000,
            ..news_rota::Usage::default()
        }),
        now,
    );
    let board = news_chief::board::status(
        &f.pool,
        ServiceState::Up,
        Some(&policy as &dyn StartPolicy),
        None,
        now,
    )
    .await
    .unwrap();
    news_hotline::contracts()
        .check_output("status", &board)
        .unwrap();

    // The text, byte for byte.
    let golden = include_str!("../golden/status_board_8_1.txt").replace("\r\n", "\n");
    assert_eq!(
        board["text"].as_str().unwrap(),
        golden.trim_end_matches('\n'),
        "the board's text changed"
    );

    // The JSON behind it.
    assert_eq!(board["as_of"], json!(now));
    assert_eq!(board["time_zone"], "America/New_York");
    assert_eq!(
        board["service"],
        json!({"state": "up", "settings_version": 12})
    );
    assert_eq!(board["heavy_slot"], Value::Null);
    assert_eq!(board["running"], json!([]));
    assert_eq!(board["queued"], json!([]));
    let waiting = board["waiting"].as_array().unwrap();
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0]["kind"], "gate");
    assert_eq!(waiting[0]["short_id"], "r_51d2");
    assert_eq!(waiting[0]["since"], json!(opened));
    assert_eq!(waiting[0]["summary"], summary);
    assert_eq!(waiting[0]["gate"]["gate"], "plan");
    assert_eq!(
        waiting[0]["gate"]["subject_sha"],
        gates::hex(&gates::subject_sha(&plan))
    );
    assert_eq!(
        waiting[0]["gate"]["actions"],
        json!(["gate.approve", "gate.changes", "gate.reject"])
    );
    let due = &board["due"];
    let schedules: Vec<(&str, DateTime<Utc>)> = due["schedules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["preset"].as_str().unwrap(),
                s["next_at"].as_str().unwrap().parse().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        schedules,
        [
            ("edition", t("2026-11-12T16:00:00Z")),
            ("frontpage", t("2026-11-13T14:00:00Z")),
            ("rollcall", t("2026-11-16T12:00:00Z")),
        ],
        "11:00 and 09:00 and 07:00 New York time, in the order they fall due"
    );
    assert_eq!(
        due["huddle"],
        json!({"last_run": "r_3e11", "age_days": 8.0, "every_days": 7, "state": "due"})
    );
    assert_eq!(
        due["rollcall"],
        json!({"last_run": "r_44aa", "age_days": 2.0, "every_days": 7, "state": "not_due"})
    );
    // What Chief does not read yet (see the table above): absent, not wrong.
    assert_eq!(due["diary"], json!([]));
    assert_eq!(due["stories_waiting"], Value::Null);
    assert_eq!(due["frontpage"], Value::Null);
    assert_eq!(board["problems"]["sources"], json!([]));
    assert_eq!(
        board["problems"]["dead_jobs"],
        json!({"total": 7, "by_class": {"ai_invalid_output": 3, "transient": 4},
               "since_salvage": "r_29c0"})
    );
    assert_eq!(board["problems"]["failed_runs"], json!([]));
    assert_eq!(board["problems"]["refused_control"], json!([]));
    assert_eq!(board["problems"]["usage"]["bureau_today"], 400_000);
    assert_eq!(board["problems"]["usage"]["cap_today"], Value::Null);
    assert_eq!(board["next"]["rule"], "gate_open");
    assert_eq!(board["next"]["action"], "open.run");
    assert_eq!(board["next"]["short_id"], "r_51d2");
    assert_eq!(
        board["next"]["reason"],
        "r_51d2 waits for your answer at gate plan"
    );

    // The terminal's `news run status` in-process, on the same database, gives the same sections.
    let (env, code, _, _) = f.call(&["run", "status"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["waiting"][0]["short_id"], "r_51d2");
    assert_eq!(env["data"]["problems"]["dead_jobs"]["total"], 7);
}
