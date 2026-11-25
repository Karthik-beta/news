//! A story's `data.json` (bundle contract, `contracts/bundle/data.schema.json`), written beside its brief:
//! every value the brief's refs stand for, with the page-local source and the locator inside the stored
//! document. Values and labels are the Ledger's verbatim text; nothing is converted or recomputed.

use crate::build::{Built, period_ref};
use crate::inputs::{Inputs, Point};
use news_core::{NewsError, NewsResult};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

/// Where each value came from: the page-local source (`src_…`) and locator of every data point, and the
/// source of every document a quote is cut from.
#[derive(Debug, Clone, Default)]
pub struct Provenance {
    pub points: BTreeMap<String, (String, Value)>,
    pub docs: BTreeMap<i64, String>,
}

/// The page's `data.json` for a built brief.
pub fn data(inputs: &Inputs, built: &Built, prov: &Provenance) -> NewsResult<Value> {
    let b = &built.brief;
    let all: BTreeMap<&str, &Point> = inputs
        .history
        .iter()
        .chain(inputs.points.iter())
        .map(|p| (p.dp_id.as_str(), p))
        .collect();
    let point = |id: &str| {
        all.get(id)
            .copied()
            .ok_or_else(|| NewsError::internal(format!("{id} is not an input point")))
    };
    let label = |r: &str| {
        b.facts
            .iter()
            .find(|f| f.reference == r)
            .map(|f| f.label.clone())
            .or_else(|| b.series.iter().find(|s| s.reference == r).map(|s| s.label.clone()))
            .unwrap_or_default()
    };

    // Every point a ref reaches: facts, derived inputs, series members, context inputs.
    let mut ids: Vec<String> = b
        .facts
        .iter()
        .filter(|f| f.reference.starts_with("dp_"))
        .map(|f| f.reference.clone())
        .collect();
    ids.extend(built.refs.derived.values().flat_map(|d| d.inputs.clone()));
    ids.extend(
        built
            .refs
            .series
            .values()
            .flat_map(|s| s.points.iter().map(|[_, dp]| dp.clone())),
    );
    ids.extend(built.refs.context.values().flat_map(|c| c.inputs.clone()));

    let mut points = Map::new();
    let mut periods = Map::new();
    for id in ids {
        if points.contains_key(&id) {
            continue;
        }
        let p = point(&id)?;
        let (source, locator) = prov
            .points
            .get(&id)
            .ok_or_else(|| NewsError::internal(format!("{id} has no source")))?;
        let (value, _) = news_ledger::decimal::parse(&p.value_text)
            .map_err(|e| NewsError::validation(format!("{id}: {e}")))?;
        let pd = period_ref(p);
        let mut entry = json!({
            "label": p.label, "value": value, "value_text": p.value_text, "unit": p.unit,
            "currency": p.currency, "scale": p.scale, "period": pd, "source": source, "locator": locator,
        });
        if let Some(sa) = p
            .seasonal_adj
            .as_deref()
            .filter(|s| ["SA", "NSA", "SAAR"].contains(s))
        {
            entry["seasonal_adjustment"] = json!(sa);
        }
        points.insert(id, entry);
        periods.entry(pd).or_insert_with(|| period(p));
    }
    // Periods the brief issues without a point of their own keep the brief's label.
    for (pd, r) in &built.refs.periods {
        periods
            .entry(pd.clone())
            .or_insert_with(|| json!({"label": r.label, "start": r.start, "end": r.end}));
    }

    let derived: Map<String, Value> = built
        .refs
        .derived
        .iter()
        .map(|(id, d)| {
            let first = point(&d.inputs[0])?;
            Ok((
                id.clone(),
                json!({
                    "label": label(id), "value": d.value, "unit": d.unit, "currency": first.currency,
                    "scale": first.scale, "formula": d.formula, "inputs": d.inputs, "rounding": d.rounding,
                }),
            ))
        })
        .collect::<NewsResult<_>>()?;

    let series: Map<String, Value> = built
        .refs
        .series
        .iter()
        .map(|(id, s)| {
            let first = point(&s.points[0][1])?;
            let members: Vec<Value> = s
                .points
                .iter()
                .map(|[pd, dp]| json!({"period": pd, "point": dp}))
                .collect();
            Ok((
                id.clone(),
                json!({"label": label(id), "unit": s.unit, "currency": first.currency,
                       "scale": first.scale, "points": members}),
            ))
        })
        .collect::<NewsResult<_>>()?;

    let quotes: Map<String, Value> = built
        .refs
        .quotes
        .iter()
        .map(|(id, q)| {
            let text = b
                .quotes
                .iter()
                .find(|x| &x.id == id)
                .map(|x| x.text.clone())
                .unwrap_or_default();
            let source = prov
                .docs
                .get(&q.doc)
                .ok_or_else(|| NewsError::internal(format!("document {} has no source", q.doc)))?;
            Ok((
                id.clone(),
                json!({"text": text, "source": source,
                       "locator": {"kind": "text_block", "doc": q.doc, "block": q.block}}),
            ))
        })
        .collect::<NewsResult<_>>()?;

    let context: Map<String, Value> = built
        .refs
        .context
        .iter()
        .map(|(id, c)| {
            let text = b
                .context
                .iter()
                .find(|x| &x.id == id)
                .map(|x| x.text.clone())
                .unwrap_or_default();
            (
                id.clone(),
                json!({"text": text, "rule": c.rule, "inputs": c.inputs}),
            )
        })
        .collect();

    Ok(json!({
        "version": 1, "points": points, "derived": derived, "series": series,
        "periods": periods, "quotes": quotes, "context": context,
    }))
}

fn period(p: &Point) -> Value {
    let mut v = json!({"label": p.period_label, "start": p.period_start, "end": p.period_end});
    if p.fiscal_period.is_some() {
        v["fiscal"] = json!(true);
    }
    v
}
