//! The write-API client and the preview push against a loopback fake server (no network).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::api::{ApiError, HttpWriteApi, WriteApi};
use crate::preview;
use news_core::ErrorCode;
use serde_json::json;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::time::Duration;

/// What the fake server saw: the request line, the lower-cased headers and the body.
struct Seen {
    line: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

/// (status, headers, body) of one canned answer.
type Answer = (u16, Vec<(&'static str, String)>, String);

/// A loopback server that answers its connections, one answer each, and returns what it saw.
fn serve(answers: Vec<Answer>) -> (String, std::thread::JoinHandle<Vec<Seen>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for (status, reply_headers, text) in answers {
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            let (end, len) = loop {
                let n = s.read(&mut chunk).unwrap();
                assert!(n > 0, "connection closed early");
                buf.extend_from_slice(&chunk[..n]);
                if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&buf[..end]).to_ascii_lowercase();
                    let len = head
                        .lines()
                        .find_map(|l| {
                            l.strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if buf.len() >= end + 4 + len {
                        break (end, len);
                    }
                }
            };
            let head = String::from_utf8_lossy(&buf[..end]).to_string();
            let mut lines = head.lines();
            let line = lines.next().unwrap().to_owned();
            let headers = lines
                .filter_map(|l| l.split_once(':'))
                .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
                .collect();
            seen.push(Seen {
                line,
                headers,
                body: buf[end + 4..end + 4 + len].to_vec(),
            });
            let extra: String = reply_headers
                .iter()
                .map(|(k, v)| format!("{k}: {v}\r\n"))
                .collect();
            let reply = format!(
                "HTTP/1.1 {status} X\r\ncontent-length: {}\r\nconnection: close\r\n{extra}\r\n{text}",
                text.len()
            );
            s.write_all(reply.as_bytes()).unwrap();
        }
        seen
    });
    (url, handle)
}

fn error_body(code: &str) -> String {
    json!({"error": {"code": code, "message": "m"}}).to_string()
}

#[tokio::test]
async fn the_http_client_sends_the_key_and_the_body_and_classifies_answers() {
    let ok = json!({"version": 7, "live_version": 7, "purged": ["https://news.karthik.nz/"]});
    let invalid = json!({"error": {"code": "invalid_curation", "message": "m",
        "details": [{"code": "page_withdrawn", "message": "x"}]}});
    let (url, server) = serve(vec![
        (200, vec![], ok.to_string()),
        (502, vec![], error_body("purge_failed")),
        (409, vec![], error_body("idempotency_in_progress")),
        (400, vec![], invalid.to_string()),
    ]);
    let api = HttpWriteApi::new(&format!("{url}/")).unwrap();
    let r = api
        .put_curation("home-curation:1:abc", "the-key", br#"{"a":1}"#.to_vec())
        .await
        .unwrap();
    assert_eq!((r.version, r.live_version), (7, 7));
    for _ in 0..2 {
        let r = api.put_curation("k", "the-key", b"{}".to_vec()).await;
        assert!(matches!(r, Err(ApiError::Retry(_))), "{r:?}");
    }
    match api.put_curation("k", "the-key", b"{}".to_vec()).await {
        Err(ApiError::Rejected {
            status: 400,
            code,
            details,
            ..
        }) => {
            assert_eq!(code, "invalid_curation");
            assert_eq!(details[0]["code"], "page_withdrawn");
        }
        other => panic!("{other:?}"),
    }
    let seen = server.join().unwrap();
    assert_eq!(seen[0].line, "PUT /v1/home/curation HTTP/1.1");
    assert_eq!(seen[0].headers["authorization"], "Bearer the-key");
    assert_eq!(seen[0].headers["idempotency-key"], "home-curation:1:abc");
    assert_eq!(seen[0].headers["content-type"], "application/json");
    assert_eq!(seen[0].body, br#"{"a":1}"#);
}

#[tokio::test]
async fn nothing_answering_is_retryable_and_never_shows_the_key() {
    // Bind and drop a listener: its port is free, so the connection is refused.
    let url = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}", l.local_addr().unwrap())
    };
    let api = HttpWriteApi::new(&url).unwrap();
    match api.put_curation("k", "the-key", b"{}".to_vec()).await {
        Err(ApiError::Retry(why)) => assert!(!why.contains("the-key"), "{why}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_preview_has_four_views_of_one_url() {
    let v = preview::views("http://127.0.0.1:4321/");
    assert_eq!(v.len(), 4);
    let combos: Vec<(String, String)> = v
        .iter()
        .map(|x| {
            (
                x["theme"].as_str().unwrap().to_owned(),
                x["size"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    for theme in ["light", "dark"] {
        for size in ["phone", "desktop"] {
            assert!(
                combos.contains(&(theme.into(), size.into())),
                "{theme} {size}"
            );
        }
    }
    assert!(v.iter().all(|x| x["url"] == "http://127.0.0.1:4321/"));
    assert!(v.iter().any(|x| x["size"] == "phone" && x["width"] == 390));
}

#[tokio::test]
async fn the_draft_is_pushed_and_the_server_must_hold_exactly_those_bytes() {
    let bytes = br#"{"schema":"curation.v1"}"#.to_vec();
    let sha = preview::sha256_hex(&bytes);
    let (url, server) = serve(vec![
        (204, vec![], String::new()),
        (200, vec![("x-news-preview", sha.clone())], "<html>".into()),
    ]);
    assert_eq!(preview::push(&url, bytes.clone()).await.unwrap(), sha);
    let seen = server.join().unwrap();
    assert_eq!(seen[0].line, "PUT /__preview/curation HTTP/1.1");
    assert_eq!(seen[0].body, bytes);
    assert_eq!(seen[1].line, "GET / HTTP/1.1");

    // A server that shows another draft is an error.
    let wrong = ("x-news-preview", "0".repeat(64));
    let (url, _s) = serve(vec![
        (204, vec![], String::new()),
        (200, vec![wrong], String::new()),
    ]);
    let err = preview::push(&url, bytes.clone()).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::Internal);
    // A build without the preview route says how to start one.
    let (url, _s) = serve(vec![(404, vec![], "Not found".into())]);
    let err = preview::push(&url, bytes.clone()).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::Precondition);
    // A refused draft is a validation failure.
    let (url, _s) = serve(vec![(400, vec![], "the draft is not curation.v1".into())]);
    let err = preview::push(&url, bytes).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::ValidationFailed);
}

#[tokio::test]
async fn the_preview_server_must_be_on_loopback() {
    let err = preview::ensure_server("https://news.karthik.nz")
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::Usage);
}

#[test]
fn only_https_or_exact_loopback_origins_are_accepted() {
    for ok in [
        "https://api.news.karthik.nz",
        "https://api.news.karthik.nz/",
        "http://127.0.0.1:8787",
        "http://localhost:8787",
        "http://[::1]:8787",
    ] {
        assert!(HttpWriteApi::new(ok).is_ok(), "{ok}");
    }
    for bad in [
        "http://localhost@evil.example",
        "http://localhost.evil.example",
        "http://evil.example/localhost",
        "http://x",
        "http://api.news.karthik.nz",
        "https://user:pw@api.news.karthik.nz",
        "https://user@api.news.karthik.nz",
        "https://api.news.karthik.nz?a=1",
        "https://api.news.karthik.nz#f",
        "ftp://127.0.0.1",
        "not a url",
    ] {
        let err = HttpWriteApi::new(bad)
            .err()
            .unwrap_or_else(|| panic!("{bad}"));
        assert_eq!(err.code, ErrorCode::Usage, "{bad}");
    }
}

#[tokio::test]
async fn a_redirect_is_not_followed_and_the_key_goes_nowhere_else() {
    // The second server would receive the key if the redirect were followed; it is never connected to.
    let other = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    other.set_nonblocking(true).unwrap();
    let target = format!("http://{}/steal", other.local_addr().unwrap());
    let (url, server) = serve(vec![(307, vec![("location", target)], String::new())]);
    let api = HttpWriteApi::new(&url).unwrap();
    match api.put_curation("k", "the-key", b"{}".to_vec()).await {
        Err(ApiError::Rejected { status: 307, .. }) => {}
        other => panic!("{other:?}"),
    }
    assert_eq!(server.join().unwrap().len(), 1);
    assert!(other.accept().is_err(), "the redirect target was contacted");
}

#[tokio::test]
async fn the_preview_push_stays_on_loopback_and_does_not_follow_redirects() {
    let err = preview::push("http://evil.example", b"{}".to_vec())
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::Usage);
    let other = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    other.set_nonblocking(true).unwrap();
    let target = format!("http://{}/steal", other.local_addr().unwrap());
    let (url, server) = serve(vec![(307, vec![("location", target)], String::new())]);
    assert!(preview::push(&url, b"{}".to_vec()).await.is_err());
    server.join().unwrap();
    assert!(other.accept().is_err(), "the redirect target was contacted");
}
