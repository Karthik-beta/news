//! SEC EDGAR (U.S. Securities and Exchange Commission).
//!
//! * Discovery: `submissions/CIK##########.json` per tracked CIK. Earnings items are chosen by code
//!   from structured fields: forms 10-Q and 10-K (and amendments), and 8-K only when its `items`
//!   include 2.02 ("Results of Operations and Financial Condition"). No AI.
//! * Fetch: 10-Q/10-K → `api/xbrl/companyfacts/CIK##########.json`; 8-K → its primary document.
//! * Parse (companyfacts): every fact of the filing, plus every earlier fact of the same concept and
//!   unit (the history comparisons need). Values are the JSON number tokens exactly as SEC wrote them;
//!   each filing's report of a period is its own point (the accession number is in the context key),
//!   so a restatement in a later filing is a new vintage that supersedes the earlier one.
//!
//! SEC requires a declared User-Agent with a contact e-mail; the Runner adds `NEWS_CONTACT_EMAIL` for
//! this source only (`requires_contact_email` in Contacts).
//!
//! Fiscal periods are the company's own. companyfacts gives `fy`/`fp` *of the filing*, not of each
//! fact, so a fact gets `fiscal_year`/`fiscal_period` only when its period is the filing's own period
//! (it ends on the filing's latest period end and has the length `fp` implies) or the same period a
//! year earlier (`fy - 1`). Anything else keeps its dates and no fiscal label. Nothing is calendarised.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use serde_json::json;
use serde_json::value::RawValue;

use news_ledger::units::classify;
use news_ledger::{Draft, PeriodType, eventkey};

use crate::util::{date, day_vintage, find, shape, value_err};
use crate::{
    CandidateDraft, Connector, Cx, Discovery, EntityRef, ItemRef, NeedsHuman, ParseError,
    ParseErrorKind, Parsed, Request, Snapshot,
};

pub const ID: &str = "sec-edgar";
pub const SOURCE_ID: &str = "sec-edgar";
pub const PARSER_VERSION: u32 = 1;
const DATA: &str = "https://data.sec.gov/";
/// On a first run (no cursor), only filings this recent are discovered.
const FIRST_RUN_DAYS: i64 = 30;

pub struct SecEdgar;

/// Is this filing an earnings item? Code only, from the submissions JSON (Bureau §3.2).
pub fn is_earnings(form: &str, items: &str) -> bool {
    match form {
        "10-Q" | "10-K" | "10-Q/A" | "10-K/A" => true,
        "8-K" | "8-K/A" => items.split(',').any(|i| i.trim() == "2.02"),
        _ => false,
    }
}

#[derive(Deserialize)]
struct Submissions {
    cik: String,
    name: String,
    filings: Filings,
}

#[derive(Deserialize)]
struct Filings {
    recent: Recent,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Recent {
    accession_number: Vec<String>,
    filing_date: Vec<String>,
    report_date: Vec<String>,
    acceptance_date_time: Vec<String>,
    form: Vec<String>,
    items: Vec<String>,
    primary_document: Vec<String>,
}

#[derive(Deserialize)]
struct CompanyFacts<'a> {
    #[serde(rename = "entityName")]
    entity_name: String,
    #[serde(borrow)]
    facts: BTreeMap<String, BTreeMap<String, Concept<'a>>>,
}

#[derive(Deserialize)]
struct Concept<'a> {
    #[serde(default)]
    label: Option<String>,
    #[serde(borrow)]
    units: BTreeMap<String, Vec<Fact<'a>>>,
}

#[derive(Deserialize)]
struct Fact<'a> {
    #[serde(default)]
    start: Option<String>,
    end: String,
    #[serde(borrow)]
    val: &'a RawValue,
    accn: String,
    #[serde(default)]
    fy: Option<i16>,
    #[serde(default)]
    fp: Option<String>,
    form: String,
    filed: String,
    #[serde(default)]
    frame: Option<String>,
}

fn cursor_map(cursor: Option<&str>) -> BTreeMap<String, String> {
    cursor
        .and_then(|c| serde_json::from_str(c).ok())
        .unwrap_or_default()
}

