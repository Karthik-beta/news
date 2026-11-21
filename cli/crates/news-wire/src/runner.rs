//! The Runner: one shared, polite HTTP client (Bureau Design §3.2).
//!
//! Every connector request goes through [`Runner::fetch`]: daily quota, robots.txt (RFC 9309, cached
//! 24 h; unreachable or 5xx snoozes the job and is remembered only for minutes), one token bucket and
//! `max_concurrency` slots per source (across all its hosts: SEC's limit is per user, not per host)
//! inside the global `net` pool of 6, the User-Agent (with the contact e-mail only for sources
//! that require it), conditional GET, `Retry-After` (both forms → snooze), the content-type
//! allow-list, the size cap and a streaming SHA-256.
//!
//! The network sits behind [`Transport`], so all of this is tested against a scripted fake;
//! [`ReqwestTransport`] is the only implementation that opens sockets.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use news_connectors::Request;
use news_core::job::{ErrorClass, JobFailure};
use sha2::{Digest, Sha256};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::pace::{BoxFut, Clock, TokenBucket, retry_after};
use crate::robots::{self, Robots};

/// Size of the global `net` pool (Bureau §5.7).
pub const NET_POOL: usize = 6;
pub const PRODUCT: &str = "News";
const ROBOTS_TTL_HOURS: i64 = 24;
/// robots.txt unreachable or 5xx: snooze this long (unless it sent a `Retry-After`)…
const ROBOTS_RETRY: Duration = Duration::from_secs(5 * 60);
/// …and remember the miss at most this long.
const ROBOTS_MISS_TTL_MAX: Duration = Duration::from_secs(10 * 60);
/// A 403 from a source that requires a contact e-mail is its throttle (SEC): snooze this long.
const THROTTLE_403_SNOOZE: Duration = Duration::from_secs(10 * 60);
/// RFC 9309 asks parsers to read at least 500 KiB.
const ROBOTS_MAX: u64 = 512 * 1024;
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub const TOTAL_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub timeout: Duration,
}

pub trait Body: Send {
    /// The next chunk, `None` at the end.
    fn chunk(&mut self) -> BoxFut<'_, Result<Option<Vec<u8>>, TransportError>>;
}

pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Box<dyn Body>,
}

impl HttpResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct TransportError {
    pub message: String,
}

pub trait Transport: Send + Sync {
    fn get(&self, req: HttpRequest) -> BoxFut<'_, Result<HttpResponse, TransportError>>;
}

/// Install ring as the process-wide rustls provider. reqwest 0.13 is built with `rustls-no-provider` here
/// (its default provider, aws-lc, is not in the tree), and Cargo unifies features, so every reqwest
/// client in the binary, including the blocking Ticker and OTLP ones, needs this to have run first.
/// Idempotent: an already-installed provider is fine.
pub fn install_tls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// The live transport: reqwest with rustls, gzip and brotli, 10 s connect timeout.
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new() -> Result<Self, TransportError> {
        install_tls_provider();
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .map_err(|e| TransportError {
                message: e.to_string(),
            })?;
        Ok(Self { client })
    }
}

struct ReqwestBody(reqwest::Response);

impl Body for ReqwestBody {
    fn chunk(&mut self) -> BoxFut<'_, Result<Option<Vec<u8>>, TransportError>> {
        Box::pin(async move {
            self.0
                .chunk()
                .await
                .map(|c| c.map(|b| b.to_vec()))
                .map_err(|e| TransportError {
                    message: e.to_string(),
                })
        })
    }
}

impl Transport for ReqwestTransport {
    fn get(&self, req: HttpRequest) -> BoxFut<'_, Result<HttpResponse, TransportError>> {
        Box::pin(async move {
            let mut b = self.client.get(&req.url).timeout(req.timeout);
            for (k, v) in &req.headers {
                b = b.header(k, v);
            }
            let resp = b.send().await.map_err(|e| TransportError {
                // Never echo the URL: it may carry a key.
                message: e.without_url().to_string(),
            })?;
            let headers = resp
                .headers()
                .iter()
                .filter_map(|(k, v)| Some((k.as_str().to_owned(), v.to_str().ok()?.to_owned())))
                .collect();
            Ok(HttpResponse {
                status: resp.status().as_u16(),
                headers,
                body: Box::new(ReqwestBody(resp)),
            })
        })
    }
}

