//! `home.rollback`: make an earlier curation live again (Bureau §6.4, H tier). Board sends it over the
//! Hotline; the command has no CLI form. It calls `POST /v1/home/curation/rollback` and then records
//! the new live version, the same way [`crate::api::publish`] records a publish.
//!
//! The write API keeps an `Idempotency-Key` for days and replays the stored reply, so a key names one
//! attempt, never a version: v1, then v2, then v1 again must send a new key the third time. Each
//! attempt is a `home_rollbacks` row written before the send ([`begin`]); a resend after a crash
//! finds the open row for the same version and the same live draft and sends its key again.

use crate::api::{Retry, WriteApi, send_to};
use news_core::{ErrorCode, NewsError, NewsResult};
use news_store::{PgPool, map_err};
use serde_json::{Value, json};
use sqlx::Row;

/// The request body (`write-api.schema.json#/$defs/rollbackRequest`).
pub fn body(version: i32) -> Vec<u8> {
    json!({"version": version}).to_string().into_bytes()
}

/// What [`begin`] decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Begun {
    /// The version is live already: nothing to send.
    AlreadyLive,
    /// Send the request under `key`; `draft_id` is the curation that becomes live.
    Send { draft_id: i64, key: String },
}

/// Check the version is one this laptop published, and open (or find) this attempt's row. Only a
/// version that was published or superseded can be named, so the key is read and the API called only
/// for a real curation.
pub async fn begin(db: &PgPool, version: i32) -> NewsResult<Begun> {
    let target: Option<(i64, String)> = sqlx::query(
        "select draft_id, status from home_curations
          where version = $1 and status in ('published', 'superseded')",
    )
    .bind(version)
    .fetch_optional(db)
    .await
    .map_err(map_err)?
    .map(|r| (r.get("draft_id"), r.get("status")));
    let Some((draft_id, status)) = target else {
        return Err(NewsError::not_found(format!(
            "no published curation has version {version}"
        )));
    };
    if status == "published" {
        return Ok(Begun::AlreadyLive);
    }
    let from: i64 = sqlx::query_scalar(
        "select coalesce(max(draft_id), 0) from home_curations where status = 'published'",
    )
    .fetch_one(db)
    .await
    .map_err(map_err)?;
    sqlx::query(
        "insert into home_rollbacks (version, from_draft) values ($1, $2)
         on conflict (version, from_draft) where done_at is null do nothing",
    )
    .bind(version)
    .bind(from)
    .execute(db)
    .await
    .map_err(map_err)?;
    let key: String = sqlx::query_scalar(
        "select idempotency_key from home_rollbacks
          where version = $1 and from_draft = $2 and done_at is null",
    )
    .bind(version)
    .bind(from)
    .fetch_one(db)
    .await
    .map_err(map_err)?;
    Ok(Begun::Send { draft_id, key })
}

/// Make `version` live. Naming the live version changes nothing.
pub async fn rollback<A: WriteApi>(
    db: &PgPool,
    version: i32,
    api: &A,
    read_key: impl FnOnce() -> NewsResult<String>,
    retry: Retry,
) -> NewsResult<Value> {
    let (draft_id, key) = match begin(db, version).await? {
        Begun::AlreadyLive => {
            return Ok(
                json!({"version": version, "live_version": version, "already_live": true, "purged": []}),
            );
        }
        Begun::Send { draft_id, key } => (draft_id, key),
    };
    let bearer = read_key()?;
    let reply = match send_to(api, true, &key, &bearer, &body(version), retry).await {
        Ok(r) => r,
        // No answer after every try: the same attempt, the same key, on the next request.
        Err(e) if e.code == ErrorCode::UpstreamUnavailable => return Err(e),
        // The API answered and refused: that attempt is over, a retry is a new request.
        Err(e) => {
            finish(db, &key, None, false).await?;
            return Err(e);
        }
    };
    // What the API did, stored before anything local changes: a crash from here on sends this key
    // again and the API replays this reply. A reply for another version ends the attempt.
    let asked = reply.live_version == version;
    let response = json!({"version": reply.version, "live_version": reply.live_version});
    finish(db, &key, Some(response), asked).await?;
    if !asked {
        return Err(NewsError::conflict(format!(
            "the write API has curation v{} live, not v{version}",
            reply.live_version
        ))
        .with_hint("the site and this laptop disagree: check the live version in Board"));
    }
    mark_live(db, draft_id, &key).await?;
    Ok(json!({
        "version": reply.version,
        "live_version": reply.live_version,
        "already_live": false,
        "purged": reply.purged,
    }))
}

/// Record the API's answer on the attempt. `open` keeps it open (the answer is the one asked for and
/// the local record is still to be written); otherwise the attempt is done.
async fn finish(db: &PgPool, key: &str, response: Option<Value>, open: bool) -> NewsResult<()> {
    sqlx::query(
        "update home_rollbacks set sent_at = now(), response = $2,
                done_at = case when $3 then null else now() end
          where idempotency_key = $1",
    )
    .bind(key)
    .bind(response)
    .bind(open)
    .execute(db)
    .await
    .map_err(map_err)?;
    Ok(())
}

/// The write API has this draft's curation live: it is the one `published` curation again, the curation
/// it replaced is `superseded`, and the attempt is done.
async fn mark_live(db: &PgPool, draft_id: i64, key: &str) -> NewsResult<()> {
    let mut tx = db.begin().await.map_err(map_err)?;
    sqlx::query(
        "update home_curations set status = 'superseded' where status = 'published' and draft_id <> $1",
    )
    .bind(draft_id)
    .execute(&mut *tx)
    .await
    .map_err(map_err)?;
    sqlx::query("update home_curations set status = 'published' where draft_id = $1")
        .bind(draft_id)
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;
    sqlx::query("update home_rollbacks set done_at = now() where idempotency_key = $1")
        .bind(key)
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;
    tx.commit().await.map_err(map_err)
}
