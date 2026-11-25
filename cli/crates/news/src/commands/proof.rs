//! `news check <story>`: Proof's code checks C1-C14 on a rendered page bundle (Bureau §3.10). The output
//! is the Markup. A page that fails a blocking check is a `validation_failed` error (exit 6) whose
//! `details` are the Markup, so an agent sees exactly what to fix.
//!
//! The bundle is the directory `<NEWS_WORK_DIR>/pages/<story>/` (default `work/`), or any directory given
//! instead of a story id. Switches for a machine without the network or a browser: `NEWS_PROOF_LINKS=off`
//! skips C10, `NEWS_PROOF_AXE=off` skips C13 (the Markup says "skipped").

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::Utc;

use super::{Reply, database};
use news_core::{NewsError, NewsResult};
use news_proof::axe::BunAxe;
use news_proof::bundle::Bundle;
use news_proof::checks::{self, AxeMode, Duplicate, Env, Markup};
use news_proof::lexicon::Lexicon;
use news_proof::links::{CachedLinks, HttpProbe};
use news_telemetry::{CheckResultRow, ClickHouse};

fn work_dir() -> PathBuf {
    std::env::var("NEWS_WORK_DIR")
        .ok()
        .filter(|d| !d.is_empty())
        .map_or_else(|| PathBuf::from("work"), PathBuf::from)
}

fn off(var: &str) -> bool {
    matches!(std::env::var(var).as_deref(), Ok("off" | "0" | "false"))
}

fn db_err(e: sqlx::Error) -> NewsError {
    news_store::map_err(e)
}

/// Where the bundle is, and the story id when `story` is one.
fn locate(story: &str) -> NewsResult<(PathBuf, Option<i64>)> {
    if Path::new(story).is_dir() {
        return Ok((PathBuf::from(story), None));
    }
    let id = story.parse::<i64>().map_err(|_| {
        NewsError::usage(format!(
            "{story:?} is neither a story id nor a bundle directory"
        ))
    })?;
    let dir = work_dir().join("pages").join(story);
    if !dir.is_dir() {
        return Err(
            NewsError::not_found(format!("no rendered bundle at {}", dir.display()))
                .with_hint("run `news render` for the story first"),
        );
    }
    Ok((dir, Some(id)))
}

/// Pages that clash with this one (C14): the same slug, or, when the story is known, the same story
/// already has another page.
async fn duplicates(
    db: &news_store::PgPool,
    page_id: &str,
    path: &str,
    story: Option<i64>,
) -> Result<Vec<Duplicate>, sqlx::Error> {
    let slugs: Vec<String> = [path, path.rsplit('/').next().unwrap_or(path)]
        .into_iter()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    let rows: Vec<(String, String, Option<i64>)> = sqlx::query_as(
        "select page_id, slug, story_id from pages
         where page_id <> $1 and status in ('ready', 'published') and (slug = any($2) or story_id = $3)",
    )
    .bind(page_id)
    .bind(&slugs)
    .bind(story)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(page_id, slug, story_id)| Duplicate {
            reason: if story.is_some() && story_id == story {
                "event key (the story already has a page)"
            } else {
                "slug"
            }
            .into(),
            page_id,
            slug,
        })
        .collect())
}

/// The stored text blocks the bundle's quotes point at (C6).
async fn text_blocks(
    db: &news_store::PgPool,
    b: &Bundle,
) -> Result<BTreeMap<(i64, i64), String>, sqlx::Error> {
    let keys: Vec<(i64, i64)> = b
        .data_map("quotes")
        .into_iter()
        .flatten()
        .filter_map(|(_, q)| {
            Some((
                q["locator"]["doc"].as_i64()?,
                q["locator"]["block"].as_i64()?,
            ))
        })
        .collect();
    if keys.is_empty() {
        return Ok(BTreeMap::new());
    }
    let docs: Vec<i64> = keys.iter().map(|k| k.0).collect();
    let blocks: Vec<i32> = keys
        .iter()
        .filter_map(|k| i32::try_from(k.1).ok())
        .collect();
    let rows: Vec<(i64, i32, String)> = sqlx::query_as(
        "select t.doc_id, t.block_no, t.text from text_blocks t
         join unnest($1::bigint[], $2::int[]) as k(doc_id, block_no) using (doc_id, block_no)",
    )
    .bind(&docs)
    .bind(&blocks)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(d, n, t)| ((d, i64::from(n)), t))
        .collect())
}

