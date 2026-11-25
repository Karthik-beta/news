//! Desk: fill a run breadth-first with diminishing returns per bucket, under hard limits, and assign
//! planned tiers (Bureau §4.4). Pure: the same candidates and target always give the same plan.

use crate::score::{Action, Score, StoryInput, round2};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

/// One scored story, as Desk sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub story_id: i64,
    /// S, as stored.
    pub s: f64,
    /// The tier S earns on its own; `None` (below the tier 2 floor) stays queued.
    pub tier: Option<u8>,
    /// Up to three; the first is the primary category (the cap's bucket).
    pub categories: Vec<String>,
    pub regions: Vec<String>,
    pub page_type: String,
    pub main_entity: Option<String>,
    pub action: Action,
}

impl Candidate {
    pub fn new(story_id: i64, input: &StoryInput, score: &Score) -> Self {
        Self {
            story_id,
            s: score.total,
            tier: score.tier,
            categories: input.categories.clone(),
            regions: input.regions.clone(),
            page_type: input.page.type_name().to_owned(),
            main_entity: input.main_entity.as_ref().map(|e| e.id.clone()),
            action: input.action,
        }
    }

    fn primary_category(&self) -> &str {
        self.categories.first().map_or("", String::as_str)
    }
}

/// The hard limits for a target (§4.4), in integer arithmetic so 0.35 · 20 is exactly 7.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub target: u32,
    /// `max(2, ⌈0.35 · target⌉)` stories per primary category.
    pub category_cap: u32,
    /// Stories per main entity.
    pub entity_limit: u32,
    /// `max(1, ⌊0.20 · target⌋)` Tier 1 pages.
    pub tier1_quota: u32,
    /// AI updates: at most 10 % of the run, `⌊0.10 · target⌋`.
    pub update_cap: u32,
}

impl Limits {
    pub fn for_target(target: u32) -> Self {
        Self {
            target,
            category_cap: (35 * target).div_ceil(100).max(2),
            entity_limit: 2,
            tier1_quota: (20 * target / 100).max(1),
            update_cap: 10 * target / 100,
        }
    }
}

/// One row of the Budget (`run_plans`).
#[derive(Debug, Clone, PartialEq)]
pub struct Pick {
    pub rank: u32,
    pub story_id: i64,
    pub s: f64,
    /// The planned tier: 2 when the story earned tier 1 after the quota was used.
    pub tier: u8,
    /// The primary category.
    pub bucket: String,
    /// S after the diminishing-returns factors, rounded to 2 decimals.
    pub adjusted: f64,
    pub reason: String,
    pub action: Action,
}

/// A story with S ≥ 40 that is not planned this run: it stays queued and is re-scored next run.
#[derive(Debug, Clone, PartialEq)]
pub struct Held {
    pub story_id: i64,
    pub s: f64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub limits: Limits,
    pub picks: Vec<Pick>,
    pub queued: Vec<Held>,
    /// Stories below the tier 2 floor, left queued.
    pub below_floor: usize,
}

#[derive(Default)]
struct Counts<'a> {
    category: BTreeMap<&'a str, u32>,
    region: BTreeMap<&'a str, u32>,
    page_type: BTreeMap<&'a str, u32>,
    entity: BTreeMap<&'a str, u32>,
    primary: BTreeMap<&'a str, u32>,
}

impl<'a> Counts<'a> {
    /// The story's most-covered bucket in a dimension: (count, bucket).
    fn most(map: &BTreeMap<&'a str, u32>, keys: &'a [String]) -> (u32, &'a str) {
        keys.iter()
            .map(|k| (map.get(k.as_str()).copied().unwrap_or(0), k.as_str()))
            .fold((0, ""), |best, x| if x.0 > best.0 { x } else { best })
    }

