//! The client for Plate, the warm Bun render server (`site/src/renderer/plate.ts`; ADR 0007). The `render` pool's
//! two slots share one server on 127.0.0.1.
//!
//! `NEWS_PLATE_URL` names a running Plate (the Bureau service keeps one warm). Without it, a call starts its own
//! Plate (`bun …/plate.ts --port 0 --lifeline`) for as long as the [`Plate`] value lives: the child picks a free
//! port, and exits when its stdin closes, so it never outlives the command that started it.

use crate::render::Rendered;
use crate::validate::Checked;
use news_core::{NewsError, NewsResult};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::{Child, Command};

pub const URL_VAR: &str = "NEWS_PLATE_URL";
pub const SCRIPT_VAR: &str = "NEWS_PLATE_SCRIPT";
const SCRIPT_REL: &str = "site/src/renderer/plate.ts";
/// Loading React, OpenUI and the component library takes a few seconds on a busy laptop.
const START_TIMEOUT: Duration = Duration::from_secs(90);
const CALL_TIMEOUT: Duration = Duration::from_secs(60);

pub struct Plate {
    base: String,
    http: reqwest::Client,
    /// The Plate this value started, if any; killed when dropped.
    _child: Option<Child>,
}

fn unavailable(message: impl Into<String>) -> NewsError {
    NewsError::precondition(message).with_hint(format!(
        "start Plate (`bun run plate` in site/) and set {URL_VAR}, or install Bun so news can start one"
    ))
}

/// `NEWS_PLATE_URL` without its trailing slash. Plate serves pages to this machine only, so a URL that is not
/// http on 127.0.0.1, localhost or [::1] is refused rather than sent Copies.
fn loopback_base(url: &str) -> NewsResult<String> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|e| NewsError::usage(format!("{URL_VAR} {url:?} is not a URL: {e}")))?;
    if parsed.scheme() != "http"
        || !matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))
    {
        return Err(NewsError::usage(format!(
            "{URL_VAR} {url:?} is not a loopback address: Plate runs on 127.0.0.1, localhost or [::1]"
        )));
    }
    Ok(url.trim_end_matches('/').to_string())
}

/// `plate.ts` in the nearest ancestor of the working directory, or `NEWS_PLATE_SCRIPT`.
fn script() -> NewsResult<PathBuf> {
    if let Some(p) = std::env::var_os(SCRIPT_VAR).filter(|p| !p.is_empty()) {
        return Ok(PathBuf::from(p));
    }
    let cwd = std::env::current_dir().map_err(|e| NewsError::internal(e.to_string()))?;
    cwd.ancestors()
        .map(|d| d.join(SCRIPT_REL))
        .find(|p| p.is_file())
        .ok_or_else(|| {
            unavailable(format!(
                "{SCRIPT_REL} not found above the working directory"
            ))
        })
}

impl Plate {
    /// The Plate at `NEWS_PLATE_URL`, or a new one for this call.
    pub async fn connect() -> NewsResult<Plate> {
        let http = reqwest::Client::builder()
            .timeout(CALL_TIMEOUT)
            .no_proxy()
            .build()
            .map_err(|e| NewsError::internal(e.to_string()))?;
        if let Some(url) = std::env::var(URL_VAR).ok().filter(|u| !u.is_empty()) {
            let plate = Plate {
                base: loopback_base(&url)?,
                http,
                _child: None,
            };
            plate.health().await?;
            return Ok(plate);
        }
        Self::start(http).await
    }

    async fn start(http: reqwest::Client) -> NewsResult<Plate> {
        let script = script()?;
        let mut child = Command::new("bun")
            .arg(&script)
            .args(["--port", "0", "--lifeline"])
            .env("NODE_ENV", "production")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| unavailable(format!("cannot start Plate with bun: {e}")))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| NewsError::internal("Plate has no stdout"))?;
        let mut first = String::new();
        let read =
            tokio::time::timeout(START_TIMEOUT, BufReader::new(stdout).read_line(&mut first)).await;
        let port = serde_json::from_str::<Value>(&first)
            .ok()
            .and_then(|v| v["port"].as_u64());
        let Some(port) = port.filter(|_| matches!(read, Ok(Ok(_)))) else {
            let _ = child.start_kill();
            let mut err = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                let _ =
                    tokio::time::timeout(Duration::from_secs(5), stderr.read_to_string(&mut err))
                        .await;
            }
            return Err(unavailable(format!(
                "Plate did not start: {}",
                err.lines().last().unwrap_or("no output")
            )));
        };
        // Nothing reads stderr after the start, so keep it from filling its pipe.
        drop(child.stderr.take());
        Ok(Plate {
            base: format!("http://127.0.0.1:{port}"),
            http,
            _child: Some(child),
        })
    }

    pub async fn health(&self) -> NewsResult<String> {
        let v = self.call(reqwest::Method::GET, "/health", None).await?;
        Ok(v["renderer"].as_str().unwrap_or_default().to_string())
    }

    /// OpenUI parse and props against the specs, and the literals the parser read (for the lints).
    pub async fn check(&self, oui: &str) -> NewsResult<Checked> {
        let v = self
            .call(reqwest::Method::POST, "/check", Some(json!({ "oui": oui })))
            .await?;
        serde_json::from_value::<Checked>(v)
            .map_err(|e| NewsError::internal(format!("Plate's /check reply: {e}")))
    }

    /// Render a bundle. A page that breaks the bundle contract or does not parse is `validation_failed` (C1).
    pub async fn render(&self, request: &Value) -> NewsResult<Rendered> {
        let v = self
            .call(reqwest::Method::POST, "/render", Some(request.clone()))
            .await?;
        serde_json::from_value(v)
            .map_err(|e| NewsError::internal(format!("Plate's /render reply: {e}")))
    }

    async fn call(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> NewsResult<Value> {
        let mut req = self.http.request(method, format!("{}{path}", self.base));
        if let Some(body) = body {
            req = req
                .header("content-type", "application/json")
                .body(body.to_string());
        }
        let res = req
            .send()
            .await
            .map_err(|e| unavailable(format!("Plate at {} did not answer: {e}", self.base)))?;
        let status = res.status();
        let text = res
            .text()
            .await
            .map_err(|e| NewsError::upstream(format!("Plate's reply: {e}")))?;
        let v: Value = serde_json::from_str(&text).map_err(|e| {
            NewsError::internal(format!("Plate replied with non-JSON ({status}): {e}"))
        })?;
        if status.is_success() {
            return Ok(v);
        }
        let message = v["error"]["message"]
            .as_str()
            .unwrap_or("no message")
            .to_string();
        let details = v["error"]["details"].clone();
        let err = if status.as_u16() == 422 {
            NewsError::validation(message)
        } else {
            NewsError::internal(format!("Plate {path} failed ({status}): {message}"))
        };
        Err(if details.is_null() {
            err
        } else {
            err.with_details(details)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plate_url_must_be_loopback() {
        for ok in [
            "http://127.0.0.1:4321/",
            "http://localhost:4321",
            "http://[::1]:4321",
        ] {
            assert_eq!(
                loopback_base(ok).ok().as_deref(),
                Some(ok.trim_end_matches('/'))
            );
        }
        for bad in [
            "http://example.com:4321",
            "http://10.0.0.5:4321",
            "http://127.0.0.1.example.com:4321",
            "https://127.0.0.1:4321",
            "127.0.0.1:4321",
        ] {
            assert!(loopback_base(bad).is_err(), "{bad}");
        }
    }
}
