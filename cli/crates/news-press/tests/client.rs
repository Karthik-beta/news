//! The write-API client against the scripted fake: no sockets, a virtual clock.

#![allow(clippy::unwrap_used)]

use std::sync::Arc;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use news_core::job::ErrorClass;
use news_press::api::{
    ApiRequest, Client, Failure, MAX_ATTEMPTS, Method, checked_origin, page_key,
};
use news_press::fake::{FakeApi, Fault};
use news_wire::pace::{Clock, VirtualClock};
use serde_json::json;

const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn put(page: &str, version: i32) -> ApiRequest {
    ApiRequest {
        method: Method::Put,
        path: format!("/v1/pages/{page}"),
        idempotency_key: page_key(page, version, SHA, "put_page").unwrap(),
        body: json!({"manifest": {"version": version}, "files": {}})
            .to_string()
            .into_bytes(),
    }
}

fn setup() -> (Arc<FakeApi>, Arc<VirtualClock>, Client) {
    let api = Arc::new(FakeApi::new());
    let clock = Arc::new(VirtualClock::new(
        Utc.with_ymd_and_hms(2026, 11, 23, 9, 0, 0).unwrap(),
    ));
    let client = Client::new(api.clone(), clock.clone());
    (api, clock, client)
}

fn sleeps(c: &VirtualClock) -> Vec<Duration> {
    c.sleeps.lock().unwrap().clone()
}

#[tokio::test]
async fn a_lost_answer_is_retried_with_the_same_key_and_writes_once() {
    let (api, _, client) = setup();
    api.script([Fault::LoseAnswer]);
    let a = client.send(&put("us-cpi", 1)).await;
    assert_eq!(a.attempts, 2);
    assert_eq!(a.result.unwrap()["status"], "published");
    let reqs = api.requests();
    assert_eq!(reqs.len(), 2);
    assert_eq!(reqs[0].idempotency_key, reqs[1].idempotency_key);
    assert_eq!(
        api.applied(),
        [page_key("us-cpi", 1, SHA, "put_page").unwrap()],
        "applied once"
    );
}

#[tokio::test]
async fn sending_the_same_page_again_replays_and_a_different_body_is_a_bug() {
    let (api, _, client) = setup();
    client.send(&put("us-cpi", 1)).await.result.unwrap();
    client.send(&put("us-cpi", 1)).await.result.unwrap();
    assert_eq!(api.applied().len(), 1, "the replay wrote nothing");
    let mut changed = put("us-cpi", 1);
    changed.body.push(b' ');
    let a = client.send(&changed).await;
    let Err(Failure::Dead { status, message }) = a.result else {
        panic!("expected a dead row");
    };
    assert_eq!(status, 409);
    assert!(message.contains("idempotency_key_reused"), "{message}");
}

#[tokio::test]
async fn a_5xx_is_retried_with_backoff_and_honours_retry_after() {
    let (api, clock, client) = setup();
    api.script([
        Fault::Status(503, "throttled", Some("7")),
        Fault::Status(502, "purge_failed", None),
        Fault::Status(500, "internal", None),
    ]);
    let a = client.send(&put("us-cpi", 1)).await;
    assert_eq!(a.attempts, 4);
    a.result.unwrap();
    // Retry-After 7 beats the 1 s backoff; then 2 s, then 4 s.
    assert_eq!(
        sleeps(&clock),
        [
            Duration::from_secs(7),
            Duration::from_secs(2),
            Duration::from_secs(4)
        ]
    );
    assert_eq!(api.applied().len(), 1);
}

#[tokio::test]
async fn a_busy_page_and_a_rate_limit_wait_and_retry() {
    let (api, clock, client) = setup();
    api.script([
        Fault::Status(409, "page_busy", Some("5")),
        Fault::Status(429, "rate_limited", Some("60")),
    ]);
    client.send(&put("us-cpi", 1)).await.result.unwrap();
    assert_eq!(
        sleeps(&clock),
        [Duration::from_secs(5), Duration::from_secs(60)]
    );
}

#[tokio::test]
async fn any_other_4xx_is_a_bug_and_is_never_retried() {
    for (status, code) in [
        (400, "invalid_bundle"),
        (409, "version_conflict"),
        (413, "payload_too_large"),
    ] {
        let (api, clock, client) = setup();
        api.script([Fault::Status(status, code, None)]);
        let a = client.send(&put("us-cpi", 1)).await;
        assert_eq!(a.attempts, 1, "{status}");
        assert_eq!(api.requests().len(), 1, "{status}");
        assert!(sleeps(&clock).is_empty());
        let failure = a.result.unwrap_err();
        assert_eq!(failure.job_failure().class, ErrorClass::Bug, "{status}");
        assert!(failure.message().contains(code), "{}", failure.message());
    }
}

