//! `news plan --run` and `news scores explain` against a real database: the Bureau §4.6 worked example
//! (news-nose's fixture) prints the golden plan, stores score rows with version, components and Pitch,
//! logs them to ClickHouse once, leaves capped stories queued, and a re-run changes nothing.

use super::fixture::{Fixture, call};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const FIXTURE: &str = include_str!("../../../news-nose/tests/fixtures/worked-example.json");
const GOLDEN: &str = include_str!("../../../news-nose/tests/golden/plan-worked-example.json");
const RUN_ID: &str = "00000000-0000-4000-8000-000000000046";

/// The six open stories (ids 1–6), and 100 pages published in the week before so the coverage
/// shares are the fixture's.
async fn seed(f: &Fixture) -> chrono::DateTime<chrono::Utc> {
    let fx: Value = serde_json::from_str(FIXTURE).unwrap();
    let as_of: chrono::DateTime<chrono::Utc> = fx["as_of"].as_str().unwrap().parse().unwrap();
    for s in fx["stories"].as_array().unwrap() {
        let id: i64 = sqlx::query_scalar(
            "insert into stories (title, first_seen, last_seen, score_input)
             values ($1, $2, $2, $3) returning story_id",
        )
        .bind(s["name"].as_str().unwrap())
        .bind(as_of - chrono::TimeDelta::hours(2))
        .bind(&s["input"])
        .fetch_one(&f.pool)
        .await
        .unwrap();
        assert_eq!(id, s["id"].as_i64().unwrap());
    }
    // One list of 100 buckets per dimension, from the shares; page k takes the k-th of each.
    let spread = |dim: &str| -> Vec<String> {
        let mut out = Vec::new();
        for (k, share) in fx["coverage"][dim].as_object().unwrap() {
            let n = (share.as_f64().unwrap() * 100.0).round() as usize;
            out.extend(std::iter::repeat_n(k.clone(), n));
        }
        assert_eq!(out.len(), 100, "{dim}");
        out
    };
    let (cats, regions, types) = (spread("category"), spread("region"), spread("page_type"));
    for k in 0..100 {
        let input =
            json!({"categories": [cats[k]], "regions": [regions[k]], "page": {"type": types[k]}});
        let story_id: i64 = sqlx::query_scalar(
            "insert into stories (title, first_seen, last_seen, status, score_input)
             values ('covered', $1, $1, 'covered', $2) returning story_id",
        )
        .bind(as_of - chrono::TimeDelta::days(2))
        .bind(input)
        .fetch_one(&f.pool)
        .await
        .unwrap();
        sqlx::query(
            "insert into pages (page_id, slug, story_id, page_type, version, status, published_at)
             values ($1, $1, $2, $3, 1, 'published', $4)",
        )
        .bind(format!("2026/11/covered-{k}"))
        .bind(story_id)
        .bind(&types[k])
        .bind(as_of - chrono::TimeDelta::days(1))
        .execute(&f.pool)
        .await
        .unwrap();
    }
    // A page from before the window does not count.
    sqlx::query(
        "insert into pages (page_id, slug, page_type, version, status, published_at)
         values ('2026/10/old', '2026/10/old', 'macro', 1, 'published', $1)",
    )
    .bind(as_of - chrono::TimeDelta::days(8))
    .execute(&f.pool)
    .await
    .unwrap();
    as_of
}

/// A running Edition with a target of 4, started 25 minutes after the hour of `as_of`.
async fn run(f: &Fixture, as_of: chrono::DateTime<chrono::Utc>) {
    sqlx::query(
        "insert into runs (run_id, short_id, preset, args, heavy, origin, settings_version, status, config,
                           budget, started_at, last_seen_at)
         values ($1, 'r_plan1', 'edition', '{\"target\": 4}', false, 'board',
                 (select max(version) from standing), 'running', '{}', '{}', $2, $2)",
    )
    .bind(uuid::Uuid::parse_str(RUN_ID).unwrap())
    .bind(as_of + chrono::TimeDelta::minutes(25))
    .execute(&f.pool)
    .await
    .unwrap();
}

/// A stand-in for ClickHouse that answers 200 to every request until stopped, and returns them.
struct FakeClickHouse {
    url: String,
    stop: Arc<AtomicBool>,
    handle: std::thread::JoinHandle<Vec<String>>,
}

impl FakeClickHouse {
    fn start() -> Self {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let handle = std::thread::spawn(move || {
            let mut requests = Vec::new();
            while !flag.load(Ordering::SeqCst) {
                let Ok((mut s, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                };
                s.set_nonblocking(false).unwrap();
                s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut buf = Vec::new();
                let mut chunk = [0u8; 8192];
                loop {
                    let n = s.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    if let Some(end) = text.find("\r\n\r\n") {
                        let len = text[..end]
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        if buf.len() >= end + 4 + len {
                            break;
                        }
                    }
                }
                let _ = s.write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                );
                requests.push(String::from_utf8_lossy(&buf).into_owned());
            }
            requests
        });
        Self { url, stop, handle }
    }

