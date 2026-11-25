//! The Debrief (Bureau §8.12): the week's statistics, built in SQL from existing tables, rendered by
//! code. `news review weekly` and Huddle's first step show it; no model writes any figure.
//!
//! [`gather`] runs the queries; [`render`] turns the result into the command's JSON and the text Karthik
//! reads. Splitting them keeps the text a golden test needs no database.

use chrono::{DateTime, TimeDelta, Utc};
use news_core::{NewsError, NewsResult};
use news_nose::config::{self, Shares};
use news_store::{PgPool, map_err};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Everything the Debrief reports, as the queries return it.
#[derive(Debug, Clone, Default)]
pub struct Data {
    /// The window is the 7 days before this.
    pub as_of: DateTime<Utc>,
    pub pages: i64,
    pub by_tier: BTreeMap<String, i64>,
    pub by_type: BTreeMap<String, i64>,
    pub actual: Shares,
    pub target: Shares,
    pub jobs_discarded: BTreeMap<String, i64>,
    pub runs_failed: i64,
    pub indexed: i64,
    pub index_per_day: i64,
    pub curation_published: Option<DateTime<Utc>>,
    pub curation_stale_days: i64,
    pub unresolved: i64,
    /// Highest-scoring stories of the window that no run planned: (story id, title, S).
    pub top_rejected: Vec<(i64, String, f64)>,
}

fn counts(rows: Vec<(String, i64)>) -> BTreeMap<String, i64> {
    rows.into_iter().collect()
}

pub async fn gather(db: &PgPool, as_of: DateTime<Utc>) -> NewsResult<Data> {
    let one = |e| map_err(e);
    let from = as_of - TimeDelta::days(7);
    let standing = news_standing::current(db).await?;
    let published = "status = 'published' and published_at >= $1 and published_at < $2";
    let by = |col: &str| {
        format!(
            "select coalesce({col}::text, 'none'), count(*) from pages where {published} group by 1"
        )
    };
    let by_tier = counts(
        sqlx::query_as(sqlx::AssertSqlSafe(by("tier")))
            .bind(from)
            .bind(as_of)
            .fetch_all(db)
            .await
            .map_err(one)?,
    );
    let by_type = counts(
        sqlx::query_as(sqlx::AssertSqlSafe(by("page_type")))
            .bind(from)
            .bind(as_of)
            .fetch_all(db)
            .await
            .map_err(one)?,
    );
    let indexed: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "select count(*) from pages where {published} and index_status = 'index'"
    )))
    .bind(from)
    .bind(as_of)
    .fetch_one(db)
    .await
    .map_err(one)?;
    let jobs_discarded = counts(
        sqlx::query_as(
            "select coalesce(error_class, 'unknown'), count(*) from jobs
              where state = 'discarded' and finished_at >= $1 and finished_at < $2 group by 1",
        )
        .bind(from)
        .bind(as_of)
        .fetch_all(db)
        .await
        .map_err(one)?,
    );
    let runs_failed: i64 = sqlx::query_scalar(
        "select count(*) from runs where status = 'failed' and finished_at >= $1 and finished_at < $2",
    )
    .bind(from)
    .bind(as_of)
    .fetch_one(db)
    .await
    .map_err(one)?;
    let curation_published: Option<DateTime<Utc>> = sqlx::query_scalar(
        "select max(published_at) from home_curations where status = 'published' and published_at < $1",
    )
    .bind(as_of)
    .fetch_one(db)
    .await
    .map_err(one)?;
    let unresolved: i64 =
        sqlx::query_scalar("select count(*) from unresolved_mentions where status = 'open'")
            .fetch_one(db)
            .await
            .map_err(one)?;
    let top_rejected: Vec<(i64, String, f64)> = sqlx::query_as(
        "select s.story_id, coalesce(st.title, 'Story ' || s.story_id), s.total::float8
           from (select distinct on (story_id) story_id, total from scores
                  where computed_at >= $1 and computed_at < $2 order by story_id, computed_at desc) s
           join stories st on st.story_id = s.story_id
          where not exists (select 1 from run_plans rp where rp.story_id = s.story_id)
          order by s.total desc, s.story_id limit 5",
    )
    .bind(from)
    .bind(as_of)
    .fetch_all(db)
    .await
    .map_err(one)?;
    Ok(Data {
        as_of,
        pages: by_tier.values().sum(),
        by_tier,
        by_type,
        actual: news_nose::store::coverage(db, as_of).await.map_err(one)?,
        target: config::v1().map_err(NewsError::internal)?.coverage_targets,
        jobs_discarded,
        runs_failed,
        indexed,
        index_per_day: standing.int("publish.index_per_day"),
        curation_published,
        curation_stale_days: standing.int("home.stale_days"),
        unresolved,
        top_rejected,
    })
}

fn pct(x: f64) -> String {
    format!("{:.0}%", x * 100.0)
}