/// Politeness settings of one source, from its Contacts row.
#[derive(Debug, Clone, PartialEq)]
pub struct SourcePolicy {
    pub source_id: String,
    pub rate_per_sec: f64,
    /// Requests of this source in flight at once (Contacts `max_concurrency`, at least 1).
    pub max_concurrency: u32,
    pub daily_quota: Option<u32>,
    pub robots_respect: bool,
    /// URL prefixes (`https://host/path/`) not checked against robots.txt (Contacts `robots_exempt`).
    pub robots_exempt: Vec<String>,
    pub requires_contact_email: bool,
}

/// `ETag` / `Last-Modified` from the previous fetch of a URL (`fetch_state`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Validators {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    /// The URL without secrets.
    pub url: String,
    pub status: u16,
    pub content_type: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub sha256: [u8; 32],
    pub bytes: Vec<u8>,
    pub fetched_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// 304: nothing changed, nothing to parse.
    NotModified,
    Fetched(Fetched),
}

/// A failed fetch, already classed for Dispatch.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{class}: {message}")]
pub struct FetchError {
    pub class: ErrorClass,
    pub message: String,
    pub retry_after: Option<Duration>,
}

impl FetchError {
    fn new(class: ErrorClass, message: impl Into<String>) -> Self {
        Self {
            class,
            message: message.into(),
            retry_after: None,
        }
    }
}

impl From<FetchError> for JobFailure {
    fn from(e: FetchError) -> Self {
        let f = JobFailure::new(e.class, e.message);
        match e.retry_after {
            Some(d) => f.with_retry_after(d),
            None => f,
        }
    }
}

