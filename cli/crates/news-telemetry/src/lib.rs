//! Ticker for the `news` CLI (Bureau Design §3.13, §9.2).
//!
//! * [`Event`]: one row in ClickHouse `news.events` per CLI call, carrying `run_id` and `job_id` from
//!   `NEWS_RUN_ID` / `NEWS_JOB_ID` (§6.2). Written over HTTP to `NEWS_CH_HTTP` as the `news` user.
//! * [`init_tracing`]: logs to stderr (never stdout, which carries only the envelope), filtered by
//!   `NEWS_LOG` (default `warn`), plus spans exported over OTLP/HTTP to `NEWS_OTEL_HTTP` when set.
//!
//! * [`KINDS`] and [`handle`]: the analytics ingestion job, the daily Cloudflare Web Analytics pull
//!   into `traffic_daily` (`traffic.pull`; TD "Event sources": cloud usage pulled through their APIs).
//!   Ticker owns it; it was Board's until the name `news-board` went to the desktop app (#29).
//!
//! Telemetry never fails a command: when ClickHouse or the collector is down, the call still succeeds
//! and the problem is logged at `debug`. `NEWS_TELEMETRY=off` turns both off.
//!
//! The job handler is a skeleton: it is built in #16 and fails with class `bug` ("not built yet")
//! until then, so the Dispatch registry is complete.

use chrono::{DateTime, Utc};
use news_core::job::{Handler, JobFailure, JobFuture, JobInput};
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

/// The issue that builds this subsystem (Ticker's data jobs and Board's Analytics screen).
pub const ISSUE: &str = "#16";

/// Job kinds this subsystem handles (see `news_dispatch::EXTRA_KINDS`).
pub const KINDS: &[&str] = &["traffic.pull"];

/// Run one job.
pub fn handle(input: JobInput) -> JobFuture {
    Box::pin(async move { Err(JobFailure::not_built(&input.kind, ISSUE)) })
}

pub const HANDLER: Handler = handle;

pub const RUN_ID_VAR: &str = "NEWS_RUN_ID";
pub const JOB_ID_VAR: &str = "NEWS_JOB_ID";

/// One row of `news.events` (columns from `infra/clickhouse/002_events.sql` and `003_ticker.sql`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Event {
    #[serde(serialize_with = "ch_time")]
    pub ts: DateTime<Utc>,
    pub run_id: String,
    pub job_id: String,
    pub item_id: String,
    pub page_id: String,
    pub stage: String,
    pub event: String,
    pub status: String,
    pub error: String,
    pub error_class: String,
    /// The Dispatch job's attempt (0 when the call is not about one job attempt).
    pub attempt: u8,
    pub duration_ms: u32,
    pub preset: String,
    pub step: String,
    pub trace_id: String,
    pub span_id: String,
    /// JSON text: anything else (command, exit code, warnings, truncated).
    pub attrs: String,
}

fn ch_time<S: serde::Serializer>(t: &DateTime<Utc>, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&t.format("%Y-%m-%d %H:%M:%S%.3f").to_string())
}

impl Event {
    /// A CLI call event: `stage` = `cli`, `event` = the command path (`jobs.claim`), `run_id` and
    /// `job_id` from the environment.
    pub fn cli_call(command: &str, lookup: impl Fn(&str) -> Option<String>) -> Self {
        Self {
            ts: Utc::now(),
            run_id: lookup(RUN_ID_VAR).unwrap_or_default(),
            job_id: lookup(JOB_ID_VAR).unwrap_or_default(),
            item_id: String::new(),
            page_id: String::new(),
            stage: "cli".into(),
            event: command.into(),
            status: String::new(),
            error: String::new(),
            error_class: String::new(),
            attempt: 0,
            duration_ms: 0,
            preset: String::new(),
            step: String::new(),
            trace_id: String::new(),
            span_id: String::new(),
            attrs: "{}".into(),
        }
    }

