//! Unit tests: ranking and diversity, every check rule, the draft and body, and the retry classes. The
//! HTTP client and the preview push are in `tests_http.rs`; the PostgreSQL round trips (save, check,
//! show, publish) are in `crates/news/tests/postgres/home.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::api::{ApiError, CurationReply, Retry, WriteApi, idempotency_key, send};
use crate::candidates::{Input, TOP, home_score, rank, visual_richness};
use crate::check::{self, RULES};
use crate::draft::{Draft, body, schema_failures};
use crate::facts::PageFacts;
use crate::ops;
use chrono::{DateTime, TimeDelta, Utc};
use news_core::ErrorCode;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

fn now() -> DateTime<Utc> {
    "2026-11-23T09:00:00Z".parse().unwrap()
}

fn page(id: &str, cat: &str, region: &str) -> PageFacts {
    PageFacts {
        page_id: id.into(),
        status: "published".into(),
        title: Some(format!("Title of {id}")),
        tier: Some(2),
        page_type: Some("macro".into()),
        published_at: Some(now() - TimeDelta::days(1)),
        categories: vec![cat.into()],
        region: Some(region.into()),
        entities: vec![format!("e-{id}")],
        figures: vec!["dp_x".into()],
        charts: vec!["c1".into()],
        input: None,
    }
}

// --- candidates ------------------------------------------------------------------------------

fn input(id: &str, cat: &str, s_now: f64) -> Input {
    Input {
        page: page(id, cat, "global"),
        s_now,
        views: 0,
    }
}

fn ids(items: &[Value]) -> Vec<&str> {
    items
        .iter()
        .map(|i| i["page_id"].as_str().unwrap())
        .collect()
}

#[test]
fn home_score_is_the_design_formula() {
    // 0.45·80 + 20 + 0.20·50 + 0.15·40 = 36 + 20 + 10 + 6
    assert!((home_score(80.0, Some(1), 50.0, 40) - 72.0).abs() < 1e-9);
    assert!((home_score(80.0, Some(2), 0.0, 0) - 36.0).abs() < 1e-9);
}

#[test]
fn visual_richness_counts_figures_and_charts_up_to_100() {
    let mut p = page("a", "economy", "global");
    p.figures = vec!["dp_a".into(), "dp_b".into(), "dp_c".into()];
    p.charts = vec!["c1".into()];
    assert_eq!(visual_richness(&p), 90);
    p.figures = (0..6).map(|n| format!("dp_{n}")).collect();
    assert_eq!(visual_richness(&p), 100);
    p.figures.clear();
    p.charts.clear();
    assert_eq!(visual_richness(&p), 0);
}

#[test]
fn candidates_are_ranked_by_home_score_and_traffic_is_a_percentile() {
    let mut low = input("low", "economy", 50.0);
    let mut mid = input("mid", "tech", 50.0);
    let mut high = input("high", "science", 50.0);
    low.views = 0;
    mid.views = 10;
    high.views = 20;
    let items = rank(&[low, mid, high], now());
    // Same S, tier and figures: traffic decides the order.
    assert_eq!(ids(&items), ["high", "mid", "low"]);
    assert_eq!(items[0]["traffic_pct"], 100.0);
    assert_eq!(items[1]["traffic_pct"], 50.0);
    assert_eq!(items[2]["traffic_pct"], 0.0);
    // 0.45·50 + 0.20·100 + 0.15·20 (one figure, one chart: 20 + 30 = 50 → 7.5): 22.5 + 20 + 7.5
    assert_eq!(items[0]["home_score"], 50.0);
    assert_eq!(items[0]["rank"], 1);
}

#[test]
fn tier_1_earns_twenty_points() {
    let mut t1 = input("t1", "economy", 50.0);
    t1.page.tier = Some(1);
    let t2 = input("t2", "tech", 60.0);
    let items = rank(&[t2, t1], now());
    assert_eq!(ids(&items), ["t1", "t2"]);
}

