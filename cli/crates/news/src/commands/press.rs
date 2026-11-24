//! `news prune` and `news stats`: Press's local commands (Bureau §3.12). Publishing itself
//! (`news publish`, `news home publish`) has no CLI form: Chief runs it in process as the authorised
//! step of a live run (see `run::gated`).

use super::{Reply, database};
use news_core::NewsResult;
use serde_json::json;

/// `NEWS_WORK_DIR`, default `work` (the Morgue's root too).
fn work_dir() -> std::path::PathBuf {
    std::env::var_os("NEWS_WORK_DIR").map_or_else(|| "work".into(), Into::into)
}

pub async fn prune() -> NewsResult<Reply> {
    let (db, _) = database(2).await?;
    let (deleted, bytes) = news_press::prune::prune(&db, &work_dir()).await?;
    Ok(Reply::new(json!({"deleted": deleted, "bytes": bytes})))
}

pub async fn stats() -> NewsResult<Reply> {
    let (db, _) = database(2).await?;
    let per_day = news_standing::current(&db)
        .await?
        .int("publish.index_per_day");
    let data = news_press::store::stats(&db, per_day, chrono::Utc::now()).await?;
    Ok(Reply::new(data))
}