fn list(m: &BTreeMap<String, i64>) -> String {
    m.iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn stamp(t: DateTime<Utc>) -> String {
    t.format("%Y-%m-%d %H:%M UTC").to_string()
}

/// The command's data: `text` plus the figures it was made from.
pub fn render(d: &Data) -> Value {
    type Share<'a> = &'a BTreeMap<String, f64>;
    let dims: [(&str, Share, Share); 3] = [
        ("category", &d.actual.category, &d.target.category),
        ("region", &d.actual.region, &d.target.region),
        ("page_type", &d.actual.page_type, &d.target.page_type),
    ];
    let mut coverage = Map::new();
    let mut text = format!("Debrief: the 7 days to {}\n", stamp(d.as_of));
    text.push_str(&format!(
        "Pages published: {} (by tier: {}; by type: {})\n",
        d.pages,
        list(&d.by_tier),
        list(&d.by_type)
    ));
    text.push_str("Coverage, actual / target:\n");
    for (name, actual, target) in dims {
        let mut buckets = Map::new();
        let mut line = Vec::new();
        let keys: BTreeSet<&String> = actual.keys().chain(target.keys()).collect();
        for k in keys {
            let (a, t) = (
                actual.get(k).copied().unwrap_or(0.0),
                target.get(k).copied(),
            );
            buckets.insert(
                k.clone(),
                json!({"actual": (a * 1000.0).round() / 1000.0, "target": t}),
            );
            line.push(format!(
                "{k} {} / {}",
                pct(a),
                t.map_or("none".to_owned(), pct)
            ));
        }
        coverage.insert(name.to_owned(), Value::Object(buckets));
        text.push_str(&format!(
            "  {}: {}\n",
            name.replace('_', " "),
            line.join("; ")
        ));
    }
    text.push_str(&format!(
        "Failures: jobs discarded {} ({}); runs failed {}\n",
        d.jobs_discarded.values().sum::<i64>(),
        if d.jobs_discarded.is_empty() {
            "none".to_owned()
        } else {
            list(&d.jobs_discarded)
        },
        d.runs_failed
    ));
    text.push_str("Tokens per page: not recorded yet (Ghost, #18)\n");
    text.push_str(&format!(
        "Index budget: {} of {} indexed ({} a day)\n",
        d.indexed,
        d.index_per_day * 7,
        d.index_per_day
    ));
    let curation = match d.curation_published {
        Some(t) => {
            let age = (d.as_of - t).num_days();
            text.push_str(&format!(
                "Home curation: published {}, {age} days ago{}\n",
                stamp(t),
                if age >= d.curation_stale_days {
                    " (stale)"
                } else {
                    ""
                }
            ));
            json!({"published_at": t, "age_days": age, "stale": age >= d.curation_stale_days})
        }
        None => {
            text.push_str("Home curation: none published\n");
            json!({"published_at": null, "age_days": null, "stale": true})
        }
    };
    text.push_str(&format!("Unresolved entities: {} open\n", d.unresolved));
    text.push_str("Top rejected stories (scored, never planned):\n");
    if d.top_rejected.is_empty() {
        text.push_str("  none\n");
    }
    for (id, title, s) in &d.top_rejected {
        text.push_str(&format!("  story {id}: {title} (S {s:.1})\n"));
    }
    json!({
        "text": text,
        "window_days": 7,
        "pages": {"total": d.pages, "by_tier": d.by_tier, "by_type": d.by_type},
        "coverage": coverage,
        "failures": {"jobs_discarded": d.jobs_discarded, "runs_failed": d.runs_failed},
        "tokens_per_page": {"recorded": false, "note": "token counts are not stored yet (Ghost, #18)"},
        "index_budget": {"indexed": d.indexed, "per_day": d.index_per_day, "window": d.index_per_day * 7},
        "curation": curation,
        "unresolved_entities": d.unresolved,
        "top_rejected": d.top_rejected.iter()
            .map(|(id, title, s)| json!({"story_id": id, "title": title, "total": s}))
            .collect::<Vec<_>>(),
    })
}

/// `news review weekly`: the Debrief for the 7 days before `as_of`.
pub async fn weekly(db: &PgPool, as_of: DateTime<Utc>) -> NewsResult<Value> {
    Ok(render(&gather(db, as_of).await?))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// The Debrief of a hand-made week; `tests/golden/debrief.txt` is the text Karthik would read.
    pub fn week() -> Data {
        let share = |l: &[(&str, f64)]| l.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect();
        Data {
            as_of: "2026-11-23T12:00:00Z".parse().unwrap(),
            pages: 10,
            by_tier: BTreeMap::from([("1".into(), 3), ("2".into(), 7)]),
            by_type: BTreeMap::from([("earnings".into(), 4), ("macro".into(), 6)]),
            actual: Shares {
                category: share(&[("Economy", 0.6), ("Finance", 0.4)]),
                region: share(&[("india", 0.25), ("us", 0.75)]),
                page_type: share(&[("earnings", 0.4), ("macro", 0.6)]),
            },
            target: Shares {
                category: share(&[("Economy", 0.35), ("Finance", 0.25), ("Science", 0.1)]),
                region: share(&[("india", 0.15), ("us", 0.3)]),
                page_type: share(&[("earnings", 0.22), ("macro", 0.2)]),
            },
            jobs_discarded: BTreeMap::from([("parse".into(), 2), ("upstream".into(), 1)]),
            runs_failed: 1,
            indexed: 12,
            index_per_day: 20,
            curation_published: Some("2026-11-19T09:00:00Z".parse().unwrap()),
            curation_stale_days: 4,
            unresolved: 4,
            top_rejected: vec![
                (12, "Australian CPI".into(), 58.34),
                (15, "Apple EU ruling".into(), 55.0),
            ],
        }
    }

    #[test]
    fn the_debrief_text_is_the_golden_text() {
        let out = render(&week());
        let golden = include_str!("../tests/golden/debrief.txt");
        assert_eq!(out["text"].as_str().unwrap(), golden);
        assert_eq!(out["pages"]["total"], 10);
        assert_eq!(out["coverage"]["category"]["Science"]["actual"], 0.0);
        assert_eq!(out["index_budget"]["window"], 140);
        assert_eq!(out["curation"]["stale"], true);
        assert_eq!(out["tokens_per_page"]["recorded"], false);
    }

    #[test]
    fn a_week_with_nothing_still_renders() {
        let out = render(&Data::default());
        let text = out["text"].as_str().unwrap();
        assert!(text.contains("Home curation: none published"));
        assert!(text.contains("  none\n"));
    }
}
