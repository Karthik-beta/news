//! Edge cases around the Runner, robots.txt, the token bucket and the Morgue that `runner.rs` does not
//! reach: malformed headers, boundary sizes, unavailable robots.txt, hostile config and concurrency.
//! Fake transport and a virtual clock only; no sockets.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use news_connectors::Request;
use news_core::job::ErrorClass;
use news_wire::fake::{FakeTransport, Reply};
use news_wire::morgue::{self, Morgue};
use news_wire::pace::{TokenBucket, VirtualClock, retry_after};
use news_wire::robots;
use news_wire::runner::{FetchError, Outcome, Runner, RunnerConfig, SourcePolicy};
use sha2::{Digest, Sha256};

const URL: &str = "https://h.test/data.json";

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 16, 8, 0, 0).unwrap()
}

fn policy() -> SourcePolicy {
    SourcePolicy {
        source_id: "x".into(),
        rate_per_sec: 2.0,
        max_concurrency: 1,
        daily_quota: None,
        robots_respect: true,
        robots_exempt: vec![],
        requires_contact_email: false,
    }
}

fn runner(fake: &Arc<FakeTransport>, clock: &Arc<VirtualClock>, email: Option<&str>) -> Runner {
    Runner::new(
        fake.clone(),
        clock.clone(),
        RunnerConfig {
            contact_email: email.map(str::to_owned),
            secret: Arc::new(|_| None),
        },
    )
}

fn setup() -> (Arc<FakeTransport>, Arc<VirtualClock>) {
    let fake = Arc::new(FakeTransport::new());
    fake.route("https://h.test/robots.txt", vec![Reply::status(404)]);
    (fake, Arc::new(VirtualClock::new(t0())))
}

async fn get(r: &Runner, req: &Request) -> Result<Outcome, FetchError> {
    r.fetch(&policy(), req, None).await
}

// ----------------------------------------------------------------------------- Retry-After

#[tokio::test]
async fn retry_after_oddities_snooze_sanely() {
    // (header, the snooze the job gets). A header we cannot read is a minute, never zero and never
    // an error; zero and a date in the past mean "now", which the halved rate then paces.
    let cases: [(Option<&str>, Duration); 9] = [
        (Some("0"), Duration::ZERO),
        (Some(" 30 "), Duration::from_secs(30)),
        (Some("-5"), Duration::from_secs(60)),
        (Some("1.5"), Duration::from_secs(60)),
        (Some("1e3"), Duration::from_secs(60)),
        (Some("soon"), Duration::from_secs(60)),
        (Some(""), Duration::from_secs(60)),
        (Some("Sun, 15 Nov 2026 07:00:00 GMT"), Duration::ZERO),
        (None, Duration::from_secs(60)),
    ];
    for (header, want) in cases {
        for status in [429, 503] {
            let (fake, clock) = setup();
            let mut reply = Reply::status(status);
            if let Some(h) = header {
                reply = reply.header("Retry-After", h);
            }
            fake.route(URL, vec![reply]);
            let e = get(&runner(&fake, &clock, None), &Request::json(URL))
                .await
                .unwrap_err();
            assert_eq!(e.class, ErrorClass::RateLimited, "{status} {header:?}");
            assert_eq!(e.retry_after, Some(want), "{status} {header:?}");
        }
    }
}

#[test]
fn retry_after_parser_edges() {
    let now = t0();
    // RFC 9110 allows only delay-seconds (digits) or an HTTP-date.
    for bad in [
        "",
        "-1",
        "+5",
        "1.0",
        "0x10",
        "5 s",
        "tomorrow",
        "Mon 16 Nov 2026",
    ] {
        assert_eq!(retry_after(bad, now), None, "{bad:?}");
    }
    assert_eq!(retry_after("007", now), Some(Duration::from_secs(7)));
    // Anything longer than a day is a day, in either form, even with more digits than a u64.
    let day = news_wire::pace::MAX_RETRY_AFTER;
    assert_eq!(retry_after("99999999999999999999999999", now), Some(day));
    assert_eq!(retry_after("Fri, 31 Dec 9999 23:59:59 GMT", now), Some(day));
    assert_eq!(retry_after("86400", now), Some(day));
    assert_eq!(retry_after("86399", now), Some(Duration::from_secs(86399)));
    // An HTTP-date exactly now is zero, one second ahead is one second.
    assert_eq!(
        retry_after("Mon, 16 Nov 2026 08:00:00 GMT", now),
        Some(Duration::ZERO)
    );
    assert_eq!(
        retry_after("Mon, 16 Nov 2026 08:00:01 GMT", now),
        Some(Duration::from_secs(1))
    );
}

