//! `news home preview`: render the home page with a draft, locally, by the same SSR code as production
//! (ADR 0010, section 6). The site's preview build holds the draft in memory (`PUT /__preview/curation`)
//! and marks every response with `X-News-Preview: <sha256 of the bytes it was sent>`. This module starts
//! that server when none is listening, sends the draft, checks the header and names the four views.
//! Screenshots are Board's: the colour scheme and the width are properties of the browser, not of the URL.

use news_core::{NewsError, NewsResult};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The Astro dev server's own address; `NEWS_PREVIEW_URL` overrides it.
pub const DEFAULT_URL: &str = "http://127.0.0.1:4321";
const START_WAIT: Duration = Duration::from_secs(120);
/// (name, width, height): the phone and desktop widths Board shows.
const SIZES: [(&str, u32, u32); 2] = [("phone", 390, 844), ("desktop", 1440, 900)];

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Light and dark at phone and desktop width, all of one URL.
pub fn views(url: &str) -> Vec<Value> {
    ["light", "dark"]
        .iter()
        .flat_map(|theme| {
            SIZES.iter().map(move |(size, w, h)| {
                json!({"theme": theme, "size": size, "width": w, "height": h, "url": url})
            })
        })
        .collect()
}

fn local_addr(url: &reqwest::Url) -> NewsResult<(String, u16)> {
    let host = url.host_str().unwrap_or_default();
    // The preview server accepts a draft from loopback only (ADR 0010).
    if !matches!(host, "127.0.0.1" | "localhost" | "[::1]") {
        return Err(NewsError::usage(format!(
            "the preview server must be on loopback, not {host}"
        )));
    }
    let port = url.port_or_known_default().unwrap_or(80);
    Ok((host.trim_matches(['[', ']']).to_owned(), port))
}

async fn listening(host: &str, port: u16) -> bool {
    tokio::net::TcpStream::connect((host, port)).await.is_ok()
}

/// The nearest ancestor of the working directory with a `site/astro.config.mjs`.
fn site_dir() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    cwd.ancestors()
        .map(|d| d.join("site"))
        .find(|s| s.join("astro.config.mjs").is_file())
}

fn start(site: &Path, host: &str, port: u16) -> NewsResult<()> {
    // The preview integration is compiled in only with NEWS_PREVIEW=1, and the Worker runs in local mode.
    std::process::Command::new("bun")
        .args([
            "--bun",
            "astro",
            "dev",
            "--host",
            host,
            "--port",
            &port.to_string(),
        ])
        .current_dir(site)
        .env("NEWS_PREVIEW", "1")
        .env("CLOUDFLARE_ENV", "local")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| {
            NewsError::precondition(format!("cannot start the site's preview server: {e}"))
        })
}

/// Make sure a preview server is listening at `base`; start one from `site/` if not. `true` if started.
pub async fn ensure_server(base: &str) -> NewsResult<bool> {
    let url = reqwest::Url::parse(base)
        .map_err(|e| NewsError::usage(format!("{base:?} is not a URL: {e}")))?;
    let (host, port) = local_addr(&url)?;
    if listening(&host, port).await {
        return Ok(false);
    }
    let site = site_dir().ok_or_else(|| {
        NewsError::precondition(format!(
            "nothing listens on {base} and no site/ directory is above the working directory"
        ))
        .with_hint("run `bun run dev` in site/ with NEWS_PREVIEW=1, or set NEWS_PREVIEW_URL")
    })?;
    start(&site, &host, port)?;
    let deadline = tokio::time::Instant::now() + START_WAIT;
    while !listening(&host, port).await {
        if tokio::time::Instant::now() >= deadline {
            return Err(NewsError::upstream(format!(
                "the preview server did not listen on {base} within {} s",
                START_WAIT.as_secs()
            )));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Ok(true)
}

/// Send `body` to the preview server and confirm the server holds exactly these bytes: a page request
/// must answer with `X-News-Preview` equal to their SHA-256. Returns that hash.
pub async fn push(base: &str, body: Vec<u8>) -> NewsResult<String> {
    let sha = sha256_hex(&body);
    let base = base.trim_end_matches('/');
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| NewsError::internal(e.to_string()))?;
    let put = client
        .put(format!("{base}/__preview/curation"))
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| {
            NewsError::upstream(format!(
                "the preview server did not answer: {}",
                e.without_url()
            ))
        })?;
    if !put.status().is_success() {
        let status = put.status().as_u16();
        let why = put.text().await.unwrap_or_default();
        return Err(match status {
            404 => NewsError::precondition("the server has no /__preview/curation route")
                .with_hint("start the site with NEWS_PREVIEW=1 and CLOUDFLARE_ENV=local"),
            _ => NewsError::validation(format!(
                "the preview server refused the draft ({status}): {why}"
            )),
        });
    }
    let page = client.get(format!("{base}/")).send().await.map_err(|e| {
        NewsError::upstream(format!(
            "the preview page did not load: {}",
            e.without_url()
        ))
    })?;
    let held = page
        .headers()
        .get("x-news-preview")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if held != sha {
        return Err(NewsError::internal(format!(
            "the preview server shows a different draft (X-News-Preview {held:?}, expected {sha})"
        )));
    }
    Ok(sha)
}
