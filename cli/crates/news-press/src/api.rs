//! The write-API client (Bureau Design §3.12, `contracts/cloud/write-api.openapi.json`).
//!
//! Every write carries an `Idempotency-Key`, so a send that got no answer is simply sent again: the API
//! returns the first result unchanged or finishes the job. [`Client`] paces every request (a token
//! bucket of 5 a second, bursts of 10), retries 5xx, 429, `page_busy` and lost connections with
//! backoff, and treats any other 4xx as a `bug`: the bundle or the code is wrong, and sending it again
//! cannot help. The network sits behind [`Api`]; [`ReqwestApi`] is the only implementation that opens
//! sockets and the only one that holds the key.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use news_core::NewsError;
use news_core::job::{ErrorClass, JobFailure};
use news_wire::pace::{BoxFut, Clock, retry_after};
use serde_json::Value;

/// Pages a second the bucket refills at (below DynamoDB's 25 write units, ADR 0004).
pub const RATE_PER_SEC: f64 = 5.0;
/// The bucket's size: a burst of this many requests goes at once.
pub const BURST: f64 = 10.0;
/// Sends per request before the outcome is left unknown for the next drain.
pub const MAX_ATTEMPTS: u32 = 5;
const BACKOFF_CAP: Duration = Duration::from_secs(30);
const WAIT_CAP: Duration = Duration::from_secs(300);
const TOTAL_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Put,
    Post,
}

#[derive(Debug, Clone)]
pub struct ApiRequest {
    pub method: Method,
    /// `/v1/pages/<id>`, `/v1/taxonomy`, …
    pub path: String,
    pub idempotency_key: String,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ApiResponse {
    pub status: u16,
    pub retry_after: Option<String>,
    pub body: Vec<u8>,
}

/// No answer: a timeout, a refused or dropped connection. The request may or may not have been applied.
#[derive(Debug, Clone)]
pub struct ApiError(pub String);

pub trait Api: Send + Sync {
    fn send(&self, req: ApiRequest) -> BoxFut<'_, Result<ApiResponse, ApiError>>;
}

/// Why a request ended without a 2xx.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// A 4xx the API will refuse every time. Class `bug`; the outbox row goes on the dead list.
    Dead { status: u16, message: String },
    /// Still failing after the attempts (5xx, 429, busy, no answer). The write may have been applied;
    /// the same key is sent again later and the API answers once.
    Unknown { message: String },
}

impl Failure {
    pub fn message(&self) -> &str {
        match self {
            Failure::Dead { message, .. } | Failure::Unknown { message } => message,
        }
    }

    pub fn job_failure(&self) -> JobFailure {
        match self {
            Failure::Dead { .. } => JobFailure::new(ErrorClass::Bug, self.message()),
            Failure::Unknown { .. } => JobFailure::new(ErrorClass::Transient, self.message()),
        }
    }

    pub fn error(&self) -> NewsError {
        match self {
            Failure::Dead { .. } => NewsError::internal(self.message()),
            Failure::Unknown { .. } => NewsError::upstream(self.message()),
        }
    }
}

/// The result of one request and how many sends it took.
#[derive(Debug)]
pub struct Attempt {
    pub attempts: u32,
    pub result: Result<Value, Failure>,
}

/// A bucket of [`BURST`] tokens refilled at [`RATE_PER_SEC`].
#[derive(Debug)]
struct Bucket {
    tokens: f64,
    last: DateTime<Utc>,
}

impl Bucket {
    /// Take a token now (`ZERO`) or say how long to wait before asking again.
    fn reserve(&mut self, now: DateTime<Utc>) -> Duration {
        let elapsed = (now - self.last).num_microseconds().unwrap_or(0).max(0) as f64 / 1e6;
        self.tokens = (self.tokens + elapsed * RATE_PER_SEC).min(BURST);
        self.last = now;
        if self.tokens >= 1.0 - 1e-9 {
            self.tokens -= 1.0;
            Duration::ZERO
        } else {
            Duration::from_secs_f64((1.0 - self.tokens) / RATE_PER_SEC)
        }
    }
}

pub struct Client {
    api: Arc<dyn Api>,
    clock: Arc<dyn Clock>,
    bucket: Mutex<Bucket>,
}

