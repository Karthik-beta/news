//! Huddle's database side (Bureau §4.7, §8.12): Karthik's pairwise labels, the questions chosen by code,
//! the calibration proposal and the scorer versions. No model is involved anywhere.
//!
//! * [`questions`]: about 30 pairs of recently scored stories, the closest calls first, for Board's form.
//! * [`store_labels`]: what `news scores label` and the Hotline's `huddle.answer` both call.
//! * [`propose`]: fit new weights from the labels since the live version; stores an open proposal.
//! * [`adopt`]: make a proposal the live version. The CLI form is G-tier and always `gated` (ADR 0008
//!   §6.3); Chief calls this in process as the authorised `huddle.apply` step, after the change's approval.

use crate::calibrate::{self, MIN_LABELS, NAMES, Params, Vec6};
use crate::config::{self, ScorerConfig, Weights};
use crate::score::Components;
use chrono::{DateTime, Utc};
use news_core::{NewsError, NewsResult};
use news_store::{PgPool, map_err};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::types::Json;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Questions in one Huddle (§4.7: about 30, five minutes).
pub const QUESTIONS: usize = 30;
/// Labels in one answer.
pub const MAX_LABELS: usize = 200;

/// The live scorer version: the latest adopted row, else v1 from the seed.
#[derive(Debug, Clone)]
pub struct Live {
    pub version: String,
    pub n: i32,
    pub weights: Weights,
    /// When it was adopted; `None` for v1. Labels count towards the next version from here.
    pub since: Option<DateTime<Utc>>,
}

pub async fn live(db: &PgPool) -> NewsResult<Live> {
    let row: Option<(String, i32, Json<Weights>, DateTime<Utc>)> = sqlx::query_as(
        "select version, n, weights, adopted_at from scorer_versions
          where status = 'adopted' order by n desc limit 1",
    )
    .fetch_optional(db)
    .await
    .map_err(map_err)?;
    if let Some((version, n, Json(weights), at)) = row {
        return Ok(Live {
            version,
            n,
            weights,
            since: Some(at),
        });
    }
    let v1 = config::v1().map_err(NewsError::internal)?;
    Ok(Live {
        version: v1.version,
        n: 1,
        weights: v1.weights,
        since: None,
    })
}

/// The scorer config `news plan` scores with: the seed, with the live version's name and weights.
pub async fn current_config(db: &PgPool) -> NewsResult<ScorerConfig> {
    let mut cfg = config::v1().map_err(NewsError::internal)?;
    let live = live(db).await?;
    cfg.version = live.version;
    cfg.weights = live.weights;
    Ok(cfg)
}

async fn resolve_run(db: &PgPool, run_ref: &str) -> NewsResult<Uuid> {
    let id: Option<Uuid> = match Uuid::parse_str(run_ref) {
        Ok(id) => {
            sqlx::query_scalar("select run_id from runs where run_id = $1")
                .bind(id)
                .fetch_optional(db)
                .await
        }
        Err(_) => {
            sqlx::query_scalar("select run_id from runs where short_id = $1")
                .bind(run_ref)
                .fetch_optional(db)
                .await
        }
    }
    .map_err(map_err)?;
    id.ok_or_else(|| NewsError::not_found(format!("no run {run_ref:?}")))
}

/// One answer: which of two stories deserved a page first.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabelIn {
    pub a: i64,
    pub b: i64,
    /// `a` or `b`.
    pub winner: String,
}

/// The labels of an answer file or Hotline request: `{"labels": [{"a": 12, "b": 15, "winner": "a"}]}`.
pub fn parse_labels(v: &Value) -> NewsResult<Vec<LabelIn>> {
    let list = v
        .get("labels")
        .cloned()
        .ok_or_else(|| NewsError::validation("the answer has no `labels` list"))?;
    let labels: Vec<LabelIn> = serde_json::from_value(list)
        .map_err(|e| NewsError::validation(format!("labels are not {{a, b, winner}}: {e}")))?;
    if labels.is_empty() || labels.len() > MAX_LABELS {
        return Err(NewsError::validation(format!(
            "an answer holds 1 to {MAX_LABELS} labels, not {}",
            labels.len()
        )));
    }
    for l in &labels {
        if l.a == l.b || !matches!(l.winner.as_str(), "a" | "b") {
            return Err(NewsError::validation(format!(
                "label {} vs {} must name two different stories and a winner of a or b",
                l.a, l.b
            )));
        }
    }
    Ok(labels)
}

