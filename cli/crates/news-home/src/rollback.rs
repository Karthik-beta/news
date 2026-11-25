//! `home.rollback`: make an earlier curation live again (Bureau §6.4, H tier). Board sends it over the
//! Hotline; the command has no CLI form. It calls `POST /v1/home/curation/rollback` and then records
//! the new live version, the same way [`crate::api::publish`] records a publish.

use crate::api::{CurationReply, Retry, WriteApi, send_to};
use news_core::{NewsError, NewsResult};
use news_store::{PgPool, map_err};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;

/// `Idempotency-Key` for rolling back to `version` while draft `from` is live (0 when none is). A resend
/// after a crash has the same live draft, so the same key; a later rollback to the same version, after
/// something else went live, has a different one and is a new request.
pub fn idempotency_key(version: i32, from: i64) -> String {
    let body = body(version);
    let sha: String = Sha256::digest(&body)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("home-rollback:{version}:{from}:{}", &sha[..16])
}

/// The request body (`write-api.schema.json#/$defs/rollbackRequest`).
pub fn body(version: i32) -> Vec<u8> {
    json!({"version": version}).to_string().into_bytes()
}

/// Make `version` live. Only a version this laptop knows (published or superseded) can be named, so the
/// key is read and the API called only for a real curation. Naming the live version changes nothing.
pub async fn rollback<A: WriteApi>(
    db: &PgPool,
    version: i32,
    api: &A,
    read_key: impl FnOnce() -> NewsResult<String>,
    retry: Retry,
) -> NewsResult<Value> {
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
        return Ok(
            json!({"version": version, "live_version": version, "draft_id": draft_id, "already_live": true, "purged": []}),
        );
    }
    let from: i64 = sqlx::query_scalar(
        "select coalesce(max(draft_id), 0) from home_curations where status = 'published'",
    )
    .fetch_one(db)
    .await
    .map_err(map_err)?;
    let key = idempotency_key(version, from);
    let bearer = read_key()?;
    let reply: CurationReply = send_to(api, true, &key, &bearer, &body(version), retry).await?;
    mark_live(db, draft_id).await?;
    Ok(json!({
        "version": reply.version,
        "live_version": reply.live_version,
        "draft_id": draft_id,
        "already_live": false,
        "purged": reply.purged,
        "idempotency_key": key,
    }))
}

/// The write API has this draft's curation live: it is the one `published` curation again and the
/// curation it replaced is `superseded`.
async fn mark_live(db: &PgPool, draft_id: i64) -> NewsResult<()> {
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
    tx.commit().await.map_err(map_err)
}
