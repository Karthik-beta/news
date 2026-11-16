//! Contacts, the source registry (Bureau Design §3.1).
//!
//! Each source is a data file, `data/sources/<source_id>.toml`, written by developers and reviewed
//! like code (no runtime model writes these). [`load_dir`] reads them, [`lint`] checks them (CI runs it
//! through this crate's tests), and [`sync`] upserts them into `sources`. [`licence_gate`] is what a
//! Stringer checks before emitting candidates: a source with no licence emits none (`licence_missing`).
//!
//! `news sources list|show|health|probe|sync` and `enable|disable` (through `news-chief`'s
//! `authorize()`) are wired in the CLI by a follow-up: TODO(#10, #29) once the Bureau service and Board
//! own the command surface.

use std::path::{Path, PathBuf};

use news_core::job::{Handler, JobFailure, JobFuture, JobInput};
use serde::Deserialize;

/// The issue that builds this subsystem.
pub const ISSUE: &str = "#9";

/// Job kinds this subsystem handles (see `news_dispatch::KINDS`).
pub const KINDS: &[&str] = &[];

/// Run one job.
pub fn handle(input: JobInput) -> JobFuture {
    Box::pin(async move { Err(JobFailure::not_built(&input.kind, ISSUE)) })
}

pub const HANDLER: Handler = handle;

pub const LICENCES: &[&str] = &["public-domain", "cc-by-4.0", "custom", "unclear"];
const CALENDAR_KINDS: &[&str] = &["ics", "api", "html", "derived"];

/// One `data/sources/<id>.toml`. Mirrors the `sources` table plus the documented limits the lint
/// checks against (kept in the file, not the table).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub source_id: String,
    pub name: String,
    pub publisher: String,
    pub connector: String,
    pub base_url: String,
    pub source_tier: i16,
    /// `None` when the file has no licence: the lint fails and the source emits no candidates.
    #[serde(default)]
    pub licence: Option<String>,
    #[serde(default)]
    pub attribution: String,
    pub redistribution: bool,
    pub image_policy: String,
    pub rate_per_sec: f64,
    /// The source's documented request rate; `rate_per_sec` must be at most half of it.
    pub documented_rate_per_sec: f64,
    /// The documented rule in words, with its reference.
    pub documented_limit: String,
    #[serde(default = "one")]
    pub max_concurrency: i16,
    #[serde(default)]
    pub daily_quota: Option<i32>,
    #[serde(default)]
    pub documented_daily_quota: Option<i32>,
    /// NAME of the env var holding a free key, never the key.
    #[serde(default)]
    pub auth_env: Option<String>,
    #[serde(default = "yes")]
    pub robots_respect: bool,
    /// URL prefixes (`https://host/path/`) where robots.txt is not applied although
    /// `robots_respect` is on: a documented API that robots.txt would otherwise refuse. Each one is
    /// a policy decision for Karthik to approve.
    #[serde(default)]
    pub robots_exempt: Vec<String>,
    /// SEC: the Runner adds `NEWS_CONTACT_EMAIL` to the User-Agent.
    #[serde(default)]
    pub requires_contact_email: bool,
    pub regions: Vec<String>,
    pub categories: Vec<String>,
    #[serde(default)]
    pub calendar_kind: Option<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub notes: Option<String>,
}

fn one() -> i16 {
    1
}
fn yes() -> bool {
    true
}

#[derive(Debug, thiserror::Error)]
pub enum ContactsError {
    #[error("{path}: {message}")]
    File { path: PathBuf, message: String },
    #[error("contacts lint failed: {0:?}")]
    Lint(Vec<LintIssue>),
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
}

/// `data/sources` in the nearest ancestor of `start` that has one, or `NEWS_SOURCES_DIR`.
pub fn default_dir(start: &Path) -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("NEWS_SOURCES_DIR") {
        return Some(PathBuf::from(d));
    }
    start
        .ancestors()
        .map(|a| a.join("data").join("sources"))
        .find(|p| p.is_dir())
}