#[tokio::test]
async fn an_absurd_retry_after_is_capped() {
    let (fake, clock) = setup();
    fake.route(
        URL,
        vec![Reply::status(429).header("Retry-After", "18446744073709551615")],
    );
    let e = get(&runner(&fake, &clock, None), &Request::json(URL))
        .await
        .unwrap_err();
    assert!(
        e.retry_after == Some(news_wire::pace::MAX_RETRY_AFTER),
        "{:?}",
        e.retry_after
    );
}

// ------------------------------------------------------------------------------ robots.txt

#[tokio::test]
async fn every_unavailable_4xx_for_robots_allows_and_is_cached_for_the_day() {
    for status in [400u16, 401, 403, 404, 410] {
        let fake = Arc::new(FakeTransport::new());
        fake.route("https://h.test/robots.txt", vec![Reply::status(status)]);
        fake.route(URL, vec![Reply::ok("application/json", "{}")]);
        let clock = Arc::new(VirtualClock::new(t0()));
        let r = runner(&fake, &clock, None);
        get(&r, &Request::json(URL)).await.expect("allowed");
        // Still cached 23 hours later, asked again after 24.
        clock.advance(Duration::from_secs(23 * 3600));
        get(&r, &Request::json(URL)).await.expect("allowed");
        let robots_hits = |f: &FakeTransport| {
            f.urls()
                .iter()
                .filter(|u| u.ends_with("/robots.txt"))
                .count()
        };
        assert_eq!(robots_hits(&fake), 1, "{status}");
        clock.advance(Duration::from_secs(2 * 3600));
        get(&r, &Request::json(URL)).await.expect("allowed");
        assert_eq!(robots_hits(&fake), 2, "{status}");
    }
}

#[tokio::test]
async fn robots_503_with_retry_after_is_honoured_for_that_long() {
    let fake = Arc::new(FakeTransport::new());
    fake.route(
        "https://h.test/robots.txt",
        vec![
            Reply::status(503).header("Retry-After", "120"),
            Reply::status(404),
        ],
    );
    fake.route(URL, vec![Reply::ok("application/json", "{}")]);
    let clock = Arc::new(VirtualClock::new(t0()));
    let r = runner(&fake, &clock, None);
    let e = get(&r, &Request::json(URL)).await.unwrap_err();
    assert_eq!(e.class, ErrorClass::RateLimited);
    assert_eq!(e.retry_after, Some(Duration::from_secs(120)));
    assert!(!fake.urls().iter().any(|u| u == URL), "never fetched");
}

#[tokio::test]
async fn an_oversized_robots_txt_is_read_up_to_its_limit_not_forever() {
    // 600 KiB of comments, then a Disallow: the file is cut at 512 KiB (RFC 9309 asks for at least
    // 500 KiB), so the rule beyond the cut is not seen and the fetch proceeds; nothing hangs or panics.
    let mut body = b"User-agent: *\n".to_vec();
    while body.len() < 600 * 1024 {
        body.extend_from_slice(b"# padding padding padding padding padding padding padding\n");
    }
    body.extend_from_slice(b"Disallow: /\n");
    let fake = Arc::new(FakeTransport::new());
    fake.route(
        "https://h.test/robots.txt",
        vec![Reply::ok("text/plain", body)],
    );
    fake.route(URL, vec![Reply::ok("application/json", "{}")]);
    let clock = Arc::new(VirtualClock::new(t0()));
    get(&runner(&fake, &clock, None), &Request::json(URL))
        .await
        .expect("a rule past the cut is not applied");
}

