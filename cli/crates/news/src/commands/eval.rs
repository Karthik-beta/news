//! `news eval run|report|draft|export`: Critic (Bureau §6.4, §9.1). `eval promote` is G tier and gated here.
//! Karthik's labels are not here: `eval label` is H tier, so it is Board's, over the Hotline.

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

/// `news eval draft [--pairs n]`: judge items from the golden sets and G3 pairs from Taster's embeddings.
pub async fn draft(pairs: Option<u32>) -> NewsResult<Reply> {
    let root = news_critic::golden::find_root()?;
    let (db, _) = database(2).await?;
    let t = news_taster::config::v1().map_err(news_core::NewsError::internal)?;
    let items = news_critic::labels::draft_items(&db, &root).await?;
    let p = news_taster::pairs::draft(
        &db,
        &t,
        pairs.map_or(news_critic::g3::TARGET, |n| n as usize),
    )
    .await?;
    let text = format!(
        "{} judge items ({} new); {} story pairs ({} new{}).",
        items.total,
        items.added,
        p.total,
        p.added,
        if p.considered == 0 {
            ": no candidate and story embeddings sit near the join threshold yet"
        } else {
            ""
        }
    );
    Ok(Reply::new(serde_json::json!({
        "judge_items_added": items.added, "judge_items": items.total,
        "pairs_added": p.added, "pairs": p.total, "text": text,
    })))
}

/// `news eval export`: Karthik's labels into `pipeline/eval/g3/` and `judges.md`.
pub async fn export() -> NewsResult<Reply> {
    let root = news_critic::golden::find_root()?;
    let (db, _) = database(2).await?;
    let g3 = news_critic::g3::export(&db, &root).await?;
    let judges = news_critic::labels::export_judges(&db, &root).await?;
    let labels: i64 = sqlx::query_scalar("select count(*) from judge_labels")
        .fetch_one(&db)
        .await
        .map_err(news_store::map_err)?;
    let mut written = g3.written;
    if judges {
        written.push("pipeline/eval/judges.md".to_owned());
    }
    let text = format!(
        "G3: {} labelled pairs (set version {}); {labels} judge labels; {} files written.",
        g3.cases,
        g3.version,
        written.len()
    );
    Ok(Reply::new(serde_json::json!({
        "g3_cases": g3.cases, "g3_version": g3.version, "judge_labels": labels,
        "written": written, "text": text,
    })))
}
