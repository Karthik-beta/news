//! The second batch of Stringers through the Runner on a virtual clock (#12): the registry's own
//! policies (loaded from `data/sources/*.toml`) paced and honoured, robots.txt respected, and one
//! golden discover → fetch → parse pass. No sockets.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use chrono::{NaiveDate, TimeZone, Utc};
use news_clerk::candidates::prepare;
use news_connectors::{Cx, Request, connector};
use news_core::job::ErrorClass;
use news_wire::fake::{FakeTransport, Reply};
use news_wire::morgue::Morgue;
use news_wire::pace::{Clock, VirtualClock};
use news_wire::pipeline::{MemoryState, Wire};
use news_wire::runner::{Runner, RunnerConfig, SourcePolicy};

fn fixture(path: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../news-connectors/tests/fixtures/{path}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// The registry row of `id`, as the Runner sees it.
fn registry_policy(id: &str) -> (SourcePolicy, Option<String>) {
    let dir = news_contacts::default_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let s = news_contacts::load_dir(&dir)
        .unwrap()
        .into_iter()
        .find(|s| s.source_id == id)
        .unwrap_or_else(|| panic!("no registry row {id}"));
    (
        SourcePolicy {
            source_id: s.source_id.clone(),
            rate_per_sec: s.rate_per_sec,
            max_concurrency: u32::try_from(s.max_concurrency).unwrap(),
            daily_quota: s.daily_quota.map(|q| u32::try_from(q).unwrap()),
            robots_respect: s.robots_respect,
            robots_exempt: s.robots_exempt.clone(),
            requires_contact_email: s.requires_contact_email,
        },
        s.licence,
    )
}

fn t0() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 18, 6, 0, 0).unwrap()
}

fn runner(fake: &Arc<FakeTransport>, clock: &Arc<VirtualClock>) -> Runner {
    Runner::new(
        fake.clone(),
        clock.clone(),
        RunnerConfig {
            contact_email: Some("desk@example.org".into()),
            secret: Arc::new(|_| Some("TESTKEY".into())),
        },
    )
}

/// arXiv: concurrency 1, and no two requests closer than 3 seconds (the registry paces at one every
/// 6.25 s, stricter than arXiv's limit).
#[tokio::test]
async fn arxiv_keeps_one_connection_and_the_three_second_gap() {
    let (policy, _) = registry_policy("arxiv");
    assert_eq!(policy.max_concurrency, 1);
    let fake = Arc::new(FakeTransport::new());
    // oaipmh.arxiv.org publishes no robots.txt (404): allowed (RFC 9309).
    fake.route(
        "https://oaipmh.arxiv.org/robots.txt",
        vec![Reply::status(404)],
    );
    fake.route(
        "https://oaipmh.arxiv.org/oai",
        vec![Reply::ok(
            "text/xml",
            fixture("arxiv/record-2112.14514.xml"),
        )],
    );
    let clock = Arc::new(VirtualClock::new(t0()));
    let r = runner(&fake, &clock);
    let mut starts = Vec::new();
    for i in 0..6 {
        let url = format!(
            "https://oaipmh.arxiv.org/oai?verb=GetRecord&identifier=oai:arXiv.org:2112.1451{i}"
        );
        r.fetch(&policy, &Request::xml(url), None).await.unwrap();
        starts.push(clock.now());
    }
    for w in starts.windows(2) {
        assert!(
            (w[1] - w[0]).num_milliseconds() >= 3000,
            "gap {:?}",
            w[1] - w[0]
        );
    }
    assert!(
        clock
            .sleeps
            .lock()
            .unwrap()
            .iter()
            .all(|d| d.as_secs_f64() >= 3.0)
    );
}

/// OECD: 60 downloads an hour at most; the registry allows about 29.
#[tokio::test]
async fn oecd_stays_inside_its_hourly_budget_on_a_virtual_clock() {
    let (policy, _) = registry_policy("oecd");
    let fake = Arc::new(FakeTransport::new());
    fake.route("https://sdmx.oecd.org/robots.txt", vec![Reply::status(404)]);
    fake.route(
        "https://sdmx.oecd.org/public/rest/data/",
        vec![Reply::ok(
            "application/vnd.sdmx.data+json",
            fixture("oecd/CPI.USA-probe.json"),
        )],
    );
    let clock = Arc::new(VirtualClock::new(t0()));
    let r = runner(&fake, &clock);
    let mut times = Vec::new();
    for i in 0..40 {
        let url = format!(
            "https://sdmx.oecd.org/public/rest/data/X{i}?lastNObservations=1&format=jsondata"
        );
        r.fetch(&policy, &Request::sdmx(url), None).await.unwrap();
        times.push(clock.now());
    }
    // In every sliding hour, at most 60 requests started: check each window starting at a request.
    for (i, start) in times.iter().enumerate() {
        let in_hour = times[i..]
            .iter()
            .take_while(|t| (**t - *start).num_seconds() < 3600)
            .count();
        assert!(in_hour <= 60, "{in_hour} requests in the hour from #{i}");
    }
    assert!(times.windows(2).all(|w| (w[1] - w[0]).num_seconds() >= 60));
}