    /// `adjusted(s) = S · 0.92^n_cat · 0.92^n_region · 0.95^n_type · 0.5^n_entity`, and the factors.
    fn adjusted(&self, c: &'a Candidate) -> (f64, String) {
        let (n_cat, cat) = Self::most(&self.category, &c.categories);
        let (n_region, region) = Self::most(&self.region, &c.regions);
        let n_type = self
            .page_type
            .get(c.page_type.as_str())
            .copied()
            .unwrap_or(0);
        let n_entity = c
            .main_entity
            .as_deref()
            .and_then(|e| self.entity.get(e))
            .copied()
            .unwrap_or(0);
        let mut adj = c.s;
        let mut text = String::new();
        let main_entity = c.main_entity.as_deref().unwrap_or("");
        for (base, n, what, name) in [
            (0.92, n_cat, "category", cat),
            (0.92, n_region, "region", region),
            (0.95, n_type, "type", c.page_type.as_str()),
            (0.5, n_entity, "entity", main_entity),
        ] {
            if n > 0 {
                adj *= f64::powi(base, n as i32);
                text.push_str(&format!(" × {base}^{n} ({what} {name})"));
            }
        }
        (adj, text)
    }

    /// Why a hard limit blocks the story, if one does.
    fn blocked(&self, c: &Candidate, limits: &Limits) -> Option<String> {
        let primary = c.primary_category();
        let n = self.primary.get(primary).copied().unwrap_or(0);
        if n >= limits.category_cap {
            return Some(format!(
                "category cap: {primary} has {n} of {}",
                limits.category_cap
            ));
        }
        let e = c.main_entity.as_deref()?;
        let n = self.entity.get(e).copied().unwrap_or(0);
        (n >= limits.entity_limit)
            .then(|| format!("entity limit: {e} has {n} of {}", limits.entity_limit))
    }

    fn add(&mut self, c: &'a Candidate) {
        for k in &c.categories {
            *self.category.entry(k).or_default() += 1;
        }
        for k in &c.regions {
            *self.region.entry(k).or_default() += 1;
        }
        *self.page_type.entry(&c.page_type).or_default() += 1;
        if let Some(e) = &c.main_entity {
            *self.entity.entry(e).or_default() += 1;
        }
        *self.primary.entry(c.primary_category()).or_default() += 1;
    }
}

/// Higher S first, then the lower story id: a fixed order for ties.
fn before(a: &Candidate, b: &Candidate) -> bool {
    a.s > b.s || (a.s == b.s && a.story_id < b.story_id)
}

/// The planned tier: a story that earned tier 1 gets it while the quota lasts, else tier 2 (§4.4).
fn plan_tier(earned: Option<u8>, used: &mut u32, quota: u32) -> (u8, String) {
    match earned {
        Some(1) if *used < quota => {
            *used += 1;
            (1, format!("; tier 1 ({used} of {quota})"))
        }
        Some(1) => (
            2,
            format!("; earned tier 1, planned tier 2 (Tier 1 quota {quota} used)"),
        ),
        _ => (2, "; tier 2".to_owned()),
    }
}

