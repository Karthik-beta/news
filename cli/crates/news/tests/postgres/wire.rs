//! Contacts, Wire and the Ledger against PostgreSQL (#9): source sync, snapshots and fetch_state,
//! data points with provenance, vintages and `supersedes`, candidates, idempotence.

use super::fixture::Fixture;
use chrono::{NaiveDate, TimeZone, Utc};
use news_clerk::{candidates, ledger, rules};
use news_connectors::{Cx, Parsed, connector};
use news_wire::fake::{FakeTransport, Reply};
use news_wire::morgue::Morgue;
use news_wire::pace::VirtualClock;
use news_wire::pipeline::Wire;
use news_wire::runner::{Runner, RunnerConfig};
use news_wire::store::{PgState, policy};
use std::path::Path;
use std::sync::Arc;

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

fn transport(bls_body: Vec<u8>) -> Arc<FakeTransport> {
    let t = Arc::new(FakeTransport::new());
    t.route("https://data.sec.gov/robots.txt", vec![Reply::status(404)]);
    t.route("https://www.sec.gov/robots.txt", vec![Reply::status(404)]);
    t.route(
        "https://data.sec.gov/submissions/",
        vec![json("sec/submissions-CIK0000320193.json")],
    );
    t.route(
        "https://data.sec.gov/api/xbrl/companyfacts/",
        vec![json("sec/companyfacts-CIK0000320193.json")],
    );
    t.route(
        "https://www.sec.gov/Archives/",
        vec![Reply::ok("text/html", fixture("sec/primary-8k.htm"))],
    );
    t.route(
        "https://api.stlouisfed.org/robots.txt",
        vec![Reply::ok("text/plain", fixture("fred/api-robots.txt"))],
    );
    t.route(
        "https://api.stlouisfed.org/fred/series/vintagedates",
        vec![json("fred/vintagedates-CPIAUCSL.json")],
    );
    t.route(
        "https://api.stlouisfed.org/fred/release/dates",
        vec![json("fred/release-dates-10.json")],
    );
    t.route(
        "https://api.stlouisfed.org/fred/series/observations",
        vec![json("fred/observations-CPIAUCSL.json")],
    );
    t.route(
        "https://api.stlouisfed.org/fred/series?",
        vec![json("fred/series-CPIAUCSL.json")],
    );
    t.route(
        "https://api.bls.gov/publicAPI/v2/timeseries/data/",
        vec![Reply::ok("application/json", bls_body)],
    );
    t
}

fn cx() -> Cx {
    Cx {
        today: NaiveDate::from_ymd_opt(2026, 11, 16).unwrap(),
        sec_ciks: vec!["320193".into()],
        fred_series: vec!["CPIAUCSL".into()],
        bls_series: vec!["CES0000000001".into()],
    }
}

