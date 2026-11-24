//! Fixer and PostgreSQL: loading a story's [`Inputs`], `news related`, and saving a built brief (the
//! `item.brief` job's writes: the brief files, derived points, and an A5 job when one is needed).

use crate::build::Built;
use crate::inputs::{Block, Document, Inputs, Point};
use crate::prefix::skill_page_type;
use news_core::brief::{Action, Entity, RelatedPage};
use news_core::{NewsError, NewsResult};
use news_dispatch::queue::{NewJob, enqueue_on};
use news_ledger::PeriodType;
use serde_json::{Value, json};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};
use std::path::{Path, PathBuf};

fn db_err(e: sqlx::Error) -> NewsError {
    news_store::map_err(e)
}

/// Pages shown as related, at most.
pub const MAX_RELATED: i64 = 10;

/// The `data_points` columns [`point`] reads (a macro, so queries stay static strings).
macro_rules! point_columns {
    () => {
        "dp_id, series_key, label, value_text, unit, currency::text as currency, scale, seasonal_adj,
         period_label, period_start, period_end, period_type, fiscal_year, fiscal_period, vintage"
    };
}

fn point(r: &PgRow) -> Result<Point, sqlx::Error> {
    let period_type: String = r.try_get("period_type")?;
    Ok(Point {
        dp_id: r.try_get("dp_id")?,
        series_key: r.try_get("series_key")?,
        label: r.try_get("label")?,
        value_text: r.try_get("value_text")?,
        unit: r.try_get("unit")?,
        currency: r.try_get("currency")?,
        scale: r.try_get("scale")?,
        seasonal_adj: r.try_get("seasonal_adj")?,
        period_label: r.try_get("period_label")?,
        period_start: r.try_get("period_start")?,
        period_end: r.try_get("period_end")?,
        period_type: if period_type == "instant" {
            PeriodType::Instant
        } else {
            PeriodType::Duration
        },
        fiscal_year: r.try_get("fiscal_year")?,
        fiscal_period: r.try_get("fiscal_period")?,
        vintage: r.try_get("vintage")?,
    })
}

/// Published pages that share the most entities with `entity_ids`, newest first: (page_id, slug,
/// story title).
async fn related_pages(
    db: &PgPool,
    story_id: i64,
    entity_ids: &[i64],
) -> Result<Vec<(String, String, Option<String>)>, sqlx::Error> {
    sqlx::query_as(
        "select p.page_id, p.slug, s.title
           from pages p join stories s on s.story_id = p.story_id
          where p.status = 'published' and p.story_id <> $1 and s.entity_ids && $2
          order by cardinality(array(select unnest(s.entity_ids) intersect select unnest($2::bigint[]))) desc,
                   p.published_at desc nulls last, p.page_id
          limit $3",
    )
    .bind(story_id)
    .bind(entity_ids)
    .bind(MAX_RELATED)
    .fetch_all(db)
    .await
}

