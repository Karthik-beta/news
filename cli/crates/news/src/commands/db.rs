//! `news db migrate|status`.

use super::{Reply, database};
use crate::cli::DbCmd;
use news_core::NewsResult;
use serde_json::json;

pub async fn run(cmd: DbCmd) -> NewsResult<Reply> {
    let (pool, cfg) = database(2).await?;
    match cmd {
        DbCmd::Migrate => {
            let report = news_store::migrate(&pool, &cfg.schema).await?;
            // The `ai` pool follows NEWS_PARALLEL (§5.7).
            let parallel = std::env::var("NEWS_PARALLEL")
                .ok()
                .and_then(|v| v.parse::<i16>().ok())
                .filter(|n| (1..=32).contains(n));
            if let Some(n) = parallel {
                news_dispatch::queue::set_max_running(&pool, "ai", n).await?;
            }
            Ok(Reply::new(json!({
                "schema": report.schema,
                "applied": report.applied,
                "current": report.current,
                "ai_max_running": parallel,
            })))
        }
        DbCmd::Status => {
            let applied = news_store::applied_versions(&pool).await?;
            let known = news_store::known_versions();
            let pending: Vec<i64> = known
                .iter()
                .copied()
                .filter(|v| !applied.contains(v))
                .collect();
            Ok(Reply::new(json!({
                "schema": cfg.schema,
                "applied": applied,
                "pending": pending,
                "up_to_date": pending.is_empty(),
            })))
        }
    }
}
