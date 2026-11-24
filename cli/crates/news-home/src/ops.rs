//! The home commands that are not G tier, as functions from inputs to the command's JSON data:
//! `save`, `check`, `preview` and `show` (candidates are in [`crate::candidates`], publish in
//! [`crate::api`]). The `news` binary only parses arguments and wraps these.

use crate::check;
use crate::draft::{Draft, body};
use crate::facts;
use crate::preview;
use crate::store::{self, Stored};
use chrono::{DateTime, Utc};
use news_core::{NewsError, NewsResult};
use news_nose::score::round2;
use news_store::PgPool;
use serde_json::{Value, json};
use std::fmt::Write as _;

pub fn parse_id(s: &str) -> NewsResult<i64> {
    s.parse().ok().filter(|n| *n > 0).ok_or_else(|| {
        NewsError::usage(format!(
            "{s:?} is not a draft number (from `news home save`)"
        ))
    })
}

/// A draft file, parsed. Shape errors name the field; counts and ID formats are `check`'s.
pub fn parse_draft(v: Value) -> NewsResult<Draft> {
    let d: Draft = serde_json::from_value(v).map_err(|e| {
        NewsError::validation(format!("not a home draft: {e}")).with_hint("news schema home save")
    })?;
    if d.curator_model.trim().is_empty() {
        return Err(NewsError::validation(
            "not a home draft: curator_model is empty",
        ));
    }
    Ok(d)
}

/// `news home save <file>`.
pub async fn save(db: &PgPool, file: Value) -> NewsResult<Value> {
    let d = parse_draft(file)?;
    // `run_id` is the preset step's to give; a terminal save has none.
    let (draft_id, created) = store::save(db, &d, None).await?;
    Ok(json!({"draft_id": draft_id, "created": created, "pages": d.page_ids().len()}))
}

/// `news home check <draft>`: run the rules on the pages as they are now and keep the outcome.
pub async fn check(db: &PgPool, draft_id: i64, now: DateTime<Utc>) -> NewsResult<Value> {
    let st = store::get(db, draft_id).await?;
    if !matches!(st.status.as_str(), "draft" | "checked" | "approved") {
        return Err(NewsError::precondition(format!(
            "draft {draft_id} is {}; it cannot be checked",
            st.status
        )));
    }
    let pages = facts::load(db, &st.draft.page_ids()).await?;
    let report = check::run(&st.draft, &pages, now);
    store::store_check(db, draft_id, &report, now).await?;
    let status = if st.status == "approved" {
        "approved"
    } else if report.pass {
        "checked"
    } else {
        "draft"
    };
    Ok(json!({
        "draft_id": draft_id,
        "status": status,
        "pass": report.pass,
        "failures": report.failures,
        "results": report.results,
    }))
}

/// The bytes a preview sends: the draft as `curation.v1`. Until it is approved its approval time is its
/// creation time, so the same draft always has the same hash.
pub fn preview_body(st: &Stored) -> NewsResult<Vec<u8>> {
    if !st.passed() || !matches!(st.status.as_str(), "checked" | "approved") {
        return Err(NewsError::precondition(format!(
            "draft {} has not passed `news home check`",
            st.draft_id
        )));
    }
    let at = st.approved_at.unwrap_or(st.created_at);
    serde_json::to_vec(&body(&st.draft, st.created_at, at, &st.results()))
        .map_err(|e| NewsError::internal(e.to_string()))
}

/// `news home preview <draft>`.
pub async fn preview(db: &PgPool, draft_id: i64, base: &str) -> NewsResult<Value> {
    let st = store::get(db, draft_id).await?;
    let bytes = preview_body(&st)?;
    let started = preview::ensure_server(base).await?;
    let sha = preview::push(base, bytes).await?;
    let url = format!("{}/", base.trim_end_matches('/'));
    Ok(json!({
        "draft_id": draft_id,
        "url": url,
        "sha256": sha,
        "server_started": started,
        "views": preview::views(&url),
    }))
}

/// `news home show`: the newest published curation and its age.
pub async fn show(db: &PgPool, now: DateTime<Utc>) -> NewsResult<Value> {
    let st = store::live(db).await?.ok_or_else(|| {
        NewsError::not_found("no curation has been published")
            .with_hint("`news home publish` runs in the Frontpage preset (Board)")
    })?;
    let at = st.published_at.unwrap_or(st.created_at);
    let age_days = round2((now - at).num_seconds() as f64 / 86_400.0);
    let mut text = format!(
        "Curation v{}, published {}, {age_days} days old\nLead: {}\nSide: {}\n",
        st.version.unwrap_or(0),
        at.format("%Y-%m-%d"),
        st.draft.slots.lead.page_id,
        st.draft
            .slots
            .side
            .iter()
            .map(|s| s.page_id.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    for s in &st.draft.slots.sections {
        let _ = writeln!(
            text,
            "{}: {}",
            s.category,
            s.pages
                .iter()
                .map(|p| p.page_id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(json!({
        "draft_id": st.draft_id,
        "version": st.version,
        "published_at": at,
        "age_days": age_days,
        "curator_model": st.draft.curator_model,
        "slots": st.draft.slots,
        "text": text.trim_end(),
    }))
}