/// Everything Fixer reads for one story (a number or an event key).
pub async fn load(db: &PgPool, story: &str) -> NewsResult<Inputs> {
    let story_id = news_taster::read::find(db, story).await?;
    let (event_key, entity_ids, has_page): (Option<String>, Vec<i64>, bool) = sqlx::query_as(
        "select s.event_key, s.entity_ids,
                s.page_id is not null or exists (select 1 from pages p where p.story_id = s.story_id
                                                  and p.status = 'published')
           from stories s where s.story_id = $1",
    )
    .bind(story_id)
    .fetch_one(db)
    .await
    .map_err(db_err)?;
    // The latest plan's tier and action; a story never planned is briefed as Tier 2.
    let plan: Option<(i16, String)> = sqlx::query_as(
        "select rp.tier, rp.action from run_plans rp join runs r on r.run_id = rp.run_id
          where rp.story_id = $1 order by r.started_at desc limit 1",
    )
    .bind(story_id)
    .fetch_optional(db)
    .await
    .map_err(db_err)?;
    // Members, best first (the most authoritative source, then the earliest), as Taster ranks them.
    type Member = (Option<String>, Option<Vec<String>>, Vec<i64>, Vec<String>);
    let members: Vec<Member> = sqlx::query_as(
        "select c.page_type, c.categories, c.doc_ids, coalesce(c.dp_ids, '{}')
           from candidates c join sources s on s.source_id = c.source_id
          where c.story_id = $1 order by s.source_tier, c.cand_id",
    )
    .bind(story_id)
    .fetch_all(db)
    .await
    .map_err(db_err)?;
    let candidate_type = members.iter().find_map(|m| m.0.clone()).ok_or_else(|| {
        NewsError::precondition(format!("story {story_id} has no page type yet"))
            .with_hint("Clerk classifies candidates (rules, then A3) before Fixer briefs them")
    })?;
    let page_type = skill_page_type(&candidate_type).ok_or_else(|| {
        NewsError::validation(format!("no page skill for page type {candidate_type:?}"))
    })?;
    let categories = members.iter().find_map(|m| m.1.clone()).unwrap_or_default();
    let mut dp_ids: Vec<String> = Vec::new();
    let mut doc_ids: Vec<i64> = Vec::new();
    for m in &members {
        for id in &m.3 {
            if !dp_ids.contains(id) {
                dp_ids.push(id.clone());
            }
        }
        for id in &m.2 {
            if !doc_ids.contains(id) {
                doc_ids.push(*id);
            }
        }
    }

    let entities: Vec<Entity> = sqlx::query_as::<_, (i64, String, String)>(
        "select e.entity_id, e.name, e.kind
           from unnest($1::bigint[]) with ordinality u(id, n) join entities e on e.entity_id = u.id
          order by u.n",
    )
    .bind(&entity_ids)
    .fetch_all(db)
    .await
    .map_err(db_err)?
    .into_iter()
    .map(|(id, name, kind)| Entity {
        id: format!("ent_{id}"),
        name,
        kind,
    })
    .collect();

    let mut points: Vec<Point> = sqlx::query(concat!(
        "select ",
        point_columns!(),
        " from data_points where dp_id = any($1)"
    ))
    .bind(&dp_ids)
    .fetch_all(db)
    .await
    .map_err(db_err)?
    .iter()
    .map(point)
    .collect::<Result<_, _>>()
    .map_err(db_err)?;
    points.sort_by_key(|p| dp_ids.iter().position(|d| *d == p.dp_id));

    // Each story series' history as of its latest story point (latest vintage per period).
    let mut history = Vec::new();
    let mut done: Vec<(String, String)> = Vec::new();
    for p in &points {
        let key = (p.series_key.clone(), p.unit.clone());
        if done.contains(&key) {
            continue;
        }
        let latest = points
            .iter()
            .filter(|q| (q.series_key.as_str(), q.unit.as_str()) == (&key.0[..], &key.1[..]))
            .max_by_key(|q| (q.period_end, q.vintage))
            .unwrap_or(p);
        let rows = sqlx::query(concat!(
            "select distinct on (period_start, period_end) ",
            point_columns!(),
            " from data_points
              where series_key = $1 and unit = $2 and period_end <= $3 and vintage <= $4
              order by period_start, period_end, vintage desc, dp_id desc"
        ))
        .bind(&key.0)
        .bind(&key.1)
        .bind(latest.period_end)
        .bind(latest.vintage)
        .fetch_all(db)
        .await
        .map_err(db_err)?;
        for r in &rows {
            history.push(point(r).map_err(db_err)?);
        }
        done.push(key);
    }

    let rows: Vec<(i64, i32, Option<String>, String)> = sqlx::query_as(
        "select b.doc_id, b.block_no, b.kind, b.text from text_blocks b
          where b.doc_id = any($1) order by array_position($1, b.doc_id), b.block_no",
    )
    .bind(&doc_ids)
    .fetch_all(db)
    .await
    .map_err(db_err)?;
    let mut documents: Vec<Document> = Vec::new();
    for (doc_id, block_no, kind, text) in rows {
        let block = Block {
            block_no,
            kind,
            text,
        };
        match documents.last_mut() {
            Some(d) if d.doc_id == doc_id => d.blocks.push(block),
            _ => documents.push(Document {
                doc_id,
                blocks: vec![block],
            }),
        }
    }

    // Our own page slugs, not the sources' headlines: the brief carries no untrusted text outside
    // `source_text` and quotes.
    let related = related_pages(db, story_id, &entity_ids)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(|(page_id, slug, _)| RelatedPage {
            page_id,
            title: slug,
        })
        .collect();

    let (tier, action) = match plan {
        Some((tier, action)) => (
            u8::try_from(tier).unwrap_or(2),
            if action == "update" {
                Action::Update
            } else {
                Action::New
            },
        ),
        None => (
            2,
            if has_page {
                Action::Update
            } else {
                Action::New
            },
        ),
    };
    Ok(Inputs {
        story_id,
        event_key,
        page_type: page_type.to_owned(),
        tier,
        action,
        categories,
        entities,
        points,
        history,
        documents,
        // shortcut: no table holds connectors' images yet (Wire/Clerk store them later); empty until then.
        images: Vec::new(),
        related,
    })
}

