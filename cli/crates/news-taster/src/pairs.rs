//! G3 candidates (Bureau §3.6, §9.1): pairs for Karthik to label same story or different, drawn from the
//! candidates and story centroids Taster already embedded. The pairs sit either side of the join
//! threshold, so the labels say where it belongs. Nothing here labels anything.
//!
//! `news eval draft` calls [`draft`]; Board shows each pair's `sides` (never the cosine) through
//! `eval.items`, and `eval.label` stores the answer in `pair_labels`.

use crate::config::Thresholds;
use crate::embed::MODEL;
use news_core::NewsResult;
use news_store::map_err;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::HashSet;

/// How far either side of the join threshold a pair may sit.
pub const BAND: f64 = 0.08;

/// A candidate and a story it is not in, with their cosine.
#[derive(Debug, Clone, PartialEq)]
pub struct Near {
    pub cand_id: i64,
    pub story_id: i64,
    pub cos: f64,
    pub shared_entity: bool,
}

impl Near {
    /// Stable, so drafting again never adds a pair twice.
    pub fn pair_id(&self) -> String {
        format!("g3-c{}-s{}", self.cand_id, self.story_id)
    }
}

/// Up to `count` pairs, alternating below and above the join threshold, each side nearest the threshold
/// first, and each candidate once so one candidate cannot fill the set.
pub fn pick(mut near: Vec<Near>, join: f64, count: usize) -> Vec<Near> {
    near.sort_by(|a, b| {
        (a.cos - join)
            .abs()
            .total_cmp(&(b.cos - join).abs())
            .then(a.cand_id.cmp(&b.cand_id))
            .then(a.story_id.cmp(&b.story_id))
    });
    let (mut below, mut above): (Vec<Near>, Vec<Near>) =
        near.into_iter().partition(|n| n.cos < join);
    // Popping takes from the end, so reverse: nearest first.
    below.reverse();
    above.reverse();
    let (mut seen, mut out) = (HashSet::new(), Vec::new());
    let mut turn = false;
    while out.len() < count && !(below.is_empty() && above.is_empty()) {
        let side = if turn { &mut above } else { &mut below };
        turn = !turn;
        while let Some(n) = side.pop() {
            if seen.insert(n.cand_id) {
                out.push(n);
                break;
            }
        }
    }
    out
}

/// What one drafting run did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drafted {
    /// Pairs in the band that the choice looked at.
    pub considered: usize,
    /// Pairs chosen and not drafted before.
    pub added: usize,
    /// Pairs drafted so far, labelled or not.
    pub total: i64,
}

/// Draft up to `count` pairs near `t.join`. Pairs already drafted are left as they are, with any label.
pub async fn draft(db: &PgPool, t: &Thresholds, count: usize) -> NewsResult<Drafted> {
    // shortcut: every candidate against every story centroid, then the band; fine while there are
    // thousands of each, so move the band into an index scan when there are many more.
    let rows: Vec<(i64, i64, f64, bool)> = sqlx::query_as(
        "select c.cand_id, s.story_id, (1 - (ec.v <=> es.v))::float8, c.entity_ids && s.entity_ids
           from candidates c
           join embeddings ec on ec.owner_kind = 'candidate' and ec.owner_id = c.cand_id::text and ec.model = $1
           join embeddings es on es.owner_kind = 'story' and es.model = $1
           join stories s on s.story_id::text = es.owner_id
          where c.story_id is distinct from s.story_id
            and s.status <> 'closed'
            and not exists (select 1 from story_redirects r where r.from_story = s.story_id)
            and abs((1 - (ec.v <=> es.v)) - $2) <= $3
          order by abs((1 - (ec.v <=> es.v)) - $2), c.cand_id, s.story_id
          limit $4",
    )
    .bind(MODEL)
    .bind(t.join)
    .bind(BAND)
    .bind(i64::try_from(count.saturating_mul(20)).unwrap_or(i64::MAX))
    .fetch_all(db)
    .await
    .map_err(map_err)?;
    let near: Vec<Near> = rows
        .into_iter()
        .map(|(cand_id, story_id, cos, shared_entity)| Near {
            cand_id,
            story_id,
            cos,
            shared_entity,
        })
        .collect();
    let considered = near.len();
    let mut added = 0;
    for n in pick(near, t.join, count) {
        let sides = sides(db, &n).await?;
        let done = sqlx::query(
            "insert into story_pairs (pair_id, sides, cos, shared_entity, model)
             values ($1, $2, $3, $4, $5) on conflict (pair_id) do nothing",
        )
        .bind(n.pair_id())
        .bind(sides)
        .bind(n.cos)
        .bind(n.shared_entity)
        .bind(MODEL)
        .execute(db)
        .await
        .map_err(map_err)?;
        added += done.rows_affected() as usize;
    }
    let total = sqlx::query_scalar("select count(*) from story_pairs")
        .fetch_one(db)
        .await
        .map_err(map_err)?;
    Ok(Drafted {
        considered,
        added,
        total,
    })
}

