//! Huddle against a real database (#19): labels, the fit and the clamp through the CLI, the G-tier gate on
//! `adopt`, adoption in process (what Chief's `huddle.apply` step calls), the questions chosen by code,
//! and the Debrief's golden text from a fixture database.

use super::fixture::Fixture;
use chrono::{DateTime, TimeDelta, Utc};
use news_nose::huddle;
use serde_json::{Value, json};
use std::collections::HashSet;

const DEBRIEF_GOLDEN: &str = include_str!("../../../news-critic/tests/golden/debrief-fixture.txt");

fn components(i: i64) -> Value {
    let x = |m: i64| f64::from(((i * m) % 100) as i32) / 100.0;
    json!({"I": x(37), "E": 0.5, "P": 0.5, "T": 0.5, "M": x(53), "F": x(71), "D": x(29), "N": x(17), "G": x(13), "Q": 1.0})
}

/// `n` stories with a score each, an hour before `as_of`, and a running run `r_hud1`.
async fn seed(f: &Fixture, n: i64, as_of: DateTime<Utc>) {
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