    /// Record the job's error class and attempt, where the call knows them (`jobs fail`, `jobs
    /// heartbeat`, …). Attempts above 255 are stored as 255.
    pub fn with_job(mut self, error_class: Option<&str>, attempt: Option<i16>) -> Self {
        self.error_class = error_class.unwrap_or_default().to_owned();
        self.attempt = attempt.map_or(0, |a| u8::try_from(a.max(0)).unwrap_or(u8::MAX));
        self
    }

    /// Fill in the outcome: `status` is `ok` or the error code.
    pub fn finish(mut self, status: &str, error: &str, duration: Duration, attrs: &Value) -> Self {
        self.status = status.into();
        self.error = error.chars().take(500).collect();
        self.duration_ms = u32::try_from(duration.as_millis()).unwrap_or(u32::MAX);
        self.attrs = attrs.to_string();
        self
    }

    pub fn to_json_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// One row of `news.scores` (`infra/clickhouse/003_ticker.sql`): every score Nose computes for a run,
/// selected or not (Bureau §4.7).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScoreRow {
    #[serde(serialize_with = "ch_time")]
    pub ts: DateTime<Utc>,
    pub run_id: String,
    pub story_id: u64,
    pub scorer_version: String,
    /// JSON text: `{"I":…, "E":…, …}`.
    pub components: String,
    pub total: f64,
    pub selected: u8,
    /// The planned tier, 0 when not selected.
    pub tier: u8,
}

/// Where events go. Built from the environment; `None` when telemetry is off or not configured.
#[derive(Clone)]
pub struct ClickHouse {
    url: String,
    user: String,
    password: String,
}

impl std::fmt::Debug for ClickHouse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClickHouse")
            .field("url", &self.url)
            .field("user", &self.user)
            .finish_non_exhaustive()
    }
}

pub fn telemetry_off(lookup: &impl Fn(&str) -> Option<String>) -> bool {
    matches!(
        lookup("NEWS_TELEMETRY").as_deref(),
        Some("off" | "0" | "false")
    )
}

impl ClickHouse {
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Option<Self> {
        if telemetry_off(&lookup) {
            return None;
        }
        let url = lookup("NEWS_CH_HTTP").filter(|u| !u.is_empty())?;
        Some(Self {
            url: url.trim_end_matches('/').to_owned(),
            user: lookup("NEWS_CH_USER")
                .filter(|u| !u.is_empty())
                .unwrap_or_else(|| "news".into()),
            password: lookup("NEWS_CH_PASSWORD").unwrap_or_default(),
        })
    }

    pub fn from_env() -> Option<Self> {
        Self::from_lookup(|n| std::env::var(n).ok())
    }

    /// Insert events. Blocking, 2-second timeout; call it outside any async runtime.
    pub fn insert(&self, events: &[Event]) -> Result<(), String> {
        self.insert_rows("news.events", events)
    }

    /// Insert rows into one of our tables as JSONEachRow. Blocking, 2-second timeout; call it outside
    /// any async runtime (or in `spawn_blocking`).
    pub fn insert_rows<T: Serialize>(&self, table: &'static str, rows: &[T]) -> Result<(), String> {
        let body: String = rows
            .iter()
            .map(|r| serde_json::to_string(r).unwrap_or_default() + "\n")
            .collect();
        let query = format!("INSERT INTO {table} FORMAT JSONEachRow");
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|e| e.to_string())?;
        let res = client
            .post(&self.url)
            .query(&[
                ("query", query.as_str()),
                ("input_format_skip_unknown_fields", "1"),
                ("date_time_input_format", "best_effort"),
            ])
            .header("X-ClickHouse-User", &self.user)
            .header("X-ClickHouse-Key", &self.password)
            .body(body)
            .send()
            .map_err(|e| format!("ClickHouse unreachable: {}", e.without_url()))?;
        if res.status().is_success() {
            Ok(())
        } else {
            let status = res.status();
            let text: String = res.text().unwrap_or_default().chars().take(200).collect();
            Err(format!("ClickHouse returned {status}: {text}"))
        }
    }
}