#[test]
fn robots_parsing_edges() {
    // CRLF line ends, comments, mixed case keys, and a rule before any user-agent line (ignored).
    let r = robots::parse(
        "Disallow: /ignored\r\nUSER-AGENT: *\r\nDISALLOW: /a # trailing comment\r\n# full line\r\nAllow: /a/ok\r\n",
    );
    assert!(r.allowed("News", "/ignored"));
    assert!(!r.allowed("News", "/a"));
    assert!(!r.allowed("News", "/a/other"));
    assert!(r.allowed("News", "/a/ok"));
    // An empty Disallow allows everything; `Disallow: /` after it in another group still applies there.
    let r = robots::parse("User-agent: news\nDisallow:\n\nUser-agent: *\nDisallow: /\n");
    assert!(r.allowed("News", "/anything"));
    assert!(!r.allowed("Other", "/anything"));
    // `$` anchors; `*` spans slashes; the query string is part of the path matched.
    let r = robots::parse("User-agent: *\nDisallow: /*.json$\nDisallow: /search?*q=\n");
    assert!(!r.allowed("News", "/a/b/c.json"));
    assert!(r.allowed("News", "/a/b/c.json?x=1"));
    assert!(!r.allowed("News", "/search?page=2&q=cpi"));
    assert!(r.allowed("News", "/search?page=2"));
    // Several groups for one agent are read as one.
    let r = robots::parse("User-agent: news\nDisallow: /a\n\nUser-agent: news\nDisallow: /b\n");
    assert!(!r.allowed("News", "/a") && !r.allowed("News", "/b") && r.allowed("News", "/c"));
    // The root path is covered by `Disallow: /`.
    let r = robots::parse("User-agent: *\nDisallow: /\n");
    assert!(!r.allowed("News", "/"));
}

#[test]
fn a_robots_txt_with_a_byte_order_mark_still_disallows() {
    let r = robots::parse("\u{feff}User-agent: *\nDisallow: /\n");
    assert!(!r.allowed("News", "/data.json"));
}

// -------------------------------------------------------------------- bodies and headers

#[tokio::test]
async fn content_type_matching_is_by_essence_and_a_missing_header_is_refused() {
    let (fake, clock) = setup();
    fake.route(
        URL,
        vec![
            Reply::ok("Application/JSON; Charset=UTF-8", "{}"),
            Reply {
                status: 200,
                headers: vec![],
                body: b"{}".to_vec(),
                chunk: 4096,
            },
            Reply::ok("application/jsonp", "{}"),
            Reply::ok("application/json-seq", "{}"),
        ],
    );
    let r = runner(&fake, &clock, None);
    assert!(matches!(
        get(&r, &Request::json(URL)).await.unwrap(),
        Outcome::Fetched(_)
    ));
    for _ in 0..3 {
        let e = get(&r, &Request::json(URL)).await.unwrap_err();
        assert_eq!(e.class, ErrorClass::InvalidData);
    }
}

#[tokio::test]
async fn the_size_cap_is_exact_and_a_lying_length_does_not_help() {
    let mut req = Request::json(URL);
    req.max_bytes = 10;
    let cases: Vec<(Reply, Option<ErrorClass>)> = vec![
        // Exactly the cap, with and without a truthful Content-Length.
        (Reply::ok("application/json", vec![b' '; 10]), None),
        (
            Reply::ok("application/json", vec![b' '; 10]).header("Content-Length", "10"),
            None,
        ),
        // One byte over.
        (
            Reply::ok("application/json", vec![b' '; 11]),
            Some(ErrorClass::InvalidData),
        ),
        // Content-Length understates; the stream is counted anyway.
        (
            Reply::ok("application/json", vec![b' '; 40]).header("Content-Length", "5"),
            Some(ErrorClass::InvalidData),
        ),
        // Content-Length is not a number: ignored, the stream decides.
        (
            Reply::ok("application/json", vec![b' '; 40]).header("Content-Length", "lots"),
            Some(ErrorClass::InvalidData),
        ),
        (
            Reply::ok("application/json", vec![b' '; 4]).header("Content-Length", "lots"),
            None,
        ),
    ];
    for (i, (reply, want)) in cases.into_iter().enumerate() {
        let (fake, clock) = setup();
        let mut reply = reply;
        reply.chunk = 3;
        fake.route(URL, vec![reply]);
        let got = runner(&fake, &clock, None)
            .fetch(&policy(), &req, None)
            .await;
        match want {
            None => assert!(matches!(got, Ok(Outcome::Fetched(_))), "case {i}: {got:?}"),
            Some(class) => assert_eq!(got.unwrap_err().class, class, "case {i}"),
        }
    }
}

