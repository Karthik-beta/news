//! Finding the pages a revision touches (Bureau §3.11, steps 1–2): published pages whose `page_refs`
//! name a data point that a later vintage supersedes, or a derived point built from one.

use crate::classify::recompute;
use news_core::{NewsError, NewsResult};
use news_ledger::point::dv_id;
use news_store::{PgPool, map_err};
use serde::Serialize;
use serde_json::Value;
use sqlx::Row;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// A data point and its newest superseding vintage.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PointMove {
    pub old_id: String,
    pub new_id: String,
    /// The values as published (verbatim text).
    pub old_text: String,
    pub new_text: String,
    /// The new value as an exact decimal (`data.json`'s `value`).
    pub new_numeric: Option<String>,
    pub new_locator: Value,
    /// The old point was an earnings fact (it has a fiscal period): a restatement.
    pub restated: bool,
}

/// A derived point whose inputs moved, with its value recomputed from the new inputs.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DvMove {
    pub old_id: String,
    /// The id the new inputs give it.
    pub new_id: String,
    pub formula: String,
    pub old_value: String,
    /// `None` when the Ledger's arithmetic refuses the new inputs.
    pub new_value: Option<String>,
    pub new_inputs: Vec<String>,
}

/// Everything that changed on one published page.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PageChange {
    pub page_id: String,
    pub version: i32,
    pub story_id: Option<i64>,
    pub points: Vec<PointMove>,
    pub derived: Vec<DvMove>,
}

impl PageChange {
    /// Moves whose displayed value really differs: a new vintage with the same text needs nothing.
    pub fn is_noop(&self) -> bool {
        self.points.iter().all(|p| p.old_text == p.new_text)
            && self
                .derived
                .iter()
                .all(|d| d.new_value.as_deref() == Some(d.old_value.as_str()))
    }

    /// Every ref the page uses that has changed, old ids.
    pub fn changed_refs(&self) -> Vec<String> {
        self.points
            .iter()
            .map(|p| p.old_id.clone())
            .chain(self.derived.iter().map(|d| d.old_id.clone()))
            .collect()
    }

    /// Old id to new id, for rewriting the page's text and data.
    pub fn renames(&self) -> Vec<(String, String)> {
        self.points
            .iter()
            .map(|p| (p.old_id.clone(), p.new_id.clone()))
            .chain(
                self.derived
                    .iter()
                    .map(|d| (d.old_id.clone(), d.new_id.clone())),
            )
            .collect()
    }
}

/// The newest vintage of every data point a published page references, where it has been superseded.
/// `source` limits it to points whose snapshot came from that source.
async fn moves(db: &PgPool, source: Option<&str>) -> NewsResult<HashMap<String, PointMove>> {
    let rows = sqlx::query(
        "with recursive live as (
             select distinct r.ref as dp_id
               from page_refs r join pages p on p.page_id = r.page_id and p.version = r.version
              where p.status = 'published' and r.ref like 'dp\\_%'),
         chain (root, dp_id, vintage) as (
             select d.dp_id, d.dp_id, d.vintage from data_points d join live l on l.dp_id = d.dp_id
             union all
             select c.root, n.dp_id, n.vintage from chain c join data_points n on n.supersedes = c.dp_id),
         newest as (
             select distinct on (root) root, dp_id from chain where dp_id <> root order by root, vintage desc)
         select o.dp_id as old_id, o.value_text as old_text, o.fiscal_period as old_fiscal,
                n.dp_id as new_id, n.value_text as new_text, n.value::text as new_numeric, n.locator as new_locator
           from newest w
           join data_points o on o.dp_id = w.root
           join data_points n on n.dp_id = w.dp_id
           join snapshots s on s.sha256 = o.snapshot_sha
          where ($1::text is null or s.source_id = $1)",
    )
    .bind(source)
    .fetch_all(db)
    .await
    .map_err(map_err)?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let old_id: String = r.get("old_id");
            let old_fiscal: Option<String> = r.get("old_fiscal");
            (
                old_id.clone(),
                PointMove {
                    old_id,
                    new_id: r.get("new_id"),
                    old_text: r.get("old_text"),
                    new_text: r.get("new_text"),
                    new_numeric: r.get("new_numeric"),
                    new_locator: r.get("new_locator"),
                    restated: old_fiscal.is_some(),
                },
            )
        })
        .collect())
}