#[test]
fn the_list_is_forty_pages_and_no_category_fills_it() {
    let mut all = Vec::new();
    for n in 0..30 {
        all.push(input(
            &format!("eco-{n}"),
            "economy",
            90.0 - f64::from(n) * 0.1,
        ));
    }
    for n in 0..20 {
        all.push(input(
            &format!("tech-{n}"),
            "tech",
            60.0 - f64::from(n) * 0.1,
        ));
    }
    for n in 0..20 {
        all.push(input(
            &format!("sci-{n}"),
            "science",
            40.0 - f64::from(n) * 0.1,
        ));
    }
    let items = rank(&all, now());
    assert_eq!(items.len(), TOP as usize);
    let count = |c: &str| items.iter().filter(|i| i["bucket"] == c).count();
    // The category cap for a target of 40 is 14, whatever the scores.
    assert_eq!(count("economy"), 14);
    assert_eq!(count("tech"), 14);
    assert_eq!(count("science"), 12);
    assert_eq!(items[0]["page_id"], "eco-0");
    let ranks: Vec<u64> = items.iter().map(|i| i["rank"].as_u64().unwrap()).collect();
    assert_eq!(ranks, (1..=40).collect::<Vec<_>>());
}

#[test]
fn one_entity_is_at_most_two_candidates() {
    let mut all: Vec<Input> = (0..5)
        .map(|n| {
            let mut i = input(&format!("apple-{n}"), "tech", 90.0);
            i.page.entities = vec!["company:apple".into()];
            i
        })
        .collect();
    all.push(input("other", "economy", 10.0));
    let items = rank(&all, now());
    assert_eq!(
        items
            .iter()
            .filter(|i| i["page_id"].as_str().unwrap().starts_with("apple"))
            .count(),
        2
    );
    assert!(ids(&items).contains(&"other"));
}

// --- check rules -------------------------------------------------------------------------------

/// A draft that passes: 12 pages over 5 categories and 5 regions, one entity each.
fn good() -> (Draft, BTreeMap<String, PageFacts>) {
    let layout = [
        ("lead", "economy", "north-america"),
        ("s1", "tech", "europe"),
        ("s2", "science", "india"),
        ("a1", "economy/inflation", "china"),
        ("a2", "economy", "global"),
        ("a3", "economy", "europe"),
        ("b1", "finance", "north-america"),
        ("b2", "finance", "europe"),
        ("b3", "finance", "india"),
        ("c1", "tech", "china"),
        ("c2", "tech", "global"),
        ("c3", "business", "north-america"),
    ];
    let pages = layout
        .iter()
        .map(|(id, c, r)| ((*id).to_owned(), page(id, c, r)))
        .collect();
    let slot = |id: &str| json!({"page_id": id});
    let draft = serde_json::from_value(json!({
        "curator_model": "claude-sonnet-5-5",
        "slots": {
            "lead": slot("lead"),
            "side": [slot("s1"), slot("s2")],
            "sections": [
                {"category": "economy/inflation", "pages": [slot("a1"), slot("a2"), slot("a3")]},
                {"category": "finance", "pages": [slot("b1"), slot("b2"), slot("b3")]},
                {"category": "tech", "pages": [slot("c1"), slot("c2"), slot("c3")]},
            ],
        },
        "rationale": {"lead": "the biggest story"},
    }))
    .unwrap();
    (draft, pages)
}

fn codes(d: &Draft, pages: &BTreeMap<String, PageFacts>) -> Vec<String> {
    check::run(d, pages, now())
        .failures
        .into_iter()
        .map(|f| f.code)
        .collect()
}

#[test]
fn a_good_draft_passes_every_rule() {
    let (d, pages) = good();
    let r = check::run(&d, &pages, now());
    assert!(r.pass, "{:?}", r.failures);
    assert_eq!(r.results.len(), RULES.len());
    assert!(r.results.iter().all(|x| x.ok && x.detail.is_none()));
}

type Case = (
    &'static str,
    &'static str,
    fn(&mut Draft, &mut BTreeMap<String, PageFacts>),
);

