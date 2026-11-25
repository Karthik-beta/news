//! What the home commands know about a page, read from PostgreSQL: the page row, its story's scoring
//! inputs (categories, region, entities) and its figures (`page_refs`, `dp_…` of the current version).

use chrono::{DateTime, Utc};
use news_core::NewsResult;
use news_nose::score::StoryInput;
use news_store::{PgPool, map_err};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct PageFacts {
    pub page_id: String,
    /// `draft`, `ready`, `published` or `withdrawn`.
    pub status: String,
    pub title: Option<String>,
    pub tier: Option<i16>,
    pub page_type: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    /// The story's categories, primary first.
    pub categories: Vec<String>,
    /// The story's primary region.
    pub region: Option<String>,
    pub entities: Vec<String>,
    /// Data IDs a slot may show (`dp_…` of the page's current version).
    pub figures: Vec<String>,
    /// Chart IDs a slot may show.
    // shortcut: PostgreSQL does not record a page's chart IDs yet (they live in the bundle's meta.json);
    // empty until Press stores them, so `check` refuses every `chart_id` meanwhile.
    pub charts: Vec<String>,
    pub input: Option<StoryInput>,
}

#[derive(sqlx::FromRow)]
struct Row {
    page_id: String,
    status: String,
    tier: Option<i16>,
    page_type: Option<String>,
    published_at: Option<DateTime<Utc>>,
    title: Option<String>,
    score_input: Option<Value>,
    figures: Vec<String>,
}

impl From<Row> for PageFacts {
    fn from(r: Row) -> Self {
        let input = r
            .score_input
            .and_then(|v| serde_json::from_value::<StoryInput>(v).ok());
        let (categories, region, entities) = input.as_ref().map_or_else(Default::default, |i| {
            (
                i.categories.clone(),
                i.regions.first().cloned(),
                i.main_entity
                    .iter()
                    .chain(&i.entities)
                    .map(|e| e.id.clone())
                    .collect(),
            )
        });
        Self {
            page_id: r.page_id,
            status: r.status,
            title: r.title,
            tier: r.tier,
            page_type: r.page_type,
            published_at: r.published_at,
            categories,
            region,
            entities,
            figures: r.figures,
            charts: Vec::new(),
            input,
        }
    }
}

macro_rules! select_pages {
    ($($tail:literal),*) => {
        concat!(
            "select p.page_id, p.status, p.tier, p.page_type, p.published_at, s.title, s.score_input,
                    array(select r.ref from page_refs r
                           where r.page_id = p.page_id and r.version = p.version and left(r.ref, 3) = 'dp_'
                           order by r.ref) as figures
               from pages p left join stories s on s.story_id = p.story_id ",
            $($tail),*
        )
    };
}

/// The pages with these IDs; an ID with no row is absent from the map.
pub async fn load(db: &PgPool, ids: &[String]) -> NewsResult<BTreeMap<String, PageFacts>> {
    let rows: Vec<Row> = sqlx::query_as(select_pages!("where p.page_id = any($1)"))
        .bind(ids)
        .fetch_all(db)
        .await
        .map_err(map_err)?;
    Ok(rows
        .into_iter()
        .map(|r| (r.page_id.clone(), r.into()))
        .collect())
}

/// Published pages eligible for the home page: published since `since`, plus Tier 1 pages published
/// since `tier1_since` (TD "Home page curation", step 1).
pub async fn eligible(
    db: &PgPool,
    since: DateTime<Utc>,
    tier1_since: DateTime<Utc>,
) -> NewsResult<Vec<PageFacts>> {
    let rows: Vec<Row> = sqlx::query_as(select_pages!(
        "where p.status = 'published'
            and (p.published_at >= $1 or (p.tier = 1 and p.published_at >= $2))
          order by p.published_at desc, p.page_id"
    ))
    .bind(since)
    .bind(tier1_since)
    .fetch_all(db)
    .await
    .map_err(map_err)?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Page views over the 7 days before `now`, for the pages that have any (`traffic_daily`).
pub async fn views(
    db: &PgPool,
    ids: &[String],
    now: DateTime<Utc>,
) -> NewsResult<BTreeMap<String, i64>> {
    let from = now.date_naive() - chrono::TimeDelta::days(7);
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "select page_id, sum(views)::bigint from traffic_daily
          where day >= $1 and page_id = any($2) group by page_id",
    )
    .bind(from)
    .bind(ids)
    .fetch_all(db)
    .await
    .map_err(map_err)?;
    Ok(rows.into_iter().collect())
}

/// When the last curation went live, if one has.
pub async fn last_published(db: &PgPool) -> NewsResult<Option<DateTime<Utc>>> {
    sqlx::query_scalar("select max(published_at) from home_curations where status = 'published'")
        .fetch_one(db)
        .await
        .map_err(map_err)
}
