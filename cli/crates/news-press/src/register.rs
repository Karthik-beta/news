//! Press's `register` step: the handoff from Proof's rendered bundle to Press's layout.
//!
//! Proof (`news render`) writes the bundle into the story's page folder, `work/pages/<story>/`, with
//! `manifest.json` beside the files. Press reads `work/pages/<page_id>/v<n>/` and a `pages` row. [`register`]
//! copies the first into the second, verifies the copy with [`bundle::load`], and records the bundle: a new
//! page is a `pages` row (status `draft`); the next version of a live page is a `page_drafts` row, and the
//! page stays `published` until the draft is sent. `page_refs` come from `meta.json`'s `data_ids` and the
//! `sr_` series of `data.json`. One transaction; the same bundle again changes nothing.

use std::path::Path;

use news_core::{NewsError, NewsResult};
use news_store::{PgPool, map_err};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::bundle;

/// Where the rendered bundle is: `work/pages/<story>/`.
fn story_dir(work: &Path, story: &str) -> std::path::PathBuf {
    work.join("pages").join(story)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registered {
    pub page_id: String,
    pub version: i32,
    pub bundle_sha: [u8; 32],
    /// False when the page was already registered with this bundle.
    pub changed: bool,
}

fn read_json(p: &Path) -> NewsResult<Value> {
    let text = std::fs::read_to_string(p)
        .map_err(|e| NewsError::not_found(format!("cannot read {}: {e}", p.display())))?;
    serde_json::from_str(&text)
        .map_err(|e| NewsError::validation(format!("{} is not JSON: {e}", p.display())))
}

/// Copy the bundle the manifest lists into `v<n>/`, unless a verified copy of the same bundle is there.
fn copy_bundle(
    work: &Path,
    from: &Path,
    page_id: &str,
    version: i32,
    manifest: &Value,
) -> NewsResult<[u8; 32]> {
    let sha_text = manifest["bundle_sha256"].as_str().unwrap_or_default();
    if let Ok(b) = bundle::load(work, page_id, version)
        && news_chief::gates::hex(&b.bundle_sha) == sha_text
    {
        return Ok(b.bundle_sha);
    }
    // Every source is vetted before anything is removed or copied. The path allowlist runs before the
    // disk is touched, as in `bundle::load`; `fs::copy` follows links, so anything that is not a regular
    // file (a symlink, a linked `charts`) is refused.
    let mut paths = vec!["manifest.json"];
    for f in manifest["files"].as_array().map_or(&[][..], Vec::as_slice) {
        let path = f["path"].as_str().unwrap_or_default();
        if !bundle::is_bundle_path(path) {
            return Err(NewsError::validation(format!(
                "manifest path {path:?} is not a bundle path"
            )));
        }
        paths.push(path);
    }
    for path in &paths {
        let charts_ok = !path.starts_with("charts/")
            || std::fs::symlink_metadata(from.join("charts")).is_ok_and(|m| m.is_dir());
        if !(charts_ok && std::fs::symlink_metadata(from.join(path)).is_ok_and(|m| m.is_file())) {
            return Err(NewsError::validation(format!(
                "{path} in the story folder is not a regular file"
            )));
        }
    }
    let to = bundle::dir(work, page_id, version);
    let io =
        |e: std::io::Error| NewsError::internal(format!("cannot copy to {}: {e}", to.display()));
    match std::fs::remove_dir_all(&to) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io(e)),
    }
    std::fs::create_dir_all(&to).map_err(io)?;
    for path in paths {
        let target = to.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        std::fs::copy(from.join(path), &target).map_err(io)?;
    }
    let b = bundle::load(work, page_id, version).map_err(NewsError::validation)?;
    Ok(b.bundle_sha)
}

