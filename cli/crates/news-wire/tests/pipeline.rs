//! Golden pipeline: discover → fetch → parse → classify for SEC EDGAR, FRED and BLS, on fixtures
//! served by the fake transport (no sockets). The output is candidates with event keys and rule-based
//! categories, and data points whose bytes are in the Morgue.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use chrono::{NaiveDate, TimeZone, Utc};
use news_clerk::candidates::prepare;
use news_connectors::{Cx, connector};
use news_wire::fake::{FakeTransport, Reply};
use news_wire::morgue::Morgue;
use news_wire::pace::VirtualClock;
use news_wire::pipeline::{MemoryState, Wire};
use news_wire::runner::{Runner, RunnerConfig, SourcePolicy};

fn fixture(path: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../news-connectors/tests/fixtures/{path}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn json(path: &str) -> Reply {
    Reply::ok("application/json", fixture(path))
}

/// `(source, event key, page type, categories, data points in the item, points stored)`.
type Row = (
    String,
    Option<String>,
    Option<String>,
    Vec<String>,
    usize,
    usize,
);

async fn run(
    source: &str,
    licence: &str,
    fake: Arc<FakeTransport>,
    policy: SourcePolicy,
    cursor: Option<&str>,
) -> Vec<Row> {
    let clock = Arc::new(VirtualClock::new(
        Utc.with_ymd_and_hms(2026, 11, 16, 8, 0, 0).unwrap(),
    ));
    let config = RunnerConfig {
        contact_email: Some("desk@example.org".into()),
        secret: Arc::new(|_| Some("TESTKEY".into())),
    };
    let runner = Runner::new(fake, clock, config);
    let dir = std::env::temp_dir().join(format!("news-golden-{source}-{}", std::process::id()));
    let morgue = Morgue::new(&dir);
    let state = MemoryState::default();
    let wire = Wire {
        runner: &runner,
        morgue: &morgue,
        state: &state,
    };
    let cx = Cx {
        today: NaiveDate::from_ymd_opt(2026, 11, 16).unwrap(),
        sec_ciks: vec!["320193".into()],
        fred_series: vec!["CPIAUCSL".into()],
        bls_series: vec!["CUUR0000SA0".into()],
    };
    let conn = connector(source).unwrap();
    let found = wire.discover(&policy, conn, &cx, cursor).await.unwrap();
    let mut rows = Vec::new();
    for item in &found.refs {
        let got = wire.fetch(&policy, conn, item).await.unwrap();
        assert!(got.changed);
        for s in &got.snapshots {
            assert_eq!(
                morgue.get(&s.sha256).unwrap(),
                s.bytes,
                "bytes in the Morgue as received"
            );
        }
        let parsed = conn.parse(item, &got.snapshots).unwrap();
        for p in &parsed.points {
            assert!(
                got.snapshots.iter().any(|s| s.sha256 == p.snapshot_sha),
                "point cites its snapshot"
            );
        }
        for c in prepare(source, Some(licence), &parsed).unwrap() {
            rows.push((
                source.to_owned(),
                c.draft.event_key.clone(),
                c.page_type.clone(),
                c.categories.clone(),
                c.draft.dp_ids.len(),
                parsed.points.len(),
            ));
        }
    }
    let _ = std::fs::remove_dir_all(dir);
    rows
}

fn policy(id: &str, robots: bool, sec: bool) -> SourcePolicy {
    SourcePolicy {
        source_id: id.into(),
        rate_per_sec: 1.0,
        daily_quota: None,
        robots_respect: robots,
        requires_contact_email: sec,
    }
}

fn row(s: &str, k: Option<&str>, t: &str, cats: &[&str], n: usize, total: usize) -> Row {
    (
        s.into(),
        k.map(str::to_owned),
        Some(t.into()),
        cats.iter().map(|c| (*c).to_owned()).collect(),
        n,
        total,
    )
}

#[tokio::test]
async fn golden_discover_fetch_parse() {
    // SEC: data.sec.gov answers 404 for robots.txt (recorded), which RFC 9309 reads as "allowed".
    let sec = Arc::new(FakeTransport::new());
    sec.route("https://data.sec.gov/robots.txt", vec![Reply::status(404)]);
    sec.route("https://www.sec.gov/robots.txt", vec![Reply::status(404)]);
    sec.route(
        "https://data.sec.gov/submissions/",
        vec![json("sec/submissions-CIK0000320193.json")],
    );
    sec.route(
        "https://data.sec.gov/api/xbrl/companyfacts/",
        vec![json("sec/companyfacts-CIK0000320193.json")],
    );
    sec.route(
        "https://www.sec.gov/Archives/",
        vec![Reply::ok("text/html", fixture("sec/primary-8k.htm"))],
    );
    let got = run(
        "sec-edgar",
        "public-domain",
        sec,
        policy("sec-edgar", true, true),
        Some(r#"{"0000320193":{"date":"2026-08-01","seen":["0000320193-26-000077"]}}"#),
    )
    .await;
    assert_eq!(
        got,
        vec![
            row(
                "sec-edgar",
                None,
                "earnings",
                &["finance", "earnings"],
                0,
                0
            ),
            row(
                "sec-edgar",
                Some("earnings:0000320193:2026FY"),
                "earnings",
                &["finance", "earnings"],
                8,
                16
            ),
            // The filing agent's 8-K/A (0001193125-…) on the same day as the 10-K.
            row(
                "sec-edgar",
                None,
                "earnings",
                &["finance", "earnings"],
                0,
                0
            ),
        ]
    );

    let fred = Arc::new(FakeTransport::new());
    fred.route(
        "https://api.stlouisfed.org/robots.txt",
        vec![Reply::ok("text/plain", fixture("fred/api-robots.txt"))],
    );
    fred.route(
        "https://api.stlouisfed.org/fred/series/vintagedates",
        vec![json("fred/vintagedates-CPIAUCSL.json")],
    );
    fred.route(
        "https://api.stlouisfed.org/fred/release/dates",
        vec![json("fred/release-dates-10.json")],
    );
    fred.route(
        "https://api.stlouisfed.org/fred/series/observations",
        vec![json("fred/observations-CPIAUCSL.json")],
    );
    fred.route(
        "https://api.stlouisfed.org/fred/series?",
        vec![json("fred/series-CPIAUCSL.json")],
    );
    let got = run(
        "fred",
        "custom",
        fred,
        policy("fred", true, false),
        Some(r#"{"CPIAUCSL":"2026-10-15"}"#),
    )
    .await;
    assert_eq!(
        got,
        vec![row(
            "fred",
            Some("macro:us.cpi:2026-10"),
            "macro",
            &["economy", "inflation"],
            1,
            6
        )]
    );

    let bls = Arc::new(FakeTransport::new());
    bls.route(
        "https://api.bls.gov/publicAPI/v2/timeseries/data/CUUR0000SA0",
        vec![json("bls/CUUR0000SA0.json")],
    );
    let got = run(
        "bls",
        "public-domain",
        bls.clone(),
        policy("bls", false, false),
        Some(r#"{"CUUR0000SA0":"2026-M09"}"#),
    )
    .await;
    assert_eq!(
        got,
        vec![row(
            "bls",
            Some("macro:us.cpi:2026-10"),
            "macro",
            &["economy", "inflation"],
            1,
            35
        )]
    );
    // robots_respect = false for the BLS API host: robots.txt is never requested.
    assert!(!bls.urls().iter().any(|u| u.ends_with("/robots.txt")));
    assert!(
        bls.urls()
            .iter()
            .all(|u| u.contains("registrationkey=TESTKEY"))
    );
}
