//! Finding the pages a revision touches (Bureau §3.11, steps 1–2): published pages whose `page_refs`
//! name a data point that a later vintage supersedes, or a derived point built from one.

use chrono::{DateTime, NaiveDate, Utc};
use news_core::{NewsError, NewsResult};
use news_ledger::guard::{Comparison, Derived, Formula, derive};
use news_ledger::point::dv_id;
use news_ledger::{DataPoint, PeriodType};
use news_store::{PgPool, map_err};
use serde::Serialize;
use serde_json::Value;
use sqlx::Row;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// A data point and its newest superseding vintage.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
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
    /// The new vintage has another unit, currency, scale or seasonal adjustment. `data.json` keeps the
    /// old ones, so this is never a by-code change.
    pub incompatible: bool,
}

/// A derived point whose inputs moved, with its value recomputed from the new inputs.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct DvMove {
    pub old_id: String,
    /// The id the new inputs give it.
    pub new_id: String,
    pub formula: String,
    pub old_value: String,
    /// `None` when the Ledger's arithmetic refuses the new inputs.
    pub new_value: Option<String>,
    pub new_inputs: Vec<String>,
    /// What `news_ledger::guard::derive` returns for the new inputs, to store as a `derived_points`
    /// row. `None` when a guard refuses them.
    #[serde(skip)]
    pub new_derived: Option<Derived>,
    /// The derived point now has another unit, currency, scale or seasonal adjustment than the one
    /// the page shows.
    pub incompatible: bool,
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
        self.points
            .iter()
            .all(|p| p.old_text == p.new_text && !p.incompatible)
            && self
                .derived
                .iter()
                .all(|d| d.new_value.as_deref() == Some(d.old_value.as_str()) && !d.incompatible)
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

/// The Ledger form of the data points `ids`, read from `data_points`, for the guards in
/// `news_ledger::guard::derive`. Provenance the guards never read is left empty. A point whose text is
/// not a number is left out, so what needs it is refused.
async fn load_points(db: &PgPool, ids: &[String]) -> NewsResult<HashMap<String, DataPoint>> {
    let rows = sqlx::query(
        "select dp_id, series_key, label, value_text, unit, currency::text as currency, scale, seasonal_adj,
                period_label, period_start, period_end, period_type, fiscal_year, fiscal_period, vintage
           from data_points where dp_id = any($1)",
    )
    .bind(ids)
    .fetch_all(db)
    .await
    .map_err(map_err)?;
    let mut out = HashMap::new();
    for r in rows {
        let value_text: String = r.get("value_text");
        let Ok((value, style)) = news_ledger::decimal::parse(&value_text) else {
            continue;
        };
        let dp_id: String = r.get("dp_id");
        let period_type = match r.get::<String, _>("period_type").as_str() {
            "instant" => PeriodType::Instant,
            _ => PeriodType::Duration,
        };
        out.insert(
            dp_id.clone(),
            DataPoint {
                dp_id,
                context_key: String::new(),
                series_key: r.get("series_key"),
                entity: None,
                concept: String::new(),
                label: r.get("label"),
                value_text,
                value: Some(value),
                style,
                unit: r.get("unit"),
                currency: r.get("currency"),
                scale: r.get("scale"),
                decimals: None,
                period_label: r.get("period_label"),
                period_start: r.get::<Option<NaiveDate>, _>("period_start"),
                period_end: r.get::<Option<NaiveDate>, _>("period_end"),
                period_type,
                fiscal_year: r.get::<Option<i16>, _>("fiscal_year"),
                fiscal_period: r.get("fiscal_period"),
                seasonal_adj: r.get("seasonal_adj"),
                vintage: r.get::<DateTime<Utc>, _>("vintage"),
                supersedes: None,
                snapshot_sha: [0; 32],
                locator: Value::Null,
                parser: String::new(),
            },
        );
    }
    Ok(out)
}

/// A derived point recomputed by the Ledger's own guards (unit, currency, scale, seasonal adjustment,
/// comparable periods) from the new inputs `a` and `b`, compared the way the old point was (its
/// `guard.comparison`). `None` when a guard refuses, or the old point does not say how it was compared.
pub fn rederive(
    formula: &str,
    rounding: u32,
    old_guard: &Value,
    a: &DataPoint,
    b: &DataPoint,
) -> Option<Derived> {
    let formula = [
        Formula::PctChange,
        Formula::PpChange,
        Formula::Diff,
        Formula::Share,
    ]
    .into_iter()
    .find(|f| f.as_str() == formula)?;
    let cmp = match old_guard["comparison"].as_str()? {
        "year_on_year" => Comparison::YearOnYear,
        "sequential" => Comparison::Sequential,
        "same_period" => Comparison::SamePeriod,
        _ => return None,
    };
    derive(formula, a, b, cmp, rounding).ok()
}

