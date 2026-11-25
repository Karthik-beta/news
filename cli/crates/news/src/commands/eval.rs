//! `news eval run|report`: Critic (Bureau §6.4, §9.1). `eval promote` is G tier and gated here.

use super::{Reply, database};
use news_core::NewsResult;

/// `news eval run <suite> [--prompt-version v]`.
pub async fn run(suite: &str, prompt_version: Option<&str>) -> NewsResult<Reply> {
    let root = news_critic::golden::find_root()?;
    let (db, _) = database(2).await?;
    Ok(Reply::new(
        news_critic::eval::run(&db, &root, suite, prompt_version).await?,
    ))
}

/// `news eval report <eval_run>`.
pub async fn report(eval_run: &str) -> NewsResult<Reply> {
    let (db, _) = database(2).await?;
    Ok(Reply::new(news_critic::eval::report(&db, eval_run).await?))
}