/// Length class of a duration, as in the Ledger guards.
fn class_of(start: NaiveDate, end: NaiveDate) -> Option<&'static str> {
    match (end - start).num_days() + 1 {
        84..=98 => Some("quarter"),
        357..=373 => Some("year"),
        _ => None,
    }
}

fn fp_class(fp: &str) -> Option<&'static str> {
    match fp {
        "Q1" | "Q2" | "Q3" | "Q4" => Some("quarter"),
        "FY" => Some("year"),
        _ => None,
    }
}

fn parse_accepted(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

impl Connector for SecEdgar {
    fn id(&self) -> &'static str {
        ID
    }
    fn source_id(&self) -> &'static str {
        SOURCE_ID
    }
    fn parser_version(&self) -> u32 {
        PARSER_VERSION
    }

    fn discovery_requests(&self, cx: &Cx, _cursor: Option<&str>) -> Vec<Request> {
        cx.sec_ciks
            .iter()
            .filter_map(|c| eventkey::cik10(c))
            .map(|c| Request::json(format!("{DATA}submissions/CIK{c}.json")))
            .collect()
    }

    fn parse_discovery(
        &self,
        cx: &Cx,
        cursor: Option<&str>,
        snaps: &[Snapshot],
    ) -> Result<Discovery, ParseError> {
        let mut seen = cursor_map(cursor);
        let first_run_from = (cx.today - chrono::Duration::days(FIRST_RUN_DAYS)).to_string();
        let mut out = Discovery::default();
        for snap in snaps.iter().filter(|s| s.url.contains("/submissions/CIK")) {
            let sub: Submissions =
                serde_json::from_slice(&snap.bytes).map_err(|e| shape("$.filings.recent", e))?;
            let cik = eventkey::cik10(&sub.cik)
                .ok_or_else(|| value_err("$.cik", format!("not a CIK: {:?}", sub.cik)))?;
            let r = &sub.filings.recent;
            let n = r.accession_number.len();
            if [
                r.filing_date.len(),
                r.report_date.len(),
                r.acceptance_date_time.len(),
                r.form.len(),
                r.items.len(),
                r.primary_document.len(),
            ]
            .iter()
            .any(|&l| l != n)
            {
                return Err(shape("$.filings.recent", "column lengths differ"));
            }
            let last = seen.get(&cik).cloned();
            let mut newest = last.clone();
            for i in 0..n {
                let token = format!("{}|{}", r.filing_date[i], r.accession_number[i]);
                let is_new = match &last {
                    Some(c) => token > *c,
                    None => r.filing_date[i] >= first_run_from,
                };
                if !is_new || !is_earnings(&r.form[i], &r.items[i]) {
                    continue;
                }
                if newest.as_ref().is_none_or(|c| token > *c) {
                    newest = Some(token);
                }
                let accn = &r.accession_number[i];
                let cik_int = cik.trim_start_matches('0');
                out.refs.push(ItemRef {
                    source_id: SOURCE_ID.into(),
                    kind: "sec.filing".into(),
                    key: accn.clone(),
                    url: format!(
                        "https://www.sec.gov/Archives/edgar/data/{cik_int}/{}/{}",
                        accn.replace('-', ""),
                        r.primary_document[i]
                    ),
                    event_key_hint: None,
                    published_at: parse_accepted(&r.acceptance_date_time[i]),
                    title: Some(format!("{} {} ({})", sub.name, r.form[i], r.report_date[i])),
                    meta: json!({
                        "cik": cik, "form": r.form[i], "items": r.items[i],
                        "report_date": r.report_date[i], "filing_date": r.filing_date[i],
                    }),
                });
            }
            if let Some(t) = newest {
                seen.insert(cik, t);
            }
        }
        out.refs.sort_by(|a, b| a.key.cmp(&b.key));
        out.next_cursor = serde_json::to_string(&seen).ok();
        Ok(out)
    }

    fn fetch_requests(&self, item: &ItemRef) -> Vec<Request> {
        let form = item.meta.get("form").and_then(|v| v.as_str()).unwrap_or("");
        let cik = item.meta.get("cik").and_then(|v| v.as_str()).unwrap_or("");
        if form.starts_with("8-K") {
            let mut r = Request::json(item.url.clone());
            r.accept = vec!["text/html".into(), "application/xhtml+xml".into()];
            r.max_bytes = 20 * 1024 * 1024;
            return vec![r];
        }
        vec![Request::json(format!(
            "{DATA}api/xbrl/companyfacts/CIK{cik}.json"
        ))]
    }

    fn parse(&self, item: &ItemRef, snaps: &[Snapshot]) -> Result<Parsed, ParseError> {
        let form = item.meta.get("form").and_then(|v| v.as_str()).unwrap_or("");
        let cik = item
            .meta
            .get("cik")
            .and_then(|v| v.as_str())
            .and_then(eventkey::cik10)
            .ok_or_else(|| value_err("item.meta.cik", "missing CIK"))?;
        if form.starts_with("8-K") {
            // Item 2.02 press release: text blocks come with the documents parser (TODO #9 follow-up);
            // the candidate exists now so the story is seen. Its fiscal period is not in structured
            // data, so it carries no event key (Taster groups it with the 10-Q/10-K by entity + time).
            find(snaps, &item.url)?;
            return Ok(Parsed {
                candidates: vec![CandidateDraft {
                    event_key: None,
                    url: item.url.clone(),
                    title: item.title.clone().unwrap_or_default(),
                    published_at: item.published_at,
                    rule_key: "form:8-K/2.02".into(),
                    dp_ids: vec![],
                    entity: Some(("cik".into(), cik)),
                }],
                ..Parsed::default()
            });
        }
        let snap = find(snaps, &format!("/companyfacts/CIK{cik}.json"))?;
        let cf: CompanyFacts =
            serde_json::from_slice(&snap.bytes).map_err(|e| shape("$.facts", e))?;
        let target = item.key.as_str();

        // (taxonomy, concept, unit) reported by this filing.
        let mut wanted: BTreeSet<(&str, &str, &str)> = BTreeSet::new();
        // Each filing's own period end: the latest end of its non-dei duration facts.
        let mut accn_end: HashMap<&str, NaiveDate> = HashMap::new();
        for (tax, concepts) in &cf.facts {
            for (name, c) in concepts {
                for (unit, facts) in &c.units {
                    for f in facts {
                        if f.accn == target {
                            wanted.insert((tax, name, unit));
                        }
                        if tax != "dei"
                            && f.start.is_some()
                            && let Some(e) = date(&f.end)
                        {
                            let slot = accn_end.entry(f.accn.as_str()).or_insert(e);
                            *slot = (*slot).max(e);
                        }
                    }
                }
            }
        }
        if wanted.is_empty() {
            return Err(ParseError::new(
                ParseErrorKind::NotYet,
                format!("$.facts[*][*].units[*][?(@.accn=='{target}')]"),
                "companyfacts has no facts for this filing yet",
            ));
        }

        let entity = Some(("cik".to_owned(), cik.clone()));
        let mut out = Parsed::default();
        let mut seen_ids = BTreeSet::new();
        let mut item_points = Vec::new();
        let mut filing_fy: Option<(i16, String)> = None;
        for (tax, name, unit) in &wanted {
            let Some(concept) = cf.facts.get(*tax).and_then(|m| m.get(*name)) else {
                continue;
            };
            let Some(facts) = concept.units.get(*unit) else {
                continue;
            };
            let series_key = format!("xbrl:{cik}:{tax}:{name}");
            let unit_info = classify(unit);
            if unit_info.is_none() {
                out.needs_human.push(NeedsHuman {
                    reason: format!("unit {unit:?} is not in the unit vocabulary"),
                    series_key: series_key.clone(),
                    locator: json!({"kind": "xbrl", "tag": format!("{tax}:{name}"), "unit": unit}),
                });
            }
            for (j, f) in facts.iter().enumerate() {
                let path = format!("$.facts['{tax}']['{name}'].units['{unit}'][{j}]");
                let end =
                    date(&f.end).ok_or_else(|| value_err(format!("{path}.end"), "not a date"))?;
                let start = match &f.start {
                    Some(s) => Some(
                        date(s).ok_or_else(|| value_err(format!("{path}.start"), "not a date"))?,
                    ),
                    None => None,
                };
                let filed = date(&f.filed)
                    .ok_or_else(|| value_err(format!("{path}.filed"), "not a date"))?;
                let (period_type, period_label) = match start {
                    Some(s) => (PeriodType::Duration, format!("{s}/{end}")),
                    None => (PeriodType::Instant, end.to_string()),
                };
                // The company's own fiscal period, only where the filing states it (see module docs).
                let (mut fiscal_year, mut fiscal_period) = (None, None);
                if let (Some(fy), Some(fp), Some(own_end)) =
                    (f.fy, f.fp.as_deref(), accn_end.get(f.accn.as_str()))
                {
                    let len_ok = match start {
                        Some(s) => class_of(s, end).is_some() && class_of(s, end) == fp_class(fp),
                        None => true,
                    };
                    let gap = (*own_end - end).num_days();
                    if len_ok && gap == 0 {
                        (fiscal_year, fiscal_period) = (Some(fy), Some(fp.to_owned()));
                    } else if len_ok && (357..=373).contains(&gap) {
                        (fiscal_year, fiscal_period) = (Some(fy - 1), Some(fp.to_owned()));
                    }
                }
                let mut locator = json!({
                    "kind": "xbrl", "accn": f.accn, "tag": format!("{tax}:{name}"), "unit": unit,
                    "path": path, "form": f.form, "filed": f.filed, "fy": f.fy, "fp": f.fp,
                });
                if let Some(frame) = &f.frame {
                    locator["frame"] = json!(frame);
                }
                let point = Draft {
                    context: vec![
                        ("accn".into(), f.accn.clone()),
                        ("unit".into(), (*unit).to_owned()),
                    ],
                    series_key: series_key.clone(),
                    entity: entity.clone(),
                    concept: format!("{tax}:{name}"),
                    label: concept.label.clone().unwrap_or_else(|| (*name).to_owned()),
                    value_text: f.val.get().to_owned(),
                    unit: (*unit).to_owned(),
                    currency: unit_info.as_ref().and_then(|u| u.currency.clone()),
                    scale: None,
                    decimals: None,
                    period_label,
                    period_start: start,
                    period_end: Some(end),
                    period_type,
                    fiscal_year,
                    fiscal_period,
                    seasonal_adj: None,
                    vintage: day_vintage(filed),
                    snapshot_sha: snap.sha256,
                    locator,
                    parser: self.parser(),
                }
                .build()
                .map_err(|e| value_err(format!("{path}.val"), e))?;
                if !seen_ids.insert(point.dp_id.clone()) {
                    continue; // the same fact listed twice (different frames)
                }
                if f.accn == target {
                    item_points.push(point.dp_id.clone());
                    if filing_fy.is_none()
                        && let (Some(fy), Some(fp)) = (f.fy, &f.fp)
                    {
                        filing_fy = Some((fy, fp.clone()));
                    }
                }
                out.points.push(point);
            }
        }
        out.points
            .sort_by(|a, b| a.vintage.cmp(&b.vintage).then(a.dp_id.cmp(&b.dp_id)));
        out.candidates.push(CandidateDraft {
            event_key: filing_fy
                .as_ref()
                .and_then(|(fy, fp)| eventkey::earnings(&cik, *fy, fp)),
            url: item.url.clone(),
            title: item.title.clone().unwrap_or_else(|| cf.entity_name.clone()),
            published_at: item.published_at,
            rule_key: format!("form:{}", form.trim_end_matches("/A")),
            dp_ids: item_points,
            entity: entity.clone(),
        });
        out.entities.push(EntityRef {
            scheme: "cik".into(),
            value: cik,
            kind: "company".into(),
            name: cf.entity_name,
        });
        Ok(out)
    }
}
