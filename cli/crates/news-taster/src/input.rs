//! `stories.score_input`: the story input Nose scores (`news_nose::score::StoryInput`, ADR 0012),
//! built by code from a story's candidates, their sources and Roster's entities.
//!
//! News, papers and science get their input from the candidates themselves. The structured types
//! (macro, policy, earnings, report) read their data points through the candidates' `dp_ids`
//! ([`structured`]); a story whose candidates carry none, or whose data is too short to compute from,
//! keeps a null input and `news plan` skips it.

mod structured;

use chrono::{DateTime, Utc};
use news_nose::score::{Action, Entity, Novelty, Page, StoryInput};
use sqlx::{PgConnection, Row};

/// One candidate of a story, with its source's tier and redistribution flag.
#[derive(Debug, Clone)]
pub struct Member {
    pub cand_id: i64,
    pub source_id: String,
    pub tier: i16,
    pub redistribution: bool,
    pub page_type: Option<String>,
    pub categories: Vec<String>,
    pub region: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub first_seen: DateTime<Utc>,
    pub dp_count: i32,
    /// The candidate's data points; empty for a candidate stored before `dp_ids` existed.
    pub dp_ids: Vec<String>,
    pub event_key: Option<String>,
    pub entity_ids: Vec<i64>,
}

/// The best member: the most authoritative source, then the earliest candidate.
fn best(members: &[Member]) -> Option<&Member> {
    members.iter().min_by_key(|m| (m.tier, m.cand_id))
}

/// A candidate's region as a coverage bucket (`data/seeds/coverage_targets.toml`). A region with no
/// bucket passes through, and then has no coverage target, so no gap.
pub fn region_bucket(region: &str) -> String {
    match region {
        "us" | "ca" => "north-america",
        "eu" | "uk" => "europe",
        "in" => "india",
        "cn" => "china",
        "jp" | "kr" | "sg" => "other-asia",
        "au" | "nz" => "australia",
        other => other,
    }
    .to_owned()
}

/// Central banks whose decisions are `policy_major` (Bureau §4.2); the others are `policy_other`.
const MAJOR_BANKS: &[&str] = &[
    "cb-fed", "cb-ecb", "cb-boe", "cb-boj", "cb-rbi", "cb-pboc", "cb-rba", "cb-boc",
];
/// Institutions whose reports are `report_flagship`; other reports are `macro_other`.
const FLAGSHIP_REPORTERS: &[&str] = &["imf", "oecd", "worldbank", "bis"];
/// Headline macro: these families in these countries (event key `macro:<country>.<family>:<period>`).
const HEADLINE_COUNTRIES: &[&str] = &["us", "ea", "uk", "cn", "in", "jp", "au", "ca"];
const HEADLINE_FAMILIES: &[&str] = &[
    "cpi",
    "cpih",
    "hicp",
    "cpi_yoy",
    "gdp",
    "payrolls",
    "unemployment",
];

fn macro_event(best: &Member) -> &'static str {
    let headline = best
        .event_key
        .as_deref()
        .and_then(|k| k.strip_prefix("macro:")?.split(':').next()?.split_once('.'))
        .is_some_and(|(country, family)| {
            HEADLINE_COUNTRIES.contains(&country) && HEADLINE_FAMILIES.contains(&family)
        });
    if headline {
        "macro_headline"
    } else {
        "macro_other"
    }
}

/// The page type with the data its magnitude formula needs, and Nose's event type for it. `data` is
/// the structured types' page data ([`structured::load`]); `None` when it is not available.
fn page(best: &Member, members: &[Member], data: Option<Page>) -> Option<(&'static str, Page)> {
    match best.page_type.as_deref()? {
        "macro" => Some((macro_event(best), data?)),
        "policy" => Some((
            if MAJOR_BANKS.contains(&best.source_id.as_str()) {
                "policy_major"
            } else {
                "policy_other"
            },
            data?,
        )),
        "earnings" => Some(("company_results", data?)),
        "report" => Some((
            if FLAGSHIP_REPORTERS.contains(&best.source_id.as_str()) {
                "report_flagship"
            } else {
                "macro_other"
            },
            data?,
        )),
        "news" => {
            let mut primary: Vec<&str> = members
                .iter()
                .filter(|m| m.tier == 1)
                .map(|m| m.source_id.as_str())
                .collect();
            primary.sort_unstable();
            primary.dedup();
            Some((
                "news_story",
                Page::News {
                    primary_sources: u32::try_from(primary.len()).unwrap_or(u32::MAX),
                },
            ))
        }
        "paper" => Some((
            if best.source_id == "arxiv" {
                "preprint"
            } else {
                "paper_peer_reviewed"
            },
            Page::Science {
                open_dataset: false,
                peer_reviewed_preprint: false,
            },
        )),
        "science" => Some((
            "agency_science",
            Page::Science {
                open_dataset: false,
                peer_reviewed_preprint: false,
            },
        )),
        _ => None,
    }
}