#[tokio::test]
async fn the_hash_is_of_the_bytes_whatever_the_chunking() {
    let body: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
    let want: [u8; 32] = Sha256::digest(&body).into();
    for chunk in [1usize, 3, 4096, 20_000] {
        let (fake, clock) = setup();
        let mut reply = Reply::ok("application/json", body.clone());
        reply.chunk = chunk;
        fake.route(URL, vec![reply]);
        let Outcome::Fetched(f) = get(&runner(&fake, &clock, None), &Request::json(URL))
            .await
            .unwrap()
        else {
            panic!("fetched");
        };
        assert_eq!(f.sha256, want, "chunk {chunk}");
        assert_eq!(f.bytes, body);
    }
}

#[tokio::test]
async fn other_statuses_and_a_dead_transport_are_classified_not_swallowed() {
    // (status, class). Redirects the transport did not follow, auth and gone-for-good.
    for (status, class) in [
        (301, ErrorClass::InvalidData),
        (401, ErrorClass::InvalidData),
        (410, ErrorClass::UpstreamChanged),
        (502, ErrorClass::Transient),
        (504, ErrorClass::Transient),
    ] {
        let (fake, clock) = setup();
        fake.route(URL, vec![Reply::status(status)]);
        let e = get(&runner(&fake, &clock, None), &Request::json(URL))
            .await
            .unwrap_err();
        assert_eq!(e.class, class, "{status}");
    }
    // No route at all stands for a refused connection.
    let (fake, clock) = setup();
    let e = get(
        &runner(&fake, &clock, None),
        &Request::json("https://h.test/none"),
    )
    .await
    .unwrap_err();
    assert_eq!(e.class, ErrorClass::Transient);
}

#[tokio::test]
async fn a_contact_email_that_could_inject_a_header_is_refused_before_any_request() {
    for bad in [
        "desk@example.org\r\nX-Evil: 1",
        "desk@example.org\nX-Evil: 1",
        "desk example@org.test",
        "@example.org",
        "desk@localhost",
        "a@b@c.d",
        "<desk@example.org>",
        "",
    ] {
        let (fake, clock) = setup();
        fake.route(URL, vec![Reply::ok("application/json", "{}")]);
        let mut sec = policy();
        sec.requires_contact_email = true;
        let e = runner(&fake, &clock, Some(bad))
            .fetch(&sec, &Request::json(URL), None)
            .await
            .unwrap_err();
        assert_eq!(e.class, ErrorClass::Bug, "{bad:?}");
        assert!(fake.urls().is_empty(), "{bad:?}: a request went out");
    }
}

#[tokio::test]
async fn a_source_that_does_not_need_the_email_never_sends_it() {
    let (fake, clock) = setup();
    fake.route(URL, vec![Reply::ok("application/json", "{}")]);
    // Even a malformed address in the settings cannot break a source that does not use it.
    get(
        &runner(&fake, &clock, Some("not an address")),
        &Request::json(URL),
    )
    .await
    .unwrap();
    let reqs = fake.requests.lock().unwrap().clone();
    for q in reqs {
        let ua = &q.headers.iter().find(|(k, _)| k == "User-Agent").unwrap().1;
        assert!(!ua.contains("not an address"), "{ua}");
    }
}

#[tokio::test]
async fn the_quota_resets_at_midnight_and_a_zero_quota_never_fetches() {
    let (fake, clock) = setup();
    fake.route(URL, vec![Reply::ok("application/json", "{}")]);
    let r = runner(&fake, &clock, None);
    let mut p = policy();
    p.robots_respect = false;
    p.daily_quota = Some(1);
    r.fetch(&p, &Request::json(URL), None).await.unwrap();
    let e = r.fetch(&p, &Request::json(URL), None).await.unwrap_err();
    assert_eq!(e.class, ErrorClass::RateLimited);
    // Waiting exactly what it said is enough.
    clock.advance(e.retry_after.unwrap());
    r.fetch(&p, &Request::json(URL), None).await.unwrap();
    let before = fake.urls().len();
    let mut fresh = policy();
    fresh.daily_quota = Some(0);
    fresh.robots_respect = false;
    let e = runner(&fake, &clock, None)
        .fetch(&fresh, &Request::json(URL), None)
        .await
        .unwrap_err();
    assert_eq!(e.class, ErrorClass::RateLimited);
    assert_eq!(fake.urls().len(), before, "a spent quota sends nothing");
}

// ------------------------------------------------------------------------- token bucket

