//! `news home candidates`: the pages the curator may choose from (Bureau §4.5), at most 40, ranked by
//! `home_score = 0.45·S_now + 20·[Tier 1] + 0.20·traffic_pct + 0.15·visual_richness` and picked with
//! Desk's bucket diversity, so the list is broad and short. Every number is computed here, never typed.

use crate::facts::{self, PageFacts};
use chrono::{DateTime, TimeDelta, Utc};
use news_core::NewsResult;
use news_nose::desk::{self, Candidate};
use news_nose::score::{Action, round2, score};
use news_nose::{config, store};
use news_store::{PgPool, map_err};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const TOP: u32 = 40;
/// A Tier 1 page stays a candidate this long after publication, even if a curation has run since.
const TIER1_DAYS: i64 = 14;
/// With no earlier curation, candidates are the last 7 days; with one, nothing older than 30.
const FIRST_DAYS: i64 = 7;
const MAX_DAYS: i64 = 30;
/// Key figures shown per candidate, and what each figure and chart adds to visual richness (max 100).
const SHOWN_FIGURES: usize = 8;
const FIGURE_RICHNESS: u32 = 20;
const CHART_RICHNESS: u32 = 30;

/// A published page with the numbers its score needs.
#[derive(Debug, Clone)]
pub struct Input {
    pub page: PageFacts,
    /// S re-scored at the current time (freshness decays).
    pub s_now: f64,
    /// Page views over the last 7 days (`traffic_daily`).
    pub views: i64,
}

pub fn visual_richness(page: &PageFacts) -> u32 {
    let figures = u32::try_from(page.figures.len()).unwrap_or(u32::MAX);
    let charts = u32::try_from(page.charts.len()).unwrap_or(u32::MAX);
    figures
        .saturating_mul(FIGURE_RICHNESS)
        .saturating_add(charts.saturating_mul(CHART_RICHNESS))
        .min(100)
}

/// The share of candidates with fewer views, 0–100; 0 while nobody has views.
fn traffic_pct(views: i64, all: &[i64]) -> f64 {
    if views <= 0 || all.len() < 2 {
        return 0.0;
    }
    let below = all.iter().filter(|v| **v < views).count();
    100.0 * below as f64 / (all.len() - 1) as f64
}

pub fn home_score(s_now: f64, tier: Option<i16>, traffic_pct: f64, richness: u32) -> f64 {
    0.45 * s_now
        + if tier == Some(1) { 20.0 } else { 0.0 }
        + 0.20 * traffic_pct
        + 0.15 * f64::from(richness)
}

/// Rank `inputs` and pick the top 40 with the bucket-diversity rule (category cap and entity limit).
pub fn rank(inputs: &[Input], now: DateTime<Utc>) -> Vec<Value> {
    let views: Vec<i64> = inputs.iter().map(|i| i.views).collect();
    let scored: Vec<(f64, f64)> = inputs
        .iter()
        .map(|i| {
            let pct = traffic_pct(i.views, &views);
            let s = home_score(i.s_now, i.page.tier, pct, visual_richness(&i.page));
            (s, pct)
        })
        .collect();
    // The index is the id: stories need not be unique here, pages are.
    let cands: Vec<Candidate> = inputs
        .iter()
        .zip(&scored)
        .enumerate()
        .map(|(k, (i, (s, _)))| Candidate {
            story_id: k as i64,
            s: *s,
            tier: Some(2),
            categories: i.page.categories.clone(),
            regions: i.page.region.iter().cloned().collect(),
            page_type: i.page.page_type.clone().unwrap_or_default(),
            // `entities` lists the main entity first.
            main_entity: i.page.entities.first().cloned(),
            action: Action::New,
        })
        .collect();
    desk::select(&cands, TOP)
        .picks
        .iter()
        .filter_map(|p| {
            let k = usize::try_from(p.story_id).ok()?;
            let (i, (s, pct)) = (inputs.get(k)?, scored.get(k)?);
            let page = &i.page;
            Some(json!({
                "rank": p.rank,
                "page_id": page.page_id,
                "title": page.title,
                "tier": page.tier,
                "page_type": page.page_type,
                "categories": page.categories,
                "region": page.region,
                "published_at": page.published_at,
                "age_hours": page.published_at.map(|t| (now - t).num_hours()),
                "home_score": round2(*s),
                "s_now": round2(i.s_now),
                "traffic_pct": round2(*pct),
                "visual_richness": visual_richness(page),
                "figures": page.figures.iter().take(SHOWN_FIGURES).collect::<Vec<_>>(),
                "charts": page.charts,
                "bucket": p.bucket,
                "reason": p.reason,
            }))
        })
        .collect()
}

/// The eligible pages with S_now and views, and a warning for each page left out.
pub async fn gather(db: &PgPool, now: DateTime<Utc>) -> NewsResult<(Vec<Input>, Vec<String>)> {
    let since = match facts::last_published(db).await? {
        Some(t) => t.max(now - TimeDelta::days(MAX_DAYS)),
        None => now - TimeDelta::days(FIRST_DAYS),
    };
    let pages = facts::eligible(db, since, now - TimeDelta::days(TIER1_DAYS)).await?;
    let cfg = config::v1().map_err(news_core::NewsError::internal)?;
    let coverage = store::coverage(db, now).await.map_err(map_err)?;
    let ids: Vec<String> = pages.iter().map(|p| p.page_id.clone()).collect();
    let views: BTreeMap<String, i64> = facts::views(db, &ids, now).await?;
    let mut warnings = Vec::new();
    let mut out = Vec::new();
    for page in pages {
        let Some(input) = &page.input else {
            warnings.push(format!(
                "page {} left out: its story has no scoring inputs",
                page.page_id
            ));
            continue;
        };
        match score(input, now, &coverage, &cfg) {
            Ok(s) => out.push(Input {
                views: views.get(&page.page_id).copied().unwrap_or(0),
                s_now: s.total,
                page,
            }),
            Err(e) => warnings.push(format!("page {} left out: {e}", page.page_id)),
        }
    }
    Ok((out, warnings))
}

/// `news home candidates`.
pub async fn run(db: &PgPool, now: DateTime<Utc>) -> NewsResult<(Value, Vec<String>)> {
    let (inputs, warnings) = gather(db, now).await?;
    let items = rank(&inputs, now);
    Ok((
        json!({"as_of": now, "eligible": inputs.len(), "count": items.len(), "items": items}),
        warnings,
    ))
}