/// Store Karthik's answers for a run, with each story's latest component values; answering a pair
/// again replaces the earlier answer. Returns the labels written.
pub async fn store_labels(db: &PgPool, run_ref: &str, labels: &[LabelIn]) -> NewsResult<usize> {
    let run_id = resolve_run(db, run_ref).await?;
    let ids: Vec<i64> = labels.iter().flat_map(|l| [l.a, l.b]).collect();
    let rows: Vec<(i64, Json<Components>)> = sqlx::query_as(
        "select distinct on (story_id) story_id, components from scores
          where story_id = any($1) order by story_id, computed_at desc",
    )
    .bind(&ids)
    .fetch_all(db)
    .await
    .map_err(map_err)?;
    let x: HashMap<i64, Components> = rows.into_iter().map(|(id, Json(c))| (id, c)).collect();
    let mut tx = db.begin().await.map_err(map_err)?;
    let mut written = 0;
    for l in labels {
        let (Some(xa), Some(xb)) = (x.get(&l.a), x.get(&l.b)) else {
            let missing = if x.contains_key(&l.a) { l.b } else { l.a };
            return Err(NewsError::validation(format!(
                "story {missing} has no score to label"
            )));
        };
        // Stored with story_a < story_b, so one pair is one row however Board ordered it.
        let (lo, hi, xlo, xhi, winner) = if l.a < l.b {
            (l.a, l.b, xa, xb, l.winner.as_str())
        } else {
            (l.b, l.a, xb, xa, if l.winner == "a" { "b" } else { "a" })
        };
        written += sqlx::query(
            "insert into score_labels (run_id, story_a, story_b, winner, x_a, x_b)
             values ($1, $2, $3, $4, $5, $6)
             on conflict (run_id, story_a, story_b)
             do update set winner = excluded.winner, x_a = excluded.x_a, x_b = excluded.x_b, created_at = now()",
        )
        .bind(run_id)
        .bind(lo)
        .bind(hi)
        .bind(winner)
        .bind(Json(xlo))
        .bind(Json(xhi))
        .execute(&mut *tx)
        .await
        .map_err(map_err)?
        .rows_affected() as usize;
    }
    tx.commit().await.map_err(map_err)?;
    Ok(written)
}

/// A story in a question.
#[derive(Debug, Clone, Serialize)]
pub struct Side {
    pub story_id: i64,
    pub title: String,
    pub total: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Question {
    pub a: Side,
    pub b: Side,
}

/// A recently scored story.
#[derive(Debug, Clone)]
pub struct Item {
    pub id: i64,
    pub title: String,
    pub total: f64,
    pub z: Vec6,
}

/// Choose up to `n` pairs of `items` (indices): neighbours in score order, closest S first, since a label
/// between two stories the scorer already separates widely says little. A pair already labelled, or with
/// identical features, is skipped, and a story is asked about at most twice. Pure and deterministic.
pub fn pick_pairs(items: &[Item], done: &HashSet<(i64, i64)>, n: usize) -> Vec<(usize, usize)> {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&x, &y| {
        items[y]
            .total
            .total_cmp(&items[x].total)
            .then(items[x].id.cmp(&items[y].id))
    });
    let mut cands = Vec::new();
    for (p, &i) in order.iter().enumerate() {
        for &j in order[p + 1..].iter().take(3) {
            let (lo, hi) = (items[i].id.min(items[j].id), items[i].id.max(items[j].id));
            let same = items[i]
                .z
                .iter()
                .zip(&items[j].z)
                .all(|(x, y)| (x - y).abs() < 1e-9);
            if !done.contains(&(lo, hi)) && !same {
                cands.push(((items[i].total - items[j].total).abs(), lo, hi, i, j));
            }
        }
    }
    cands.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let mut used: HashMap<i64, u8> = HashMap::new();
    let mut picks = Vec::new();
    for (_, lo, hi, i, j) in cands {
        if picks.len() >= n {
            break;
        }
        if used.get(&lo).copied().unwrap_or(0) >= 2 || used.get(&hi).copied().unwrap_or(0) >= 2 {
            continue;
        }
        *used.entry(lo).or_default() += 1;
        *used.entry(hi).or_default() += 1;
        picks.push((i, j));
    }
    picks
}