/// `news related <story>`: up to [`MAX_RELATED`] published pages sharing the story's entities, then
/// entities linked to them in Roster's graph.
pub async fn related(db: &PgPool, story: &str) -> NewsResult<Value> {
    let story_id = news_taster::read::find(db, story).await?;
    let entity_ids: Vec<i64> =
        sqlx::query_scalar("select entity_ids from stories where story_id = $1")
            .bind(story_id)
            .fetch_one(db)
            .await
            .map_err(db_err)?;
    let mut items: Vec<Value> = related_pages(db, story_id, &entity_ids)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(|(id, slug, title)| json!({"kind": "page", "id": id, "title": title.unwrap_or(slug)}))
        .collect();
    let room = MAX_RELATED - i64::try_from(items.len()).unwrap_or(MAX_RELATED);
    if room > 0 {
        let rows: Vec<(i64, String)> = sqlx::query_as(
            "select e.entity_id, e.name from edges g join entities e on e.entity_id = g.dst
              where g.src = any($1) and g.dst <> all($1)
              group by e.entity_id, e.name order by max(g.weight) desc, e.entity_id limit $2",
        )
        .bind(&entity_ids)
        .bind(room)
        .fetch_all(db)
        .await
        .map_err(db_err)?;
        items.extend(
            rows.into_iter().map(
                |(id, name)| json!({"kind": "entity", "id": format!("ent_{id}"), "title": name}),
            ),
        );
    }
    Ok(json!({ "items": items }))
}

/// What [`save`] wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub brief: PathBuf,
    pub refs: PathBuf,
    /// The `ai.condense` job, when the source text needs A5.
    pub condense_job: Option<i64>,
}

/// The `item.brief` job's writes: `<work>/briefs/<story>.json` (compact, what Ghost pipes to
/// `claude -p`) and `<story>.refs.json`, the derived points the brief issues (`derived_points`, kept
/// once), and an `ai.condense` job when the source text was condensed by code.
pub async fn save(db: &PgPool, work: &Path, built: &Built) -> NewsResult<Saved> {
    let story = built.brief.item.story_id;
    let dir = work.join("briefs");
    let io = |e: std::io::Error| NewsError::internal(format!("cannot write the brief: {e}"));
    std::fs::create_dir_all(&dir).map_err(io)?;
    let brief = dir.join(format!("{story}.json"));
    let refs = dir.join(format!("{story}.refs.json"));
    let json = |e: serde_json::Error| NewsError::internal(e.to_string());
    std::fs::write(&brief, serde_json::to_string(&built.brief).map_err(json)?).map_err(io)?;
    std::fs::write(&refs, serde_json::to_string(&built.refs).map_err(json)?).map_err(io)?;

    let mut tx = db.begin().await.map_err(db_err)?;
    for (id, d) in &built.refs.derived {
        sqlx::query(
            "insert into derived_points (dp_id, formula, inputs, value, unit, rounding, guard)
             values ($1, $2, $3, $4::numeric, $5, $6, $7)
             on conflict (dp_id) do nothing",
        )
        .bind(id)
        .bind(if d.formula == "share" {
            "share(a,total)".to_owned()
        } else {
            format!("{}(a,b)", d.formula)
        })
        .bind(&d.inputs)
        .bind(&d.value)
        .bind(&d.unit)
        .bind(i16::try_from(d.rounding).unwrap_or(i16::MAX))
        .bind(&d.guard)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
    }
    let mut condense_job = None;
    if let Some(c) = &built.condense {
        let job = NewJob::new("ai.condense")
            .unique_key(format!("ai.condense:story:{}", c.story_id))
            .args(json!({"story_id": c.story_id, "page_type": c.page_type, "blocks": c.blocks}));
        let job = NewJob {
            item_ref: Some(format!("story:{}", c.story_id)),
            ..job
        };
        condense_job = Some(enqueue_on(&mut tx, &job).await?.job_id);
    }
    tx.commit().await.map_err(db_err)?;
    Ok(Saved {
        brief,
        refs,
        condense_job,
    })
}
