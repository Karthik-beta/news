//! Link checks for C10: do the source, image and embed URLs answer? Answers that were fine are cached for
//! 24 hours (`NEWS_WORK_DIR/cache/proof-links.json`), so a re-check does not hit the sources again; a failure
//! is never cached, so a transient outage cannot block a page for a day.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq)]
pub struct Probe {
    pub ok: bool,
    pub note: String,
}

pub trait LinkChecker {
    fn check(&self, url: &str) -> Probe;
}

/// The live checker: HEAD, then GET when the server does not do HEAD. Only public addresses are probed: a page
/// is written from outside data, so a link to `http://169.254.169.254/` or `http://localhost:5432/` must not make
/// this machine call its own network. Redirects are re-checked at every hop. Only the status line is read (the
/// body is never downloaded), and the whole request is capped at 10 seconds.
pub struct HttpProbe {
    client: reqwest::blocking::Client,
    allow_local: bool,
}

/// shortcut: the name is resolved here and again by the HTTP client, so a DNS server that answers differently the
/// second time (rebinding) could slip past; upgrade to a pinned resolver if links ever come from untrusted pages.
fn public_url(url: &reqwest::Url, allow_local: bool) -> Result<(), String> {
    use std::net::ToSocketAddrs;
    if allow_local {
        return Ok(());
    }
    if !matches!(url.scheme(), "http" | "https") {
        return Err("only http and https links are checked".into());
    }
    let host = url.host_str().ok_or("the link has no host")?;
    let port = url.port_or_known_default().unwrap_or(80);
    let addrs: Vec<_> = (host.trim_matches(['[', ']']), port)
        .to_socket_addrs()
        .map_err(|_| "the host does not resolve".to_string())?
        .collect();
    if addrs.is_empty() || addrs.iter().any(|a| !is_public(a.ip())) {
        return Err("the link points at a private or local address".into());
    }
    Ok(())
}

fn is_public(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || o[0] == 0
                || (o[0] == 100 && (o[1] & 0xc0) == 64) // 100.64.0.0/10, carrier-grade NAT
                || (o[0] == 198 && (o[1] & 0xfe) == 18) // 198.18.0.0/15, benchmarking
                || o[0] >= 240) // 240.0.0.0/4, reserved
        }
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_public(IpAddr::V4(v4)),
            None => {
                let [first, second, ..] = v6.segments();
                !(v6.is_loopback()
                    || v6.is_unspecified()
                    || v6.is_multicast()
                    || (first == 0x64 && second == 0xff9b) // 64:ff9b::/32, NAT64 (well-known and local)
                    || first == 0x2002 // 2002::/16, 6to4
                    || (first & 0xfe00) == 0xfc00 // fc00::/7, unique local
                    || (first & 0xffc0) == 0xfe80) // fe80::/10, link local
            }
        },
    }
}

impl HttpProbe {
    pub fn new() -> Result<Self, String> {
        Self::build(false)
    }

    fn build(allow_local: bool) -> Result<Self, String> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(format!("News/{} link-check", news_core::VERSION))
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() >= 5 {
                    attempt.error("too many redirects")
                } else if let Err(why) = public_url(attempt.url(), allow_local) {
                    attempt.error(why)
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .map_err(|e| e.without_url().to_string())?;
        Ok(Self {
            client,
            allow_local,
        })
    }
}

/// 401, 403 and 429 mean the server is there and does not like a bot: the link is not dead.
fn answers(status: u16) -> bool {
    status < 400 || matches!(status, 401 | 403 | 429)
}

impl LinkChecker for HttpProbe {
    fn check(&self, url: &str) -> Probe {
        let refused = match reqwest::Url::parse(url) {
            Ok(u) => public_url(&u, self.allow_local).err(),
            Err(e) => Some(e.to_string()),
        };
        if let Some(note) = refused {
            return Probe { ok: false, note };
        }
        let head = self.client.head(url).send();
        let status = match head {
            Ok(r) if !matches!(r.status().as_u16(), 405 | 501) && r.status().as_u16() < 500 => {
                r.status().as_u16()
            }
            // No HEAD support, or the server failed it: try a GET.
            _ => match self.client.get(url).send() {
                Ok(r) => r.status().as_u16(),
                Err(e) => {
                    return Probe {
                        ok: false,
                        note: e.without_url().to_string(),
                    };
                }
            },
        };
        Probe {
            ok: answers(status),
            note: format!("HTTP {status}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    at: DateTime<Utc>,
    note: String,
}

/// Wraps a checker with the 24-hour cache of good answers.
pub struct CachedLinks<L> {
    inner: L,
    path: Option<PathBuf>,
    ttl: Duration,
    now: DateTime<Utc>,
    entries: Mutex<BTreeMap<String, Entry>>,
}

impl<L: LinkChecker> CachedLinks<L> {
    /// `path` is the cache file (read now, written by [`save`](Self::save)); `None` keeps it in memory.
    pub fn new(inner: L, path: Option<PathBuf>, ttl_hours: i64, now: DateTime<Utc>) -> Self {
        let entries = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Self {
            inner,
            path,
            ttl: Duration::hours(ttl_hours),
            now,
            entries: Mutex::new(entries),
        }
    }

    /// Writes the cache, dropping entries that have expired. A cache that cannot be written is only slower.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let Ok(mut entries) = self.entries.lock() else {
            return Ok(());
        };
        entries.retain(|_, e| self.now - e.at < self.ttl);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&*entries)?)
    }
}