/// Register story `story_id`'s rendered bundle (folder `story`) as a draft page of `run`, the run that
/// checked it: only that run publishes it. The bundle's version is the page's next one: 1 for a new
/// page, the published version plus one for a revision; anything else is a conflict, so a stale folder
/// never overwrites a newer page. `checked` is the `bundle_sha256` Proof passed: other bytes are refused,
/// and a bundle already staged for sending (`ready`) is never replaced.
pub async fn register(
    db: &PgPool,
    work: &Path,
    story: &str,
    story_id: i64,
    run: Option<Uuid>,
    checked: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> NewsResult<Registered> {
    let dir = story_dir(work, story);
    let manifest = read_json(&dir.join("manifest.json"))
        .map_err(|e| e.with_hint("render the story first (`news render`)"))?;
    let meta = read_json(&dir.join("meta.json"))?;
    let page_id = manifest["page_id"].as_str().unwrap_or_default().to_owned();
    if !bundle::is_page_id(&page_id) || meta["id"] != page_id.as_str() {
        return Err(NewsError::validation(format!(
            "manifest and meta disagree on the page id ({page_id:?})"
        )));
    }
    let version = i32::try_from(manifest["version"].as_i64().unwrap_or(0))
        .ok()
        .filter(|v| *v >= 1)
        .ok_or_else(|| NewsError::validation("the manifest has no version"))?;
    let not_checked = || {
        NewsError::conflict(format!(
            "the bundle of page {page_id} is not the one Proof checked; check it again"
        ))
    };
    if manifest["bundle_sha256"].as_str() != Some(checked) {
        return Err(not_checked());
    }
    // Before the copy replaces v<n>/: a bundle staged for sending keeps its bytes on disk.
    let staged: Option<Vec<u8>> = sqlx::query_scalar(
        "select bundle_sha from pages where page_id = $1 and version = $2 and status = 'ready'
         union all
         select bundle_sha from page_drafts where page_id = $1 and version = $2 and state = 'ready'
         limit 1",
    )
    .bind(&page_id)
    .bind(version)
    .fetch_optional(db)
    .await
    .map_err(map_err)?;
    if staged.is_some_and(|s| news_chief::gates::hex(&s) != checked) {
        return Err(staged_conflict(&page_id));
    }
    let sha = copy_bundle(work, &dir, &page_id, version, &manifest)?;
    if news_chief::gates::hex(&sha) != checked {
        return Err(not_checked());
    }
    let sha_vec = sha.to_vec();

    let mut tx = db.begin().await.map_err(map_err)?;
    let row =
        sqlx::query("select version, status, bundle_sha from pages where page_id = $1 for update")
            .bind(&page_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map_err)?;
    let tier = meta["tier"].as_i64().and_then(|t| i16::try_from(t).ok());
    let flags: Vec<String> = meta["flags"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| f.as_str().map(str::to_owned))
        .collect();
    let oui = std::fs::read(dir.join("page.oui")).unwrap_or_default();
    let oui_sha = Sha256::digest(&oui).to_vec();
    let changed = match row {
        None => {
            if version != 1 {
                return Err(NewsError::conflict(format!(
                    "page {page_id} is new but its bundle is version {version}"
                )));
            }
            sqlx::query(
                "insert into pages (page_id, slug, story_id, page_type, tier, version, status, updated_at,
                                    renderer_version, bundle_sha, oui_sha, flags, run_id)
                 values ($1, $1, $2, $3, $4, 1, 'draft', $5, $6, $7, $8, $9, $10)",
            )
            .bind(&page_id)
            .bind(story_id)
            .bind(meta["page_type"].as_str())
            .bind(tier)
            .bind(now)
            .bind(meta["renderer_version"].as_str())
            .bind(&sha_vec)
            .bind(&oui_sha)
            .bind(&flags)
            .bind(run)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
            true
        }
        Some(r) => {
            let (have, status, have_sha): (i32, String, Option<Vec<u8>>) =
                (r.get("version"), r.get("status"), r.get("bundle_sha"));
            if have == version && have_sha.as_deref() == Some(&sha_vec[..]) && status != "withdrawn"
            {
                // The same bytes, checked by this run: an unsent draft becomes this run's to publish.
                sqlx::query("update pages set run_id = $2 where page_id = $1 and status = 'draft'")
                    .bind(&page_id)
                    .bind(run)
                    .execute(&mut *tx)
                    .await
                    .map_err(map_err)?;
                false
            } else if have == version && status == "ready" {
                return Err(staged_conflict(&page_id));
            } else if have == version && status == "draft" {
                // A re-render of the unsent version: the new bytes replace the old.
                sqlx::query(
                    "update pages set status = 'draft', bundle_sha = $2, oui_sha = $3, renderer_version = $4,
                            flags = $5, updated_at = $6, run_id = $7 where page_id = $1",
                )
                .bind(&page_id)
                .bind(&sha_vec)
                .bind(&oui_sha)
                .bind(meta["renderer_version"].as_str())
                .bind(&flags)
                .bind(now)
                .bind(run)
                .execute(&mut *tx)
                .await
                .map_err(map_err)?;
                true
            } else if version == have + 1 && status == "published" {
                // The live page stays published; the next version waits in `page_drafts`.
                let pending: Option<(i32, Vec<u8>, String)> = sqlx::query_as(
                    "select version, bundle_sha, state from page_drafts where page_id = $1 for update",
                )
                .bind(&page_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(map_err)?;
                match pending {
                    Some((v, s, _)) if v == version && s == sha_vec => {
                        sqlx::query(
                            "update page_drafts set run_id = $2 where page_id = $1 and state = 'draft'",
                        )
                        .bind(&page_id)
                        .bind(run)
                        .execute(&mut *tx)
                        .await
                        .map_err(map_err)?;
                        false
                    }
                    Some((_, _, st)) if st == "ready" => return Err(staged_conflict(&page_id)),
                    _ => {
                        sqlx::query(
                            "insert into page_drafts (page_id, version, bundle_sha, oui_sha, renderer_version, tier,
                                                      page_type, flags, state, updated_at, run_id)
                             values ($1, $2, $3, $4, $5, $6, $7, $8, 'draft', $9, $10)
                             on conflict (page_id) do update set version = excluded.version,
                                bundle_sha = excluded.bundle_sha, oui_sha = excluded.oui_sha,
                                renderer_version = excluded.renderer_version, tier = excluded.tier,
                                page_type = excluded.page_type, flags = excluded.flags, state = 'draft',
                                updated_at = excluded.updated_at, run_id = excluded.run_id",
                        )
                        .bind(&page_id)
                        .bind(version)
                        .bind(&sha_vec)
                        .bind(&oui_sha)
                        .bind(meta["renderer_version"].as_str())
                        .bind(tier)
                        .bind(meta["page_type"].as_str())
                        .bind(&flags)
                        .bind(now)
                        .bind(run)
                        .execute(&mut *tx)
                        .await
                        .map_err(map_err)?;
                        true
                    }
                }
            } else {
                return Err(NewsError::conflict(format!(
                    "page {page_id} is at version {have} ({status}); the bundle is version {version}"
                )));
            }
        }
    };
    // The refs this version uses: data and derived points from meta.json, series from data.json.
    let mut refs: Vec<String> = meta["data_ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r.as_str().map(str::to_owned))
        .collect();
    if let Ok(data) = read_json(&dir.join("data.json"))
        && let Some(series) = data["series"].as_object()
    {
        refs.extend(series.keys().filter(|k| k.starts_with("sr_")).cloned());
    }
    refs.sort();
    refs.dedup();
    if changed {
        sqlx::query("delete from page_refs where page_id = $1 and version = $2")
            .bind(&page_id)
            .bind(version)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
    }
    sqlx::query(
        "insert into page_refs (page_id, version, ref) select $1, $2, r from unnest($3::text[]) as r
         on conflict do nothing",
    )
    .bind(&page_id)
    .bind(version)
    .bind(&refs)
    .execute(&mut *tx)
    .await
    .map_err(map_err)?;
    sqlx::query("update stories set page_id = $1 where story_id = $2 and page_id is null")
        .bind(&page_id)
        .bind(story_id)
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;
    tx.commit().await.map_err(map_err)?;
    Ok(Registered {
        page_id,
        version,
        bundle_sha: sha,
        changed,
    })
}

fn staged_conflict(page_id: &str) -> NewsError {
    NewsError::conflict(format!(
        "page {page_id} has a bundle staged for sending; it cannot be replaced until it is sent"
    ))
}

/// Throw away the unsent next version of a live page (a rejected or dead revision): the draft and its
/// refs go, and the live page stays as it was. A draft already staged for sending is left to the outbox.
/// Returns whether a draft was discarded.
pub async fn discard_draft(db: &PgPool, page_id: &str) -> NewsResult<bool> {
    let mut tx = db.begin().await.map_err(map_err)?;
    let gone: Option<i32> = sqlx::query_scalar(
        "delete from page_drafts where page_id = $1 and state = 'draft' returning version",
    )
    .bind(page_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_err)?;
    if let Some(v) = gone {
        sqlx::query("delete from page_refs where page_id = $1 and version = $2")
            .bind(page_id)
            .bind(v)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
    }
    tx.commit().await.map_err(map_err)?;
    Ok(gone.is_some())
}
