//! The write API's curation route, and `news home publish` on top of it (G tier: Chief runs it as the
//! authorised step of a live run; the CLI form is always `gated`).
//!
//! The client is deliberately small and local to the home module: `PUT /v1/home/curation` with a bearer
//! key and an `Idempotency-Key`, and nothing else.
// shortcut: Press builds its own publish client for pages (#20, in parallel); when both have landed,
// fold this one into it (same bearer and Idempotency-Key headers, same retry classes) and keep only
// `put_curation` here.

use crate::check::{self, Failure};
use crate::draft::{body, ts};
use crate::facts;
use crate::store::{self, Stored};
use chrono::{DateTime, Utc};
use news_core::{NewsError, NewsResult};
use news_store::PgPool;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::future::Future;
use std::time::Duration;

/// `200` from `PUT /v1/home/curation` (`write-api.schema.json#/$defs/curationResponse`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CurationReply {
    pub version: i32,
    pub live_version: i32,
    pub purged: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ApiError {
    /// No answer, 429, 5xx (a failed purge is 502), or a request still in progress: send it again with
    /// the same key.
    Retry(String),
    /// The API answered and will not change its mind.
    Rejected {
        status: u16,
        code: String,
        message: String,
        details: Value,
    },
}

pub trait WriteApi {
    fn put_curation(
        &self,
        key: &str,
        bearer: &str,
        body: Vec<u8>,
    ) -> impl Future<Output = Result<CurationReply, ApiError>> + Send;
}

/// reqwest 0.13 is built with `rustls-no-provider` and Cargo unifies features, so every client in the
/// binary needs ring installed first. Idempotent; `news-wire` does the same for its own clients.
pub(crate) fn install_tls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// The write-API origin, checked: `https`, or `http` only for loopback (`127.0.0.1`, `::1`,
/// `localhost`, compared exactly), and no userinfo, query or fragment, so the bearer key can only go
/// to the host the setting names.
pub(crate) fn checked_origin(base: &str) -> NewsResult<reqwest::Url> {
    let url = reqwest::Url::parse(base)
        .map_err(|e| NewsError::usage(format!("the write-API URL is not a URL: {e}")))?;
    let loopback = matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "localhost"));
    let scheme_ok = url.scheme() == "https" || (url.scheme() == "http" && loopback);
    if !scheme_ok || url.host_str().is_none() {
        return Err(NewsError::usage(
            "the write-API URL must be https (http only for 127.0.0.1, ::1 or localhost)",
        ));
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(NewsError::usage(
            "the write-API URL must be an origin: no user, password, query or fragment",
        ));
    }
    Ok(url)
}

pub struct HttpWriteApi {
    client: reqwest::Client,
    base: String,
}

impl HttpWriteApi {
    /// `base` is the API origin, for example `https://api.news.karthik.nz` (see [`checked_origin`]).
    /// Redirects are never followed: a redirect would carry the bearer key to another host.
    pub fn new(base: &str) -> NewsResult<Self> {
        let url = checked_origin(base)?;
        install_tls_provider();
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| NewsError::internal(e.to_string()))?;
        Ok(Self {
            client,
            base: url.as_str().trim_end_matches('/').to_owned(),
        })
    }
}

impl WriteApi for HttpWriteApi {
    async fn put_curation(
        &self,
        key: &str,
        bearer: &str,
        body: Vec<u8>,
    ) -> Result<CurationReply, ApiError> {
        let resp = self
            .client
            .put(format!("{}/v1/home/curation", self.base))
            .bearer_auth(bearer)
            .header("idempotency-key", key)
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            // The text of a transport error names the URL, never the key.
            .map_err(|e| ApiError::Retry(format!("no answer: {}", e.without_url())))?;
        let status = resp.status().as_u16();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| ApiError::Retry(format!("answer cut short: {}", e.without_url())))?;
        let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        if status == 200 {
            return serde_json::from_value(json)
                .map_err(|e| ApiError::Retry(format!("unreadable answer: {e}")));
        }
        let code = json["error"]["code"].as_str().unwrap_or("").to_owned();
        let message = json["error"]["message"].as_str().unwrap_or("").to_owned();
        let in_progress = matches!(code.as_str(), "idempotency_in_progress" | "page_busy");
        if status == 429 || status >= 500 || in_progress {
            return Err(ApiError::Retry(format!("{status} {code}")));
        }
        Err(ApiError::Rejected {
            status,
            code,
            message,
            details: json["error"]["details"].clone(),
        })
    }
}

/// How hard to try: `attempts` sends, `pause` before the second, doubling after.
#[derive(Debug, Clone, Copy)]
pub struct Retry {
    pub attempts: u32,
    pub pause: Duration,
}

