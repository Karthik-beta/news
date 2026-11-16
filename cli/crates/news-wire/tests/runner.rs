//! Fake-server tests for the Runner (Bureau §3.2 "Tests"): no sockets, a virtual clock.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use news_connectors::Request;
use news_core::job::{ErrorClass, JobFailure, Policy};
use news_wire::fake::{FakeTransport, Reply};
use news_wire::morgue::Morgue;
use news_wire::pace::{Clock, VirtualClock};
use news_wire::pipeline::{MemoryState, Wire};
use news_wire::runner::{Outcome, Runner, RunnerConfig, SourcePolicy, Validators};

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 16, 8, 0, 0).unwrap()
}

fn policy(id: &str) -> SourcePolicy {
    SourcePolicy {
        source_id: id.into(),
        rate_per_sec: 2.0,
        daily_quota: None,
        robots_respect: true,
        requires_contact_email: false,
    }
}

fn config(email: Option<&str>) -> RunnerConfig {
    RunnerConfig {
        contact_email: email.map(str::to_owned),
        secret: Arc::new(|name| (name == "FRED_API_KEY").then(|| "TESTKEY".to_owned())),
    }
}

fn runner(fake: &Arc<FakeTransport>, clock: &Arc<VirtualClock>, email: Option<&str>) -> Runner {
    Runner::new(fake.clone(), clock.clone(), config(email))
}

fn setup() -> (Arc<FakeTransport>, Arc<VirtualClock>) {
    let fake = Arc::new(FakeTransport::new());
    fake.route("https://h.test/robots.txt", vec![Reply::status(404)]);
    (fake, Arc::new(VirtualClock::new(t0())))
}

const URL: &str = "https://h.test/data.json";

async fn fetch(
    r: &Runner,
    p: &SourcePolicy,
    v: Option<&Validators>,
) -> Result<Outcome, news_wire::runner::FetchError> {
    r.fetch(p, &Request::json(URL), v).await
}

