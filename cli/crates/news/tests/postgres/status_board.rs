//! The status board of Bureau §8.1 ("Example board") from a fixture database: Thu 12 Nov 2026,
//! 08:40 in New York, settings v12, Catchup waiting at its plan gate, three schedules, a Huddle 8 days
//! ago, a Rollcall 2 days ago, seven dead jobs since Salvage r_29c0 and 0.4M Bureau tokens today.
//!
//! The golden text (`tests/golden/status_board_8_1.txt`) is the design document's example board, byte
//! for byte, rendered by Chief's code from this fixture database. The fixture also holds rows the board
//! must leave out (a story with a page, one below the Tier 2 floor, a cancelled event, a green source),
//! so the readers' filters are tested too.

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

    // Sources: the three the Diary names are green; ons is red and bea amber, with why, since and the Sitrep.
    for (id, health, reason, since, sitrep) in [
        ("ecb", "green", None, None, None),
        ("bls", "green", None, None, None),
        ("fed", "green", None, None, None),
        (
            "ons",
            "red",
            Some("upstream_changed"),
            Some("2026-11-10"),
            Some("r_44aa"),
        ),
        ("bea", "amber", Some("free key not set"), None, None),
    ] {
        sqlx::query(
            "insert into sources (source_id, name, publisher, connector, base_url, source_tier, licence,
                                  attribution, redistribution, image_policy, rate_per_sec, regions, categories,
                                  health, health_reason, health_since, health_sitrep)
             values ($1, $1, 'x', $1, 'https://x.test/', 1, 'public-domain', 'x', true, 'none', 1.0, '{us}',
                     '{economy}', $2, $3, $4::text::date, $5)",
        )
        .bind(id)
        .bind(health)
        .bind(reason)
        .bind(since)
        .bind(sitrep)
        .execute(&f.pool)
        .await
        .unwrap();
    }

    // The Diary: three events today (New York), a cancelled one and one tomorrow that must not show.
    sqlx::raw_sql(
        "insert into calendar_events (source_id, event_key, title, expected_at, expected_day, precision, origin, status)
         values
          ('ecb', 'macro:ecb.account:2026-10', 'ECB account of the October meeting', '2026-11-12T12:30:00Z', '2026-11-12', 'exact', 'ics', 'captured'),
          ('bls', 'macro:us.cpi:2026-10', 'US CPI, October 2026', '2026-11-12T13:30:00Z', '2026-11-12', 'exact', 'ics', 'captured'),
          ('fed', 'macro:fed.h41:2026-11-12', 'Fed balance sheet (H.4.1)', '2026-11-12T21:30:00Z', '2026-11-12', 'exact', 'ics', 'expected'),
          ('ecb', 'macro:ecb.cancelled', 'A cancelled release', '2026-11-12T15:00:00Z', '2026-11-12', 'exact', 'ics', 'cancelled'),
          ('fed', 'macro:fed.h41:2026-11-13', 'Fed balance sheet tomorrow', '2026-11-13T21:30:00Z', '2026-11-13', 'exact', 'ics', 'expected');",
    )
    .execute(&f.pool)
    .await
    .unwrap();

    // Stories and their latest scores: 23 wait for a page (3 Tier 1: the CPI story at 75.2, one at 71.0 and one
    // at 62.0 whose main entity is a flagship), and six that do not count.
    sqlx::raw_sql(
        "insert into stories (story_id, event_key, title, first_seen, last_seen, status, page_id, score_input)
         values
          (81229, 'macro:ecb.account:2026-10', 'ECB account', '2026-11-12T12:30:00Z', '2026-11-12T12:30:00Z', 'open', null, null),
          (81234, 'macro:us.cpi:2026-10', 'US CPI', '2026-11-12T13:30:00Z', '2026-11-12T13:30:00Z', 'open', null, null),
          (91001, 'macro:t1.a', 'Tier 1 by S', '2026-11-12T10:00:00Z', '2026-11-12T10:00:00Z', 'open', null, null),
          (91002, 'macro:t1.b', 'Tier 1 by prominence', '2026-11-12T10:00:00Z', '2026-11-12T10:00:00Z', 'open', null,
           '{\"main_entity\": {\"id\": \"e1\", \"prominence\": 1}}'),
          (91003, null, 'Below the floor', '2026-11-12T10:00:00Z', '2026-11-12T10:00:00Z', 'open', null, null),
          (91004, null, 'Already on a page', '2026-11-12T10:00:00Z', '2026-11-12T10:00:00Z', 'covered', 'p1', null),
          (91005, null, 'Fell below the floor', '2026-11-12T10:00:00Z', '2026-11-12T10:00:00Z', 'open', null, null),
          (91006, null, 'Closed', '2026-11-12T10:00:00Z', '2026-11-12T10:00:00Z', 'closed', null, null);
         insert into stories (story_id, title, first_seen, last_seen)
           select 90000 + n, 'Tier 2 story ' || n, '2026-11-12T10:00:00Z', '2026-11-12T10:00:00Z' from generate_series(1, 19) n;
         insert into scores (story_id, computed_at, scorer_version, components, total, explain)
         values
          (81229, '2026-11-12T13:00:00Z', 'v1', '{}', 58.1, 'x'),
          (81234, '2026-11-12T13:00:00Z', 'v1', '{}', 75.2, 'x'),
          (91001, '2026-11-12T13:00:00Z', 'v1', '{}', 71.0, 'x'),
          (91002, '2026-11-12T13:00:00Z', 'v1', '{}', 62.0, 'x'),
          (91003, '2026-11-12T13:00:00Z', 'v1', '{}', 35.0, 'x'),
          (91004, '2026-11-12T13:00:00Z', 'v1', '{}', 80.0, 'x'),
          (91005, '2026-11-12T10:00:00Z', 'v1', '{}', 80.0, 'x'),
          (91005, '2026-11-12T13:00:00Z', 'v1', '{}', 30.0, 'x'),
          (91006, '2026-11-12T13:00:00Z', 'v1', '{}', 90.0, 'x');
         insert into scores (story_id, computed_at, scorer_version, components, total, explain)
           select 90000 + n, '2026-11-12T13:00:00Z', 'v1', '{}', 45.0, 'x' from generate_series(1, 19) n;",
    )
    .execute(&f.pool)
    .await
    .unwrap();

    // The home curation: v41 published 3.2 days ago (due after 2 days, stale after 4); older and draft rows ignored.
    sqlx::raw_sql(
        "insert into home_curations (status, version, slots, published_at)
         values ('published', 41, '[]', '2026-11-09T08:52:00Z'),
                ('superseded', 40, '[]', '2026-11-05T09:00:00Z');
         insert into home_curations (status, slots) values ('draft', '[]');",
    )
    .execute(&f.pool)
    .await
    .unwrap();

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
    assert_eq!(
        due["diary"],
        json!([
            {"event_key": "macro:ecb.account:2026-10", "title": "ECB account of the October meeting",
             "expected_at": t("2026-11-12T12:30:00Z"), "status": "captured", "story_id": 81229,
             "score": 58.1, "tier": 2},
            {"event_key": "macro:us.cpi:2026-10", "title": "US CPI, October 2026",
             "expected_at": t("2026-11-12T13:30:00Z"), "status": "captured", "story_id": 81234,
             "score": 75.2, "tier": 1},
            {"event_key": "macro:fed.h41:2026-11-12", "title": "Fed balance sheet (H.4.1)",
             "expected_at": t("2026-11-12T21:30:00Z"), "status": "expected", "story_id": null,
             "score": null, "tier": null},
        ])
    );
    assert_eq!(due["stories_waiting"]["total"], 23);
    assert_eq!(due["stories_waiting"]["tier1"], 3);
    assert_eq!(due["stories_waiting"]["last_edition"]["short_id"], "r_6b90");
    assert_eq!(
        due["stories_waiting"]["last_edition"]["finished_at"],
        json!(t("2026-11-11T23:10:00Z"))
    );
    assert_eq!(
        due["frontpage"],
        json!({"version": 41, "age_days": 3.2, "due_after_days": 2, "stale_after_days": 4,
               "state": "due"})
    );
    assert_eq!(
        board["problems"]["sources"],
        json!([
            {"source_id": "ons", "health": "red", "reason": "upstream_changed", "since": "2026-11-10",
             "sitrep": "r_44aa"},
            {"source_id": "bea", "health": "amber", "reason": "free key not set", "since": null,
             "sitrep": null},
        ])
    );
    assert_eq!(
        board["problems"]["dead_jobs"],
        json!({"total": 7, "by_class": {"ai_invalid_output": 3, "transient": 4},
               "since_salvage": "r_29c0"})
    );
    assert_eq!(board["problems"]["failed_runs"], json!([]));
    assert_eq!(board["problems"]["refused_control"], json!([]));
    assert_eq!(board["problems"]["usage"]["bureau_today"], 400_000);
    assert_eq!(board["problems"]["usage"]["cap_today"], Value::Null);
    assert_eq!(board["next"]["rule"], "tier1_waiting");
    assert_eq!(board["next"]["action"], "run.start");
    assert_eq!(board["next"]["preset"], "edition");
    assert_eq!(
        board["next"]["reason"],
        "3 Tier 1 stories captured and not yet on a page"
    );

    // The terminal's `news run status` in-process, on the same database, gives the same sections.
    let (env, code, _, _) = f.call(&["run", "status"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["waiting"][0]["short_id"], "r_51d2");
    assert_eq!(env["data"]["problems"]["dead_jobs"]["total"], 7);
}