#[test]
fn every_rule_fails_on_its_fixture() {
    let cases: &[Case] = &[
        ("a page that does not exist", "page_missing", |_, p| {
            p.remove("a1");
        }),
        ("a page still in draft", "page_unpublished", |_, p| {
            p.get_mut("a2").unwrap().status = "ready".into();
        }),
        ("a withdrawn page", "page_withdrawn", |_, p| {
            p.get_mut("b1").unwrap().status = "withdrawn".into();
        }),
        ("a page used twice", "duplicate_page", |d, _| {
            d.slots.sections[0].pages[1].page_id = "a1".into();
        }),
        (
            "a figure from another page",
            "figure_not_on_page",
            |d, _| {
                d.slots.lead.figure_id = Some("dp_other".into());
            },
        ),
        ("a chart from another page", "chart_not_on_page", |d, _| {
            d.slots.side[0].chart_id = Some("c9".into());
        }),
        ("two categories only", "breadth_categories", |_, p| {
            for (n, page) in p.values_mut().enumerate() {
                page.categories = vec![if n % 2 == 0 { "economy" } else { "tech" }.into()];
            }
        }),
        ("two regions only", "breadth_regions", |_, p| {
            for (n, page) in p.values_mut().enumerate() {
                page.region = Some(if n % 2 == 0 { "europe" } else { "india" }.into());
            }
        }),
        (
            "three stories about one entity",
            "breadth_entity",
            |_, p| {
                for id in ["a1", "a2", "a3"] {
                    p.get_mut(id).unwrap().entities = vec!["company:apple".into()];
                }
            },
        ),
        ("every section is finance", "breadth_non_finance", |d, _| {
            for s in &mut d.slots.sections {
                s.category = "finance/markets".into();
            }
        }),
        ("a lead older than 7 days", "lead_stale", |_, p| {
            p.get_mut("lead").unwrap().published_at = Some(now() - TimeDelta::days(8));
        }),
        ("still_leading on a Tier 2 page", "lead_stale", |d, p| {
            d.slots.lead.still_leading = Some(true);
            p.get_mut("lead").unwrap().tier = Some(2);
        }),
        ("three side stories", "schema.invalid", |d, _| {
            let extra = d.slots.side[0].clone();
            d.slots.side.push(extra);
        }),
        (
            "a figure and a chart in one slot",
            "schema.invalid",
            |d, _| {
                d.slots.lead.figure_id = Some("dp_x".into());
                d.slots.lead.chart_id = Some("c1".into());
            },
        ),
        ("a page ID that is not an ID", "schema.invalid", |d, _| {
            d.slots.lead.page_id = "2026/11/Lead".into();
        }),
    ];
    for (name, code, change) in cases {
        let (mut d, mut pages) = good();
        change(&mut d, &mut pages);
        let report = check::run(&d, &pages, now());
        assert!(!report.pass, "{name}");
        assert!(
            report.failures.iter().any(|f| f.code == *code),
            "{name}: expected {code}, got {:?}",
            codes(&d, &pages)
        );
        let rule = code.split('.').next().unwrap();
        let result = report.results.iter().find(|r| r.rule == rule).unwrap();
        assert!(!result.ok && result.detail.is_some(), "{name}");
    }
}

#[test]
fn a_figure_or_chart_of_the_page_and_an_excused_lead_pass() {
    let (mut d, mut pages) = good();
    d.slots.lead.figure_id = Some("dp_x".into());
    d.slots.side[0].chart_id = Some("c1".into());
    assert!(check::run(&d, &pages, now()).pass);
    // A Tier 1 lead older than 7 days passes only when marked still leading.
    let lead = pages.get_mut("lead").unwrap();
    lead.published_at = Some(now() - TimeDelta::days(10));
    lead.tier = Some(1);
    assert_eq!(codes(&d, &pages), ["lead_stale"]);
    d.slots.lead.still_leading = Some(true);
    assert!(check::run(&d, &pages, now()).pass);
}

#[test]
fn a_page_used_twice_counts_once_in_the_breadth_rules() {
    let (mut d, pages) = good();
    d.slots.sections[0].pages[1].page_id = "a1".into();
    let c = codes(&d, &pages);
    assert_eq!(c, ["duplicate_page"], "{c:?}");
}

// --- the draft and the body ----------------------------------------------------------------------

#[test]
fn a_draft_file_round_trips_and_refuses_what_it_does_not_know() {
    let (d, _) = good();
    let v = serde_json::to_value(&d).unwrap();
    assert_eq!(ops::parse_draft(v.clone()).unwrap(), d);
    let mut extra = v.clone();
    extra["slots"]["lead"]["rationale"] = json!("no");
    assert_eq!(
        ops::parse_draft(extra).unwrap_err().code,
        ErrorCode::ValidationFailed
    );
    let mut no_model = v;
    no_model["curator_model"] = json!(" ");
    assert_eq!(
        ops::parse_draft(no_model).unwrap_err().code,
        ErrorCode::ValidationFailed
    );
    assert!(ops::parse_id("12").is_ok());
    assert_eq!(ops::parse_id("x").unwrap_err().code, ErrorCode::Usage);
}