    /// The bodies of the `news.scores` inserts received.
    fn scores_bodies(self) -> Vec<String> {
        self.stop.store(true, Ordering::SeqCst);
        self.handle
            .join()
            .unwrap()
            .into_iter()
            .filter(|r| r.starts_with("POST /?query=INSERT+INTO+news.scores"))
            .map(|r| r.split("\r\n\r\n").nth(1).unwrap_or("").to_owned())
            .collect()
    }
}

fn plan(f: &Fixture, ch: &FakeClickHouse) -> (Value, i32, String) {
    let mut cmd = f.news();
    cmd.env("NEWS_CH_HTTP", &ch.url);
    let (env, code, _, stderr) = call(&mut cmd, &["plan", "--run", RUN_ID]);
    (env, code, stderr)
}

async fn plan_rows(f: &Fixture) -> String {
    f.scalar_str(
        "select coalesce(string_agg(format('%s|%s|%s|%s|%s|%s|%s', rank, story_id, tier, bucket, adjusted, action, reason),
                                    E'\n' order by rank), '')
           from run_plans",
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn plan_prints_the_golden_budget_and_a_rerun_changes_nothing() {
    let f = Fixture::migrated().await;
    let as_of = seed(&f).await;
    run(&f, as_of).await;

    let ch = FakeClickHouse::start();
    let (env, code, stderr) = plan(&f, &ch);
    assert_eq!(code, 0, "{env} {stderr}");
    let golden: Value = serde_json::from_str(GOLDEN).unwrap();
    assert_eq!(env["data"], golden, "news plan --run output");
    assert_eq!(env["warnings"].as_array().map_or(0, Vec::len), 0, "{env}");

    // Every score row: the run's start hour, scorer_version, ten components and an 8-line Pitch.
    assert_eq!(f.scalar_i64("select count(*) from scores").await, 6);
    assert_eq!(
        f.scalar_i64(&format!(
            "select count(*) from scores where scorer_version = 'v1' and computed_at = '{}'
               and (select count(*) from jsonb_object_keys(components)) = 10
               and array_length(string_to_array(explain, E'\\n'), 1) = 8",
            as_of.to_rfc3339()
        ))
        .await,
        6
    );
    // The Budget: CPI, IMF WEO, Apple, the preprint. RBI (3) and US PPI (6) are blocked by the Economy
    // cap: no plan row, and their stories stay open (queued) to be re-scored next run.
    let rows = plan_rows(&f).await;
    let ids: Vec<&str> = rows.lines().map(|l| l.split('|').nth(1).unwrap()).collect();
    assert_eq!(ids, ["1", "2", "4", "5"], "{rows}");
    assert_eq!(
        f.scalar_i64("select count(*) from stories where story_id in (3, 6) and status = 'open'")
            .await,
        2
    );

    // ClickHouse got one row per score, selected or not.
    let bodies = ch.scores_bodies();
    assert_eq!(bodies.len(), 1, "{bodies:?}");
    let ch_rows: Vec<Value> = bodies[0]
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(ch_rows.len(), 6);
    for r in &ch_rows {
        assert_eq!(r["run_id"], RUN_ID);
        assert_eq!(r["scorer_version"], "v1");
        let comp: Value = serde_json::from_str(r["components"].as_str().unwrap()).unwrap();
        assert_eq!(comp.as_object().unwrap().len(), 10);
    }
    let selected: Vec<(u64, u64)> = ch_rows
        .iter()
        .filter(|r| r["selected"] == 1)
        .map(|r| (r["story_id"].as_u64().unwrap(), r["tier"].as_u64().unwrap()))
        .collect();
    assert_eq!(selected, [(1, 1), (2, 2), (4, 2), (5, 2)]);

    // Re-run: same output, same rows, nothing new in ClickHouse.
    let ch = FakeClickHouse::start();
    let (again, code, stderr) = plan(&f, &ch);
    assert_eq!(code, 0, "{again} {stderr}");
    assert_eq!(again["data"], golden);
    assert_eq!(plan_rows(&f).await, rows);
    assert_eq!(f.scalar_i64("select count(*) from scores").await, 6);
    assert!(ch.scores_bodies().is_empty());

    // The Pitch is readable from the CLI.
    let (env, code, _, _) = f.call(&["scores", "explain", "1"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["total"], 75.17);
    assert_eq!(env["data"]["scorer_version"], "v1");
    assert!(
        env["data"]["explain"]
            .as_str()
            .unwrap()
            .starts_with("S 75.17 = 100 × Q 1.000"),
        "{env}"
    );
    let (env, code, _, _) = f.call(&["scores", "explain", "999"]);
    assert_eq!(
        (code, env["error"]["code"].as_str()),
        (3, Some("not_found"))
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn plan_needs_a_known_run_and_something_to_score() {
    let f = Fixture::migrated().await;
    let (env, _, _, _) = f.call(&["plan", "--run", RUN_ID]);
    assert_eq!(env["error"]["code"], "not_found", "{env}");
    run(&f, "2026-11-13T18:00:00Z".parse().unwrap()).await;
    let (env, code, _, _) = f.call(&["plan", "--run", RUN_ID]);
    assert_eq!(
        (code, env["error"]["code"].as_str()),
        (4, Some("precondition")),
        "{env}"
    );
}