/// Union of the members' values in member order (best member first), without repeats, at most three.
fn union(best: &Member, members: &[Member], get: impl Fn(&Member) -> Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for m in std::iter::once(best).chain(members.iter().filter(|m| m.cand_id != best.cand_id)) {
        for v in get(m) {
            if !out.contains(&v) {
                out.push(v);
            }
        }
    }
    out.truncate(3);
    out
}

/// Build a story's input. `entities` are `(entity_id, prominence)` for every member entity. `update`
/// is true when the story already has a published page (Taster step 5); `novelty` is what the story
/// had when it was created, replaced by `FollowUp` for an update. `None` when the story cannot be
/// scored by code: no members, no category, or a structured page type whose `data` (the page data of
/// [`structured::load`]) is not available.
pub fn build(
    members: &[Member],
    entities: &[(i64, i16)],
    update: bool,
    novelty: Novelty,
    data: Option<Page>,
) -> Option<StoryInput> {
    let best = best(members)?;
    let (event_type, page) = page(best, members, data)?;
    let categories = union(best, members, |m| m.categories.clone());
    if categories.is_empty() {
        return None;
    }
    let mut regions = union(best, members, |m| {
        m.region.iter().map(|r| region_bucket(r)).collect()
    });
    if regions.is_empty() {
        regions.push("global".to_owned());
    }
    let entity = |id: i64| Entity {
        id: id.to_string(),
        prominence: entities
            .iter()
            .find(|(e, _)| *e == id)
            .map_or(3, |(_, p)| u8::try_from(*p).unwrap_or(3)),
    };
    // The best member's first entity is the main one; the rest, in id order, are the others.
    let main = best.entity_ids.first().copied().or_else(|| {
        members
            .iter()
            .flat_map(|m| m.entity_ids.first().copied())
            .min()
    });
    let mut others: Vec<i64> = members
        .iter()
        .flat_map(|m| m.entity_ids.iter().copied())
        .filter(|e| Some(*e) != main)
        .collect();
    others.sort_unstable();
    others.dedup();
    Some(StoryInput {
        event_type: event_type.to_owned(),
        series: matches!(&page, Page::Macro { values, .. } if values.len() >= 12),
        page,
        categories,
        regions,
        main_entity: main.map(entity),
        entities: others.into_iter().map(entity).collect(),
        source_tier: u8::try_from(best.tier).unwrap_or(4),
        published_at: best.published_at.unwrap_or(best.first_seen),
        data_points: u32::try_from(members.iter().map(|m| m.dp_count.max(0)).sum::<i32>())
            .unwrap_or(0),
        novelty: if update { Novelty::FollowUp } else { novelty },
        redistribution: best.redistribution,
        parse_warnings: false,
        action: if update { Action::Update } else { Action::New },
    })
}

