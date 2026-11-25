//! Detection (Bureau §3.6): every new candidate is dropped into a story, in design order.
//!
//! 1. exact duplicate: the same content hash within 30 days joins that candidate's story;
//! 2. near duplicate: SimHash-64 Hamming distance <= 3 within 7 days joins it too;
//! 3. an event key maps to its story (created when new);
//! 4. no key: the [`Joiner`] finds similar stories. Join, grey zone (an A4 job, and the candidate stays
//!    apart until an answer says otherwise) or a new story;
//! 5. a candidate that joins a story with a published page is an update: the story reopens with
//!    `action = update` so Nose scores it again.
//!
//! The embedding joiner is [`crate::embed::EmbedJoiner`] (ADR 0011). Without one the service uses
//! [`NoEmbedder`], which finds no neighbours, so step 4 makes a new story. Only candidates with status
//! `new` are read and each ends `grouped`, so running detection again changes nothing.

use std::collections::BTreeMap;
use std::future::Future;

use chrono::{DateTime, Utc};
use news_core::NewsError;
use news_dispatch::{NewJob, queue::enqueue_on};
use news_nose::score::Novelty;
use serde::Serialize;
use serde_json::json;
use sqlx::{PgConnection, PgPool, Row};

use crate::config::Thresholds;
use crate::{Fail, input, recount};

/// A candidate waiting for a story.
#[derive(Debug, Clone)]
pub struct Cand {
    pub cand_id: i64,
    pub title: Option<String>,
    pub event_key: Option<String>,
    pub entity_ids: Vec<i64>,
    pub first_seen: DateTime<Utc>,
    /// Its first document: `(content_sha, simhash)`.
    pub doc: Option<(Vec<u8>, i64)>,
}

/// A story close to a candidate in meaning.
#[derive(Debug, Clone, PartialEq)]
pub struct Neighbour {
    pub story_id: i64,
    /// Cosine to the story's centroid (or to its page, for a story that has one).
    pub cos: f64,
    /// A resolved entity in common.
    pub shared_entity: bool,
    /// The story has a published page.
    pub has_page: bool,
}

/// The embedding seam: stories near a candidate that has no event key, within the 72 h window for open
/// stories and the 30-day window for stories with a page (`Thresholds`). [`crate::embed::EmbedJoiner`]
/// embeds and queries pgvector; tests inject cosines through this trait.
pub trait Joiner {
    fn neighbours(
        &self,
        db: &mut PgConnection,
        cand: &Cand,
        t: &Thresholds,
    ) -> impl Future<Output = Result<Vec<Neighbour>, sqlx::Error>> + Send;

    /// Called for a candidate with an event key (it is never matched by similarity, so `neighbours` is
    /// skipped) so its vector still reaches its story's centroid, where a later keyless candidate can
    /// find it. Candidates without a key store theirs inside `neighbours`.
    fn store_vector(
        &self,
        _: &mut PgConnection,
        _: &Cand,
    ) -> impl Future<Output = Result<(), sqlx::Error>> + Send {
        async { Ok(()) }
    }

    /// Called once a candidate that is not a duplicate is in `story_id` (its own new story included), so
    /// a joiner can keep the story's centroid.
    fn assigned(
        &self,
        _: &mut PgConnection,
        _: &Cand,
        _: i64,
    ) -> impl Future<Output = Result<(), sqlx::Error>> + Send {
        async { Ok(()) }
    }
}

/// No embedder configured: step 4 never joins.
pub struct NoEmbedder;

impl Joiner for NoEmbedder {
    async fn neighbours(
        &self,
        _: &mut PgConnection,
        _: &Cand,
        _: &Thresholds,
    ) -> Result<Vec<Neighbour>, sqlx::Error> {
        Ok(Vec::new())
    }
}

/// What step 4 does with the neighbours.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Decision {
    Join {
        story_id: i64,
    },
    /// Similar enough to ask A4, not to merge.
    Grey {
        story_id: i64,
        cos: f64,
    },
    New {
        max_cos: Option<f64>,
    },
}

/// The closest neighbour decides. A story with a page is joined only from the update threshold, as an
/// update; any other neighbour from the join threshold. Both need a shared entity; below the join
/// threshold, or without the entity, anything from the grey bound up is grey.
pub fn decide(t: &Thresholds, neighbours: &[Neighbour]) -> Decision {
    let Some(n) = neighbours
        .iter()
        .max_by(|a, b| a.cos.total_cmp(&b.cos).then(b.story_id.cmp(&a.story_id)))
    else {
        return Decision::New { max_cos: None };
    };
    let bar = if n.has_page { t.update } else { t.join };
    if n.cos >= bar && n.shared_entity {
        Decision::Join {
            story_id: n.story_id,
        }
    } else if n.cos >= t.grey {
        Decision::Grey {
            story_id: n.story_id,
            cos: n.cos,
        }
    } else {
        Decision::New {
            max_cos: Some(n.cos),
        }
    }
}