/// Every `*.toml` in `dir`, sorted by `source_id`. Unknown keys are errors, so a typo cannot hide.
pub fn load_dir(dir: &Path) -> Result<Vec<Source>, ContactsError> {
    let err = |path: &Path, message: String| ContactsError::File {
        path: path.to_owned(),
        message,
    };
    let entries = std::fs::read_dir(dir).map_err(|e| err(dir, e.to_string()))?;
    let mut out = Vec::new();
    for entry in entries {
        let path = entry.map_err(|e| err(dir, e.to_string()))?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let text = std::fs::read_to_string(&path).map_err(|e| err(&path, e.to_string()))?;
        out.push(parse(&text).map_err(|m| err(&path, m))?);
    }
    out.sort_by(|a, b| a.source_id.cmp(&b.source_id));
    Ok(out)
}

pub fn parse(text: &str) -> Result<Source, String> {
    toml::from_str(text).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintIssue {
    pub source_id: String,
    pub rule: &'static str,
    pub message: String,
}

/// The Contacts lint (Bureau §3.1, "Tests"). Empty means pass.
pub fn lint(sources: &[Source]) -> Vec<LintIssue> {
    let mut issues = Vec::new();
    let mut ids = std::collections::BTreeSet::new();
    for s in sources {
        let mut issue = |rule: &'static str, message: String| {
            issues.push(LintIssue {
                source_id: s.source_id.clone(),
                rule,
                message,
            })
        };
        if !ids.insert(s.source_id.as_str()) {
            issue("duplicate_id", "source_id appears twice".into());
        }
        let id_ok = !s.source_id.is_empty()
            && s.source_id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if !id_ok {
            issue("source_id", format!("{:?} is not kebab-case", s.source_id));
        }
        if news_connectors::connector(&s.connector).is_none() {
            issue("connector", format!("no connector {:?}", s.connector));
        }
        if !s.base_url.starts_with("https://") {
            issue("base_url", "base_url must be https".into());
        }
        if !(1..=4).contains(&s.source_tier) {
            issue("tier", format!("source_tier {} is not 1–4", s.source_tier));
        }
        if let Some(c) = &s.calendar_kind
            && !CALENDAR_KINDS.contains(&c.as_str())
        {
            issue(
                "calendar_kind",
                format!("{c:?} is not one of {CALENDAR_KINDS:?}"),
            );
        }
        if let Some(env) = &s.auth_env
            && !env
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        {
            issue(
                "auth_env",
                "auth_env must be an env var NAME, never a key".into(),
            );
        }
        for prefix in &s.robots_exempt {
            // `https://host/…/`: a whole host and a path ending in `/`, so a prefix cannot spill over
            // into another host (`api.bls.gov.example`) or a sibling path (`/publicAPIx`).
            let ok = prefix
                .strip_prefix("https://")
                .and_then(|rest| rest.split_once('/'))
                .is_some_and(|(host, _)| !host.is_empty())
                && prefix.ends_with('/');
            if !ok {
                issue(
                    "robots_exempt",
                    format!("{prefix:?} must look like https://host/path/"),
                );
            }
        }
        if s.max_concurrency < 1 {
            issue("concurrency", "max_concurrency must be at least 1".into());
        }
        if s.regions.is_empty() || s.categories.is_empty() {
            issue(
                "taxonomy",
                "regions and categories must not be empty".into(),
            );
        }
        if !s.enabled {
            continue;
        }
        match s.licence.as_deref() {
            None | Some("") => issue(
                "licence_missing",
                "an enabled source needs a licence".into(),
            ),
            Some(l) if !LICENCES.contains(&l) => {
                issue("licence", format!("{l:?} is not one of {LICENCES:?}"));
            }
            Some("unclear") if s.redistribution => issue(
                "redistribution",
                "an unclear licence must have redistribution = false".into(),
            ),
            Some(_) => {}
        }
        if s.attribution.trim().is_empty() {
            issue(
                "attribution",
                "an enabled source needs attribution text".into(),
            );
        }
        if s.documented_limit.trim().is_empty() || s.documented_rate_per_sec <= 0.0 {
            issue("rate", "the documented limit must be recorded".into());
        }
        if !(s.rate_per_sec > 0.0 && s.rate_per_sec <= s.documented_rate_per_sec / 2.0) {
            issue(
                "rate",
                format!(
                    "rate_per_sec {} is above half the documented {} req/s",
                    s.rate_per_sec, s.documented_rate_per_sec
                ),
            );
        }
        if let (Some(q), Some(d)) = (s.daily_quota, s.documented_daily_quota)
            && q > d
        {
            issue(
                "quota",
                format!("daily_quota {q} is above the documented {d}"),
            );
        }
    }
    issues
}