/// Keeps the OTLP pipeline alive; flushes and shuts it down on drop. Drop it outside any async runtime.
pub struct TracingGuard {
    provider: Option<opentelemetry_sdk::trace::SdkTracerProvider>,
}

impl TracingGuard {
    pub fn otlp_enabled(&self) -> bool {
        self.provider.is_some()
    }
}

impl Drop for TracingGuard {
    fn drop(&mut self) {
        if let Some(p) = self.provider.take() {
            let _ = p.shutdown();
        }
    }
}

/// Install the global subscriber: stderr logs filtered by `NEWS_LOG`, plus OTLP spans to
/// `NEWS_OTEL_HTTP` (for example `http://127.0.0.1:4318`) when set and telemetry is on.
/// Call before starting the async runtime.
pub fn init_tracing() -> TracingGuard {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{EnvFilter, Layer};

    let lookup = |n: &str| std::env::var(n).ok();
    let filter = EnvFilter::try_from_env("NEWS_LOG").unwrap_or_else(|_| EnvFilter::new("warn"));
    let stderr = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_target(false)
        .with_filter(filter);

    let endpoint = (!telemetry_off(&lookup))
        .then(|| lookup("NEWS_OTEL_HTTP"))
        .flatten()
        .filter(|e| !e.is_empty());
    let provider = endpoint.and_then(|e| otlp_provider(&e));
    let otel_layer = provider.as_ref().map(|p| {
        use opentelemetry::trace::TracerProvider as _;
        tracing_opentelemetry::layer()
            .with_tracer(p.tracer("news-cli"))
            .with_filter(tracing_subscriber::filter::LevelFilter::INFO)
    });
    let _ = tracing_subscriber::registry()
        .with(stderr)
        .with(otel_layer)
        .try_init();
    TracingGuard { provider }
}

fn otlp_provider(endpoint: &str) -> Option<opentelemetry_sdk::trace::SdkTracerProvider> {
    use opentelemetry_otlp::{WithExportConfig, WithHttpConfig};
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .ok()?;
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_http_client(client)
        .with_endpoint(format!("{}/v1/traces", endpoint.trim_end_matches('/')))
        .with_timeout(Duration::from_secs(2))
        .build()
        .ok()?;
    let resource = opentelemetry_sdk::Resource::builder()
        .with_service_name("news-cli")
        .build();
    Some(
        opentelemetry_sdk::trace::SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(resource)
            .build(),
    )
}

