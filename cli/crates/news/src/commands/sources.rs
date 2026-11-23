//! `news sources …`: Contacts' tools (Bureau §6.4).
//!
//! `list` and `show` read `sources`. `health` reads `fetch_state` and `snapshots`; the error classes
//! and the quota used live in ClickHouse and in `jobs`, so `health` leaves them empty (`{}` and
//! `null`). `probe` makes one live discovery through the Runner and writes nothing. `sync` loads
//! `data/sources/*.toml` into `sources`; `enable` and `disable` wait for Chief's `authorize()` (#10).

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use news_connectors::{Cx, Snapshot, bls, fred};
use news_core::job::ErrorClass;
use news_core::{ErrorCode, NewsError, NewsResult};
use news_wire::pace::SystemClock;
use news_wire::runner::{Outcome, ReqwestTransport, Runner, RunnerConfig, install_tls_provider};
use serde_json::{Value, json};
use sqlx::Row;

use super::{Reply, database};
use crate::cli::SourcesCmd;

fn db_err(e: sqlx::Error) -> NewsError {
    news_store::map_err(e)
}

pub async fn run(cmd: SourcesCmd) -> NewsResult<Reply> {
    // Validate arguments before touching the database.
    let since = match &cmd {
        SourcesCmd::Health { since } => Some(parse_since(since)?),
        _ => None,
    };
    let (db, _) = database(2).await?;
    match cmd {
        SourcesCmd::List => {
            let rows: Vec<Value> = sqlx::query_scalar(
                "select jsonb_build_object('source_id', source_id, 'name', name, 'publisher', publisher,
                          'source_tier', source_tier, 'licence', licence, 'enabled', enabled, 'health', health)
                   from sources order by source_id",
            )
            .fetch_all(&db)
            .await
            .map_err(db_err)?;
            Ok(Reply::new(json!({ "items": rows })))
        }
        SourcesCmd::Show { source } => {
            let row: Option<Value> =
                sqlx::query_scalar("select to_jsonb(s) from sources s where source_id = $1")
                    .bind(&source)
                    .fetch_optional(&db)
                    .await
                    .map_err(db_err)?;
            row.map(Reply::new).ok_or_else(|| not_found(&source))
        }
        SourcesCmd::Health { .. } => health(&db, Utc::now() - since.unwrap_or_default()).await,
        SourcesCmd::Probe { source } => probe(&db, &source).await,
        SourcesCmd::Sync => {
            let cwd = std::env::current_dir().unwrap_or_default();
            let dir = news_contacts::default_dir(&cwd)
                .ok_or_else(|| NewsError::not_found("no data/sources directory found"))?;
            let sources = news_contacts::load_dir(&dir).map_err(contacts_err)?;
            let synced = news_contacts::sync(&db, &sources)
                .await
                .map_err(contacts_err)?;
            Ok(Reply::new(json!({ "synced": synced })))
        }
    }
}

fn not_found(source: &str) -> NewsError {
    NewsError::not_found(format!("no source {source:?}")).with_hint("news sources list")
}

fn contacts_err(e: news_contacts::ContactsError) -> NewsError {
    match e {
        news_contacts::ContactsError::Db(e) => db_err(e),
        other => NewsError::validation(other.to_string()),
    }
}

/// `7d`, `36h`: a number and a unit, nothing else.
fn parse_since(text: &str) -> NewsResult<Duration> {
    let bad = || NewsError::usage(format!("--since {text:?} is not like 7d or 36h"));
    let (n, unit) = text.split_at(text.len().saturating_sub(1));
    let n: i64 = n.parse().map_err(|_| bad())?;
    match unit {
        "d" if n > 0 => Duration::try_days(n),
        "h" if n > 0 => Duration::try_hours(n),
        _ => None,
    }
    .ok_or_else(bad)
}

/// What one source's rows say, before grading.
struct Facts {
    enabled: bool,
    /// The env var that must hold the free key, when the source needs one and it is absent.
    missing_key: Option<String>,
    last_success: Option<DateTime<Utc>>,
    fetched: i64,
    unchanged: i64,
    /// The most consecutive errors on any URL of the source.
    worst_errors: i16,
}

