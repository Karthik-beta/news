//! `news discover|fetch|parse <source>` and the ingest job handlers on recorded fixtures (#9).
//!
//! The binary runs with `NEWS_WIRE_FIXTURES` set, so every request is answered from a routes file in
//! `tests/fixtures/wire/` (BLS: recorded; FRED and SEC: synthetic, see
//! `news-connectors/tests/fixtures/README.md`) and no test opens a socket. The envelope's `data` of each
//! command is compared with a golden file in `tests/golden/wire/`; run with `NEWS_UPDATE_GOLDEN=1` to
//! write them, then read the diff: the event keys and rule-based categories are what the test is for.

use super::fixture::{Fixture, call};
use chrono::NaiveDate;
use news::ingest::{Ingest, run_kind};
use news_connectors::CalendarEntry;
use news_core::job::{ErrorClass, JobFailure, JobInput, JobLease, LeaseCheck};
use serde_json::{Value, json};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

fn manifest() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn routes(name: &str) -> PathBuf {
    manifest().join(format!("tests/fixtures/wire/{name}.routes.json"))
}

/// The Morgue's root for one test: inside the build folder, never in the system temp directory.
fn work_dir(f: &Fixture) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("wire-{}", f.schema))
}

/// The real registry, so the sources are the ones that ship.
fn sources_dir() -> PathBuf {
    manifest().join("../../../data/sources")
}

fn golden(name: &str, got: &Value) {
    let path = manifest().join(format!("tests/golden/wire/{name}.json"));
    let text = format!("{}\n", serde_json::to_string_pretty(got).unwrap());
    if std::env::var_os("NEWS_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, text).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}; run with NEWS_UPDATE_GOLDEN=1", path.display()))
        .replace("\r\n", "\n");
    assert_eq!(text, want, "{name} differs from {}", path.display());
}

/// The binary on fixtures: `routes` file, the tracked ids of one connector.
fn news(f: &Fixture, routes_name: &str, tracked: Option<(&str, &str)>) -> assert_cmd::Command {
    let mut cmd = f.news();
    cmd.env("NEWS_SOURCES_DIR", sources_dir())
        .env("NEWS_WIRE_FIXTURES", routes(routes_name))
        .env("NEWS_WORK_DIR", work_dir(f))
        .env_remove("NEWS_CONTACT_EMAIL");
    for name in [
        "NEWS_TRACKED_BLS",
        "NEWS_TRACKED_FRED",
        "NEWS_TRACKED_SEC_EDGAR",
    ] {
        cmd.env_remove(name);
    }
    if let Some((name, value)) = tracked {
        cmd.env(name, value);
    }
    cmd
}

async fn synced(f: &Fixture) {
    let (env, code, _, _) = call(&mut news(f, "bls", None), &["sources", "sync"]);
    assert_eq!(code, 0, "{env}");
}

type CandidateRow = (
    Option<String>,
    Option<String>,
    String,
    Option<Vec<String>>,
    Option<String>,
);

/// One candidate as the database holds it: `(event_key, page_type, page_type_by, categories, region)`.
async fn candidates(f: &Fixture, source: &str) -> Vec<Value> {
    let rows: Vec<CandidateRow> = sqlx::query_as(
        "select event_key, page_type, page_type_by, categories, region
               from candidates where source_id = $1 order by event_key nulls first, url",
    )
    .bind(source)
    .fetch_all(&f.pool)
    .await
    .unwrap();
    rows.into_iter()
        .map(|(k, t, by, c, r)| json!([k, t, by, c, r]))
        .collect()
}

