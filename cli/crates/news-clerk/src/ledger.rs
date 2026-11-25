//! Storing parsed output: data points (with vintages and `supersedes`), Roster step 1 entities, and
//! candidates. Every write is idempotent: storing the same parse twice changes nothing.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use news_connectors::{EntityRef, Parsed};
use news_ledger::DataPoint;
use serde::Serialize;
use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::candidates::Candidate;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct StoreReport {
    pub points_inserted: usize,
    /// Already stored (same dp_id) or an unchanged re-observation of a source without vintages.
    pub points_unchanged: usize,
    /// New vintages that supersede an earlier one with a different value (revisions).
    pub revisions: usize,
    pub candidates_inserted: usize,
    pub missing: usize,
    pub needs_human: usize,
}

/// Roster step 1: the entity named by `(scheme, value)`, created on first sight.
pub async fn ensure_entity(
    tx: &mut Transaction<'_, Postgres>,
    e: &EntityRef,
) -> Result<i64, sqlx::Error> {
    if let Some(id) = sqlx::query_scalar::<_, i64>(
        "select entity_id from entity_ids where scheme = $1 and value = $2 and valid_to is null
          order by valid_from desc limit 1",
    )
    .bind(&e.scheme)
    .bind(&e.value)
    .fetch_optional(&mut **tx)
    .await?
    {
        return Ok(id);
    }
    let id: i64 = sqlx::query_scalar(
        "insert into entities (kind, name, region) values ($1, $2, $3) returning entity_id",
    )
    .bind(&e.kind)
    .bind(&e.name)
    .bind((e.scheme == "iso3166").then(|| e.value.to_lowercase()))
    .fetch_one(&mut **tx)
    .await?;
    sqlx::query("insert into entity_ids (scheme, value, entity_id) values ($1, $2, $3)")
        .bind(&e.scheme)
        .bind(&e.value)
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(id)
}

/// The latest earlier vintage of the same series and period (in the same unit): what a new point
/// supersedes.
async fn previous(
    tx: &mut Transaction<'_, Postgres>,
    p: &DataPoint,
) -> Result<Option<(String, String, String)>, sqlx::Error> {
    let row = sqlx::query(
        "select dp_id, value_text, context_key from data_points
          where series_key = $1 and unit = $2 and period_type = $3 and period_label = $4
            and period_start is not distinct from $5 and period_end is not distinct from $6
            and vintage < $7 and dp_id <> $8
          order by vintage desc, dp_id desc limit 1",
    )
    .bind(&p.series_key)
    .bind(&p.unit)
    .bind(p.period_type.as_str())
    .bind(&p.period_label)
    .bind(p.period_start)
    .bind(p.period_end)
    .bind(p.vintage)
    .bind(&p.dp_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(|r| (r.get("dp_id"), r.get("value_text"), r.get("context_key"))))
}

/// Store parsed points and candidates in one transaction.
pub async fn store(
    pool: &PgPool,
    parsed: &Parsed,
    candidates: &[Candidate],
    retrieved_at: DateTime<Utc>,
) -> Result<StoreReport, sqlx::Error> {
    let mut report = StoreReport {
        missing: parsed.missing.len(),
        needs_human: parsed.needs_human.len(),
        ..StoreReport::default()
    };
    let mut tx = pool.begin().await?;
    let mut entities: HashMap<(String, String), i64> = HashMap::new();
    for e in &parsed.entities {
        let id = ensure_entity(&mut tx, e).await?;
        entities.insert((e.scheme.clone(), e.value.clone()), id);
    }
    // Oldest vintage first, so each point can link to the one before it.
    let mut points: Vec<&DataPoint> = parsed.points.iter().collect();
    points.sort_by(|a, b| a.vintage.cmp(&b.vintage).then(a.dp_id.cmp(&b.dp_id)));
    for p in points {
        let exists: bool =
            sqlx::query_scalar("select exists (select 1 from data_points where dp_id = $1)")
                .bind(&p.dp_id)
                .fetch_one(&mut *tx)
                .await?;
        if exists {
            report.points_unchanged += 1;
            continue;
        }
        let prev = previous(&mut tx, p).await?;
        if let Some((_, value_text, context_key)) = &prev
            && *value_text == p.value_text
            && *context_key == p.context_key
        {
            // A source without vintages (BLS) re-observed the same value: not a new vintage.
            report.points_unchanged += 1;
            continue;
        }
        if prev.as_ref().is_some_and(|(_, v, _)| *v != p.value_text) {
            report.revisions += 1;
        }
        let entity_id = p.entity.as_ref().and_then(|k| entities.get(k)).copied();
        sqlx::query(
            "insert into data_points (dp_id, context_key, series_key, entity_id, concept, label,
               value_text, value, unit, currency, scale, decimals, period_label, period_start,
               period_end, period_type, fiscal_year, fiscal_period, seasonal_adj, vintage, supersedes,
               snapshot_sha, locator, parser, retrieved_at)
             values ($1,$2,$3,$4,$5,$6,$7,$8::numeric,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,
               $21,$22,$23,$24,$25)
             on conflict (dp_id) do nothing",
        )
        .bind(&p.dp_id)
        .bind(&p.context_key)
        .bind(&p.series_key)
        .bind(entity_id)
        .bind(&p.concept)
        .bind(&p.label)
        .bind(&p.value_text)
        .bind(p.value.as_deref())
        .bind(&p.unit)
        .bind(p.currency.as_deref())
        .bind(p.scale.as_deref())
        .bind(p.decimals)
        .bind(&p.period_label)
        .bind(p.period_start)
        .bind(p.period_end)
        .bind(p.period_type.as_str())
        .bind(p.fiscal_year)
        .bind(p.fiscal_period.as_deref())
        .bind(p.seasonal_adj.as_deref())
        .bind(p.vintage)
        .bind(prev.map(|(id, ..)| id))
        .bind(p.snapshot_sha.as_slice())
        .bind(&p.locator)
        .bind(&p.parser)
        .bind(retrieved_at)
        .execute(&mut *tx)
        .await?;
        report.points_inserted += 1;
    }
    for c in candidates {
        let entity_ids: Vec<i64> = c
            .draft
            .entity
            .as_ref()
            .and_then(|k| entities.get(k))
            .copied()
            .into_iter()
            .collect();
        let inserted = sqlx::query(
            "insert into candidates (source_id, url, title, event_key, dp_count, entity_ids, page_type,
               page_type_by, categories, region, published_at, dp_ids, status)
             select $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,'new'
              where not exists (select 1 from candidates where source_id = $1 and url = $2
                                  and event_key is not distinct from $4)",
        )
        .bind(&c.source_id)
        .bind(&c.draft.url)
        .bind(&c.draft.title)
        .bind(c.draft.event_key.as_deref())
        .bind(i32::try_from(c.draft.dp_ids.len()).unwrap_or(i32::MAX))
        .bind(&entity_ids)
        .bind(c.page_type.as_deref())
        .bind(c.page_type_by)
        .bind(&c.categories)
        .bind(c.region.as_deref())
        .bind(c.draft.published_at)
        .bind(&c.draft.dp_ids)
        .execute(&mut *tx)
        .await?;
        report.candidates_inserted += inserted.rows_affected() as usize;
    }
    tx.commit().await?;
    Ok(report)
}