/// Green, amber or red with the reasons (Bureau §3.1: a missing free key is amber).
fn grade(f: &Facts, since: DateTime<Utc>) -> (&'static str, Vec<String>) {
    let (mut level, mut reasons) = (0, Vec::new());
    let mut note = |l: i32, why: String| {
        level = level.max(l);
        reasons.push(why);
    };
    if !f.enabled {
        note(1, "disabled".into());
    }
    if let Some(env) = &f.missing_key {
        note(1, format!("free key absent ({env})"));
    }
    match f.last_success {
        None => note(1, "never fetched".into()),
        Some(t) if t < since => note(2, format!("no success since {}", t.format("%Y-%m-%d"))),
        Some(_) => {}
    }
    if f.worst_errors >= 3 {
        note(
            2,
            format!("{} consecutive errors on one URL", f.worst_errors),
        );
    } else if f.worst_errors > 0 {
        note(
            1,
            format!("{} consecutive errors on one URL", f.worst_errors),
        );
    }
    (["green", "amber", "red"][level as usize], reasons)
}

async fn health(db: &news_store::PgPool, since: DateTime<Utc>) -> NewsResult<Reply> {
    let rows = sqlx::query(
        "select s.source_id, s.enabled, s.auth_env,
                greatest((select max(f.last_fetched) from fetch_state f
                           where f.source_id = s.source_id and f.last_status in (200, 304)),
                         (select max(n.fetched_at) from snapshots n where n.source_id = s.source_id))
                  as last_success,
                (select count(*) from fetch_state f
                  where f.source_id = s.source_id and f.last_fetched >= $1) as fetched,
                (select count(*) from fetch_state f
                  where f.source_id = s.source_id and f.last_fetched >= $1 and f.last_status = 304)
                  as unchanged,
                (select coalesce(max(f.consecutive_errors), 0) from fetch_state f
                  where f.source_id = s.source_id) as worst_errors
           from sources s order by s.source_id",
    )
    .bind(since)
    .fetch_all(db)
    .await
    .map_err(db_err)?;
    let mut items = Vec::new();
    for r in rows {
        let key_set = |name: &str| std::env::var(name).is_ok_and(|v| !v.is_empty());
        let f = Facts {
            enabled: r.get("enabled"),
            missing_key: r
                .get::<Option<String>, _>("auth_env")
                .filter(|name| !key_set(name)),
            last_success: r.get("last_success"),
            fetched: r.get("fetched"),
            unchanged: r.get("unchanged"),
            worst_errors: r.get("worst_errors"),
        };
        let (health, reasons) = grade(&f, since);
        items.push(json!({
            "source_id": r.get::<String, _>("source_id"),
            "health": health,
            "reasons": reasons,
            "ratio_304": if f.fetched == 0 { 0.0 } else { f.unchanged as f64 / f.fetched as f64 },
            "error_classes": {},
            "last_success": f.last_success,
            "quota_used": null,
        }));
    }
    Ok(Reply::new(json!({ "items": items })))
}

/// The Runner's failure, as the CLI's error codes (the ones `sources.probe` lists).
fn fetch_failure(class: ErrorClass, message: String) -> NewsError {
    let code = match class {
        ErrorClass::RateLimited => ErrorCode::RateLimited,
        ErrorClass::LicenceBlocked => ErrorCode::LicenceBlocked,
        // Bug: the Runner refuses a request it must not make (for example SEC without a contact e-mail).
        ErrorClass::Bug => ErrorCode::Precondition,
        _ => ErrorCode::UpstreamUnavailable,
    };
    NewsError::new(code, message)
}

