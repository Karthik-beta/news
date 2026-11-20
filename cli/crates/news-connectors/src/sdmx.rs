//! SDMX-JSON data messages (ECB, OECD, ABS): a pure reader shared by those three Stringers.
//!
//! Handles the 1.0 layout (`dataSets` + `structure`, optionally inside `data`) and ABS's 2.0 layout
//! (`data.structures[0]`). Observation values are read as raw JSON text, so `2.10` stays `2.10`.
//! A `null` observation is a missing value. Unit, adjustment and multiplier are looked up by the
//! source's own dimension and attribute ids and stored with the names the source gives them.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;
use serde_json::value::RawValue;

use news_ledger::decimal::is_missing_marker;
use news_ledger::{Draft, Missing, PeriodType, eventkey};

use crate::stat::StatSeries;
use crate::util::{date_prefix, period_from_label, shape, value_err};
use crate::{CandidateDraft, EntityRef, ItemRef, NeedsHuman, ParseError, Parsed, Snapshot};

#[derive(Deserialize)]
struct Doc<'a> {
    #[serde(borrow, default)]
    data: Option<Body<'a>>,
    #[serde(borrow, default, rename = "dataSets")]
    data_sets: Option<Vec<DataSet<'a>>>,
    #[serde(default)]
    structure: Option<Value>,
}

#[derive(Deserialize)]
struct Body<'a> {
    #[serde(borrow, rename = "dataSets")]
    data_sets: Vec<DataSet<'a>>,
    #[serde(default)]
    structure: Option<Value>,
    #[serde(default)]
    structures: Option<Vec<Value>>,
}

#[derive(Deserialize)]
struct DataSet<'a> {
    #[serde(borrow)]
    series: BTreeMap<String, Series<'a>>,
}

#[derive(Deserialize)]
struct Series<'a> {
    #[serde(default)]
    attributes: Vec<Option<usize>>,
    #[serde(borrow)]
    observations: BTreeMap<String, Vec<&'a RawValue>>,
}

/// Which of the source's own ids carry what.
#[derive(Debug, Clone, Copy)]
pub struct Rules {
    /// Dimension or attribute ids that hold the unit (first present wins).
    pub unit: &'static [&'static str],
    pub seasonal: &'static [&'static str],
    /// Attribute ids for the unit multiplier and the decimals.
    pub multiplier: &'static [&'static str],
    pub decimals: &'static [&'static str],
}

/// `FM/B.U2…` → (`FM`, `B.U2…`): a catalogue `spec` is `flow/key`.
pub fn split_spec(spec: &str) -> (&str, &str) {
    spec.split_once('/').unwrap_or((spec, ""))
}

struct Slot<'a> {
    id: &'a str,
    value_id: Option<&'a str>,
    name: Option<&'a str>,
}

fn lookup<'a>(slots: &'a [Slot<'a>], ids: &[&str]) -> Option<&'a Slot<'a>> {
    ids.iter()
        .find_map(|id| slots.iter().find(|s| s.id == *id && s.name.is_some()))
}

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// The `values[i]` entry of the dimension or attribute list `list[pos]`.
fn nth(list: &Value, pos: usize, i: usize) -> Option<&Value> {
    list.get(pos)?.get("values")?.get(i)
}