/// The query API host's robots.txt (recorded 18 Nov 2026) disallows everything: the Runner refuses,
/// which is why arXiv goes through OAI-PMH.
#[tokio::test]
async fn arxiv_query_host_is_refused_by_its_robots_txt() {
    let (policy, _) = registry_policy("arxiv");
    let fake = Arc::new(FakeTransport::new());
    fake.route(
        "https://export.arxiv.org/robots.txt",
        vec![Reply::ok("text/plain", "User-agent: *\nDisallow: /")],
    );
    let clock = Arc::new(VirtualClock::new(t0()));
    let r = runner(&fake, &clock);
    let e = r
        .fetch(
            &policy,
            &Request::xml("https://export.arxiv.org/api/query?search_query=all:economics"),
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(e.class, ErrorClass::LicenceBlocked);
}

/// Discover → fetch → parse → classify for the ECB, from the registry row and the fake transport.
#[tokio::test]
async fn ecb_golden_pass_from_the_registry_row() {
    let (policy, licence) = registry_policy("ecb");
    let fake = Arc::new(FakeTransport::new());
    fake.route(
        "https://data-api.ecb.europa.eu/robots.txt",
        vec![Reply::ok("text/plain", "# robots.txt\n")],
    );
    fake.route(
        "https://data-api.ecb.europa.eu/service/data/FM/B.U2.EUR.4F.KR.MRR_FR.LEV?lastNObservations=1",
        vec![Reply::ok(
            "application/vnd.sdmx.data+json;version=1.0.0-wd",
            fixture("ecb/FM.MRR-probe.json"),
        )],
    );
    fake.route(
        "https://data-api.ecb.europa.eu/service/data/FM/B.U2.EUR.4F.KR.MRR_FR.LEV?lastNObservations=12",
        vec![Reply::ok(
            "application/vnd.sdmx.data+json;version=1.0.0-wd",
            fixture("ecb/FM.MRR-history.json"),
        )],
    );
    let clock = Arc::new(VirtualClock::new(t0()));
    let r = runner(&fake, &clock);
    let dir = std::env::temp_dir().join(format!("news-batch2-ecb-{}", std::process::id()));
    let morgue = Morgue::new(&dir);
    let state = MemoryState::default();
    let wire = Wire {
        runner: &r,
        morgue: &morgue,
        state: &state,
    };
    let mut tracked = std::collections::BTreeMap::new();
    tracked.insert("ecb".to_owned(), vec!["FM.MRR".to_owned()]);
    let cx = Cx {
        today: NaiveDate::from_ymd_opt(2026, 11, 18).unwrap(),
        sec_ciks: vec![],
        fred_series: vec![],
        bls_series: vec![],
        tracked,
    };
    let conn = connector("ecb").unwrap();
    let found = wire.discover(&policy, conn, &cx, None).await.unwrap();
    assert_eq!(found.refs.len(), 1);
    // The same probe again is a 304-less repeat of identical bytes: the cursor says nothing is new.
    let again = wire
        .discover(&policy, conn, &cx, found.next_cursor.as_deref())
        .await
        .unwrap();
    assert!(again.refs.is_empty());
    let got = wire.fetch(&policy, conn, &found.refs[0]).await.unwrap();
    assert!(got.changed);
    let parsed = conn.parse(&found.refs[0], &got.snapshots).unwrap();
    assert_eq!(parsed.points.len(), 12);
    for p in &parsed.points {
        assert_eq!(
            morgue.get(&p.snapshot_sha).unwrap(),
            got.snapshots[0].bytes,
            "bytes as received"
        );
    }
    let rows = prepare("ecb", licence.as_deref(), &parsed).unwrap();
    assert_eq!(
        rows[0].draft.event_key.as_deref(),
        Some("macro:ea.policy_rate:2026-09-16")
    );
    assert_eq!(rows[0].page_type.as_deref(), Some("macro"));
    assert_eq!(rows[0].categories, ["economy", "monetary-policy"]);
    let _ = std::fs::remove_dir_all(dir);
}

/// BLS's schedule lives on www.bls.gov, whose robots.txt (recorded) allows `/schedule/`: the registry
/// row's `robots_exempt` covers only `api.bls.gov/publicAPI/`, so the calendar fetch is checked.
#[tokio::test]
async fn the_bls_calendar_is_fetched_under_the_normal_robots_rules() {
    let (policy, _) = registry_policy("bls");
    let fake = Arc::new(FakeTransport::new());
    fake.route(
        "https://www.bls.gov/robots.txt",
        vec![Reply::ok(
            "text/plain",
            "User-agent:*\nDisallow:/scripts\nDisallow:/schedule/archives/\n",
        )],
    );
    fake.route(
        "https://www.bls.gov/schedule/news_release/bls.ics",
        vec![Reply::ok(
            "text/calendar",
            "BEGIN:VCALENDAR\nEND:VCALENDAR\n",
        )],
    );
    let clock = Arc::new(VirtualClock::new(t0()));
    let r = runner(&fake, &clock);
    r.fetch(
        &policy,
        &Request::ics("https://www.bls.gov/schedule/news_release/bls.ics"),
        None,
    )
    .await
    .unwrap();
    let e = r
        .fetch(
            &policy,
            &Request::ics("https://www.bls.gov/schedule/archives/x.ics"),
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(e.class, ErrorClass::LicenceBlocked);
}