async fn probe(db: &news_store::PgPool, source: &str) -> NewsResult<Reply> {
    let row = sqlx::query("select connector, licence, enabled from sources where source_id = $1")
        .bind(source)
        .fetch_optional(db)
        .await
        .map_err(db_err)?
        .ok_or_else(|| not_found(source))?;
    let licence: Option<String> = row.get("licence");
    news_contacts::licence_gate(source, licence.as_deref())
        .map_err(|e| NewsError::new(ErrorCode::LicenceBlocked, e.to_string()))?;
    let connector: String = row.get("connector");
    let conn = news_connectors::connector(&connector)
        .ok_or_else(|| NewsError::internal(format!("no connector {connector:?}")))?;
    let policy = news_wire::store::policy(db, source)
        .await
        .map_err(|e| NewsError::upstream(e.to_string()))?
        .ok_or_else(|| {
            NewsError::precondition(format!("source {source:?} is disabled"))
                .with_hint("enable it first: probing a disabled source would still call it")
        })?;

    // shortcut: probe asks about one well-known item per connector (tracked lists have no home until
    // Standing, #10); a connector that needs other tracked ids finds no requests and says so.
    let cx = Cx {
        today: Utc::now().date_naive(),
        sec_ciks: vec!["320193".into()],
        fred_series: fred::SERIES.iter().take(1).map(|s| s.id.into()).collect(),
        bls_series: bls::SERIES.iter().take(1).map(|s| s.id.into()).collect(),
        tracked: Default::default(),
    };
    let requests = conn.discovery_requests(&cx, None);
    let mut reply_warnings = Vec::new();
    if requests.is_empty() {
        reply_warnings.push(format!(
            "{connector} has no tracked ids to ask about yet: probe made no request"
        ));
    }
    install_tls_provider();
    let transport = ReqwestTransport::new().map_err(|e| NewsError::internal(e.message))?;
    let runner = Runner::new(
        Arc::new(transport),
        Arc::new(SystemClock),
        RunnerConfig::from_env(),
    );
    let mut snaps = Vec::new();
    for req in &requests {
        // No validators: the probe wants the bytes, not a 304, and stores nothing.
        match runner
            .fetch(&policy, req, None)
            .await
            .map_err(|e| fetch_failure(e.class, e.message))?
        {
            Outcome::Fetched(f) => snaps.push(Snapshot {
                url: f.url,
                sha256: f.sha256,
                fetched_at: f.fetched_at,
                content_type: f.content_type,
                bytes: f.bytes,
            }),
            Outcome::NotModified => {}
        }
    }
    let found = conn
        .parse_discovery(&cx, None, &snaps)
        .map_err(|e| fetch_failure(e.class(), e.to_string()))?;
    let mut reply = Reply::new(json!({
        "refs": found.refs.len(),
        "sample": found.refs.iter().take(5).collect::<Vec<_>>(),
    }));
    reply.warnings = reply_warnings;
    Ok(reply)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn facts() -> Facts {
        Facts {
            enabled: true,
            missing_key: None,
            last_success: Some(Utc::now()),
            fetched: 4,
            unchanged: 1,
            worst_errors: 0,
        }
    }

    #[test]
    fn since_is_a_count_of_days_or_hours() {
        assert_eq!(parse_since("7d").unwrap(), Duration::days(7));
        assert_eq!(parse_since("36h").unwrap(), Duration::hours(36));
        for bad in ["", "d", "7", "0d", "-1d", "7w", "1.5d"] {
            assert_eq!(
                parse_since(bad).unwrap_err().code,
                ErrorCode::Usage,
                "{bad}"
            );
        }
    }

    #[test]
    fn grading_follows_the_reasons() {
        let since = Utc::now() - Duration::days(7);
        assert_eq!(grade(&facts(), since), ("green", vec![]));
        let amber = Facts {
            missing_key: Some("FRED_API_KEY".into()),
            ..facts()
        };
        assert_eq!(
            grade(&amber, since),
            ("amber", vec!["free key absent (FRED_API_KEY)".to_owned()])
        );
        let never = Facts {
            last_success: None,
            ..facts()
        };
        assert_eq!(grade(&never, since).0, "amber");
        let stale = Facts {
            last_success: Some(since - Duration::days(1)),
            ..facts()
        };
        assert_eq!(grade(&stale, since).0, "red");
        let failing = Facts {
            worst_errors: 3,
            ..facts()
        };
        assert_eq!(grade(&failing, since).0, "red");
        let off = Facts {
            enabled: false,
            ..facts()
        };
        assert_eq!(grade(&off, since), ("amber", vec!["disabled".to_owned()]));
    }
}