/// The current span's trace and span ids as hex, or empty strings when OTLP is off.
pub fn current_ids(span: &tracing::Span) -> (String, String) {
    use opentelemetry::trace::TraceContextExt;
    use tracing_opentelemetry::OpenTelemetrySpanExt;
    let cx = span.context();
    let sc = cx.span().span_context().clone();
    if sc.is_valid() {
        (sc.trace_id().to_string(), sc.span_id().to_string())
    } else {
        (String::new(), String::new())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |n| {
            pairs
                .iter()
                .find(|(k, _)| *k == n)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[test]
    fn cli_event_carries_run_and_job_ids_from_the_environment() {
        let e = Event::cli_call(
            "jobs.claim",
            env(&[("NEWS_RUN_ID", "r-1"), ("NEWS_JOB_ID", "42")]),
        )
        .finish("ok", "", Duration::from_millis(12), &json!({"exit": 0}));
        let v: Value = serde_json::from_str(&e.to_json_line()).unwrap();
        assert_eq!(v["run_id"], "r-1");
        assert_eq!(v["job_id"], "42");
        assert_eq!(v["stage"], "cli");
        assert_eq!(v["event"], "jobs.claim");
        assert_eq!(v["duration_ms"], 12);
        assert_eq!(v["attrs"], r#"{"exit":0}"#);
        assert!(v["ts"].as_str().unwrap().len() == 23, "{}", v["ts"]);
    }

    #[test]
    fn job_fields_are_recorded_and_clamped() {
        let e = Event::cli_call("jobs.fail", env(&[])).with_job(Some("transient"), Some(2));
        let v: Value = serde_json::from_str(&e.to_json_line()).unwrap();
        assert_eq!(
            (v["error_class"].as_str(), v["attempt"].as_u64()),
            (Some("transient"), Some(2))
        );
        let e = Event::cli_call("x", env(&[])).with_job(None, Some(1000));
        assert_eq!((e.error_class.as_str(), e.attempt), ("", 255));
        assert_eq!(Event::cli_call("x", env(&[])).attempt, 0);
    }

    #[test]
    fn missing_ids_are_empty_strings() {
        let e = Event::cli_call("version", env(&[]));
        assert_eq!(e.run_id, "");
        assert_eq!(e.job_id, "");
    }

    #[test]
    fn clickhouse_needs_a_url_and_respects_the_off_switch() {
        assert!(ClickHouse::from_lookup(env(&[])).is_none());
        assert!(
            ClickHouse::from_lookup(env(&[("NEWS_CH_HTTP", "http://127.0.0.1:8123")])).is_some()
        );
        assert!(
            ClickHouse::from_lookup(env(&[
                ("NEWS_CH_HTTP", "http://127.0.0.1:8123"),
                ("NEWS_TELEMETRY", "off")
            ]))
            .is_none()
        );
    }

    #[test]
    fn debug_hides_the_password() {
        let ch = ClickHouse::from_lookup(env(&[
            ("NEWS_CH_HTTP", "http://127.0.0.1:8123"),
            ("NEWS_CH_PASSWORD", "hunter2"),
        ]))
        .unwrap();
        assert!(!format!("{ch:?}").contains("hunter2"));
    }

    /// A one-shot HTTP server standing in for ClickHouse (as in the CLI's contract tests): answers 200
    /// and returns the request it received.
    fn fake_clickhouse() -> (String, std::thread::JoinHandle<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = s.read(&mut chunk).unwrap_or(0);
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let len = text[..end]
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if buf.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            let _ =
                s.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
            String::from_utf8_lossy(&buf).into_owned()
        });
        (url, handle)
    }

    #[test]
    fn a_score_row_reaches_clickhouse_scores() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let (url, server) = fake_clickhouse();
        let ch =
            ClickHouse::from_lookup(move |n| (n == "NEWS_CH_HTTP").then(|| url.clone())).unwrap();
        let row = ScoreRow {
            ts: Utc::now(),
            run_id: "00000000-0000-4000-8000-000000000046".into(),
            story_id: 1,
            scorer_version: "v1".into(),
            components: r#"{"I":0.95,"M":0.528}"#.into(),
            total: 75.17,
            selected: 1,
            tier: 1,
        };
        ch.insert_rows("news.scores", &[row]).unwrap();
        let request = server.join().unwrap();
        assert!(
            request.starts_with("POST /?query=INSERT+INTO+news.scores+FORMAT+JSONEachRow"),
            "{request}"
        );
        let body = request.split("\r\n\r\n").nth(1).unwrap();
        let v: Value = serde_json::from_str(body.trim()).unwrap();
        assert_eq!(v["story_id"], 1);
        assert_eq!(v["scorer_version"], "v1");
        assert_eq!(v["components"], r#"{"I":0.95,"M":0.528}"#);
        assert_eq!(v["total"], 75.17);
        assert_eq!(
            (v["selected"].as_u64(), v["tier"].as_u64()),
            (Some(1), Some(1))
        );
        assert_eq!(v["ts"].as_str().unwrap().len(), 23);
    }

    #[test]
    fn long_errors_are_cut() {
        let e = Event::cli_call("x", env(&[])).finish(
            "internal",
            &"e".repeat(2000),
            Duration::ZERO,
            &json!({}),
        );
        assert_eq!(e.error.chars().count(), 500);
    }
}