/// discover, fetch and parse one source, each against its golden file; then the checks that hold for
/// every source: the stored bytes are in the Morgue, every point cites a stored snapshot and carries a
/// locator and its parser, and a second parse stores nothing.
async fn walk(source: &str, routes_name: &str, tracked: (&str, &str)) -> Fixture {
    let f = Fixture::migrated().await;
    synced(&f).await;
    sqlx::query("update sources set enabled = true where source_id = $1")
        .bind(source)
        .execute(&f.pool)
        .await
        .unwrap();
    let mut data = Vec::new();
    for command in ["discover", "fetch", "parse"] {
        let (env, code, _, stderr) = call(
            &mut news(&f, routes_name, Some(tracked)),
            &[command, source],
        );
        assert_eq!(code, 0, "{command}: {env} / {stderr}");
        assert_eq!(env["warnings"], json!([]), "{command}");
        golden(&format!("{source}.{command}"), &env["data"]);
        data.push(env["data"].clone());
    }
    let (fetched, parsed) = (&data[1], &data[2]);

    // Every stored file is in the Morgue, as received (zstd), under its content address.
    for item in fetched["items"].as_array().unwrap() {
        for s in item["snapshots"].as_array().unwrap() {
            let sha = s["sha256"].as_str().unwrap();
            let file = work_dir(&f)
                .join("raw/sha256")
                .join(&sha[0..2])
                .join(&sha[2..4])
                .join(format!("{sha}.zst"));
            assert!(file.is_file(), "{}", file.display());
        }
    }
    let loose: i64 = f
        .scalar_i64(
            "select count(*) from data_points d left join snapshots s on s.sha256 = d.snapshot_sha
              where s.sha256 is null or d.locator is null or d.parser is null or d.value_text is null",
        )
        .await;
    assert_eq!(loose, 0, "a point without its snapshot, locator or parser");
    assert_eq!(
        f.scalar_i64("select count(*) from data_points").await,
        parsed["stored"]["points_inserted"].as_i64().unwrap()
    );

    // Idempotent: the same parse again stores nothing new.
    let before = candidates(&f, source).await;
    let (env, code, _, _) = call(
        &mut news(&f, routes_name, Some(tracked)),
        &["parse", source],
    );
    assert_eq!(code, 0, "{env}");
    let again = &env["data"]["stored"];
    assert_eq!(again["points_inserted"], 0, "{env}");
    assert_eq!(again["candidates_inserted"], 0, "{env}");
    assert_eq!(
        again["points_unchanged"], parsed["stored"]["points_inserted"],
        "{env}"
    );
    assert_eq!(candidates(&f, source).await, before);
    f
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn bls_discover_fetch_parse_gives_a_cpi_candidate() {
    let f = walk("bls", "bls", ("NEWS_TRACKED_BLS", "CUUR0000SA0")).await;
    assert_eq!(
        candidates(&f, "bls").await,
        vec![json!([
            "macro:us.cpi:2026-10",
            "macro",
            "rule",
            ["economy", "inflation"],
            "us"
        ])]
    );
    let _ = std::fs::remove_dir_all(work_dir(&f));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn fred_discover_fetch_parse_gives_a_cpi_candidate() {
    let f = walk("fred", "fred", ("NEWS_TRACKED_FRED", "CPIAUCSL")).await;
    assert_eq!(
        candidates(&f, "fred").await,
        vec![json!([
            "macro:us.cpi:2026-10",
            "macro",
            "rule",
            ["economy", "inflation"],
            "us"
        ])]
    );
    let _ = std::fs::remove_dir_all(work_dir(&f));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn sec_discover_fetch_parse_gives_earnings_candidates() {
    let f = walk("sec-edgar", "sec", ("NEWS_TRACKED_SEC_EDGAR", "320193")).await;
    // The 10-K carries the event key; the 8-Ks have none (no Item 2.02 facts to key on).
    let rows = candidates(&f, "sec-edgar").await;
    assert!(
        rows.contains(&json!([
            "earnings:0000320193:2026FY",
            "earnings",
            "rule",
            ["finance", "earnings"],
            "us"
        ])),
        "{rows:?}"
    );
    let _ = std::fs::remove_dir_all(work_dir(&f));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn a_missing_a_disabled_and_a_swept_source() {
    let f = Fixture::migrated().await;
    synced(&f).await;
    let tracked = Some(("NEWS_TRACKED_BLS", "CUUR0000SA0"));

    let (env, code, _, _) = call(&mut news(&f, "bls", tracked), &["discover", "no-such"]);
    assert_eq!(
        (code, env["error"]["code"].as_str()),
        (3, Some("not_found"))
    );
    // sec-edgar ships disabled: nothing is requested for it.
    let (env, code, _, _) = call(&mut news(&f, "sec", tracked), &["parse", "sec-edgar"]);
    assert_eq!(
        (code, env["error"]["code"].as_str()),
        (4, Some("precondition")),
        "{env}"
    );

    // With no source, every enabled source in turn; one that cannot be read is a warning.
    sqlx::query("update sources set enabled = (source_id = 'bls')")
        .execute(&f.pool)
        .await
        .unwrap();
    let (env, code, _, _) = call(&mut news(&f, "bls", tracked), &["discover"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["by_source"], json!({"bls": 1}));
    assert_eq!(env["data"]["items"][0]["event_key"], "macro:us.cpi:2026-10");
    let _ = std::fs::remove_dir_all(work_dir(&f));
}

struct Held;

impl LeaseCheck for Held {
    fn ensure(&self) -> Pin<Box<dyn Future<Output = Result<(), JobFailure>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }
}

fn job(kind: &str, pool: &str, args: Value) -> JobInput {
    JobInput {
        job_id: 1,
        kind: kind.into(),
        pool: pool.into(),
        attempt: 1,
        run_id: None,
        item_ref: None,
        args,
        lease: JobLease::new(1, None, Arc::new(Held)),
    }
}

/// What the presets queue (`steps/edition.rs`, `steps/flash.rs`, `steps/catchup.rs`): `source.discover
/// {source_id}`, `calendar.capture {event_key}` and `story.detect {}`, run by the handlers' own function
/// on the CLI's fixtures, with the single-item kinds `doc.fetch` and `doc.parse` besides.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn the_ingest_jobs_the_presets_queue_run_to_the_same_result() {
    let f = Fixture::migrated().await;
    let sources = news_contacts::load_dir(&sources_dir()).unwrap();
    news_contacts::sync(&f.pool, &sources).await.unwrap();
    let mut ing = Ingest::with_fixtures(f.pool.clone(), work_dir(&f), &routes("bls")).unwrap();
    ing.tracked.insert("bls".into(), vec!["CUUR0000SA0".into()]);

    // source.discover: the whole pass inside the one job, so candidates exist when stories are detected.
    let out = run_kind(
        &ing,
        job("source.discover", "net", json!({"source_id": "bls"})),
    )
    .await
    .unwrap();
    assert_eq!(
        (
            out["refs"].clone(),
            out["items"].clone(),
            out["failed"].clone()
        ),
        (json!(1), json!(1), json!([]))
    );
    assert_eq!(out["event_keys"], json!(["macro:us.cpi:2026-10"]), "{out}");
    assert_eq!(out["stored"]["candidates_inserted"], 1, "{out}");
    // The catch-up preset's arguments are the same job.
    let again = run_kind(
        &ing,
        job(
            "source.discover",
            "net",
            json!({"source_id": "bls", "from": "2026-01-01", "to": "2026-11-01", "backfill": true}),
        ),
    )
    .await
    .unwrap();
    assert_eq!(again["stored"]["points_inserted"], 0, "{again}");
    assert_eq!(again["stored"]["candidates_inserted"], 0, "{again}");

    // doc.fetch and doc.parse take the ItemRef a discovery gave.
    let s = ing.source("bls").await.unwrap();
    let item = serde_json::to_value(&ing.discover(&s).await.unwrap().refs[0]).unwrap();
    let args = json!({"source_id": "bls", "item": item});
    let fetched = run_kind(&ing, job("doc.fetch", "net", args.clone()))
        .await
        .unwrap();
    assert_eq!(fetched["stored"]["points_inserted"], 0, "{fetched}");
    let points = f.scalar_i64("select count(*) from data_points").await;
    let reparsed = run_kind(&ing, job("doc.parse", "cpu", args)).await.unwrap();
    // The Morgue's bytes parse to the same points. (BLS has no vintages of its own: a point's vintage is
    // the fetch time, and the stored row keeps the first sighting's, so a re-parse stamps that earlier
    // time. Salvage will have to settle which stamp wins; this test does not rely on either.)
    assert_eq!(reparsed["points"].as_i64(), Some(points), "{reparsed}");

    // calendar.capture: the event is captured when its key turns up in the pass.
    let day = NaiveDate::from_ymd_opt(2026, 11, 13).unwrap();
    let entry = |key: &str| CalendarEntry {
        source_id: "bls".into(),
        event_key: key.into(),
        title: "CPI".into(),
        expected_day: day,
        expected_at: None,
        window_days: 0,
        precision: "day".into(),
        origin: "derived".into(),
    };
    news_diary::store::upsert(
        &f.pool,
        &[entry("macro:us.cpi:2026-10"), entry("macro:us.cpi:2026-11")],
    )
    .await
    .unwrap();
    for (key, captured) in [
        ("macro:us.cpi:2026-10", true),
        ("macro:us.cpi:2026-11", false),
    ] {
        let out = run_kind(
            &ing,
            job("calendar.capture", "net", json!({"event_key": key})),
        )
        .await
        .unwrap();
        assert_eq!(out["captured"], captured, "{out}");
    }
    let status: Vec<(String, String)> =
        sqlx::query_as("select event_key, status from calendar_events order by event_key")
            .fetch_all(&f.pool)
            .await
            .unwrap();
    assert_eq!(
        status,
        vec![
            ("macro:us.cpi:2026-10".to_owned(), "captured".to_owned()),
            ("macro:us.cpi:2026-11".to_owned(), "expected".to_owned()),
        ]
    );
    let none = run_kind(
        &ing,
        job("calendar.capture", "net", json!({"event_key": "x"})),
    )
    .await
    .unwrap();
    assert_eq!(none["captured"], false);

    // story.detect: the keyed candidate becomes a story.
    let out = run_kind(&ing, job("story.detect", "cpu", json!({})))
        .await
        .unwrap();
    assert_eq!(
        (out["candidates"].clone(), out["new_stories"].clone()),
        (json!(1), json!(1)),
        "{out}"
    );

    // A bad argument or an unknown source is for the dead list, not a retry.
    for args in [json!({}), json!({"source_id": "no-such"})] {
        let err = run_kind(&ing, job("source.discover", "net", args))
            .await
            .unwrap_err();
        assert_eq!(err.class, ErrorClass::InvalidData, "{err}");
    }
    let _ = std::fs::remove_dir_all(work_dir(&f));
}