/// Parse one SDMX-JSON message of the series `item` follows. The vintage is the snapshot's fetch
/// time: these services state no publication time (BLS rule, ADR 0018).
pub fn parse(
    series: &StatSeries,
    item: &ItemRef,
    snap: &Snapshot,
    parser: &str,
    rules: &Rules,
    key_prefix: &str,
) -> Result<Parsed, ParseError> {
    let doc: Doc = serde_json::from_slice(&snap.bytes).map_err(|e| shape("$", e))?;
    let (sets, structure) = match (doc.data, doc.data_sets, doc.structure) {
        (Some(b), _, _) => {
            let st = b
                .structure
                .or_else(|| b.structures.and_then(|v| v.into_iter().next()));
            (b.data_sets, st)
        }
        (None, Some(sets), st) => (sets, st),
        _ => return Err(shape("$", "no dataSets")),
    };
    let structure = structure.ok_or_else(|| shape("$.structure", "missing"))?;
    let set = sets
        .into_iter()
        .next()
        .ok_or_else(|| shape("$.dataSets", "empty"))?;
    let series_dims = structure
        .pointer("/dimensions/series")
        .ok_or_else(|| shape("$.structure.dimensions.series", "missing"))?;
    let time_dim = structure
        .pointer("/dimensions/observation")
        .and_then(|o| o.as_array())
        .and_then(|a| a.iter().find(|d| s(d, "id") == Some("TIME_PERIOD")))
        .ok_or_else(|| shape("$.structure.dimensions.observation", "no TIME_PERIOD"))?;
    let times = time_dim
        .get("values")
        .and_then(Value::as_array)
        .ok_or_else(|| shape("$.structure.dimensions.observation[0].values", "missing"))?;
    let series_attrs = structure.pointer("/attributes/series");
    let obs_attrs = structure.pointer("/attributes/observation");

    let mut out = Parsed::default();
    let mut latest: Option<(String, String)> = None; // (period label, dp_id)
    for (skey, ser) in &set.series {
        // Dimension values of this series, in key order.
        let mut slots: Vec<Slot> = Vec::new();
        let mut key_ids: Vec<&str> = Vec::new();
        for (pos, idx) in skey.split(':').enumerate() {
            let i: usize = idx
                .parse()
                .map_err(|e| shape(format!("$.series[{skey}]"), e))?;
            let v = nth(series_dims, pos, i).ok_or_else(|| {
                shape(format!("$.series[{skey}]"), "dimension value out of range")
            })?;
            let dim = series_dims.get(pos);
            key_ids.push(s(v, "id").unwrap_or(""));
            slots.push(Slot {
                id: dim.and_then(|d| s(d, "id")).unwrap_or(""),
                value_id: s(v, "id"),
                name: s(v, "name"),
            });
        }
        // Series attributes by position in `attributes.series`.
        if let Some(list) = series_attrs {
            for (pos, a) in ser.attributes.iter().enumerate() {
                if let Some(i) = a
                    && let Some(v) = nth(list, pos, *i)
                {
                    slots.push(Slot {
                        id: list.get(pos).and_then(|d| s(d, "id")).unwrap_or(""),
                        value_id: s(v, "id"),
                        name: s(v, "name"),
                    });
                }
            }
        }
        let skey_text = format!("{key_prefix}:{}", key_ids.join("."));
        let unit = lookup(&slots, rules.unit)
            .and_then(|u| u.name)
            .unwrap_or_default()
            .to_owned();
        let seasonal = lookup(&slots, rules.seasonal)
            .and_then(|u| u.name)
            .map(str::to_owned);
        let scale = lookup(&slots, rules.multiplier)
            .filter(|m| m.value_id != Some("0"))
            .and_then(|m| m.name)
            .map(str::to_owned);
        let decimals = lookup(&slots, rules.decimals)
            .and_then(|d| d.value_id)
            .and_then(|d| d.parse::<i16>().ok());
        let label = structure
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(series.id)
            .to_owned();
        if news_ledger::units::classify(&unit).is_none() {
            out.needs_human.push(NeedsHuman {
                reason: format!("unit {unit:?} is not in the unit vocabulary"),
                series_key: skey_text.clone(),
                locator: serde_json::json!({"kind": "json", "path": format!("$.series[{skey}]"), "unit": unit}),
            });
        }

        let mut ordered: Vec<(usize, &String, &Vec<&RawValue>)> = Vec::new();
        for (k, v) in &ser.observations {
            let i: usize = k
                .parse()
                .map_err(|e| shape(format!("$.series[{skey}].observations"), e))?;
            ordered.push((i, k, v));
        }
        ordered.sort_by_key(|(i, ..)| *i);
        for (i, k, cells) in ordered {
            let path = format!("$.dataSets[0].series[{skey}].observations[{k}][0]");
            let t = times
                .get(i)
                .ok_or_else(|| shape(&path, "time index out of range"))?;
            let period_label = s(t, "id")
                .ok_or_else(|| shape(&path, "time value has no id"))?
                .to_owned();
            let text = cells
                .first()
                .ok_or_else(|| shape(&path, "empty observation"))?
                .get()
                .trim()
                .trim_matches('"')
                .to_owned();
            let locator = serde_json::json!({
                "kind": "json", "path": path, "series": skey_text, "period": period_label,
                "dimensions": skey,
            });
            if text == "null" || is_missing_marker(&text) {
                out.missing.push(Missing {
                    series_key: skey_text.clone(),
                    period_label,
                    value_text: text,
                    locator,
                });
                continue;
            }
            let (start, end) = match (
                s(t, "start").and_then(date_prefix),
                s(t, "end").and_then(date_prefix),
            ) {
                (Some(a), Some(b)) => (a, b),
                _ => period_from_label(&period_label)
                    .ok_or_else(|| value_err(&path, "period is not a date or calendar period"))?,
            };
            let mut loc = locator;
            if let (Some(list), Some(attr_idx)) = (obs_attrs, cells.get(1))
                && list.get(0).and_then(|d| s(d, "id")) == Some("OBS_STATUS")
                && let Ok(ai) = attr_idx.get().parse::<usize>()
                && let Some(st) = nth(list, 0, ai).and_then(|v| s(v, "id"))
            {
                loc["obs_status"] = st.into();
            }
            let point = Draft {
                context: vec![],
                series_key: skey_text.clone(),
                entity: Some((series.entity.0.into(), series.entity.1.into())),
                concept: series.id.into(),
                label: label.clone(),
                value_text: text,
                unit: unit.clone(),
                currency: None,
                scale: scale.clone(),
                decimals,
                period_label: period_label.clone(),
                period_start: Some(start),
                period_end: Some(end),
                period_type: PeriodType::Duration,
                fiscal_year: None,
                fiscal_period: None,
                seasonal_adj: seasonal.clone(),
                vintage: snap.fetched_at,
                snapshot_sha: snap.sha256,
                locator: loc,
                parser: parser.to_owned(),
            }
            .build()
            .map_err(|e| value_err(&path, e))?;
            if latest.as_ref().is_none_or(|(p, _)| period_label > *p) {
                latest = Some((period_label, point.dp_id.clone()));
            }
            out.points.push(point);
        }
    }
    if let Some((period, dp)) = latest {
        out.candidates.push(CandidateDraft {
            event_key: eventkey::macro_release(series.country, series.family, &period),
            url: item.url.clone(),
            title: format!("{}, {period}", series.id),
            published_at: item.published_at,
            rule_key: format!("family:{}", series.family),
            dp_ids: vec![dp],
            entity: Some((series.entity.0.into(), series.entity.1.into())),
        });
    }
    out.entities.push(EntityRef {
        scheme: series.entity.0.into(),
        value: series.entity.1.into(),
        kind: "country".into(),
        name: series.entity.2.into(),
    });
    Ok(out)
}