/// Plan a run of `target` new pages (§4.4). New stories are picked greedily by `adjusted` until the
/// target is met or no story with S ≥ 40 that the hard limits allow remains; then AI updates, outside
/// the breadth caps, up to the update cap. The Tier 1 quota covers both.
pub fn select(cands: &[Candidate], target: u32) -> Plan {
    let limits = Limits::for_target(target);
    let below_floor = cands.iter().filter(|c| c.tier.is_none()).count();
    let mut news: Vec<&Candidate> = cands
        .iter()
        .filter(|c| c.tier.is_some() && c.action == Action::New)
        .collect();
    news.sort_by(|a, b| b.s.total_cmp(&a.s).then(a.story_id.cmp(&b.story_id)));
    let mut updates: Vec<&Candidate> = cands
        .iter()
        .filter(|c| c.tier.is_some() && c.action == Action::Update)
        .collect();
    updates.sort_by(|a, b| b.s.total_cmp(&a.s).then(a.story_id.cmp(&b.story_id)));

    let mut counts = Counts::default();
    let mut picked = vec![false; news.len()];
    let mut picks: Vec<Pick> = Vec::new();
    let mut tier1_used = 0;
    let quota = limits.tier1_quota;

    // shortcut: a linear scan per pick (O(stories × target)), not the lazy max-heap of §4.4; fine
    // for a few thousand open stories, upgrade when a plan takes more than a second.
    while (picks.len() as u32) < limits.target {
        let mut best: Option<(usize, f64, String)> = None;
        for (k, c) in news.iter().enumerate() {
            if picked[k] || counts.blocked(c, &limits).is_some() {
                continue;
            }
            let (adj, factors) = counts.adjusted(c);
            let better = match &best {
                None => true,
                Some((bk, badj, _)) => adj > *badj || (adj == *badj && before(c, news[*bk])),
            };
            if better {
                best = Some((k, adj, factors));
            }
        }
        let Some((k, adj, factors)) = best else {
            break;
        };
        picked[k] = true;
        let c = news[k];
        let (tier, tier_text) = plan_tier(c.tier, &mut tier1_used, quota);
        let rank = picks.len() as u32 + 1;
        picks.push(Pick {
            rank,
            story_id: c.story_id,
            s: c.s,
            tier,
            bucket: c.primary_category().to_owned(),
            adjusted: round2(adj),
            reason: format!("pick {rank}: S {:.2}{factors} = {adj:.2}{tier_text}", c.s),
            action: Action::New,
        });
        counts.add(c);
    }

    let mut queued: Vec<Held> = news
        .iter()
        .zip(&picked)
        .filter(|(_, p)| !**p)
        .map(|(c, _)| Held {
            story_id: c.story_id,
            s: c.s,
            reason: counts
                .blocked(c, &limits)
                .unwrap_or_else(|| format!("target of {} reached", limits.target)),
        })
        .collect();

    for (k, c) in updates.iter().enumerate() {
        if (k as u32) < limits.update_cap {
            let (tier, tier_text) = plan_tier(c.tier, &mut tier1_used, quota);
            let rank = picks.len() as u32 + 1;
            picks.push(Pick {
                rank,
                story_id: c.story_id,
                s: c.s,
                tier,
                bucket: c.primary_category().to_owned(),
                adjusted: c.s,
                reason: format!(
                    "update {} of {}: S {:.2}, outside the breadth caps{tier_text}",
                    k + 1,
                    limits.update_cap,
                    c.s
                ),
                action: Action::Update,
            });
        } else {
            queued.push(Held {
                story_id: c.story_id,
                s: c.s,
                reason: format!(
                    "update cap: AI updates take at most {} of a {}-page run",
                    limits.update_cap, limits.target
                ),
            });
        }
    }

    Plan {
        limits,
        picks,
        queued,
        below_floor,
    }
}

/// The `news plan` output (contract `plan.schema.json`).
pub fn summary(run_id: &str, plan: &Plan) -> Value {
    let mut by_tier = Map::new();
    let mut by_bucket: BTreeMap<&str, u64> = BTreeMap::new();
    for p in &plan.picks {
        let n = by_tier
            .entry(p.tier.to_string())
            .or_insert_with(|| json!(0));
        *n = json!(n.as_u64().unwrap_or(0) + 1);
        *by_bucket.entry(&p.bucket).or_default() += 1;
    }
    let l = plan.limits;
    json!({
        "run_id": run_id,
        "planned": plan.picks.len(),
        "by_tier": by_tier,
        "by_bucket": by_bucket,
        "limits": {
            "target": l.target,
            "category_cap": l.category_cap,
            "entity_limit": l.entity_limit,
            "tier1_quota": l.tier1_quota,
            "update_cap": l.update_cap,
        },
        "top": plan.picks.iter().take(10).map(|p| json!({
            "story_id": p.story_id,
            "S": p.s,
            "tier": p.tier,
            "action": p.action.as_str(),
            "reason": p.reason,
        })).collect::<Vec<_>>(),
        "queued": plan.queued.iter().map(|h| json!({
            "story_id": h.story_id,
            "S": h.s,
            "reason": h.reason,
        })).collect::<Vec<_>>(),
        "below_floor": plan.below_floor,
    })
}
