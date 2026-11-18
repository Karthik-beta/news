//! `news db migrate|status`.

use super::{Reply, open_database};
use crate::cli::DbCmd;
use news_core::NewsResult;
use serde_json::json;

pub async fn run(cmd: DbCmd) -> NewsResult<Reply> {
    // Not `database`: migrate creates the schema, and status reports a missing one as unmigrated.
    let (pool, cfg) = open_database(2).await?;
    match cmd {
        DbCmd::Migrate => {
            // The `ai` pool's size is the Standing key `pools.ai.max_running`; the service applies it.
            let report = news_store::migrate(&pool, &cfg.schema).await?;
            Ok(Reply::new(json!({
                "schema": report.schema,
                "applied": report.applied,
                "current": report.current,
            })))
        }
        DbCmd::Status => {
            // A missing schema would make current_schema() fall back to `public`.
            let applied = if news_store::schema_exists(&pool, &cfg.schema).await? {
                news_store::applied_versions(&pool).await?
            } else {
                Vec::new()
            };
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
