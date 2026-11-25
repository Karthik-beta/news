//! H-tier `merge` and `split` (Bureau §6.4: Karthik only). Board sends them over the Hotline; there is
//! no `news stories merge|split` (ADR 0007), and `retired.rs` keeps it that way.
//!
//! Two pages are better than a wrong merge, so a merge never joins two stories that both have a page,
//! and the story folded in must be page-less. Both operations rebuild the scoring inputs they touch.

use news_core::NewsError;
use news_nose::score::Novelty;
use sqlx::{PgPool, Row};

use crate::{Fail, embed, input, recount};

/// Fold story `from` into `into`: its candidates move, it closes, and its event key keeps pointing
/// here through `story_redirects`.
pub async fn merge(db: &PgPool, from: i64, into: i64) -> Result<(), NewsError> {
    Ok(merge_tx(db, from, into).await?)
}

async fn merge_tx(db: &PgPool, from: i64, into: i64) -> Result<(), Fail> {
    if from == into {
        return Err(NewsError::usage("a story cannot be merged into itself").into());
    }
    let mut tx = db.begin().await?;
    crate::lock(&mut tx).await?; // every check below reads after it
    // Lock in id order so two merges cannot deadlock.
    let rows = sqlx::query(
        "select story_id, status, (select count(*) from pages p where p.story_id = s.story_id) pages,
                exists (select 1 from story_redirects r where r.from_story = s.story_id) merged
           from stories s where story_id = any($1) order by story_id for update",
    )
    .bind(&[from, into][..])
    .fetch_all(&mut *tx)
    .await?;
    let info = |id: i64| rows.iter().find(|r| r.get::<i64, _>("story_id") == id);
    let (Some(a), Some(b)) = (info(from), info(into)) else {
        return Err(NewsError::not_found(format!("no story {from} or {into}")).into());
    };
    if a.get::<bool, _>("merged") || b.get::<bool, _>("merged") {
        return Err(
            NewsError::precondition("a story that was merged away cannot be merged again").into(),
        );
    }
    if a.get::<i64, _>("pages") > 0 {
        return Err(NewsError::precondition(format!(
            "story {from} has a page: merge it from the other side, into the story that has the page"
        ))
        .into());
    }
    if b.get::<String, _>("status") == "closed" {
        return Err(NewsError::precondition(format!("story {into} is closed")).into());
    }
    sqlx::query("update candidates set story_id = $2 where story_id = $1")
        .bind(from)
        .bind(into)
        .execute(&mut *tx)
        .await?;
    sqlx::query("insert into story_redirects (from_story, into_story) values ($1, $2)")
        .bind(from)
        .bind(into)
        .execute(&mut *tx)
        .await?;
    // Nothing is left pointing at a story that is itself merged away.
    sqlx::query("update story_redirects set into_story = $2 where into_story = $1")
        .bind(from)
        .bind(into)
        .execute(&mut *tx)
        .await?;
    sqlx::query("update stories set status = 'closed', score_input = null where story_id = $1")
        .bind(from)
        .execute(&mut *tx)
        .await?;
    // Folding a candidate into a story that has a page makes it an update.
    sqlx::query("update stories set status = 'open' where story_id = $1 and status = 'covered'")
        .bind(into)
        .execute(&mut *tx)
        .await?;
    recount(&mut tx, from).await?;
    recount(&mut tx, into).await?;
    // `from` has no members now, so its centroid goes; `into` gets the mean of the larger set.
    embed::recompute_centroid(&mut tx, from).await?;
    embed::recompute_centroid(&mut tx, into).await?;
    input::refresh(&mut tx, into, Novelty::NewEvent).await?;
    tx.commit().await?;
    Ok(())
}

/// Move `cands` of `story` into a new story and return its id. The rest stay; at least one must.
pub async fn split(db: &PgPool, story: i64, cands: &[i64]) -> Result<i64, NewsError> {
    Ok(split_tx(db, story, cands).await?)
}

async fn split_tx(db: &PgPool, story: i64, cands: &[i64]) -> Result<i64, Fail> {
    if cands.is_empty() {
        return Err(NewsError::usage("name at least one candidate to split off").into());
    }
    let mut tx = db.begin().await?;
    crate::lock(&mut tx).await?;
    let status: Option<String> =
        sqlx::query_scalar("select status from stories where story_id = $1 for update")
            .bind(story)
            .fetch_optional(&mut *tx)
            .await?;
    match status.as_deref() {
        None => return Err(NewsError::not_found(format!("no story {story}")).into()),
        Some("closed") => {
            return Err(NewsError::precondition(format!("story {story} is closed")).into());
        }
        _ => {}
    }
    let members: Vec<i64> =
        sqlx::query_scalar("select cand_id from candidates where story_id = $1 order by cand_id")
            .bind(story)
            .fetch_all(&mut *tx)
            .await?;
    if let Some(x) = cands.iter().find(|c| !members.contains(c)) {
        return Err(NewsError::validation(format!("candidate {x} is not in story {story}")).into());
    }
    if members.iter().all(|m| cands.contains(m)) {
        return Err(NewsError::precondition("a split must leave a candidate behind").into());
    }
    let new: i64 = sqlx::query_scalar(
        "insert into stories (title, first_seen, last_seen)
         select title, first_seen, first_seen from candidates where cand_id = $1 returning story_id",
    )
    .bind(cands.iter().min())
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("update candidates set story_id = $2 where cand_id = any($1)")
        .bind(cands)
        .bind(new)
        .execute(&mut *tx)
        .await?;
    for s in [story, new] {
        recount(&mut tx, s).await?;
        embed::recompute_centroid(&mut tx, s).await?;
        input::refresh(&mut tx, s, Novelty::NewEvent).await?;
    }
    tx.commit().await?;
    Ok(new)
}