/// Rebuild and store a story's input. `initial` is the novelty of a story with no input yet; an
/// existing input keeps its own, so a re-run changes nothing. A story that cannot be scored gets a
/// null input. Returns whether the story is scoreable.
pub async fn refresh(
    tx: &mut PgConnection,
    story_id: i64,
    initial: Novelty,
) -> Result<bool, sqlx::Error> {
    let rows = sqlx::query(
        "select c.cand_id, c.source_id, s.source_tier, s.redistribution, c.page_type, c.categories,
                c.region, c.published_at, c.first_seen, c.dp_count, c.dp_ids, c.event_key, c.entity_ids
           from candidates c join sources s using (source_id)
          where c.story_id = $1 order by c.cand_id",
    )
    .bind(story_id)
    .fetch_all(&mut *tx)
    .await?;
    let members = rows
        .iter()
        .map(|r| {
            Ok(Member {
                cand_id: r.try_get("cand_id")?,
                source_id: r.try_get("source_id")?,
                tier: r.try_get("source_tier")?,
                redistribution: r.try_get("redistribution")?,
                page_type: r.try_get("page_type")?,
                categories: r
                    .try_get::<Option<Vec<String>>, _>("categories")?
                    .unwrap_or_default(),
                region: r.try_get("region")?,
                published_at: r.try_get("published_at")?,
                first_seen: r.try_get("first_seen")?,
                dp_count: r.try_get("dp_count")?,
                dp_ids: r
                    .try_get::<Option<Vec<String>>, _>("dp_ids")?
                    .unwrap_or_default(),
                event_key: r.try_get("event_key")?,
                entity_ids: r.try_get("entity_ids")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let ids: Vec<i64> = members
        .iter()
        .flat_map(|m| m.entity_ids.iter().copied())
        .collect();
    let entities: Vec<(i64, i16)> =
        sqlx::query_as("select entity_id, prominence from entities where entity_id = any($1)")
            .bind(&ids)
            .fetch_all(&mut *tx)
            .await?;
    let (update, existing): (bool, Option<serde_json::Value>) = sqlx::query_as(
        "select exists (select 1 from pages where story_id = $1 and status = 'published'), score_input
           from stories where story_id = $1",
    )
    .bind(story_id)
    .fetch_one(&mut *tx)
    .await?;
    let novelty = existing
        .and_then(|v| serde_json::from_value::<StoryInput>(v).ok())
        .map_or(initial, |i| i.novelty);
    let data = match best(&members) {
        Some(b) => structured::load(tx, b, &members).await?,
        None => None,
    };
    let input = build(&members, &entities, update, novelty, data);
    sqlx::query("update stories set score_input = $2 where story_id = $1 and score_input is distinct from $2")
        .bind(story_id)
        .bind(input.as_ref().map(sqlx::types::Json))
        .execute(&mut *tx)
        .await?;
    Ok(input.is_some())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use news_nose::config;

    fn member(id: i64, source: &str, tier: i16, page_type: &str) -> Member {
        Member {
            cand_id: id,
            source_id: source.into(),
            tier,
            redistribution: true,
            page_type: Some(page_type.into()),
            categories: vec!["economy".into(), "inflation".into()],
            region: Some("us".into()),
            published_at: None,
            first_seen: "2026-11-20T10:00:00Z".parse().unwrap(),
            dp_count: 0,
            dp_ids: vec![],
            event_key: None,
            entity_ids: vec![],
        }
    }

    #[test]
    fn a_news_story_is_built_from_its_members() {
        let mut a = member(5, "wire-b", 2, "news");
        a.entity_ids = vec![9, 7];
        let mut b = member(6, "fed", 1, "news");
        b.categories = vec!["finance".into(), "economy".into()];
        b.region = Some("uk".into());
        b.entity_ids = vec![8];
        b.published_at = Some("2026-11-20T09:00:00Z".parse().unwrap());
        let i = build(&[a, b], &[(8, 1), (9, 2)], false, Novelty::NewEvent, None).unwrap();
        assert_eq!(i.event_type, "news_story");
        assert_eq!(i.page, Page::News { primary_sources: 1 });
        // The Fed member is the best (tier 1): its category and region lead.
        assert_eq!(i.categories, ["finance", "economy", "inflation"]);
        assert_eq!(i.regions, ["europe", "north-america"]);
        let main = i.main_entity.unwrap();
        assert_eq!((main.id.as_str(), main.prominence), ("8", 1));
        assert_eq!(
            i.entities
                .iter()
                .map(|e| (e.id.as_str(), e.prominence))
                .collect::<Vec<_>>(),
            [("7", 3), ("9", 2)]
        );
        assert_eq!((i.source_tier, i.action), (1, Action::New));
        assert_eq!(i.published_at.to_rfc3339(), "2026-11-20T09:00:00+00:00");
    }

    #[test]
    fn an_update_is_a_follow_up_and_structured_pages_wait_for_their_data() {
        let a = [member(1, "arxiv", 1, "paper")];
        let i = build(&a, &[], true, Novelty::NewEvent, None).unwrap();
        assert_eq!(
            (i.action, i.novelty.clone()),
            (Action::Update, Novelty::FollowUp)
        );
        assert_eq!(i.event_type, "preprint");
        let macro_member = [member(1, "bls", 1, "macro")];
        assert!(build(&macro_member, &[], false, Novelty::NewEvent, None).is_none());
        let mut bare = member(1, "x", 2, "news");
        bare.categories.clear();
        assert!(build(&[bare], &[], false, Novelty::NewEvent, None).is_none());
        assert!(build(&[], &[], false, Novelty::NewEvent, None).is_none());
    }

    #[test]
    fn structured_pages_take_their_data_and_event_type() {
        let values: Vec<f64> = (0..12).map(f64::from).collect();
        let mut cpi = member(1, "bls", 1, "macro");
        cpi.event_key = Some("macro:us.cpi:2026-10".into());
        let i = build(
            std::slice::from_ref(&cpi),
            &[],
            false,
            Novelty::NewEvent,
            Some(Page::Macro {
                values,
                per_year: 12,
            }),
        )
        .unwrap();
        // Twelve observations are a series (D); the headline family gets the headline weight.
        assert_eq!((i.event_type.as_str(), i.series), ("macro_headline", true));
        let mut other = member(1, "worldbank", 1, "macro");
        other.event_key = Some("macro:us.gdp_wb:2025".into());
        let page = Page::Macro {
            values: vec![1.0, 2.0],
            per_year: 1,
        };
        let i = build(&[other], &[], false, Novelty::NewEvent, Some(page)).unwrap();
        assert_eq!((i.event_type.as_str(), i.series), ("macro_other", false));
        let policy = Page::Policy {
            change_bp: 25,
            statement_change: 0.0,
            holds_before: 0,
            reversal: false,
        };
        let bank = |id: &str| member(2, id, 1, "policy");
        let major = build(
            &[bank("cb-ecb")],
            &[],
            false,
            Novelty::NewEvent,
            Some(policy.clone()),
        )
        .unwrap();
        assert_eq!(major.event_type, "policy_major");
        let minor = build(
            &[bank("cb-xyz")],
            &[],
            false,
            Novelty::NewEvent,
            Some(policy),
        )
        .unwrap();
        assert_eq!(minor.event_type, "policy_other");
        let earn = Page::Earnings {
            growth: 0.1,
            past_growth: vec![],
            flip: false,
        };
        let sec = [member(3, "sec-edgar", 1, "earnings")];
        let i = build(&sec, &[], false, Novelty::NewEvent, Some(earn)).unwrap();
        assert_eq!(i.event_type, "company_results");
        let rep = Page::Report {
            revision: None,
            past_revisions: vec![],
        };
        let i = build(
            &[member(4, "imf", 1, "report")],
            &[],
            false,
            Novelty::NewEvent,
            Some(rep),
        )
        .unwrap();
        assert_eq!(i.event_type, "report_flagship");
        // A structured member with no data yields no input.
        assert!(build(&sec, &[], false, Novelty::NewEvent, None).is_none());
    }

    #[test]
    fn the_input_round_trips_through_nose_and_uses_known_buckets() {
        let i = build(
            &[member(1, "x", 2, "news")],
            &[],
            false,
            Novelty::Similar { max_cos: 0.4 },
            None,
        )
        .unwrap();
        let json = serde_json::to_value(&i).unwrap();
        assert_eq!(serde_json::from_value::<StoryInput>(json).unwrap(), i);
        let cfg = config::v1().unwrap();
        for region in ["us", "ca", "eu", "uk", "in", "cn", "jp", "au", "global"] {
            let b = region_bucket(region);
            assert!(
                cfg.coverage_targets.region.contains_key(&b),
                "{region} -> {b}"
            );
        }
        for t in [
            "news_story",
            "preprint",
            "paper_peer_reviewed",
            "agency_science",
            "macro_headline",
            "macro_other",
            "policy_major",
            "policy_other",
            "company_results",
            "report_flagship",
        ] {
            assert!(cfg.event_weights.contains_key(t), "{t}");
        }
    }
}
