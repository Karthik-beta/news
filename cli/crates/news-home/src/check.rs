//! `news home check`: the TD's rules and ADR 0005's table, ported from `workers/shared/src/curation.ts`
//! (the write API runs the same rules again on the live page records). Failure codes are the ADR's.

use crate::draft::{Draft, body, schema_failures};
use crate::facts::PageFacts;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub const MIN_CATEGORIES: usize = 3;
pub const MIN_REGIONS: usize = 3;
pub const MAX_PER_ENTITY: usize = 2;
pub const LEAD_MAX_AGE_DAYS: f64 = 7.0;

/// The rule names of `checks.results` (ADR 0005); a failure's code belongs to the rule before its dot.
pub const RULES: [&str; 12] = [
    "schema",
    "page_missing",
    "page_unpublished",
    "page_withdrawn",
    "duplicate_page",
    "figure_not_on_page",
    "chart_not_on_page",
    "breadth_categories",
    "breadth_regions",
    "breadth_entity",
    "breadth_non_finance",
    "lead_stale",
];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Failure {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl Failure {
    pub fn new(code: &str, message: impl Into<String>, path: Option<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
            path,
        }
    }
}

/// One entry of `curation.v1`'s `checks.results`.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub struct RuleResult {
    pub rule: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub pass: bool,
    pub failures: Vec<Failure>,
    pub results: Vec<RuleResult>,
}

fn top(slug: &str) -> &str {
    slug.split('/').next().unwrap_or("")
}

fn clip(s: &str) -> String {
    s.chars().take(300).collect()
}

/// Run every rule on `draft` against the pages as PostgreSQL has them now. A page absent from `pages`
/// does not exist.
pub fn run(draft: &Draft, pages: &BTreeMap<String, PageFacts>, now: DateTime<Utc>) -> Report {
    let mut failures = schema_failures(&body(draft, now, now, &[]));
    let mut fail = |code: &str, msg: String, path: Option<&str>| {
        failures.push(Failure::new(code, msg, path.map(str::to_owned)));
    };

    let mut seen: BTreeMap<&str, String> = BTreeMap::new();
    let mut live: Vec<&PageFacts> = Vec::new();
    for (slot, path) in draft.placed() {
        let first = seen.get(slot.page_id.as_str()).cloned();
        match &first {
            Some(at) => fail(
                "duplicate_page",
                format!("page {} is already used at {at}", slot.page_id),
                Some(path.as_str()),
            ),
            None => {
                seen.insert(&slot.page_id, path.clone());
            }
        }
        let Some(page) = pages.get(&slot.page_id) else {
            fail(
                "page_missing",
                format!("page {} does not exist", slot.page_id),
                Some(path.as_str()),
            );
            continue;
        };
        match page.status.as_str() {
            "published" => {}
            "withdrawn" => {
                fail(
                    "page_withdrawn",
                    format!("page {} is withdrawn", slot.page_id),
                    Some(path.as_str()),
                );
                continue;
            }
            _ => {
                fail(
                    "page_unpublished",
                    format!("page {} is not published", slot.page_id),
                    Some(path.as_str()),
                );
                continue;
            }
        }
        if first.is_none() {
            live.push(page);
        }
        if let Some(f) = &slot.figure_id
            && !page.figures.contains(f)
        {
            fail(
                "figure_not_on_page",
                format!("figure {f} is not in page {}'s data", slot.page_id),
                Some(format!("{path}/figure_id").as_str()),
            );
        }
        if let Some(c) = &slot.chart_id
            && !page.charts.contains(c)
        {
            fail(
                "chart_not_on_page",
                format!("chart {c} is not one of page {}'s charts", slot.page_id),
                Some(format!("{path}/chart_id").as_str()),
            );
        }
    }

    // Breadth, over the pages that resolved.
    let categories: BTreeSet<&str> = live
        .iter()
        .map(|p| top(p.categories.first().map_or("", String::as_str)))
        .collect();
    if categories.len() < MIN_CATEGORIES {
        fail(
            "breadth_categories",
            format!(
                "curated pages span {} categories; at least {MIN_CATEGORIES} needed",
                categories.len()
            ),
            None,
        );
    }
    let regions: BTreeSet<&str> = live.iter().filter_map(|p| p.region.as_deref()).collect();
    if regions.len() < MIN_REGIONS {
        fail(
            "breadth_regions",
            format!(
                "curated pages span {} regions; at least {MIN_REGIONS} needed",
                regions.len()
            ),
            None,
        );
    }
    let mut per_entity: BTreeMap<&str, usize> = BTreeMap::new();
    for p in &live {
        for e in p
            .entities
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
        {
            *per_entity.entry(e).or_default() += 1;
        }
    }
    for (e, n) in per_entity {
        if n > MAX_PER_ENTITY {
            fail(
                "breadth_entity",
                format!("{n} stories are about {e}; at most {MAX_PER_ENTITY} allowed"),
                None,
            );
        }
    }
    if !draft
        .slots
        .sections
        .iter()
        .any(|s| top(&s.category) != "finance")
    {
        fail(
            "breadth_non_finance",
            "at least one section must be outside finance".into(),
            Some("/slots/sections"),
        );
    }

    // Freshness of the lead.
    let lead = &draft.slots.lead;
    if let Some(page) = pages.get(&lead.page_id).filter(|p| p.status == "published")
        && let Some(at) = page.published_at
    {
        let age_days = (now - at).num_seconds() as f64 / 86_400.0;
        let excused = lead.still_leading == Some(true) && page.tier == Some(1);
        if age_days > LEAD_MAX_AGE_DAYS && !excused {
            fail(
                "lead_stale",
                format!(
                    "the lead is {} days old; only a Tier 1 page marked still_leading may be older than {LEAD_MAX_AGE_DAYS} days",
                    age_days.floor()
                ),
                Some("/slots/lead"),
            );
        }
        if lead.still_leading == Some(true) && page.tier != Some(1) {
            fail(
                "lead_stale",
                "only a Tier 1 page can be marked still_leading".into(),
                Some("/slots/lead/still_leading"),
            );
        }
    }

    let results = RULES
        .iter()
        .map(|rule| {
            let msgs: Vec<&str> = failures
                .iter()
                .filter(|f| f.code.split('.').next() == Some(rule))
                .map(|f| f.message.as_str())
                .collect();
            RuleResult {
                rule: (*rule).to_owned(),
                ok: msgs.is_empty(),
                detail: (!msgs.is_empty()).then(|| clip(&msgs.join("; "))),
            }
        })
        .collect();
    Report {
        pass: failures.is_empty(),
        failures,
        results,
    }
}
