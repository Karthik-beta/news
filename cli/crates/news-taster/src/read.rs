//! What `news stories show|members|similar` print (read-only, R tier).
//!
//! `similar` answers from SimHash until embeddings exist (ADR 0011): `cos` is `1 - Hamming / 64` over
//! the first document of each candidate, and only stories within [`MAX_DISTANCE`] bits are listed.
//! Candidates without text (structured data) have no neighbours that way.

use news_clerk::simhash;
use news_core::NewsError;
use serde_json::{Value, json};
use sqlx::PgPool;

/// SimHash neighbours are listed up to this Hamming distance (`cos >= 0.75`).
pub const MAX_DISTANCE: i32 = 16;

fn db_err(e: sqlx::Error) -> NewsError {
    news_store::map_err(e)
}

/// A story by number or event key.
async fn find(db: &PgPool, story: &str) -> Result<i64, NewsError> {
    let id: Option<i64> = match story.parse::<i64>() {
        Ok(n) => {
            sqlx::query_scalar("select story_id from stories where story_id = $1")
                .bind(n)
                .fetch_optional(db)
                .await
        }
        Err(_) => {
            sqlx::query_scalar("select story_id from stories where event_key = $1")
                .bind(story)
                .fetch_optional(db)
                .await
        }
    }
    .map_err(db_err)?;
    id.ok_or_else(|| {
        NewsError::not_found(format!("no story {story:?}"))
            .with_hint("use a story number or an event key such as macro:us.cpi:2026-10")
    })
}

pub async fn show(db: &PgPool, story: &str) -> Result<Value, NewsError> {
    let id = find(db, story).await?;
    let row: (i64, Option<String>, Option<String>, i32, String, Option<String>) = sqlx::query_as(
        "select s.story_id, s.event_key, s.title, s.member_count, s.status,
                coalesce(s.page_id, (select p.page_id from pages p where p.story_id = s.story_id
                                      and p.status = 'published' order by p.published_at desc limit 1))
           from stories s where s.story_id = $1",
    )
    .bind(id)
    .fetch_one(db)
    .await
    .map_err(db_err)?;
    Ok(
        json!({"story_id": row.0, "event_key": row.1, "title": row.2, "member_count": row.3,
              "status": row.4, "page_id": row.5}),
    )
}

pub async fn members(db: &PgPool, story: &str) -> Result<Value, NewsError> {
    let id = find(db, story).await?;
    type Row = (
        i64,
        String,
        Option<String>,
        Option<chrono::DateTime<chrono::Utc>>,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "select cand_id, source_id, title, published_at from candidates
              where story_id = $1 order by cand_id",
    )
    .bind(id)
    .fetch_all(db)
    .await
    .map_err(db_err)?;
    let items: Vec<Value> = rows
        .into_iter()
        .map(|r| json!({"cand_id": r.0, "source_id": r.1, "title": r.2, "published_at": r.3}))
        .collect();
    Ok(json!({ "items": items }))
}

/// Stories with a document within [`MAX_DISTANCE`] of `hash`, nearest first, leaving out candidate
/// `except`.
async fn neighbours(db: &PgPool, hash: i64, except: Option<i64>) -> Result<Value, NewsError> {
    let rows: Vec<(i64, Option<String>, i32)> = sqlx::query_as(
        "select s.story_id, s.title, min(bit_count((d.simhash # $1)::bit(64)))::int as dist
           from candidates c join documents d on d.doc_id = c.doc_ids[1]
                             join stories s on s.story_id = c.story_id
          where c.cand_id is distinct from $2
          group by s.story_id, s.title
         having min(bit_count((d.simhash # $1)::bit(64))) <= $3
          order by dist, s.story_id",
    )
    .bind(hash)
    .bind(except)
    .bind(MAX_DISTANCE)
    .fetch_all(db)
    .await
    .map_err(db_err)?;
    let items: Vec<Value> = rows
        .into_iter()
        .map(|(id, title, dist)| {
            json!({"story_id": id, "title": title, "cos": 1.0 - f64::from(dist) / 64.0})
        })
        .collect();
    Ok(json!({ "items": items }))
}

pub async fn similar_text(db: &PgPool, text: &str) -> Result<Value, NewsError> {
    neighbours(db, simhash::as_bigint(simhash::simhash(text)), None).await
}

/// `None` when the candidate has no document to compare.
pub async fn similar_cand(db: &PgPool, cand: &str) -> Result<Option<Value>, NewsError> {
    let id: i64 = cand
        .parse()
        .map_err(|_| NewsError::usage(format!("{cand:?} is not a candidate number")))?;
    let found: Option<(Option<i64>,)> = sqlx::query_as(
        "select (select d.simhash from documents d where d.doc_id = c.doc_ids[1])
           from candidates c where c.cand_id = $1",
    )
    .bind(id)
    .fetch_optional(db)
    .await
    .map_err(db_err)?;
    match found {
        None => Err(NewsError::not_found(format!("no candidate {id}"))),
        Some((None,)) => Ok(None),
        Some((Some(hash),)) => neighbours(db, hash, Some(id)).await.map(Some),
    }
}