impl Default for Retry {
    fn default() -> Self {
        Self {
            attempts: 4,
            pause: Duration::from_secs(2),
        }
    }
}

/// `Idempotency-Key`: one draft and its exact bytes (8–255 of `A-Za-z0-9:._-`). A resend of the same
/// approved draft has the same bytes, so it is the same request and never a second version.
pub fn idempotency_key(draft_id: i64, body: &[u8]) -> String {
    let sha: String = Sha256::digest(body)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("home-curation:{draft_id}:{sha}")
}

fn rejected(status: u16, code: &str, message: &str, details: &Value) -> NewsError {
    let text = format!("the write API refused the curation ({status} {code}): {message}");
    let err = match status {
        400 => NewsError::validation(text),
        401 | 403 => NewsError::precondition(text).with_hint("check the write-API key in Board"),
        409 => NewsError::conflict(text),
        _ => NewsError::internal(text),
    };
    if details.is_null() {
        err
    } else {
        err.with_details(details.clone())
    }
}

fn failure_error(draft_id: i64, failures: &[Failure]) -> NewsError {
    NewsError::validation(format!(
        "draft {draft_id} no longer passes the checks ({} failures)",
        failures.len()
    ))
    .with_details(json!({"failures": failures}))
}

/// Publish an approved draft. `read_key` is called only when there is something to send, so a draft
/// that is not ready never touches the key. Pages are checked again first (one may have been
/// withdrawn since the preview); the approval time and results are then fixed, so a crash and a rerun
/// send the same bytes under the same key and the API answers with the version it already committed.
/// `now` is the approval time of a first send and the time the draft is marked published.
pub async fn publish<A: WriteApi>(
    db: &PgPool,
    draft_id: i64,
    now: DateTime<Utc>,
    api: &A,
    read_key: impl FnOnce() -> NewsResult<String>,
    retry: Retry,
) -> NewsResult<Value> {
    let mut st = store::get(db, draft_id).await?;
    if st.status == "published" {
        return Ok(json!({"draft_id": draft_id, "version": st.version, "already_published": true}));
    }
    if st.approved_at.is_none() {
        if !matches!(st.status.as_str(), "checked") {
            return Err(NewsError::precondition(format!(
                "draft {draft_id} is {}: run `news home check {draft_id}` until it passes",
                st.status
            )));
        }
        let pages = facts::load(db, &st.draft.page_ids()).await?;
        let report = check::run(&st.draft, &pages, now);
        if !report.pass {
            store::store_check(db, draft_id, &report, now).await?;
            return Err(failure_error(draft_id, &report.failures));
        }
        store::approve(db, draft_id, now, &report, now).await?;
        st = store::get(db, draft_id).await?;
    }
    let (Some(approved), true) = (st.approved_at, st.status == "approved") else {
        return Err(NewsError::precondition(format!(
            "draft {draft_id} is {}",
            st.status
        )));
    };
    let bytes = serde_json::to_vec(&body(&st.draft, st.created_at, approved, &st.results()))
        .map_err(|e| NewsError::internal(e.to_string()))?;
    let key = idempotency_key(draft_id, &bytes);
    let bearer = read_key()?;
    let reply = send(api, &key, &bearer, &bytes, retry).await?;
    store::mark_published(db, draft_id, reply.version, now).await?;
    Ok(published_json(draft_id, &key, &reply, &st))
}

fn published_json(draft_id: i64, key: &str, r: &CurationReply, st: &Stored) -> Value {
    json!({
        "draft_id": draft_id,
        "version": r.version,
        "live_version": r.live_version,
        "purged": r.purged,
        "idempotency_key": key,
        "approved_at": st.approved_at.map(ts),
    })
}

pub(crate) async fn send<A: WriteApi>(
    api: &A,
    key: &str,
    bearer: &str,
    bytes: &[u8],
    retry: Retry,
) -> NewsResult<CurationReply> {
    let mut pause = retry.pause;
    for attempt in 1..=retry.attempts.max(1) {
        match api.put_curation(key, bearer, bytes.to_vec()).await {
            Ok(r) => return Ok(r),
            Err(ApiError::Rejected {
                status,
                code,
                message,
                details,
            }) => return Err(rejected(status, &code, &message, &details)),
            Err(ApiError::Retry(why)) if attempt >= retry.attempts.max(1) => {
                return Err(NewsError::upstream(format!(
                    "the write API did not take the curation after {attempt} tries ({why})"
                ))
                .with_hint("run the step again: the same Idempotency-Key makes a resend safe"));
            }
            Err(ApiError::Retry(_)) => {
                tokio::time::sleep(pause).await;
                pause *= 2;
            }
        }
    }
    Err(NewsError::internal("no attempt was made"))
}