/// The new derived point has the unit, currency, scale and seasonal adjustment the page shows.
pub fn same_shape(old_unit: &str, old_guard: &Value, new: &Derived) -> bool {
    new.unit == old_unit
        && ["currency", "scale", "seasonal_adj"]
            .iter()
            .all(|k| old_guard[k] == new.guard[k])
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
                n.dp_id as new_id, n.value_text as new_text, n.value::text as new_numeric, n.locator as new_locator,
                (o.unit, o.currency::text, o.scale, o.seasonal_adj)
                  is distinct from (n.unit, n.currency::text, n.scale, n.seasonal_adj) as incompatible
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
                    incompatible: r.get("incompatible"),
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
        "select dp_id, formula, inputs, value::text as value, rounding::int4 as rounding, unit, guard
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
    let points = load_points(db, &input_ids.into_iter().collect::<Vec<_>>()).await?;
    let mut derived: HashMap<String, DvMove> = HashMap::new();
    for r in dvs {
        let (old_id, formula): (String, String) = (r.get("dp_id"), r.get("formula"));
        let inputs: Vec<String> = r.get("inputs");
        let new_inputs: Vec<String> = inputs
            .iter()
            .map(|i| moved.get(i).map_or_else(|| i.clone(), |m| m.new_id.clone()))
            .collect();
        let rounding = r.get::<i32, _>("rounding").max(0) as u32;
        let old_guard: Value = r.get("guard");
        let new_derived = match new_inputs.as_slice() {
            [a, b] => match (points.get(a), points.get(b)) {
                (Some(a), Some(b)) => rederive(&formula, rounding, &old_guard, a, b),
                _ => None,
            },
            _ => None,
        };
        let incompatible = new_derived
            .as_ref()
            .is_some_and(|d| !same_shape(&r.get::<String, _>("unit"), &old_guard, d));
        let refs: Vec<&str> = new_inputs.iter().map(String::as_str).collect();
        derived.insert(
            old_id.clone(),
            DvMove {
                new_id: new_derived
                    .as_ref()
                    .map_or_else(|| dv_id(&formula, &refs), |d| d.dp_id.clone()),
                old_id,
                formula,
                old_value: r.get("value"),
                new_value: new_derived.as_ref().map(|d| d.value.clone()),
                new_inputs,
                new_derived,
                incompatible,
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use news_ledger::Draft;
    use serde_json::json;

    fn point(value: &str, scale: Option<&str>, year: i32, month: u32) -> DataPoint {
        let d = |m: u32| NaiveDate::from_ymd_opt(year, m, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(year, month, 28).unwrap();
        Draft {
            context: vec![],
            series_key: "co:revenue".into(),
            entity: None,
            concept: "Revenues".into(),
            label: "Revenue".into(),
            value_text: value.into(),
            unit: "USD".into(),
            currency: Some("USD".into()),
            scale: scale.map(str::to_owned),
            decimals: None,
            period_label: format!("{year}-{month:02}"),
            period_start: Some(d(month)),
            period_end: Some(end),
            period_type: PeriodType::Duration,
            fiscal_year: None,
            fiscal_period: None,
            seasonal_adj: None,
            vintage: Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap(),
            snapshot_sha: [1; 32],
            locator: Value::Null,
            parser: "t@1".into(),
        }
        .build()
        .unwrap()
    }

    fn old_guard() -> Value {
        json!({"unit": "USD", "currency": "USD", "scale": "millions", "seasonal_adj": null,
               "comparison": "sequential"})
    }

    #[test]
    fn a_revision_with_the_same_shape_is_recomputed_by_the_ledger() {
        let (a, b) = (
            point("110", Some("millions"), 2026, 2),
            point("100", Some("millions"), 2026, 1),
        );
        let d = rederive("pct_change(a,b)", 1, &old_guard(), &a, &b).unwrap();
        assert_eq!(d.value, "10.0");
        assert_eq!(d.dp_id, dv_id("pct_change(a,b)", &[&a.dp_id, &b.dp_id]));
        assert!(same_shape(
            "Percent",
            &json!({"currency": "USD", "scale": "millions"}),
            &d
        ));
    }

    #[test]
    fn a_scale_change_is_refused_or_not_the_same_shape() {
        // The new vintage is in billions, the other input in millions: the Ledger refuses to compare.
        let (a, b) = (
            point("0.11", Some("billions"), 2026, 2),
            point("100", Some("millions"), 2026, 1),
        );
        assert!(rederive("pct_change(a,b)", 1, &old_guard(), &a, &b).is_none());
        // Both inputs moved to billions: the change is computable, but it is no longer in millions.
        let (a, b) = (
            point("0.11", Some("billions"), 2026, 2),
            point("0.10", Some("billions"), 2026, 1),
        );
        let d = rederive("pct_change(a,b)", 1, &old_guard(), &a, &b).unwrap();
        assert!(!same_shape("Percent", &old_guard(), &d));
    }

    #[test]
    fn a_guard_that_does_not_say_how_it_compared_is_not_rederived() {
        let (a, b) = (
            point("110", Some("millions"), 2026, 2),
            point("100", Some("millions"), 2026, 1),
        );
        assert!(rederive("pct_change(a,b)", 1, &json!({}), &a, &b).is_none());
        assert!(rederive("nope(a,b)", 1, &old_guard(), &a, &b).is_none());
    }
}