/// Looks up a secret by env var name.
pub type SecretLookup = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Runner configuration read from the environment. TODO(#10): read from Standing instead.
#[derive(Clone)]
pub struct RunnerConfig {
    /// `NEWS_CONTACT_EMAIL`: added to the User-Agent only for sources that require it.
    pub contact_email: Option<String>,
    /// Looks up a secret by env var name (free API keys). Never logged.
    pub secret: SecretLookup,
}

impl RunnerConfig {
    pub fn from_env() -> Self {
        Self {
            contact_email: std::env::var("NEWS_CONTACT_EMAIL")
                .ok()
                .filter(|v| !v.is_empty()),
            secret: Arc::new(|name| std::env::var(name).ok().filter(|v| !v.is_empty())),
        }
    }
}

/// A plausible e-mail with nothing that could break a header line.
fn valid_email(e: &str) -> bool {
    let ok_chars = e
        .bytes()
        .all(|b| b.is_ascii_graphic() && !matches!(b, b'(' | b')' | b'<' | b'>' | b';' | b','));
    let parts: Vec<&str> = e.split('@').collect();
    ok_chars && parts.len() == 2 && !parts[0].is_empty() && parts[1].contains('.')
}

pub fn host_of(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next()?;
    (!host.is_empty()).then_some(host)
}

fn path_of(url: &str) -> &str {
    url.split_once("://")
        .and_then(|(_, r)| r.find('/').map(|i| &r[i..]))
        .unwrap_or("/")
}

/// Per host: until when the entry holds, and the rules or the reason robots.txt was unavailable.
type RobotsEntry = (DateTime<Utc>, Result<Robots, FetchError>);

pub struct Runner {
    transport: Arc<dyn Transport>,
    clock: Arc<dyn Clock>,
    config: RunnerConfig,
    /// Per source: its token bucket and its `max_concurrency` slots.
    buckets: Mutex<HashMap<String, TokenBucket>>,
    slots: Mutex<HashMap<String, Arc<Semaphore>>>,
    robots: Mutex<HashMap<String, RobotsEntry>>,
    quota: Mutex<HashMap<(String, NaiveDate), u32>>,
    net: Semaphore,
}

impl Runner {
    pub fn new(transport: Arc<dyn Transport>, clock: Arc<dyn Clock>, config: RunnerConfig) -> Self {
        Self {
            transport,
            clock,
            config,
            buckets: Mutex::new(HashMap::new()),
            slots: Mutex::new(HashMap::new()),
            robots: Mutex::new(HashMap::new()),
            quota: Mutex::new(HashMap::new()),
            net: Semaphore::new(NET_POOL),
        }
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    /// `News/<version> (+https://news.karthik.nz/data-access)`, plus the contact e-mail for sources
    /// that require one.
    pub fn user_agent(&self, policy: &SourcePolicy) -> Result<String, FetchError> {
        let base = format!(
            "{PRODUCT}/{} (+https://news.karthik.nz/data-access)",
            news_core::VERSION
        );
        if !policy.requires_contact_email {
            return Ok(base);
        }
        match self.config.contact_email.as_deref() {
            Some(e) if valid_email(e) => Ok(format!("{base} {e}")),
            Some(_) => Err(FetchError::new(
                ErrorClass::Bug,
                "NEWS_CONTACT_EMAIL is not a valid e-mail address",
            )),
            None => Err(FetchError::new(
                ErrorClass::Bug,
                format!(
                    "{} requires a contact e-mail in the User-Agent: set NEWS_CONTACT_EMAIL",
                    policy.source_id
                ),
            )),
        }
    }

    /// One of the source's `max_concurrency` slots, held for the whole request.
    async fn slot(&self, policy: &SourcePolicy) -> Result<OwnedSemaphorePermit, FetchError> {
        let sem = self
            .slots
            .lock()
            .map_err(|_| FetchError::new(ErrorClass::Bug, "slot lock poisoned"))?
            .entry(policy.source_id.clone())
            .or_insert_with(|| Arc::new(Semaphore::new(policy.max_concurrency.max(1) as usize)))
            .clone();
        sem.acquire_owned()
            .await
            .map_err(|_| FetchError::new(ErrorClass::Bug, "source slots closed"))
    }

    /// Wait for a token from the source's bucket (one bucket per source, whatever the host).
    async fn pace(&self, policy: &SourcePolicy) {
        loop {
            let wait = match self.buckets.lock() {
                Ok(mut m) => m
                    .entry(policy.source_id.clone())
                    .or_insert_with(|| TokenBucket::new(policy.rate_per_sec, self.clock.now()))
                    .reserve(self.clock.now()),
                Err(_) => Duration::ZERO,
            };
            if wait.is_zero() {
                return;
            }
            self.clock.sleep(wait).await;
        }
    }

    fn penalise(&self, policy: &SourcePolicy) {
        if let Ok(mut m) = self.buckets.lock()
            && let Some(b) = m.get_mut(&policy.source_id)
        {
            b.penalise(self.clock.now());
        }
    }

    fn spend_quota(&self, policy: &SourcePolicy) -> Result<(), FetchError> {
        let Some(limit) = policy.daily_quota else {
            return Ok(());
        };
        let now = self.clock.now();
        let day = now.date_naive();
        let mut m = self
            .quota
            .lock()
            .map_err(|_| FetchError::new(ErrorClass::Bug, "quota lock poisoned"))?;
        let used = m.entry((policy.source_id.clone(), day)).or_insert(0);
        if *used >= limit {
            let tomorrow = day
                .succ_opt()
                .and_then(|d| d.and_hms_opt(0, 5, 0))
                .map(|t| t.and_utc())
                .unwrap_or(now);
            let mut e = FetchError::new(
                ErrorClass::RateLimited,
                format!("{} daily quota of {limit} spent", policy.source_id),
            );
            e.retry_after = (tomorrow - now).to_std().ok();
            return Err(e);
        }
        *used += 1;
        Ok(())
    }

    /// The robots.txt verdict for `url`. The file is cached for 24 h; when it cannot be read
    /// (unreachable or 5xx) the fetch is `RateLimited` with a retry-after, and that is cached only
    /// for a few minutes. Only a real `Disallow` blocks.
    async fn robots_allow(&self, policy: &SourcePolicy, url: &str) -> Result<bool, FetchError> {
        let host =
            host_of(url).ok_or_else(|| FetchError::new(ErrorClass::Bug, "URL has no host"))?;
        let now = self.clock.now();
        let cached = self.robots.lock().ok().and_then(|m| {
            m.get(host)
                .filter(|(until, _)| now < *until)
                .map(|(until, r)| (*until, r.clone()))
        });
        let rules = match cached {
            Some((_, Ok(r))) => r,
            Some((until, Err(mut e))) => {
                e.retry_after = Some((until - now).to_std().unwrap_or(Duration::ZERO));
                return Err(e);
            }
            None => {
                let scheme = url.split("://").next().unwrap_or("https");
                let got = self
                    .fetch_robots(policy, &format!("{scheme}://{host}/robots.txt"))
                    .await;
                let until = match &got {
                    Ok(_) => now + chrono::Duration::hours(ROBOTS_TTL_HOURS),
                    Err(e) => {
                        let ttl = e
                            .retry_after
                            .unwrap_or(ROBOTS_RETRY)
                            .min(ROBOTS_MISS_TTL_MAX);
                        now + chrono::Duration::from_std(ttl).unwrap_or_default()
                    }
                };
                if let Ok(mut m) = self.robots.lock() {
                    m.insert(host.to_owned(), (until, got.clone()));
                }
                got?
            }
        };
        Ok(rules.allowed(PRODUCT, path_of(url)))
    }

    /// robots.txt as rules; `Err(RateLimited)` when it is unreachable or answers 5xx (RFC 9309 §2.3.1.4
    /// treats that as temporarily unavailable, not as a licence decision).
    async fn fetch_robots(&self, policy: &SourcePolicy, url: &str) -> Result<Robots, FetchError> {
        let host = host_of(url).unwrap_or_default().to_owned();
        let _slot = self.slot(policy).await?;
        self.pace(policy).await;
        let req = HttpRequest {
            url: url.to_owned(),
            headers: vec![("User-Agent".into(), self.user_agent(policy)?)],
            timeout: TOTAL_TIMEOUT,
        };
        let _permit = self.net.acquire().await;
        let unavailable = |why: String, wait: Option<Duration>| {
            let mut e = FetchError::new(
                ErrorClass::RateLimited,
                format!("robots.txt on {host} unavailable ({why})"),
            );
            e.retry_after = Some(wait.unwrap_or(ROBOTS_RETRY));
            e
        };
        let mut resp = match self.transport.get(req).await {
            Ok(r) => r,
            Err(e) => return Err(unavailable(e.message, None)),
        };
        Ok(match resp.status {
            200..=299 => {
                let mut body = Vec::new();
                while let Ok(Some(c)) = resp.body.chunk().await {
                    body.extend_from_slice(&c);
                    if body.len() as u64 > ROBOTS_MAX {
                        break;
                    }
                }
                robots::parse(&String::from_utf8_lossy(&body))
            }
            400..=499 => Robots::AllowAll,
            s => {
                let wait = resp
                    .header("Retry-After")
                    .and_then(|v| retry_after(v, self.clock.now()));
                return Err(unavailable(format!("HTTP {s}"), wait));
            }
        })
    }

    /// Fetch one request politely. A 304 is [`Outcome::NotModified`]: no bytes, nothing to parse.
    pub async fn fetch(
        &self,
        policy: &SourcePolicy,
        req: &Request,
        validators: Option<&Validators>,
    ) -> Result<Outcome, FetchError> {
        let host = host_of(&req.url)
            .ok_or_else(|| FetchError::new(ErrorClass::Bug, "URL has no host"))?
            .to_owned();
        let ua = self.user_agent(policy)?;
        let exempt = policy
            .robots_exempt
            .iter()
            .any(|prefix| req.url.starts_with(prefix.as_str()));
        if policy.robots_respect && !exempt && !self.robots_allow(policy, &req.url).await? {
            return Err(FetchError::new(
                ErrorClass::LicenceBlocked,
                format!("robots.txt disallows {}", path_of(&req.url)),
            ));
        }
        let mut url = req.url.clone();
        if let Some(s) = &req.secret {
            let value = (self.config.secret)(&s.env).ok_or_else(|| {
                FetchError::new(
                    ErrorClass::Bug,
                    format!("{} needs the free key in {}", policy.source_id, s.env),
                )
            })?;
            url.push(if url.contains('?') { '&' } else { '?' });
            url.push_str(&s.param);
            url.push('=');
            url.push_str(&value);
        }
        self.spend_quota(policy)?;
        // Slot first, then the token: with one slot, requests stay spaced by the bucket.
        let _slot = self.slot(policy).await?;
        self.pace(policy).await;
        let mut headers = vec![
            ("User-Agent".to_owned(), ua),
            ("Accept".to_owned(), req.accept.join(", ")),
            ("Accept-Encoding".to_owned(), "gzip, br".to_owned()),
        ];
        if let Some(v) = validators {
            if let Some(e) = &v.etag {
                headers.push(("If-None-Match".into(), e.clone()));
            }
            if let Some(lm) = &v.last_modified {
                headers.push(("If-Modified-Since".into(), lm.clone()));
            }
        }
        let _permit = self.net.acquire().await;
        let mut resp = self
            .transport
            .get(HttpRequest {
                url,
                headers,
                timeout: TOTAL_TIMEOUT,
            })
            .await
            .map_err(|e| FetchError::new(ErrorClass::Transient, e.message))?;
        let now = self.clock.now();
        match resp.status {
            304 => return Ok(Outcome::NotModified),
            429 | 503 => {
                self.penalise(policy);
                let wait = resp
                    .header("Retry-After")
                    .and_then(|v| retry_after(v, now))
                    .unwrap_or(Duration::from_secs(60));
                let mut e = FetchError::new(
                    ErrorClass::RateLimited,
                    format!("HTTP {} from {host}", resp.status),
                );
                e.retry_after = Some(wait);
                return Err(e);
            }
            200..=299 => {}
            403 if policy.requires_contact_email => {
                // SEC's answer to too many requests from one User-Agent: back off, don't discard.
                self.penalise(policy);
                let mut e = FetchError::new(
                    ErrorClass::RateLimited,
                    format!("HTTP 403 from {host} (throttled)"),
                );
                e.retry_after = Some(THROTTLE_403_SNOOZE);
                return Err(e);
            }
            404 | 410 => {
                return Err(FetchError::new(
                    ErrorClass::UpstreamChanged,
                    format!("HTTP {} for {}", resp.status, req.url),
                ));
            }
            500..=599 => {
                return Err(FetchError::new(
                    ErrorClass::Transient,
                    format!("HTTP {} from {host}", resp.status),
                ));
            }
            s => {
                return Err(FetchError::new(
                    ErrorClass::InvalidData,
                    format!("HTTP {s} for {}", req.url),
                ));
            }
        }
        let content_type = resp.header("Content-Type").map(str::to_owned);
        let essence = content_type
            .as_deref()
            .and_then(|c| c.split(';').next())
            .map(|c| c.trim().to_ascii_lowercase())
            .unwrap_or_default();
        if !req.accept.contains(&essence) {
            return Err(FetchError::new(
                ErrorClass::InvalidData,
                format!("content type {essence:?} is not one of {:?}", req.accept),
            ));
        }
        if let Some(len) = resp
            .header("Content-Length")
            .and_then(|v| v.trim().parse::<u64>().ok())
            && len > req.max_bytes
        {
            return Err(FetchError::new(
                ErrorClass::InvalidData,
                format!("body of {len} bytes is over the {} byte cap", req.max_bytes),
            ));
        }
        let mut hasher = Sha256::new();
        let mut bytes = Vec::new();
        loop {
            match resp.body.chunk().await {
                Ok(Some(c)) => {
                    if (bytes.len() + c.len()) as u64 > req.max_bytes {
                        return Err(FetchError::new(
                            ErrorClass::InvalidData,
                            format!("body is over the {} byte cap (aborted)", req.max_bytes),
                        ));
                    }
                    hasher.update(&c);
                    bytes.extend_from_slice(&c);
                }
                Ok(None) => break,
                Err(e) => return Err(FetchError::new(ErrorClass::Transient, e.message)),
            }
        }
        Ok(Outcome::Fetched(Fetched {
            url: req.url.clone(),
            status: resp.status,
            content_type,
            etag: resp.header("ETag").map(str::to_owned),
            last_modified: resp.header("Last-Modified").map(str::to_owned),
            sha256: hasher.finalize().into(),
            bytes,
            fetched_at: now,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_split_into_host_and_path() {
        assert_eq!(
            host_of("https://api.bls.gov/publicAPI/v2/x?y=1"),
            Some("api.bls.gov")
        );
        assert_eq!(
            path_of("https://api.bls.gov/publicAPI/v2/x?y=1"),
            "/publicAPI/v2/x?y=1"
        );
        assert_eq!(path_of("https://h"), "/");
        assert_eq!(host_of("nonsense"), None);
    }

    #[test]
    fn emails_that_could_break_a_header_are_refused() {
        assert!(valid_email("desk@example.org"));
        assert!(!valid_email("desk@example.org\r\nX: y"));
        assert!(!valid_email("no-at-sign"));
        assert!(!valid_email("a@b"));
        assert!(!valid_email("(a)@b.org"));
    }
}