/// What Board shows for a pair: the candidate (source, title, the start of its text) and the story
/// (title and up to three member titles).
async fn sides(db: &PgPool, n: &Near) -> NewsResult<Value> {
    let cand: (String, Option<String>, Option<String>) = sqlx::query_as(
        "select c.source_id, c.title,
                (select left(tb.text, 400) from text_blocks tb
                  where tb.doc_id = c.doc_ids[1] and tb.kind is distinct from 'heading' and btrim(tb.text) <> ''
                  order by tb.block_no limit 1)
           from candidates c where c.cand_id = $1",
    )
    .bind(n.cand_id)
    .fetch_one(db)
    .await
    .map_err(map_err)?;
    let story: (Option<String>,) = sqlx::query_as("select title from stories where story_id = $1")
        .bind(n.story_id)
        .fetch_one(db)
        .await
        .map_err(map_err)?;
    let members: Vec<(Option<String>,)> = sqlx::query_as(
        "select title from candidates where story_id = $1 and title is not null order by cand_id limit 3",
    )
    .bind(n.story_id)
    .fetch_all(db)
    .await
    .map_err(map_err)?;
    Ok(json!({
        "a": {"kind": "candidate", "id": n.cand_id, "source": cand.0, "title": cand.1, "text": cand.2},
        "b": {"kind": "story", "id": n.story_id, "title": story.0,
              "members": members.into_iter().filter_map(|m| m.0).collect::<Vec<_>>()},
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(cand: i64, story: i64, cos: f64) -> Near {
        Near {
            cand_id: cand,
            story_id: story,
            cos,
            shared_entity: true,
        }
    }

    #[test]
    fn pairs_alternate_either_side_of_the_threshold_nearest_first() {
        let near = vec![
            n(1, 10, 0.70),
            n(2, 10, 0.80),
            n(3, 10, 0.81),
            n(4, 10, 0.83),
            n(5, 10, 0.90),
            n(6, 10, 0.78),
        ];
        let got: Vec<(i64, f64)> = pick(near, 0.82, 4)
            .iter()
            .map(|n| (n.cand_id, n.cos))
            .collect();
        // Below: 0.81 then 0.80; above: 0.83 then 0.90 (0.82 itself counts as above).
        assert_eq!(got, vec![(3, 0.81), (4, 0.83), (2, 0.80), (5, 0.90)]);
    }

    #[test]
    fn a_candidate_is_drawn_once_and_a_short_side_is_made_up_from_the_other() {
        let near = vec![
            n(1, 10, 0.81),
            n(1, 11, 0.80),
            n(2, 10, 0.85),
            n(3, 10, 0.84),
        ];
        let got = pick(near, 0.82, 10);
        let cands: Vec<i64> = got.iter().map(|n| n.cand_id).collect();
        assert_eq!(cands, vec![1, 3, 2]);
        assert!(pick(vec![], 0.82, 5).is_empty());
        assert_eq!(pick(vec![n(1, 1, 0.8), n(2, 1, 0.9)], 0.82, 1).len(), 1);
    }

    #[test]
    fn a_pair_id_is_stable() {
        assert_eq!(n(7, 12, 0.8).pair_id(), "g3-c7-s12");
    }
}