/// What one detection run did.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    pub candidates: u32,
    pub exact_duplicates: u32,
    pub near_duplicates: u32,
    pub by_event_key: u32,
    pub by_similarity: u32,
    pub new_stories: u32,
    /// Candidates that joined a story with a published page.
    pub updates: u32,
    /// A4 jobs asked for (a pair already asked about counts too).
    pub grey_zone: u32,
    /// Touched stories with a scoring input, and without one (see `input`).
    pub scoreable: u32,
    pub unscoreable: u32,
}

/// Group every new candidate, then refresh the stories it touched. All in one transaction.
pub async fn detect<J: Joiner>(
    db: &PgPool,
    joiner: &J,
    t: &Thresholds,
) -> Result<Report, NewsError> {
    Ok(run(db, joiner, t).await?)
}

async fn run<J: Joiner>(db: &PgPool, joiner: &J, t: &Thresholds) -> Result<Report, Fail> {
    let mut tx = db.begin().await?;
    crate::lock(&mut tx).await?;
    let rows = sqlx::query(
        "select c.cand_id, c.title, c.event_key, c.entity_ids, c.first_seen, d.content_sha, d.simhash
           from candidates c left join documents d on d.doc_id = c.doc_ids[1]
          where c.status = 'new' and c.story_id is null order by c.cand_id",
    )
    .fetch_all(&mut *tx)
    .await?;
    let mut report = Report::default();
    // Touched stories, each with the novelty it starts with.
    let mut touched: BTreeMap<i64, Novelty> = BTreeMap::new();
    for r in &rows {
        let doc: Option<Vec<u8>> = r.try_get("content_sha")?;
        let c = Cand {
            cand_id: r.try_get("cand_id")?,
            title: r.try_get("title")?,
            event_key: r.try_get("event_key")?,
            entity_ids: r.try_get("entity_ids")?,
            first_seen: r.try_get("first_seen")?,
            doc: match doc {
                Some(sha) => Some((sha, r.try_get("simhash")?)),
                None => None,
            },
        };
        report.candidates += 1;
        let mut novelty = Novelty::NewEvent;
        let mut duplicate = true;
        let story = if let Some(s) = duplicate_of(&mut tx, &c, t, &mut report).await? {
            s
        } else {
            duplicate = false;
            if let Some(key) = &c.event_key {
                match keyed_story(&mut tx, key).await? {
                    Some(s) => {
                        report.by_event_key += 1;
                        s
                    }
                    None => new_story(&mut tx, &c, Some(key), &mut report).await?,
                }
            } else {
                let ns = joiner.neighbours(&mut tx, &c, t).await?;
                match decide(t, &ns) {
                    Decision::Join { story_id } => {
                        report.by_similarity += 1;
                        story_id
                    }
                    Decision::Grey { story_id, cos } => {
                        // Keep separate; A4 may later say otherwise.
                        report.grey_zone += 1;
                        let job = NewJob::new("ai.storymatch")
                            .unique_key(format!("storymatch:{}:{story_id}", c.cand_id))
                            .args(json!({"cand": c.cand_id, "story": story_id, "cos": cos}));
                        enqueue_on(&mut tx, &job).await?;
                        novelty = Novelty::Similar { max_cos: cos };
                        new_story(&mut tx, &c, None, &mut report).await?
                    }
                    Decision::New { max_cos } => {
                        if let Some(max_cos) = max_cos {
                            novelty = Novelty::Similar { max_cos };
                        }
                        new_story(&mut tx, &c, None, &mut report).await?
                    }
                }
            }
        };
        sqlx::query("update candidates set story_id = $2, status = 'grouped' where cand_id = $1 and status = 'new'",
        )
            .bind(c.cand_id)
            .bind(story)
            .execute(&mut *tx)
            .await?;
        if !duplicate {
            if c.event_key.is_some() {
                joiner.store_vector(&mut tx, &c).await?;
            }
            joiner.assigned(&mut tx, &c, story).await?;
        }
        // Step 5: a new member of a story that already has a page is an update. A copy of something
        // already in the story is not.
        if !duplicate && has_page(&mut tx, story).await? {
            report.updates += 1;
            sqlx::query(
                "update stories set status = 'open' where story_id = $1 and status = 'covered'",
            )
            .bind(story)
            .execute(&mut *tx)
            .await?;
        }
        touched.entry(story).or_insert(novelty);
    }
    for (story, novelty) in touched {
        recount(&mut tx, story).await?;
        if input::refresh(&mut tx, story, novelty).await? {
            report.scoreable += 1;
        } else {
            report.unscoreable += 1;
        }
    }
    tx.commit().await?;
    Ok(report)
}

