//! `news prune`: delete the working files of published items (Technical Design, "Local folders").
//!
//! For each published page whose current version was sent: its bundle and rendered output in
//! `work/pages/<page_id>/`, and its story's brief and parsed data in `work/briefs/<story>.json` and
//! `work/parsed/<story>/`. `work/raw/` is not touched: the Morgue keeps source snapshots by retention
//! (400 days for structured data, 90 for documents) so every figure stays provable.

use std::path::Path;

use news_core::NewsResult;

use crate::bundle;
use news_store::{PgPool, map_err};
use sqlx::Row;

/// Files and bytes under `path` (the file itself, or a directory's files); 0 when it does not exist.
fn size(path: &Path) -> (u64, u64) {
    match std::fs::metadata(path) {
        Ok(m) if m.is_dir() => std::fs::read_dir(path).map_or((0, 0), |d| {
            d.flatten()
                .map(|e| size(&e.path()))
                .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1))
        }),
        Ok(m) => (1, m.len()),
        Err(_) => (0, 0),
    }
}

/// Remove `path` (file or directory) and say what went: `(files, bytes)`, counted only if it is gone.
fn remove(path: &Path) -> (u64, u64) {
    let counted = size(path);
    let gone = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    if gone.is_ok() { counted } else { (0, 0) }
}

/// Returns `(deleted files, bytes freed)`.
pub async fn prune(db: &PgPool, work: &Path) -> NewsResult<(u64, u64)> {
    let rows = sqlx::query(
        "select p.page_id, p.story_id from pages p
          where p.status = 'published'
            and exists (select 1 from publish_outbox o
                         where o.page_id = p.page_id and o.version = p.version
                           and o.op = 'put_page' and o.sent_at is not null)",
    )
    .fetch_all(db)
    .await
    .map_err(map_err)?;
    let (mut files, mut bytes) = (0, 0);
    let mut add = |(f, b): (u64, u64)| {
        files += f;
        bytes += b;
    };
    for r in &rows {
        let id: String = r.get("page_id");
        if bundle::is_page_id(&id) {
            add(remove(&work.join("pages").join(&id)));
        }
        if let Some(story) = r.get::<Option<i64>, _>("story_id") {
            add(remove(&work.join("briefs").join(format!("{story}.json"))));
            add(remove(&work.join("parsed").join(story.to_string())));
        }
    }
    Ok((files, bytes))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn remove_counts_what_it_deletes_and_ignores_what_is_missing() {
        let d = std::env::temp_dir().join(format!("news-press-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("p/v1/charts")).unwrap();
        std::fs::write(d.join("p/v1/a.json"), "12345").unwrap();
        std::fs::write(d.join("p/v1/charts/b.csv"), "123").unwrap();
        assert_eq!(remove(&d.join("p")), (2, 8));
        assert!(!d.join("p").exists());
        assert_eq!(remove(&d.join("p")), (0, 0));
        let _ = std::fs::remove_dir_all(&d);
    }
}