#[tokio::test]
async fn a_304_costs_no_parse() {
    let (fake, clock) = setup();
    fake.route(
        URL,
        vec![
            Reply::ok("application/json", "{}").header("ETag", "\"v1\""),
            Reply::status(304),
        ],
    );
    let r = runner(&fake, &clock, None);
    let dir = std::env::temp_dir().join(format!("news-wire-304-{}", std::process::id()));
    let morgue = Morgue::new(&dir);
    let state = MemoryState::default();
    let conn = news_connectors::connector("bls").unwrap();
    let item = news_connectors::ItemRef {
        source_id: "bls".into(),
        kind: "bls.series".into(),
        key: "CUUR0000SA0".into(),
        url: "https://data.bls.gov/timeseries/CUUR0000SA0".into(),
        event_key_hint: None,
        published_at: None,
        title: None,
        meta: serde_json::json!({}),
    };
    let bls_url = "https://api.bls.gov/publicAPI/v2/timeseries/data/CUUR0000SA0";
    fake.route(
        bls_url,
        vec![
            Reply::ok(
                "application/json",
                b"{\"status\":\"REQUEST_SUCCEEDED\"}".to_vec(),
            )
            .header("Last-Modified", "Fri, 13 Nov 2026 13:30:00 GMT"),
            Reply::status(304),
        ],
    );
    let mut p = policy("bls");
    p.robots_respect = false;
    let cfg = RunnerConfig {
        contact_email: None,
        secret: Arc::new(|_| Some("K".to_owned())),
    };
    let r2 = Runner::new(fake.clone(), clock.clone(), cfg);
    let wire2 = Wire {
        runner: &r2,
        morgue: &morgue,
        state: &state,
    };
    let first = wire2.fetch(&p, conn, &item).await.unwrap();
    assert!(first.changed);
    let second = wire2.fetch(&p, conn, &item).await.unwrap();
    assert!(!second.changed, "all 304: nothing to parse");
    // The 304 still hands back the stored bytes, verified from the Morgue.
    assert_eq!(second.snapshots[0].bytes, first.snapshots[0].bytes);
    assert_eq!(*state.not_modified.lock().unwrap(), 1);
    // The conditional header went out.
    let last = fake.requests.lock().unwrap().last().cloned().unwrap();
    assert!(
        last.headers
            .iter()
            .any(|(k, v)| k == "If-Modified-Since" && v.contains("13 Nov 2026"))
    );

    // And through the Runner directly with an ETag.
    assert!(matches!(
        fetch(&r, &policy("x"), None).await.unwrap(),
        Outcome::Fetched(_)
    ));
    let v = Validators {
        etag: Some("\"v1\"".into()),
        last_modified: None,
    };
    assert_eq!(
        fetch(&r, &policy("x"), Some(&v)).await.unwrap(),
        Outcome::NotModified
    );
    let last = fake.requests.lock().unwrap().last().cloned().unwrap();
    assert!(
        last.headers
            .iter()
            .any(|(k, v)| k == "If-None-Match" && v == "\"v1\"")
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn retry_after_in_seconds_and_as_a_date_snoozes() {
    let (fake, clock) = setup();
    fake.route(
        URL,
        vec![
            Reply::status(429).header("Retry-After", "120"),
            Reply::status(503).header("Retry-After", "Mon, 16 Nov 2026 08:05:00 GMT"),
        ],
    );
    let r = runner(&fake, &clock, None);
    let e1 = fetch(&r, &policy("x"), None).await.unwrap_err();
    assert_eq!(e1.class, ErrorClass::RateLimited);
    assert_eq!(e1.retry_after, Some(Duration::from_secs(120)));
    let e2 = fetch(&r, &policy("x"), None).await.unwrap_err();
    assert_eq!(e2.class, ErrorClass::RateLimited);
    // 08:05 less the virtual time already spent pacing.
    let expected = (Utc.with_ymd_and_hms(2026, 11, 16, 8, 5, 0).unwrap() - clock.now())
        .to_std()
        .unwrap();
    assert_eq!(e2.retry_after, Some(expected));
    assert!(expected > Duration::from_secs(290));
    // Snooze, not a failed attempt: Dispatch gives the attempt back for this class.
    let failure: JobFailure = e2.into();
    assert_eq!(failure.class.policy(), Policy::Snooze);
    assert!(failure.retry_after.is_some());
}

#[tokio::test]
async fn robots_4xx_allows_and_5xx_disallows() {
    let (fake, clock) = setup();
    fake.route(URL, vec![Reply::ok("application/json", "{}")]);
    let r = runner(&fake, &clock, None);
    assert!(fetch(&r, &policy("x"), None).await.is_ok());
    assert!(fetch(&r, &policy("x"), None).await.is_ok());
    let robots_hits = fake
        .urls()
        .iter()
        .filter(|u| u.ends_with("/robots.txt"))
        .count();
    assert_eq!(robots_hits, 1, "robots.txt is cached");

    let fake = Arc::new(FakeTransport::new());
    fake.route("https://h.test/robots.txt", vec![Reply::status(503)]);
    fake.route(URL, vec![Reply::ok("application/json", "{}")]);
    let r = runner(&fake, &clock, None);
    let e = fetch(&r, &policy("x"), None).await.unwrap_err();
    assert_eq!(e.class, ErrorClass::LicenceBlocked);
    assert!(
        !fake.urls().iter().any(|u| u == URL),
        "the disallowed URL was never requested"
    );

    // A real robots.txt that disallows everything (api.bls.gov, recorded).
    let fake = Arc::new(FakeTransport::new());
    let body = std::fs::read(format!(
        "{}/../news-connectors/tests/fixtures/bls/api-robots.txt",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    fake.route(
        "https://h.test/robots.txt",
        vec![Reply::ok("text/plain", body)],
    );
    let r = runner(&fake, &clock, None);
    assert_eq!(
        fetch(&r, &policy("x"), None).await.unwrap_err().class,
        ErrorClass::LicenceBlocked
    );
    // A source with robots_respect = false (BLS's documented API) is not checked.
    fake.route(URL, vec![Reply::ok("application/json", "{}")]);
    let mut p = policy("bls");
    p.robots_respect = false;
    assert!(fetch(&r, &p, None).await.is_ok());
}

#[tokio::test]
async fn the_token_bucket_paces_requests_on_a_virtual_clock() {
    let (fake, clock) = setup();
    fake.route(URL, vec![Reply::ok("application/json", "{}")]);
    let r = runner(&fake, &clock, None);
    let mut p = policy("x");
    p.robots_respect = false;
    for _ in 0..5 {
        fetch(&r, &p, None).await.unwrap();
    }
    // 2 req/s: the first at once, four more 500 ms apart.
    assert_eq!((clock.now() - t0()).num_milliseconds(), 2000);
    assert!(
        clock
            .sleeps
            .lock()
            .unwrap()
            .iter()
            .all(|d| d.as_millis() == 500)
    );
}

#[tokio::test]
async fn oversize_or_wrong_type_is_aborted_as_invalid_data() {
    let (fake, clock) = setup();
    let r = runner(&fake, &clock, None);
    let mut req = Request::json(URL);
    req.max_bytes = 10;
    // Declared too large.
    fake.route(
        URL,
        vec![Reply::ok("application/json", vec![b' '; 5]).header("Content-Length", "11")],
    );
    assert_eq!(
        r.fetch(&policy("x"), &req, None).await.unwrap_err().class,
        ErrorClass::InvalidData
    );
    // Streamed past the cap with no Content-Length.
    let fake2 = Arc::new(FakeTransport::new());
    fake2.route("https://h.test/robots.txt", vec![Reply::status(404)]);
    let mut big = Reply::ok("application/json", vec![b' '; 25]);
    big.chunk = 4;
    fake2.route(URL, vec![big]);
    let r2 = runner(&fake2, &clock, None);
    let e = r2.fetch(&policy("x"), &req, None).await.unwrap_err();
    assert_eq!(e.class, ErrorClass::InvalidData);
    assert!(e.message.contains("aborted"));
    // Wrong content type (an HTML error page where JSON was expected).
    let fake3 = Arc::new(FakeTransport::new());
    fake3.route("https://h.test/robots.txt", vec![Reply::status(404)]);
    fake3.route(URL, vec![Reply::ok("text/html; charset=utf-8", "<html>")]);
    let r3 = runner(&fake3, &clock, None);
    assert_eq!(
        fetch(&r3, &policy("x"), None).await.unwrap_err().class,
        ErrorClass::InvalidData
    );
}

#[tokio::test]
async fn status_codes_map_to_classes() {
    for (status, class) in [
        (404, ErrorClass::UpstreamChanged),
        (500, ErrorClass::Transient),
        (403, ErrorClass::InvalidData),
    ] {
        let (fake, clock) = setup();
        fake.route(URL, vec![Reply::status(status)]);
        let r = runner(&fake, &clock, None);
        assert_eq!(
            fetch(&r, &policy("x"), None).await.unwrap_err().class,
            class,
            "{status}"
        );
    }
}

#[tokio::test]
async fn the_contact_email_goes_to_sec_only_and_keys_never_leak() {
    let (fake, clock) = setup();
    fake.route(URL, vec![Reply::ok("application/json", "{}")]);
    let mut sec = policy("sec-edgar");
    sec.requires_contact_email = true;
    // Without NEWS_CONTACT_EMAIL, SEC is refused before any request.
    let r = runner(&fake, &clock, None);
    assert_eq!(
        fetch(&r, &sec, None).await.unwrap_err().class,
        ErrorClass::Bug
    );
    assert!(fake.urls().is_empty());
    let r = runner(&fake, &clock, Some("desk@example.org"));
    fetch(&r, &sec, None).await.unwrap();
    fetch(&r, &policy("fred"), None).await.unwrap();
    let reqs = fake.requests.lock().unwrap().clone();
    let ua = |i: usize| {
        reqs[i]
            .headers
            .iter()
            .find(|(k, _)| k == "User-Agent")
            .unwrap()
            .1
            .clone()
    };
    let sec_ua = ua(reqs.iter().position(|q| q.url == URL).unwrap());
    assert!(
        sec_ua.starts_with("News/") && sec_ua.ends_with(" desk@example.org"),
        "{sec_ua}"
    );
    assert!(!ua(reqs.len() - 1).contains('@'));

    // A free key is appended at send time and never kept.
    let req = Request::json(URL).with_secret("api_key", "FRED_API_KEY");
    let mut p = policy("fred");
    p.robots_respect = false;
    let Outcome::Fetched(f) = r.fetch(&p, &req, None).await.unwrap() else {
        panic!()
    };
    assert_eq!(f.url, URL);
    assert!(fake.urls().last().unwrap().ends_with("?api_key=TESTKEY"));
    let missing = Request::json(URL).with_secret("registrationkey", "BLS_API_KEY");
    assert_eq!(
        r.fetch(&p, &missing, None).await.unwrap_err().class,
        ErrorClass::Bug
    );
}

#[tokio::test]
async fn the_daily_quota_defers_to_tomorrow() {
    let (fake, clock) = setup();
    fake.route(URL, vec![Reply::ok("application/json", "{}")]);
    let r = runner(&fake, &clock, None);
    let mut p = policy("bls");
    p.daily_quota = Some(2);
    p.robots_respect = false;
    fetch(&r, &p, None).await.unwrap();
    fetch(&r, &p, None).await.unwrap();
    let e = fetch(&r, &p, None).await.unwrap_err();
    assert_eq!(e.class, ErrorClass::RateLimited);
    let wait = e.retry_after.unwrap().as_secs();
    assert!((15 * 3600..=16 * 3600 + 600).contains(&wait), "{wait}");
}