async fn has_page(tx: &mut PgConnection, story: i64) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "select exists (select 1 from pages where story_id = $1 and status = 'published')",
    )
    .bind(story)
    .fetch_one(tx)
    .await
}

/// Steps 1 and 2: the story of an earlier candidate with the same content, or a SimHash within reach.
async fn duplicate_of(
    tx: &mut PgConnection,
    c: &Cand,
    t: &Thresholds,
    report: &mut Report,
) -> Result<Option<i64>, sqlx::Error> {
    let Some((sha, hash)) = &c.doc else {
        return Ok(None);
    };
    let exact: Option<i64> = sqlx::query_scalar(
        "select c2.story_id from candidates c2 join documents d2 on d2.doc_id = c2.doc_ids[1]
          where c2.story_id is not null and c2.cand_id < $1 and d2.content_sha = $2
            and c2.first_seen >= $3 - make_interval(days => $4)
          order by c2.cand_id limit 1",
    )
    .bind(c.cand_id)
    .bind(sha)
    .bind(c.first_seen)
    .bind(t.exact_days)
    .fetch_optional(&mut *tx)
    .await?;
    if exact.is_some() {
        report.exact_duplicates += 1;
        return Ok(exact);
    }
    let near: Option<i64> = sqlx::query_scalar(
        "select c2.story_id from candidates c2 join documents d2 on d2.doc_id = c2.doc_ids[1]
          where c2.story_id is not null and c2.cand_id < $1
            and bit_count((d2.simhash # $2)::bit(64)) <= $3
            and c2.first_seen >= $4 - make_interval(days => $5)
          order by bit_count((d2.simhash # $2)::bit(64)), c2.cand_id limit 1",
    )
    .bind(c.cand_id)
    .bind(hash)
    .bind(i32::try_from(t.simhash_distance).unwrap_or(0))
    .bind(c.first_seen)
    .bind(t.simhash_days)
    .fetch_optional(&mut *tx)
    .await?;
    if near.is_some() {
        report.near_duplicates += 1;
    }
    Ok(near)
}

/// The story that owns an event key, following a merge.
async fn keyed_story(tx: &mut PgConnection, key: &str) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar(
        "select coalesce((select into_story from story_redirects where from_story = s.story_id), s.story_id)
           from stories s where s.event_key = $1",
    )
    .bind(key)
    .fetch_optional(tx)
    .await
}

async fn new_story(
    tx: &mut PgConnection,
    c: &Cand,
    key: Option<&str>,
    report: &mut Report,
) -> Result<i64, sqlx::Error> {
    let id: Option<i64> = sqlx::query_scalar(
        "insert into stories (event_key, title, first_seen, last_seen, entity_ids)
         values ($1, $2, $3, $3, $4) on conflict (event_key) do nothing returning story_id",
    )
    .bind(key)
    .bind(&c.title)
    .bind(c.first_seen)
    .bind(&c.entity_ids)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(id) = id {
        report.new_stories += 1;
        return Ok(id);
    }
    // The key was taken meanwhile: join that story.
    keyed_story(tx, key.unwrap_or_default())
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn n(story_id: i64, cos: f64, shared_entity: bool, has_page: bool) -> Neighbour {
        Neighbour {
            story_id,
            cos,
            shared_entity,
            has_page,
        }
    }

    #[test]
    fn the_thresholds_split_join_grey_and_new() {
        let t = crate::config::v1().unwrap();
        let d = |ns: &[Neighbour]| decide(&t, ns);
        assert_eq!(d(&[]), Decision::New { max_cos: None });
        assert_eq!(
            d(&[n(1, 0.82, true, false)]),
            Decision::Join { story_id: 1 }
        );
        // Over the join bar with no shared entity, and just under it: both grey.
        assert!(matches!(
            d(&[n(1, 0.9, false, false)]),
            Decision::Grey { .. }
        ));
        assert!(matches!(
            d(&[n(1, 0.81, true, false)]),
            Decision::Grey { .. }
        ));
        assert!(matches!(
            d(&[n(1, 0.72, true, false)]),
            Decision::Grey { .. }
        ));
        assert_eq!(
            d(&[n(1, 0.71, true, false)]),
            Decision::New {
                max_cos: Some(0.71)
            }
        );
        // A story with a page joins (as an update) only from 0.85.
        assert!(matches!(
            d(&[n(1, 0.84, true, true)]),
            Decision::Grey { .. }
        ));
        assert_eq!(d(&[n(1, 0.85, true, true)]), Decision::Join { story_id: 1 });
        // The closest wins; a tie goes to the lower story id.
        assert_eq!(
            d(&[
                n(5, 0.83, true, false),
                n(2, 0.9, true, false),
                n(1, 0.9, true, false)
            ]),
            Decision::Join { story_id: 1 }
        );
    }
}