/// Huddle's questions: pairs from the stories scored in the 7 days before `as_of`, latest score each.
pub async fn questions(db: &PgPool, as_of: DateTime<Utc>, n: usize) -> NewsResult<Vec<Question>> {
    let rows: Vec<(i64, String, f64, Json<Components>)> = sqlx::query_as(
        "select distinct on (s.story_id) s.story_id, coalesce(st.title, 'Story ' || s.story_id),
                s.total::float8, s.components
           from scores s join stories st on st.story_id = s.story_id
          where s.computed_at > $1 - interval '7 days' and s.computed_at <= $1
          order by s.story_id, s.computed_at desc",
    )
    .bind(as_of)
    .fetch_all(db)
    .await
    .map_err(map_err)?;
    let done: HashSet<(i64, i64)> =
        sqlx::query_as::<_, (i64, i64)>("select story_a, story_b from score_labels")
            .fetch_all(db)
            .await
            .map_err(map_err)?
            .into_iter()
            .collect();
    let items: Vec<Item> = rows
        .into_iter()
        .map(|(id, title, total, Json(c))| Item {
            id,
            title,
            total,
            z: calibrate::features(&c),
        })
        .collect();
    let side = |i: &Item| Side {
        story_id: i.id,
        title: i.title.clone(),
        total: i.total,
    };
    Ok(pick_pairs(&items, &done, n)
        .into_iter()
        .enumerate()
        .map(|(k, (i, j))| {
            // Alternate which story is shown first, so position carries no signal.
            let (x, y) = if k % 2 == 0 { (i, j) } else { (j, i) };
            Question {
                a: side(&items[x]),
                b: side(&items[y]),
            }
        })
        .collect())
}