impl<L: LinkChecker> LinkChecker for CachedLinks<L> {
    fn check(&self, url: &str) -> Probe {
        if let Ok(entries) = self.entries.lock()
            && let Some(e) = entries.get(url).filter(|e| self.now - e.at < self.ttl)
        {
            return Probe {
                ok: true,
                note: format!("{} (cached)", e.note),
            };
        }
        let probe = self.inner.check(url);
        if probe.ok
            && let Ok(mut entries) = self.entries.lock()
        {
            entries.insert(
                url.to_owned(),
                Entry {
                    at: self.now,
                    note: probe.note.clone(),
                },
            );
        }
        probe
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::io::{Read, Write};

    struct Counting(Cell<u32>, bool);
    impl LinkChecker for Counting {
        fn check(&self, _: &str) -> Probe {
            self.0.set(self.0.get() + 1);
            Probe {
                ok: self.1,
                note: "HTTP 200".into(),
            }
        }
    }

    #[test]
    fn good_answers_are_cached_for_24_hours_and_failures_are_not() {
        let t0: DateTime<Utc> = "2026-11-23T10:00:00Z".parse().unwrap();
        let dir = std::env::temp_dir().join(format!("news-proof-links-{}", uuid::Uuid::new_v4()));
        let file = dir.join("cache.json");
        let first = CachedLinks::new(Counting(Cell::new(0), true), Some(file.clone()), 24, t0);
        assert!(first.check("https://a.example/x").ok);
        assert!(
            first
                .check("https://a.example/x")
                .note
                .ends_with("(cached)")
        );
        assert_eq!(first.inner.0.get(), 1);
        first.save().unwrap();
        // A later run within 24 hours reads the file; after 24 hours it asks again.
        let soon = CachedLinks::new(
            Counting(Cell::new(0), true),
            Some(file.clone()),
            24,
            t0 + Duration::hours(23),
        );
        assert!(soon.check("https://a.example/x").ok);
        assert_eq!(soon.inner.0.get(), 0);
        let later = CachedLinks::new(
            Counting(Cell::new(0), true),
            Some(file),
            24,
            t0 + Duration::hours(25),
        );
        later.check("https://a.example/x");
        assert_eq!(later.inner.0.get(), 1);
        let bad = CachedLinks::new(Counting(Cell::new(0), false), None, 24, t0);
        assert!(!bad.check("https://b.example/").ok);
        assert!(!bad.check("https://b.example/").ok);
        assert_eq!(bad.inner.0.get(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A tiny HTTP server: answers each request with the status its path names (`/404`), HEAD and GET alike
    /// except `/nohead`, which refuses HEAD with 405.
    fn server() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { break };
                let mut buf = [0u8; 2048];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let line = req.lines().next().unwrap_or_default().to_owned();
                let path = line
                    .split(' ')
                    .nth(1)
                    .unwrap_or("/")
                    .trim_start_matches('/')
                    .to_owned();
                let status = if path == "nohead" {
                    if line.starts_with("HEAD") { 405 } else { 200 }
                } else {
                    path.parse().unwrap_or(200)
                };
                let _ = write!(
                    s,
                    "HTTP/1.1 {status} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
            }
        });
        base
    }

    #[test]
    fn the_live_probe_reads_status_codes() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let base = server();
        let p = HttpProbe::build(true).unwrap();
        assert!(p.check(&format!("{base}/200")).ok);
        assert!(p.check(&format!("{base}/nohead")).ok, "falls back to GET");
        assert!(
            p.check(&format!("{base}/403")).ok,
            "403 means the server is there"
        );
        assert!(!p.check(&format!("{base}/404")).ok);
        assert!(!p.check(&format!("{base}/500")).ok);
        assert!(!p.check("http://127.0.0.1:1/").ok);
    }

    #[test]
    fn private_and_local_addresses_are_never_probed() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let base = server(); // a live loopback server: it must not be reached
        let p = HttpProbe::new().unwrap();
        for url in [
            format!("{base}/200"),
            "http://localhost/".into(),
            "http://127.0.0.1/".into(),
            "http://10.0.0.5/".into(),
            "http://172.16.0.1/".into(),
            "http://192.168.1.1/".into(),
            "http://169.254.169.254/latest/meta-data/".into(),
            "http://100.64.0.1/".into(),
            "http://0.0.0.0/".into(),
            "http://[::1]/".into(),
            "http://[fd00::1]/".into(),
            "http://[fe80::1]/".into(),
            "http://[::ffff:127.0.0.1]/".into(),
            "file:///etc/passwd".into(),
        ] {
            let probe = p.check(&url);
            assert!(!probe.ok, "{url} must be refused");
            assert!(
                !probe.note.starts_with("HTTP "),
                "{url} must not be fetched"
            );
        }
        assert!(is_public("8.8.8.8".parse().unwrap()));
        assert!(is_public("2606:4700::1111".parse().unwrap()));
        for private in [
            "198.18.0.1",
            "198.19.255.255",
            "240.0.0.1",
            "64:ff9b::7f00:1",
            "2002:7f00:1::",
        ] {
            assert!(!is_public(private.parse().unwrap()), "{private}");
        }
        assert!(is_public("198.20.0.1".parse().unwrap()));
    }
}