proptest::proptest! {
    /// Whatever the rate, grants are never closer than 1/rate (no bursts), and a penalty at least
    /// halves the rate, on a virtual clock.
    #[test]
    fn the_bucket_never_bursts(rate in 0.05f64..50.0, n in 2usize..40, penalised in proptest::bool::ANY) {
        let mut now = t0();
        let mut b = TokenBucket::new(rate, now);
        if penalised {
            b.penalise(now);
        }
        let eff = if penalised { rate / 2.0 } else { rate };
        let mut grants = Vec::new();
        for _ in 0..n {
            // Bounded, so a regression fails instead of hanging.
            for _ in 0..1000 {
                let wait = b.reserve(now);
                if wait.is_zero() {
                    break;
                }
                let micros = i64::try_from(wait.as_micros()).unwrap() + 1;
                now += chrono::Duration::microseconds(micros);
            }
            grants.push(now);
        }
        for w in grants.windows(2) {
            let gap = (w[1] - w[0]).num_microseconds().unwrap() as f64 / 1e6;
            proptest::prop_assert!(gap >= 1.0 / eff - 1e-3, "gap {gap} < {}", 1.0 / eff);
        }
        // And it is not needlessly slow: n grants take about (n - 1) / rate.
        let total = (grants[n - 1] - grants[0]).num_microseconds().unwrap() as f64 / 1e6;
        proptest::prop_assert!(total <= (n as f64 - 1.0) / eff + 0.01 * n as f64 + 1e-3);
    }
}

// ----------------------------------------------------------------------------- Morgue

fn morgue_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("news-wire-edges-{name}-{}", std::process::id()))
}

#[test]
fn the_morgue_survives_concurrent_writers_and_refuses_damage() {
    let dir = morgue_dir("morgue");
    let m = Arc::new(Morgue::new(&dir));
    let bytes: Vec<u8> = (0..200_000u32).map(|i| (i % 253) as u8).collect();
    let sha: [u8; 32] = Sha256::digest(&bytes).into();
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let (m, bytes) = (m.clone(), bytes.clone());
            std::thread::spawn(move || m.put(&sha, &bytes).unwrap())
        })
        .collect();
    let paths: Vec<String> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert!(paths.windows(2).all(|w| w[0] == w[1]));
    assert_eq!(m.get(&sha).unwrap(), bytes);
    // No temp file is left behind.
    let leftovers: Vec<_> = std::fs::read_dir(m.path(&sha).parent().unwrap())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");

    // An empty body is a body: stored, hashed and read back.
    let empty: [u8; 32] = Sha256::digest([]).into();
    m.put(&empty, b"").unwrap();
    assert_eq!(m.get(&empty).unwrap(), b"");

    // A file cut short, and one that is not zstd at all, are errors, never panics or wrong bytes.
    let full = std::fs::read(m.path(&sha)).unwrap();
    std::fs::write(m.path(&sha), &full[..full.len() / 2]).unwrap();
    assert!(m.get(&sha).is_err());
    std::fs::write(m.path(&sha), b"not zstd").unwrap();
    assert!(m.get(&sha).is_err());
    // A hash that was never stored is NotFound.
    let other: [u8; 32] = Sha256::digest(b"never stored").into();
    assert_eq!(
        m.get(&other).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn retention_edges() {
    let t = t0();
    let days = |ct: Option<&str>| (morgue::retain_until(ct, t) - t).num_days();
    // Structured data: 400 days, whatever the case or parameters.
    for ct in [
        "application/json",
        "APPLICATION/JSON; charset=utf-8",
        "application/vnd.sdmx.data+json",
        "application/atom+xml",
        "application/xml",
        "text/xml",
        "text/csv",
        "text/plain",
    ] {
        assert_eq!(days(Some(ct)), 400, "{ct}");
    }
    // Documents, and anything we cannot place, 90.
    for ct in [
        "text/html",
        "application/xhtml+xml",
        "application/pdf",
        "image/png",
        "",
        "garbage",
    ] {
        assert_eq!(days(Some(ct)), 90, "{ct:?}");
    }
    assert_eq!(days(None), 90);
}

#[test]
fn the_bucket_refills_after_waiting_exactly_what_it_asked() {
    let mut now = t0();
    let mut b = TokenBucket::new(3.0, now);
    assert!(b.reserve(now).is_zero());
    for _ in 0..10 {
        let wait = b.reserve(now);
        if wait.is_zero() {
            return;
        }
        now += chrono::Duration::from_std(wait).unwrap();
    }
    panic!("still waiting after ten exact waits");
}