#[derive(Debug, Clone, Serialize)]
pub struct Change {
    pub weight: String,
    pub from: f64,
    pub to: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Proposal {
    pub version: String,
    pub parent: String,
    pub labels: usize,
    pub diff: Vec<Change>,
    /// Labels held out of the fit (every fifth), and Kendall's τ on them for the live and proposed weights.
    pub held_out: usize,
    pub agreement_before: Option<f64>,
    pub agreement_after: Option<f64>,
}

fn round4(x: f64) -> f64 {
    (x * 1e4).round() / 1e4
}

/// Fit new weights from the labels since the live version was adopted (all of them for v1) and store
/// them as the open proposal, replacing an earlier unadopted one.
pub async fn propose(db: &PgPool) -> NewsResult<Proposal> {
    let live = live(db).await?;
    let rows: Vec<(Json<Components>, Json<Components>, String)> = sqlx::query_as(
        "select x_a, x_b, winner from score_labels
          where $1::timestamptz is null or created_at > $1 order by label_id",
    )
    .bind(live.since)
    .fetch_all(db)
    .await
    .map_err(map_err)?;
    if rows.len() < MIN_LABELS {
        return Err(NewsError::precondition(format!(
            "{} labels since {}; at least {MIN_LABELS} are needed",
            rows.len(),
            live.version
        ))
        .with_hint("answer Huddle's questions in Board"));
    }
    let diffs: Vec<Vec6> = rows
        .iter()
        .map(|(Json(a), Json(b), winner)| {
            let (za, zb) = (calibrate::features(a), calibrate::features(b));
            let (win, lose) = if winner == "a" { (za, zb) } else { (zb, za) };
            std::array::from_fn(|k| win[k] - lose[k])
        })
        .collect();
    let (mut train, mut held) = (Vec::new(), Vec::new());
    for (i, d) in diffs.iter().enumerate() {
        let side = if i % 5 == 4 { &mut held } else { &mut train };
        side.push(*d);
    }
    let cur = calibrate::to_vec(&live.weights);
    let w = calibrate::fit(cur, &train, &Params::default());
    calibrate::check(&cur, &w).map_err(NewsError::internal)?;

    let n = live.n + 1;
    let version = format!("v{n}");
    let mut tx = db.begin().await.map_err(map_err)?;
    sqlx::query("delete from scorer_versions where status = 'proposed'")
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;
    sqlx::query(
        "insert into scorer_versions (version, n, parent, weights, status, labels)
         values ($1, $2, $3, $4, 'proposed', $5)",
    )
    .bind(&version)
    .bind(n)
    .bind(&live.version)
    .bind(Json(calibrate::to_weights(w)))
    .bind(rows.len() as i32)
    .execute(&mut *tx)
    .await
    .map_err(map_err)?;
    tx.commit().await.map_err(map_err)?;
    Ok(Proposal {
        version,
        parent: live.version,
        labels: rows.len(),
        diff: (0..6)
            .map(|k| Change {
                weight: NAMES[k].to_owned(),
                from: round4(cur[k]),
                to: round4(w[k]),
            })
            .collect(),
        held_out: held.len(),
        agreement_before: calibrate::agreement(&cur, &held),
        agreement_after: calibrate::agreement(&w, &held),
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct Adopted {
    pub version: String,
    pub parent: String,
    /// True when the version was already live: adopting twice changes nothing.
    pub already: bool,
}

/// Make a proposed version the live one. Refused unless it is the next version after the live one and
/// still keeps the rules against its parent (≥ 0, sum 1, each weight within ±0.05).
pub async fn adopt(db: &PgPool, version: &str) -> NewsResult<Adopted> {
    let mut tx = db.begin().await.map_err(map_err)?;
    let out = adopt_in(&mut tx, version).await;
    // End the transaction here on every path. Dropping it only queues the rollback for a task that
    // runs when the connection goes back to the pool; on a current-thread runtime that task may not
    // run before the caller blocks, so the `for update` lock would outlive the call.
    match &out {
        Ok(a) if !a.already => tx.commit().await.map_err(map_err)?,
        Ok(_) => tx.rollback().await.map_err(map_err)?,
        Err(_) => {
            // The refusal is the error worth reporting; a failed rollback closes the connection.
            let _ = tx.rollback().await;
        }
    }
    out
}

async fn adopt_in(tx: &mut sqlx::PgConnection, version: &str) -> NewsResult<Adopted> {
    let row: Option<(String, i32, Json<Weights>, String)> = sqlx::query_as(
        "select parent, n, weights, status from scorer_versions where version = $1 for update",
    )
    .bind(version)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_err)?;
    let (parent, n, Json(weights), status) = row.ok_or_else(|| {
        NewsError::not_found(format!("no proposed scorer version {version:?}"))
            .with_hint("`news scores propose --from-labels` makes one")
    })?;
    if status == "adopted" {
        return Ok(Adopted {
            version: version.to_owned(),
            parent,
            already: true,
        });
    }
    let latest: i32 = sqlx::query_scalar(
        "select coalesce(max(n), 1)::int4 from scorer_versions where status = 'adopted'",
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(map_err)?;
    if latest + 1 != n {
        return Err(NewsError::conflict(format!(
            "{version} was proposed against {parent}, but v{latest} is live now"
        ))
        .with_hint("run `news scores propose --from-labels` again"));
    }
    let parent_w = if parent == "v1" {
        config::v1().map_err(NewsError::internal)?.weights
    } else {
        sqlx::query_scalar::<_, Json<Weights>>(
            "select weights from scorer_versions where version = $1 and status = 'adopted'",
        )
        .bind(&parent)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_err)?
        .0
    };
    calibrate::check(&calibrate::to_vec(&parent_w), &calibrate::to_vec(&weights)).map_err(|e| {
        NewsError::validation(format!("{version} breaks the calibration rules: {e}"))
    })?;
    sqlx::query(
        "update scorer_versions set status = 'adopted', adopted_at = now() where version = $1",
    )
    .bind(version)
    .execute(&mut *tx)
    .await
    .map_err(map_err)?;
    Ok(Adopted {
        version: version.to_owned(),
        parent,
        already: false,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn item(id: i64, total: f64, i: f64) -> Item {
        Item {
            id,
            title: format!("s{id}"),
            total,
            z: [i, 0.5, 0.5, 0.5, 0.5, 0.5],
        }
    }

    #[test]
    fn pairs_are_close_calls_not_repeated_and_each_story_at_most_twice() {
        let items: Vec<Item> = (1..=12)
            .map(|k| item(k, 90.0 - 5.0 * k as f64, k as f64 / 20.0))
            .collect();
        let none = HashSet::new();
        let picks = pick_pairs(&items, &none, 30);
        assert!(!picks.is_empty() && picks.len() <= 30);
        let mut count: HashMap<i64, u8> = HashMap::new();
        for (i, j) in &picks {
            *count.entry(items[*i].id).or_default() += 1;
            *count.entry(items[*j].id).or_default() += 1;
        }
        assert!(count.values().all(|c| *c <= 2), "{count:?}");
        // Deterministic, and a labelled pair is not asked again.
        assert_eq!(picks, pick_pairs(&items, &none, 30));
        let (i, j) = picks[0];
        let (lo, hi) = (items[i].id.min(items[j].id), items[i].id.max(items[j].id));
        let done = HashSet::from([(lo, hi)]);
        assert!(!pick_pairs(&items, &done, 30).contains(&picks[0]));
    }

    #[test]
    fn stories_with_identical_features_are_not_a_question() {
        let items = [item(1, 60.0, 0.5), item(2, 60.0, 0.5)];
        assert!(pick_pairs(&items, &HashSet::new(), 30).is_empty());
    }

    #[test]
    fn answers_are_checked_before_the_database() {
        let ok = serde_json::json!({"labels": [{"a": 1, "b": 2, "winner": "a"}]});
        assert_eq!(parse_labels(&ok).unwrap().len(), 1);
        for bad in [
            serde_json::json!({}),
            serde_json::json!({"labels": []}),
            serde_json::json!({"labels": [{"a": 1, "b": 1, "winner": "a"}]}),
            serde_json::json!({"labels": [{"a": 1, "b": 2, "winner": "tie"}]}),
            serde_json::json!({"labels": [{"a": 1, "b": 2, "winner": "a", "x": 1}]}),
        ] {
            assert!(parse_labels(&bad).is_err(), "{bad}");
        }
    }
}