pub async fn check(story: &str) -> NewsResult<Reply> {
    let (dir, story_id) = locate(story)?;
    let bundle = Bundle::load(&dir);
    let page_id = bundle.page_id();

    // The database: duplicates (C14) and text blocks (C6). A story id needs it; a bundle directory
    // checked on a machine without one skips C14 and cannot verify quotes.
    let mut warnings = Vec::new();
    let (dups, blocks) = match database(1).await {
        Ok((db, _)) => {
            let dups = duplicates(&db, &page_id, bundle.meta_str("path"), story_id)
                .await
                .map_err(db_err)?;
            let blocks = text_blocks(&db, &bundle).await.map_err(db_err)?;
            (Some(dups), blocks)
        }
        Err(e) if story_id.is_none() => {
            warnings.push(format!(
                "no database ({}): C14 skipped, quotes cannot be checked",
                e.message
            ));
            (None, BTreeMap::new())
        }
        Err(e) => return Err(e),
    };

    let lexicon = Lexicon::v1().map_err(NewsError::internal)?;
    let site = std::env::var("NEWS_SITE_DIR")
        .ok()
        .filter(|d| !d.is_empty())
        .map_or_else(|| PathBuf::from("site"), PathBuf::from);
    let cache = work_dir().join("cache").join("proof-links.json");
    let markup = tokio::task::spawn_blocking(move || {
        news_wire::runner::install_tls_provider();
        let links = if off("NEWS_PROOF_LINKS") {
            None
        } else {
            HttpProbe::new()
                .ok()
                .map(|p| CachedLinks::new(p, Some(cache), lexicon.c10.cache_hours, Utc::now()))
        };
        let env = Env {
            lexicon: &lexicon,
            text_blocks: &blocks,
            duplicates: dups.as_deref(),
            links: links
                .as_ref()
                .map(|l| l as &dyn news_proof::links::LinkChecker),
            axe: &BunAxe { site_dir: site },
            axe_mode: if off("NEWS_PROOF_AXE") {
                AxeMode::Off
            } else {
                AxeMode::Auto
            },
        };
        let markup = checks::run(&bundle, &env);
        if let Some(l) = &links {
            let _ = l.save();
        }
        markup
    })
    .await
    .map_err(|e| NewsError::internal(e.to_string()))?;

    record(&markup).await;

    let data = serde_json::to_value(&markup).map_err(|e| NewsError::internal(e.to_string()))?;
    if markup.pass {
        let mut reply = Reply::new(data);
        reply.warnings = warnings;
        return Ok(reply);
    }
    Err(NewsError::validation(format!(
        "page {} failed {} check(s): {}",
        markup.page_id,
        markup.checks.iter().filter(|c| c.status == "fail").count(),
        markup
            .checks
            .iter()
            .filter(|c| c.status == "fail")
            .map(|c| c.check.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ))
    .with_hint("fix each failure's `where`, then run `news check` again")
    .with_details(data))
}

/// One `news.check_results` row per check. Telemetry never fails the command.
async fn record(m: &Markup) {
    let Some(ch) = ClickHouse::from_env() else {
        return;
    };
    let run_id = std::env::var(news_telemetry::RUN_ID_VAR).unwrap_or_default();
    let now = Utc::now();
    let rows: Vec<CheckResultRow> = m
        .checks
        .iter()
        .map(|c| {
            let message = m
                .failures
                .iter()
                .chain(&m.warnings)
                .filter(|f| f.check == c.check)
                .map(|f| format!("{}: {}", f.at, f.message))
                .collect::<Vec<_>>()
                .join("; ");
            CheckResultRow {
                ts: now,
                run_id: run_id.clone(),
                page_id: m.page_id.clone(),
                check_id: c.check.clone(),
                pass: u8::from(c.status != "fail"),
                message: if message.is_empty() {
                    c.note.clone().unwrap_or_default()
                } else {
                    message.chars().take(1000).collect()
                },
            }
        })
        .collect();
    let sent = tokio::task::spawn_blocking(move || ch.insert_rows("news.check_results", &rows))
        .await
        .unwrap_or_else(|e| Err(e.to_string()));
    if let Err(e) = sent {
        tracing::debug!(error = %e, "check results not written to ClickHouse");
    }
}