/// Published pages with revised data, in page order. A page appears once, at its current version.
pub async fn find(db: &PgPool, source: Option<&str>) -> NewsResult<Vec<PageChange>> {
    let moved = moves(db, source).await?;
    if moved.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = moved.keys().cloned().collect();
    // Derived points built from a moved point, and the values of every input they need.
    let dvs = sqlx::query(
        "select dp_id, formula, inputs, value::text as value, rounding::int4 as rounding
           from derived_points where inputs && $1::text[]",
    )
    .bind(&ids)
    .fetch_all(db)
    .await
    .map_err(map_err)?;
    let mut input_ids: BTreeSet<String> = BTreeSet::new();
    for r in &dvs {
        for i in r.get::<Vec<String>, _>("inputs") {
            input_ids.insert(moved.get(&i).map_or(i, |m| m.new_id.clone()));
        }
    }
    let texts: HashMap<String, String> =
        sqlx::query("select dp_id, value_text from data_points where dp_id = any($1)")
            .bind(input_ids.iter().cloned().collect::<Vec<_>>())
            .fetch_all(db)
            .await
            .map_err(map_err)?
            .into_iter()
            .map(|r| (r.get("dp_id"), r.get("value_text")))
            .collect();
    let mut derived: HashMap<String, DvMove> = HashMap::new();
    for r in dvs {
        let (old_id, formula): (String, String) = (r.get("dp_id"), r.get("formula"));
        let inputs: Vec<String> = r.get("inputs");
        let new_inputs: Vec<String> = inputs
            .iter()
            .map(|i| moved.get(i).map_or_else(|| i.clone(), |m| m.new_id.clone()))
            .collect();
        let rounding = r.get::<i32, _>("rounding").max(0) as u32;
        let new_value = match new_inputs.as_slice() {
            [a, b] => match (texts.get(a), texts.get(b)) {
                (Some(a), Some(b)) => recompute(&formula, rounding, a, b),
                _ => None,
            },
            _ => None,
        };
        let refs: Vec<&str> = new_inputs.iter().map(String::as_str).collect();
        derived.insert(
            old_id.clone(),
            DvMove {
                new_id: dv_id(&formula, &refs),
                old_id,
                formula,
                old_value: r.get("value"),
                new_value,
                new_inputs,
            },
        );
    }
    // The published pages that use any of them.
    let mut wanted: Vec<String> = ids;
    wanted.extend(derived.keys().cloned());
    let pages = sqlx::query(
        "select p.page_id, p.version, p.story_id, r.ref
           from page_refs r join pages p on p.page_id = r.page_id and p.version = r.version
          where p.status = 'published' and r.ref = any($1)
          order by p.page_id, r.ref",
    )
    .bind(&wanted)
    .fetch_all(db)
    .await
    .map_err(map_err)?;
    let mut out: BTreeMap<String, PageChange> = BTreeMap::new();
    for r in pages {
        let (page_id, reference): (String, String) = (r.get("page_id"), r.get("ref"));
        let page = out.entry(page_id.clone()).or_insert_with(|| PageChange {
            page_id,
            version: r.get("version"),
            story_id: r.get("story_id"),
            points: Vec::new(),
            derived: Vec::new(),
        });
        if let Some(m) = moved.get(&reference) {
            page.points.push(m.clone());
        } else if let Some(d) = derived.get(&reference) {
            page.derived.push(d.clone());
        }
    }
    Ok(out.into_values().filter(|p| !p.is_noop()).collect())
}

/// The changes on one published page (none when its data is current), or `not_found`.
pub async fn for_page(db: &PgPool, page_id: &str) -> NewsResult<Option<PageChange>> {
    let published: Option<i32> =
        sqlx::query_scalar("select version from pages where page_id = $1 and status = 'published'")
            .bind(page_id)
            .fetch_optional(db)
            .await
            .map_err(map_err)?;
    if published.is_none() {
        return Err(NewsError::not_found(format!("no published page {page_id}")));
    }
    Ok(find(db, None)
        .await?
        .into_iter()
        .find(|p| p.page_id == page_id))
}