const CURSORS: [(&str, &str); 3] = [
    (
        "sec-edgar",
        r#"{"0000320193":{"date":"2026-08-01","seen":["0000320193-26-000077"]}}"#,
    ),
    ("fred", r#"{"CPIAUCSL":"2026-10-15"}"#),
    ("bls", r#"{"CES0000000001":"2026-M09"}"#),
];

/// discover → fetch → parse → store for every source; returns the summed store report.
async fn run_all(f: &Fixture, morgue: &Morgue, bls_body: Vec<u8>, day: u32) -> ledger::StoreReport {
    let clock = Arc::new(VirtualClock::new(
        Utc.with_ymd_and_hms(2026, 11, day, 8, 0, 0).unwrap(),
    ));
    let config = RunnerConfig {
        contact_email: Some("desk@example.org".into()),
        secret: Arc::new(|_| Some("TESTKEY".into())),
    };
    let runner = Runner::new(transport(bls_body), clock, config);
    let state = PgState { pool: &f.pool };
    let wire = Wire {
        runner: &runner,
        morgue,
        state: &state,
    };
    let mut total = ledger::StoreReport::default();
    for (source, cursor) in CURSORS {
        let pol = policy(&f.pool, source).await.unwrap().unwrap();
        let conn = connector(source).unwrap();
        let found = wire
            .discover(&pol, conn, &cx(), Some(cursor))
            .await
            .unwrap();
        assert!(!found.refs.is_empty(), "{source}");
        for item in &found.refs {
            let got = wire.fetch(&pol, conn, item).await.unwrap();
            let parsed: Parsed = conn.parse(item, &got.snapshots).unwrap();
            let licence: Option<String> =
                sqlx::query_scalar("select licence from sources where source_id = $1")
                    .bind(source)
                    .fetch_one(&f.pool)
                    .await
                    .unwrap();
            let cands = candidates::prepare(source, licence.as_deref(), &parsed).unwrap();
            let r = ledger::store(&f.pool, &parsed, &cands, Utc::now())
                .await
                .unwrap();
            total.points_inserted += r.points_inserted;
            total.points_unchanged += r.points_unchanged;
            total.revisions += r.revisions;
            total.candidates_inserted += r.candidates_inserted;
        }
    }
    total
}

async fn scalar_i64(f: &Fixture, sql: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .fetch_one(&f.pool)
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn sources_snapshots_and_the_ledger_end_to_end() {
    let f = Fixture::migrated().await;
    // Contacts: sync the repo's data files, twice (idempotent).
    let dir = news_contacts::default_dir(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let sources = news_contacts::load_dir(&dir).unwrap();
    assert_eq!(news_contacts::sync(&f.pool, &sources).await.unwrap(), 3);
    assert_eq!(news_contacts::sync(&f.pool, &sources).await.unwrap(), 3);
    assert_eq!(scalar_i64(&f, "select count(*) from sources").await, 3);
    rules::seed_taxonomy(&f.pool).await.unwrap();
    rules::seed_taxonomy(&f.pool).await.unwrap();
    assert_eq!(scalar_i64(&f, "select count(*) from taxonomy").await, 5);

    let work = std::env::temp_dir().join(format!("news-it-morgue-{}", f.schema));
    let morgue = Morgue::new(&work);
    let first = run_all(&f, &morgue, fixture("bls/CES0000000001.json"), 13).await;
    assert!(first.points_inserted > 50, "{first:?}");
    assert_eq!(first.candidates_inserted, 5, "{first:?}");

    // Every data point has its snapshot, locator and parser.
    assert_eq!(
        scalar_i64(
            &f,
            "select count(*) from data_points d left join snapshots s on s.sha256 = d.snapshot_sha
              where s.sha256 is null or d.locator is null or d.locator = '{}'::jsonb
                 or d.locator->>'kind' is null or d.parser !~ '^[a-z-]+@[0-9]+$'"
        )
        .await,
        0
    );
    // value is the exact decimal of value_text.
    let eps: String =
        sqlx::query_scalar("select value::text from data_points where value_text = '8.10'")
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(eps, "8.10");
    // The SEC restatement supersedes the figure as first filed.
    let linked = scalar_i64(
        &f,
        "select count(*) from data_points n join data_points o on o.dp_id = n.supersedes
          where n.concept = 'us-gaap:NetIncomeLoss' and n.value_text = '111990000000'
            and o.value_text = '112010000000'",
    )
    .await;
    assert_eq!(linked, 1);
    // FRED's revised August 2026 supersedes the first print.
    let fred_rev = scalar_i64(
        &f,
        "select count(*) from data_points n join data_points o on o.dp_id = n.supersedes
          where n.series_key = 'fred:CPIAUCSL' and n.period_label = '2026-08-01'
            and n.value_text = '331.112' and o.value_text = '331.000'",
    )
    .await;
    assert_eq!(fred_rev, 1);
    // Roster step 1: one entity per identifier.
    assert_eq!(
        scalar_i64(
            &f,
            "select count(*) from entity_ids where scheme = 'cik' and value = '0000320193'"
        )
        .await,
        1
    );
    assert_eq!(
        scalar_i64(
            &f,
            "select count(*) from entity_ids where scheme = 'iso3166' and value = 'US'"
        )
        .await,
        1
    );
    // Candidates carry event keys and rule categories.
    assert_eq!(
        scalar_i64(
            &f,
            "select count(*) from candidates where page_type_by = 'rule' and categories is not null
              and event_key in ('earnings:0000320193:2026FY', 'macro:us.cpi:2026-10', 'macro:us.payrolls:2026-10')"
        )
        .await,
        3
    );
    // Snapshots: structured data kept at least 400 days (a later fetch of the same bytes only extends
    // it); fetch_state remembers every URL.
    assert_eq!(
        scalar_i64(&f, "select count(*) from snapshots where content_type = 'application/json' and retain_until - fetched_at < interval '400 days'").await,
        0
    );
    assert!(scalar_i64(&f, "select count(*) from fetch_state").await >= 8);

    // Again with the same bytes: nothing new anywhere.
    let points = scalar_i64(&f, "select count(*) from data_points").await;
    let again = run_all(&f, &morgue, fixture("bls/CES0000000001.json"), 13).await;
    assert_eq!(
        (again.points_inserted, again.candidates_inserted),
        (0, 0),
        "{again:?}"
    );
    assert_eq!(
        scalar_i64(&f, "select count(*) from data_points").await,
        points
    );

    // BLS a day later with September revised: one new vintage that supersedes the old one; the
    // unchanged months are not re-stored even though BLS has no vintages of its own.
    let revised = String::from_utf8(fixture("bls/CES0000000001.json"))
        .unwrap()
        .replace("\"value\":\"159128\"", "\"value\":\"159101\"")
        .into_bytes();
    let later = run_all(&f, &morgue, revised, 14).await;
    assert_eq!(later.revisions, 1, "{later:?}");
    assert_eq!(later.points_inserted, 1, "{later:?}");
    let bls_link = scalar_i64(
        &f,
        "select count(*) from data_points n join data_points o on o.dp_id = n.supersedes
          where n.series_key = 'bls:CES0000000001' and n.value_text = '159101' and o.value_text = '159128'",
    )
    .await;
    assert_eq!(bls_link, 1);
    let _ = std::fs::remove_dir_all(work);
}