impl Client {
    pub fn new(api: Arc<dyn Api>, clock: Arc<dyn Clock>) -> Self {
        let now = clock.now();
        Self {
            api,
            clock,
            bucket: Mutex::new(Bucket {
                tokens: BURST,
                last: now,
            }),
        }
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    async fn pace(&self) {
        loop {
            let wait = self
                .bucket
                .lock()
                .map(|mut b| b.reserve(self.clock.now()))
                .unwrap_or_default();
            if wait.is_zero() {
                return;
            }
            self.clock.sleep(wait).await;
        }
    }

    /// Send one write, retrying what a retry can fix. Never gives up on the key: the caller keeps the
    /// outbox row and sends it again on `Unknown`.
    pub async fn send(&self, req: &ApiRequest) -> Attempt {
        let mut last = String::new();
        for n in 1..=MAX_ATTEMPTS {
            self.pace().await;
            let backoff = Duration::from_secs(1u64 << (n - 1).min(5)).min(BACKOFF_CAP);
            let wait = match self.api.send(req.clone()).await {
                Ok(r) if (200..300).contains(&r.status) => {
                    let body = serde_json::from_slice(&r.body).unwrap_or(Value::Null);
                    return Attempt {
                        attempts: n,
                        result: Ok(body),
                    };
                }
                Ok(r) => {
                    let (code, message) = error_of(&r.body);
                    last = format!("{} {code}: {message}", r.status);
                    let busy = r.status == 409
                        && matches!(code.as_str(), "page_busy" | "idempotency_in_progress");
                    if !(busy || r.status == 408 || r.status == 429 || r.status >= 500) {
                        return Attempt {
                            attempts: n,
                            result: Err(Failure::Dead {
                                status: r.status,
                                message: last,
                            }),
                        };
                    }
                    let asked = r
                        .retry_after
                        .as_deref()
                        .and_then(|v| retry_after(v, self.clock.now()))
                        .unwrap_or_default();
                    asked.max(backoff).min(WAIT_CAP)
                }
                Err(e) => {
                    last = format!("no answer: {}", e.0);
                    backoff
                }
            };
            if n < MAX_ATTEMPTS {
                self.clock.sleep(wait).await;
            }
        }
        Attempt {
            attempts: MAX_ATTEMPTS,
            result: Err(Failure::Unknown { message: last }),
        }
    }
}

/// `error.code` and `error.message` of an Error body (the API's contract), or placeholders.
fn error_of(body: &[u8]) -> (String, String) {
    let v: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    let text = |k: &str| v["error"][k].as_str().unwrap_or("unknown").to_owned();
    (text("code"), text("message"))
}

/// The idempotency key of a page's put (Bureau §5.8): `<page_id>:<version>:<bundle_sha256>`. Withdrawals
/// and index changes add a suffix, so each op has its own key and stored result.
pub fn page_key(page_id: &str, version: i32, sha_hex: &str, op: &str) -> String {
    match op {
        "put_page" => format!("{page_id}:{version}:{sha_hex}"),
        "withdraw" => format!("{page_id}:{version}:{sha_hex}:withdraw"),
        _ => format!("{page_id}:{version}:{sha_hex}:index"),
    }
}

/// The live client: reqwest with rustls, no redirects (the bearer key never follows one), the key read
/// once from Credential Manager when it is built. Only
/// Press's G-tier steps build one.
pub struct ReqwestApi {
    http: reqwest::Client,
    origin: String,
    key: String,
}

/// The api's origin, for example `https://api.news.karthik.nz` (the host is picked in #21).
pub const ORIGIN_VAR: &str = "NEWS_API_ORIGIN";

impl ReqwestApi {
    pub fn from_credentials(origin: &str) -> Result<Self, NewsError> {
        let origin = origin.trim_end_matches('/').to_owned();
        let local = ["http://127.0.0.1", "http://localhost", "http://[::1]"]
            .iter()
            .any(|p| origin.starts_with(p));
        if !origin.starts_with("https://") && !local {
            return Err(NewsError::usage(format!(
                "{ORIGIN_VAR} must be an https:// address (got {origin:?}); the write key is never sent in the clear"
            )));
        }
        let key = news_core::secrets::write_api_key()?;
        news_wire::runner::install_tls_provider();
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(TOTAL_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| NewsError::internal(format!("cannot build the HTTP client: {e}")))?;
        Ok(Self { http, origin, key })
    }
}

impl Api for ReqwestApi {
    fn send(&self, req: ApiRequest) -> BoxFut<'_, Result<ApiResponse, ApiError>> {
        Box::pin(async move {
            let url = format!("{}{}", self.origin, req.path);
            let mut bearer =
                reqwest::header::HeaderValue::from_str(&format!("Bearer {}", self.key))
                    .map_err(|_| ApiError("the write key is not a valid header value".into()))?;
            bearer.set_sensitive(true);
            let builder = match req.method {
                Method::Put => self.http.put(url),
                Method::Post => self.http.post(url),
            };
            let resp = builder
                .header(reqwest::header::AUTHORIZATION, bearer)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .header("Idempotency-Key", &req.idempotency_key)
                .body(req.body)
                .send()
                .await
                // Never echo the URL or headers.
                .map_err(|e| ApiError(e.without_url().to_string()))?;
            let status = resp.status().as_u16();
            let retry_after = resp
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let body = resp
                .bytes()
                .await
                .map_err(|e| ApiError(e.without_url().to_string()))?
                .to_vec();
            Ok(ApiResponse {
                status,
                retry_after,
                body,
            })
        })
    }
}