#[tokio::test]
async fn giving_up_leaves_the_outcome_unknown_not_dead() {
    let (api, _, client) = setup();
    api.script((0..MAX_ATTEMPTS).map(|_| Fault::Unreachable));
    let a = client.send(&put("us-cpi", 1)).await;
    assert_eq!(a.attempts, MAX_ATTEMPTS);
    let failure = a.result.unwrap_err();
    assert!(matches!(failure, Failure::Unknown { .. }));
    assert_eq!(failure.job_failure().class, ErrorClass::Transient);
    assert!(api.applied().is_empty());
    // The next drain sends the same key again and it goes through.
    client.send(&put("us-cpi", 1)).await.result.unwrap();
    assert_eq!(api.applied().len(), 1);
}

#[tokio::test]
async fn pacing_is_5_a_second_with_a_burst_of_10_on_a_virtual_clock() {
    let (api, clock, client) = setup();
    let t0 = clock.now();
    for i in 0..10 {
        client
            .send(&put(&format!("page-{i}"), 1))
            .await
            .result
            .unwrap();
    }
    assert!(sleeps(&clock).is_empty(), "the burst of 10 goes at once");
    for i in 10..30 {
        client
            .send(&put(&format!("page-{i}"), 1))
            .await
            .result
            .unwrap();
    }
    // 20 more at 5 a second: 4 s, each 200 ms after the last.
    let ms = (clock.now() - t0).num_milliseconds();
    assert!((3995..=4005).contains(&ms), "{ms} ms");
    assert!(
        sleeps(&clock)
            .iter()
            .all(|d| (195..=205).contains(&d.as_millis()))
    );
    assert_eq!(api.applied().len(), 30);
    // After a pause the bucket has refilled (to 10, not more).
    clock.advance(Duration::from_secs(60));
    let before = sleeps(&clock).len();
    for i in 30..41 {
        client
            .send(&put(&format!("page-{i}"), 1))
            .await
            .result
            .unwrap();
    }
    assert_eq!(sleeps(&clock).len(), before + 1, "10 free, the 11th waits");
}

#[tokio::test]
async fn a_malformed_key_is_refused_by_the_api_as_a_bug() {
    let (_, _, client) = setup();
    let mut r = put("us-cpi", 1);
    r.idempotency_key = "bad key".into();
    let Err(Failure::Dead { status, .. }) = client.send(&r).await.result else {
        panic!("expected dead");
    };
    assert_eq!(status, 400);
    for op in ["put_page", "withdraw", "index_promote"] {
        let k = page_key("a-page", 12, SHA, op).unwrap();
        assert!(
            (8..=255).contains(&k.len())
                && k.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b":._-".contains(&b))
        );
    }
}

#[tokio::test]
async fn a_refused_key_is_not_a_dead_row_and_is_never_retried() {
    for status in [401, 403] {
        let (api, clock, client) = setup();
        api.script([Fault::Status(status, "unauthorized", None)]);
        let a = client.send(&put("us-cpi", 1)).await;
        assert_eq!(a.attempts, 1);
        assert!(sleeps(&clock).is_empty());
        assert!(matches!(a.result, Err(Failure::Auth { .. })), "{status}");
    }
}

#[test]
fn an_unknown_op_has_no_key_and_origins_are_checked_by_host_not_prefix() {
    assert!(page_key("a-page", 1, SHA, "home_curation").is_err());
    for ok in [
        "https://api.example.com",
        "https://api.example.com/",
        "http://localhost:8787",
        "http://127.0.0.1:8787",
        "http://[::1]:8787",
    ] {
        assert!(checked_origin(ok).is_ok(), "{ok}");
    }
    assert_eq!(
        checked_origin("https://api.example.com/").unwrap(),
        "https://api.example.com"
    );
    for bad in [
        "http://localhost@evil.example",
        "http://localhost.evil.example",
        "http://127.0.0.1.evil.example",
        "http://evil.example",
        "https://user:secret@api.example.com",
        "https://user@api.example.com",
        "ftp://localhost",
        "localhost",
    ] {
        let e = checked_origin(bad).unwrap_err().to_string();
        assert!(!e.contains("secret") && !e.contains("evil"), "{bad}: {e}");
    }
}
