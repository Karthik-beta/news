//! `news brief <story>` and `news related <story>` (Fixer, Bureau §3.8, §6.4). Both are R tier: the
//! brief is built and printed, never saved; the `item.brief` job saves it (`news_fixer::db::save`).

use super::{Reply, database};
use news_core::NewsResult;

pub async fn brief(story: &str) -> NewsResult<Reply> {
    let (db, _) = database(2).await?;
    let inputs = news_fixer::db::load(&db, story).await?;
    let built = news_fixer::brief(&inputs)?;
    let mut reply = Reply::new(
        serde_json::to_value(&built.brief)
            .map_err(|e| news_core::NewsError::internal(e.to_string()))?,
    );
    if !built.trimmed.is_empty() {
        reply.warnings.push(format!(
            "{} facts or series left out to fit the budget: {}",
            built.trimmed.len(),
            built.trimmed.join(", ")
        ));
    }
    if built.condense.is_some() {
        reply.warnings.push(
            "source text condensed by code to fit; the item.brief job requests A5 (ai.condense)"
                .to_owned(),
        );
    }
    Ok(reply)
}

pub async fn related(story: &str) -> NewsResult<Reply> {
    let (db, _) = database(2).await?;
    Ok(Reply::new(news_fixer::db::related(&db, story).await?))
}