#[test]
fn the_body_is_curation_v1_without_the_rationale_and_is_stable() {
    let (d, pages) = good();
    let r = check::run(&d, &pages, now());
    let b = body(&d, now(), now() + TimeDelta::hours(1), &r.results);
    assert!(schema_failures(&b).is_empty(), "{:?}", schema_failures(&b));
    assert!(b.get("rationale").is_none() && !b.to_string().contains("biggest story"));
    assert_eq!(b["approved_at"], "2026-11-23T10:00:00.000Z");
    let a = serde_json::to_vec(&b).unwrap();
    let again =
        serde_json::to_vec(&body(&d, now(), now() + TimeDelta::hours(1), &r.results)).unwrap();
    assert_eq!(a, again);
    let key = idempotency_key(7, &a);
    assert!(key.starts_with("home-curation:7:") && (8..=255).contains(&key.len()));
    assert!(
        key.chars()
            .all(|c| c.is_ascii_alphanumeric() || ":._-".contains(c))
    );
    assert_ne!(key, idempotency_key(7, b"other"));
}

// --- the write API -----------------------------------------------------------------------------

struct Fake(Mutex<Vec<Result<CurationReply, ApiError>>>, Mutex<usize>);

impl Fake {
    fn new(mut answers: Vec<Result<CurationReply, ApiError>>) -> Self {
        answers.reverse();
        Self(Mutex::new(answers), Mutex::new(0))
    }
}

impl WriteApi for Fake {
    async fn put_curation(&self, _: &str, _: &str, _: Vec<u8>) -> Result<CurationReply, ApiError> {
        *self.1.lock().unwrap() += 1;
        self.0.lock().unwrap().pop().expect("an answer")
    }
    async fn post_rollback(&self, _: &str, _: &str, _: Vec<u8>) -> Result<CurationReply, ApiError> {
        *self.1.lock().unwrap() += 1;
        self.0.lock().unwrap().pop().expect("an answer")
    }
}

fn live(version: i32) -> CurationReply {
    CurationReply {
        version,
        live_version: version,
        purged: vec!["https://news.karthik.nz/".into()],
    }
}

const FAST: Retry = Retry {
    attempts: 3,
    pause: Duration::ZERO,
};

#[tokio::test]
async fn a_retryable_answer_is_sent_again_and_a_rejection_is_not() {
    let retry = || Err(ApiError::Retry("502 purge_failed".into()));
    let api = Fake::new(vec![retry(), retry(), Ok(live(5))]);
    assert_eq!(
        send(&api, "k", "secret-key", b"{}", FAST)
            .await
            .unwrap()
            .version,
        5
    );
    assert_eq!(*api.1.lock().unwrap(), 3);

    let api = Fake::new(vec![retry(), retry(), retry()]);
    let err = send(&api, "k", "secret-key", b"{}", FAST)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::UpstreamUnavailable);
    assert!(err.retryable && !err.message.contains("secret-key"));
    assert_eq!(*api.1.lock().unwrap(), 3);

    let invalid = ApiError::Rejected {
        status: 400,
        code: "invalid_curation".into(),
        message: "the curation is invalid".into(),
        details: json!([{"code": "page_withdrawn", "message": "x"}]),
    };
    let api = Fake::new(vec![Err(invalid)]);
    let err = send(&api, "k", "secret-key", b"{}", FAST)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::ValidationFailed);
    assert_eq!(err.details.unwrap()[0]["code"], "page_withdrawn");
    assert_eq!(*api.1.lock().unwrap(), 1);

    let denied = ApiError::Rejected {
        status: 401,
        code: "unauthorized".into(),
        message: "no".into(),
        details: Value::Null,
    };
    let err = send(&Fake::new(vec![Err(denied)]), "k", "b", b"{}", FAST)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::Precondition);
}

#[tokio::test]
async fn a_rollback_uses_the_rollback_route_and_maps_a_missing_version_to_not_found() {
    use crate::api::send_to;
    use crate::rollback::body;
    let api = Fake::new(vec![Ok(live(3))]);
    assert_eq!(
        send_to(&api, true, "k", "b", &body(3), FAST)
            .await
            .unwrap()
            .live_version,
        3
    );
    let missing = ApiError::Rejected {
        status: 404,
        code: "not_found".into(),
        message: "no such version".into(),
        details: Value::Null,
    };
    let err = send_to(
        &Fake::new(vec![Err(missing)]),
        true,
        "k",
        "b",
        &body(9),
        FAST,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
    assert_eq!(body(3), br#"{"version":3}"#);
}