/// Why a Stringer refuses to emit candidates.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("licence_missing: source {0} has no licence")]
pub struct LicenceMissing(pub String);

/// A source with no licence emits no candidates (Bureau §3.1, "Failure modes").
pub fn licence_gate(source_id: &str, licence: Option<&str>) -> Result<(), LicenceMissing> {
    match licence {
        Some(l) if LICENCES.contains(&l) => Ok(()),
        _ => Err(LicenceMissing(source_id.to_owned())),
    }
}

/// Upsert every source into `sources` after the lint passes. Idempotent; `health` is computed
/// elsewhere and never overwritten, and `enabled` is only set on insert (enable/disable goes through
/// Chief's `authorize()`).
pub async fn sync(pool: &sqlx::PgPool, sources: &[Source]) -> Result<usize, ContactsError> {
    let issues = lint(sources);
    if !issues.is_empty() {
        return Err(ContactsError::Lint(issues));
    }
    let mut tx = pool.begin().await?;
    for s in sources {
        sqlx::query(
            "insert into sources (source_id, name, publisher, connector, base_url, source_tier, licence,
               attribution, redistribution, image_policy, auth_env, rate_per_sec, max_concurrency,
               daily_quota, robots_respect, regions, categories, calendar_kind, enabled, notes,
               requires_contact_email, robots_exempt)
             values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12::float8::numeric(6,3),$13,$14,$15,$16,$17,$18,$19,$20,$21,$22)
             on conflict (source_id) do update set
               name = excluded.name, publisher = excluded.publisher, connector = excluded.connector,
               base_url = excluded.base_url, source_tier = excluded.source_tier, licence = excluded.licence,
               attribution = excluded.attribution, redistribution = excluded.redistribution,
               image_policy = excluded.image_policy, auth_env = excluded.auth_env,
               rate_per_sec = excluded.rate_per_sec, max_concurrency = excluded.max_concurrency,
               daily_quota = excluded.daily_quota, robots_respect = excluded.robots_respect,
               regions = excluded.regions, categories = excluded.categories,
               calendar_kind = excluded.calendar_kind, notes = excluded.notes,
               requires_contact_email = excluded.requires_contact_email,
               robots_exempt = excluded.robots_exempt",
        )
        .bind(&s.source_id)
        .bind(&s.name)
        .bind(&s.publisher)
        .bind(&s.connector)
        .bind(&s.base_url)
        .bind(s.source_tier)
        .bind(s.licence.as_deref())
        .bind(&s.attribution)
        .bind(s.redistribution)
        .bind(&s.image_policy)
        .bind(s.auth_env.as_deref())
        .bind(s.rate_per_sec)
        .bind(s.max_concurrency)
        .bind(s.daily_quota)
        .bind(s.robots_respect)
        .bind(&s.regions)
        .bind(&s.categories)
        .bind(s.calendar_kind.as_deref())
        .bind(s.enabled)
        .bind(s.notes.as_deref())
        .bind(s.requires_contact_email)
        .bind(&s.robots_exempt)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(sources.len())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn repo_sources() -> Vec<Source> {
        let dir = default_dir(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        load_dir(&dir).unwrap()
    }

    #[test]
    fn the_repo_sources_pass_the_contacts_lint() {
        let sources = repo_sources();
        let ids: Vec<&str> = sources.iter().map(|s| s.source_id.as_str()).collect();
        assert_eq!(ids, vec!["bls", "fred", "sec-edgar"]);
        assert_eq!(lint(&sources), vec![]);
        // Every built connector has a Contacts row.
        for c in news_connectors::IDS {
            assert!(sources.iter().any(|s| s.connector == *c), "{c}");
        }
    }

    #[test]
    fn licence_and_attribution_reach_the_source_record() {
        let sources = repo_sources();
        let sec = sources.iter().find(|s| s.source_id == "sec-edgar").unwrap();
        assert_eq!(sec.licence.as_deref(), Some("public-domain"));
        assert!(
            sec.attribution
                .contains("Securities and Exchange Commission")
        );
        assert!(sec.requires_contact_email);
        // Off until its fixtures are recorded and NEWS_CONTACT_EMAIL is set: without the e-mail every
        // SEC job would fail as `bug` and land on the dead list.
        assert!(!sec.enabled);
        let fred = sources.iter().find(|s| s.source_id == "fred").unwrap();
        assert!(
            !fred.redistribution,
            "FRED's per-series terms are unclear: link and summarise"
        );
        assert_eq!(fred.auth_env.as_deref(), Some("FRED_API_KEY"));
        let bls = sources.iter().find(|s| s.source_id == "bls").unwrap();
        assert_eq!(bls.daily_quota, Some(400));
        // robots.txt applies to every BLS host except the documented API prefix.
        assert!(bls.robots_respect);
        assert_eq!(bls.robots_exempt, vec!["https://api.bls.gov/publicAPI/"]);
    }

    #[test]
    fn fred_keeps_to_its_robots_crawl_delay() {
        // api.stlouisfed.org/robots.txt (recorded) asks for `Crawl-delay: 2`: at most 0.5 req/s.
        let robots = std::fs::read_to_string(format!(
            "{}/../news-connectors/tests/fixtures/fred/api-robots.txt",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let delay: f64 = robots
            .lines()
            .find_map(|l| l.strip_prefix("Crawl-delay:"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let fred = repo_sources()
            .into_iter()
            .find(|s| s.source_id == "fred")
            .unwrap();
        assert!(
            fred.rate_per_sec <= 1.0 / delay,
            "fred rate_per_sec {} is above 1/Crawl-delay ({delay} s)",
            fred.rate_per_sec
        );
    }

    fn good() -> Source {
        let mut s = repo_sources()
            .into_iter()
            .find(|s| s.source_id == "sec-edgar")
            .unwrap();
        s.enabled = true;
        s
    }

    fn rules(s: Source) -> Vec<&'static str> {
        lint(&[s]).into_iter().map(|i| i.rule).collect()
    }

    #[test]
    fn the_lint_catches_each_rule() {
        let mut s = good();
        s.licence = None;
        assert_eq!(rules(s), vec!["licence_missing"]);
        let mut s = good();
        s.attribution = " ".into();
        assert_eq!(rules(s), vec!["attribution"]);
        let mut s = good();
        s.rate_per_sec = 5.1;
        assert_eq!(rules(s), vec!["rate"]);
        let mut s = good();
        s.source_tier = 5;
        assert_eq!(rules(s), vec!["tier"]);
        let mut s = good();
        s.licence = Some("unclear".into());
        assert_eq!(rules(s), vec!["redistribution"]);
        let mut s = good();
        s.auth_env = Some("sk-live-123".into());
        assert_eq!(rules(s), vec!["auth_env"]);
        let mut s = good();
        s.connector = "nope".into();
        assert_eq!(rules(s), vec!["connector"]);
        let mut s = good();
        s.daily_quota = Some(600);
        s.documented_daily_quota = Some(500);
        assert_eq!(rules(s), vec!["quota"]);
        for bad in [
            "api.bls.gov",
            "https://api.bls.gov",
            "http://api.bls.gov/publicAPI/",
            "https:///x/",
        ] {
            let mut s = good();
            s.robots_exempt = vec![bad.into()];
            assert_eq!(rules(s), vec!["robots_exempt"], "{bad}");
        }
        // A disabled source may lack a licence.
        let mut s = good();
        s.enabled = false;
        s.licence = None;
        assert!(rules(s).is_empty());
    }

    #[test]
    fn unknown_keys_are_refused() {
        let text = std::fs::read_to_string(
            default_dir(Path::new(env!("CARGO_MANIFEST_DIR")))
                .unwrap()
                .join("bls.toml"),
        )
        .unwrap();
        assert!(parse(&text).is_ok());
        assert!(parse(&format!("{text}\napi_key = \"x\"\n")).is_err());
    }

    #[test]
    fn no_licence_means_no_candidates() {
        assert!(licence_gate("bls", Some("public-domain")).is_ok());
        assert_eq!(
            licence_gate("x", None).unwrap_err().to_string(),
            "licence_missing: source x has no licence"
        );
        assert!(licence_gate("x", Some("")).is_err());
    }
}
