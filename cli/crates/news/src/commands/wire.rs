//! `news discover|fetch|parse <source>`: Wire and Clerk from the terminal (tier W; #9).
//!
//! Each command is a longer prefix of one pass, run directly like the other commands that write the
//! local database: `discover` lists what the source has published, `fetch` also downloads each item
//! into the Morgue, `parse` also parses and stores data points and candidates. Every step is idempotent,
//! so running one twice changes nothing the second time. They share [`news::ingest`] with the service's
//! `source.discover` job, and read the listing again each time (a terminal run keeps no cursor).
//!
//! `NEWS_WIRE_FIXTURES=<routes.json>` answers every request from recorded files; see `news::ingest::fixtures`.

use std::path::PathBuf;

use news::ingest::{Ingest, IngestError, Source, info};
use news_clerk::candidates::Candidate;
use news_core::{NewsError, NewsResult};
use serde_json::{Value, json};

use super::{Reply, database};

async fn ingest() -> NewsResult<Ingest> {
    let (db, _) = database(4).await?;
    let work = std::env::var_os("NEWS_WORK_DIR").map_or_else(|| "work".into(), PathBuf::from);
    let mut ing = Ingest::from_env(db, work).map_err(NewsError::precondition)?;
    ing.fresh_listings = true;
    Ok(ing)
}

/// A connector with nothing tracked asks for nothing; say so instead of reporting an empty source as
/// a quiet one.
fn untracked(s: &Source) -> Option<String> {
    s.conn.discovery_requests(&s.cx, None).is_empty().then(|| {
        format!(
            "{} has no tracked ids, so there was nothing to ask about (NEWS_TRACKED_{})",
            s.id,
            s.conn.id().to_uppercase().replace('-', "_")
        )
    })
}

fn ref_row(r: &news_connectors::ItemRef) -> Value {
    json!({
        "source_id": r.source_id, "key": r.key, "kind": r.kind, "event_key": r.event_key_hint,
        "title": r.title, "url": r.url, "published_at": r.published_at,
    })
}

fn candidate_row(key: &str, c: &Candidate) -> Value {
    json!({
        "source_id": c.source_id, "key": key, "event_key": c.draft.event_key,
        "title": c.draft.title, "url": c.draft.url, "published_at": c.draft.published_at,
        "page_type": c.page_type, "page_type_by": c.page_type_by,
        "categories": c.categories, "region": c.region, "data_points": c.draft.dp_ids.len(),
    })
}

pub async fn discover(source: Option<String>) -> NewsResult<Reply> {
    let ing = ingest().await?;
    let ids: Vec<String> = match &source {
        Some(id) => vec![id.clone()],
        None => {
            sqlx::query_scalar("select source_id from sources where enabled order by source_id")
                .fetch_all(&ing.db)
                .await
                .map_err(news_store::map_err)?
        }
    };
    let (mut items, mut by_source, mut calendar) = (Vec::new(), serde_json::Map::new(), 0);
    let mut warnings = Vec::new();
    for id in &ids {
        // Named on the command line, a failure is the command's; in the sweep it is that source's.
        let pass = async {
            let s = ing.source(id).await?;
            let d = ing.discover(&s).await?;
            Ok::<_, IngestError>((untracked(&s), d))
        }
        .await;
        match pass {
            Ok((note, d)) => {
                warnings.extend(note);
                by_source.insert(id.clone(), json!(d.refs.len()));
                calendar += d.calendar.len();
                items.extend(d.refs.iter().map(ref_row));
            }
            Err(e) if source.is_none() => warnings.push(format!("{id}: {}", NewsError::from(e))),
            Err(e) => return Err(e.into()),
        }
    }
    let mut reply = Reply::new(json!({
        "items": items, "by_source": by_source, "calendar_entries": calendar,
    }));
    reply.warnings = warnings;
    Ok(reply)
}

pub async fn fetch(source: &str) -> NewsResult<Reply> {
    let ing = ingest().await?;
    let s = ing.source(source).await?;
    let d = ing.discover(&s).await?;
    let (mut items, mut snapshots, mut changed) = (Vec::new(), 0, 0);
    for r in &d.refs {
        let got = ing.fetch(&s, r).await?;
        snapshots += got.snapshots.len();
        changed += usize::from(got.changed);
        items.push(json!({
            "source_id": r.source_id, "key": r.key, "event_key": r.event_key_hint,
            "changed": got.changed, "snapshots": info(&got.snapshots),
        }));
    }
    let mut reply = Reply::new(json!({"items": items, "snapshots": snapshots, "changed": changed}));
    reply.warnings.extend(untracked(&s));
    Ok(reply)
}

pub async fn parse(source: &str) -> NewsResult<Reply> {
    let ing = ingest().await?;
    let s = ing.source(source).await?;
    let pass = ing.pass(&s).await?;
    let mut items = Vec::new();
    for done in &pass.items {
        items.extend(
            done.parsed
                .candidates
                .iter()
                .map(|c| candidate_row(&done.item.key, c)),
        );
    }
    let points: usize = pass.items.iter().map(|i| i.parsed.points).sum();
    let mut reply = Reply::new(json!({
        "items": items, "parsed": pass.items.len(), "points": points, "stored": pass.totals(),
    }));
    reply.warnings.extend(untracked(&s));
    reply.warnings.extend(
        pass.failed
            .iter()
            .map(|(k, f)| format!("{k}: {}: {}", f.class.as_str(), f.message)),
    );
    Ok(reply)
}
