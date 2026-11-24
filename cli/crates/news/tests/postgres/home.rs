//! `news home` against a real database (#20): candidates from pages and stories, save, check and show
//! as a round trip, and `publish` against a fake write API (no network): one version however often it
//! is rerun, the same Idempotency-Key and bytes on a resend, and nothing sent, and the key never read,
//! for a draft that is not ready.

use super::fixture::Fixture;
use chrono::{DateTime, TimeDelta, Utc};
use news_core::ErrorCode;
use news_home::api::{ApiError, CurationReply, Retry, WriteApi, publish};
use news_home::{candidates, ops};
use serde_json::{Value, json};
use std::sync::Mutex;
use std::time::Duration;

fn now() -> DateTime<Utc> {
    "2026-11-23T09:00:00Z".parse().unwrap()
}

const CATS: [&str; 12] = [
    "economy", "tech", "science", "economy", "economy", "economy", "finance", "finance", "finance",
    "tech", "tech", "business",
];
const REGIONS: [&str; 12] = [
    "north-america",
    "europe",
    "india",
    "china",
    "global",
    "europe",
    "north-america",
    "europe",
    "india",
    "china",
    "global",
    "north-america",
];

fn score_input(cat: &str, region: &str, entity: usize) -> Value {
    json!({
        "event_type": "policy_major",
        "page": {"type": "policy", "change_bp": 0},
        "categories": [cat],
        "regions": [region],
        "main_entity": {"id": format!("e-{entity}"), "prominence": 2},
        "source_tier": 1,
        "published_at": now() - TimeDelta::hours(6),
        "data_points": 5,
        "series": false,
        "novelty": {"kind": "new_event"},
    })
}

/// Twelve published pages `home-p0`…`home-p11`, one story each, p0 with a figure `dp_p0_rate`.
async fn seed(f: &Fixture) {
    for k in 0..12 {
        let story_id: i64 = sqlx::query_scalar(
            "insert into stories (title, first_seen, last_seen, status, score_input)
             values ($1, $2, $2, 'covered', $3) returning story_id",
        )
        .bind(format!("Story {k}"))
        .bind(now() - TimeDelta::hours(6))
        .bind(score_input(CATS[k], REGIONS[k], k))
        .fetch_one(&f.pool)
        .await
        .unwrap();
        sqlx::query(
            "insert into pages (page_id, slug, story_id, page_type, tier, version, status, published_at)
             values ($1, $1, $2, 'policy', $3, 1, 'published', $4)",
        )
        .bind(format!("home-p{k}"))
        .bind(story_id)
        .bind(if k == 0 { 1_i16 } else { 2 })
        .bind(now() - TimeDelta::hours(6))
        .execute(&f.pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "insert into page_refs (page_id, version, ref) values ('home-p0', 1, 'dp_p0_rate')",
    )
    .execute(&f.pool)
    .await
    .unwrap();
}

fn draft() -> Value {
    let s = |k: usize| json!({"page_id": format!("home-p{k}")});
    json!({
        "curator_model": "claude-sonnet-5-5",
        "slots": {
            "lead": {"page_id": "home-p0", "figure_id": "dp_p0_rate"},
            "side": [s(1), s(2)],
            "sections": [
                {"category": "economy", "pages": [s(3), s(4), s(5)]},
                {"category": "finance", "pages": [s(6), s(7), s(8)]},
                {"category": "tech", "pages": [s(9), s(10), s(11)]},
            ],
        },
        "rationale": {"home-p0": "the day's story"},
    })
}

#[tokio::test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn candidates_rank_the_published_pages_with_their_figures() {
    let f = Fixture::migrated().await;
    seed(&f).await;
    let (data, warnings) = candidates::run(&f.pool, now()).await.unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(data["eligible"], 12);
    let items = data["items"].as_array().unwrap();
    assert_eq!(items.len(), 12);
    // The Tier 1 page leads and shows its one figure.
    assert_eq!(items[0]["page_id"], "home-p0");
    assert_eq!(items[0]["figures"], json!(["dp_p0_rate"]));
    assert_eq!(items[0]["visual_richness"], 20);
    assert_eq!(items[0]["region"], "north-america");
}

#[tokio::test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn save_check_and_show_round_trip() {
    let f = Fixture::migrated().await;
    seed(&f).await;
    let saved = ops::save(&f.pool, draft()).await.unwrap();
    let id = saved["draft_id"].as_i64().unwrap();
    assert_eq!(saved["created"], true);
    // Saved again while open: the same draft.
    let again = ops::save(&f.pool, draft()).await.unwrap();
    assert_eq!(
        (
            again["draft_id"].as_i64().unwrap(),
            again["created"].clone()
        ),
        (id, json!(false))
    );

    let checked = ops::check(&f.pool, id, now()).await.unwrap();
    assert_eq!(checked["pass"], true, "{checked}");
    assert_eq!(checked["status"], "checked");
    // A page withdrawn since fails the next check and sends the draft back.
    sqlx::query("update pages set status = 'withdrawn' where page_id = 'home-p4'")
        .execute(&f.pool)
        .await
        .unwrap();
    let failed = ops::check(&f.pool, id, now()).await.unwrap();
    assert_eq!(failed["pass"], false);
    assert_eq!(failed["status"], "draft");
    assert_eq!(failed["failures"][0]["code"], "page_withdrawn");

    // Nothing is live until a publish.
    assert_eq!(
        ops::show(&f.pool, now()).await.unwrap_err().code,
        ErrorCode::NotFound
    );
    // The preview refuses a draft that has not passed.
    let st = news_home::store::get(&f.pool, id).await.unwrap();
    assert_eq!(
        ops::preview_body(&st).unwrap_err().code,
        ErrorCode::Precondition
    );
}

/// A write API that answers from a list and records every call's key, bearer and bytes.
struct FakeApi {
    answers: Mutex<Vec<Result<CurationReply, ApiError>>>,
    calls: Mutex<Vec<(String, String, Vec<u8>)>>,
}

impl FakeApi {
    fn new(mut answers: Vec<Result<CurationReply, ApiError>>) -> Self {
        answers.reverse();
        Self {
            answers: Mutex::new(answers),
            calls: Mutex::new(Vec::new()),
        }
    }
}

impl WriteApi for FakeApi {
    async fn put_curation(
        &self,
        key: &str,
        bearer: &str,
        body: Vec<u8>,
    ) -> Result<CurationReply, ApiError> {
        self.calls
            .lock()
            .unwrap()
            .push((key.into(), bearer.into(), body));
        self.answers.lock().unwrap().pop().expect("an answer")
    }
}

fn live(version: i32) -> Result<CurationReply, ApiError> {
    Ok(CurationReply {
        version,
        live_version: version,
        purged: vec!["https://news.karthik.nz/".into()],
    })
}

const FAST: Retry = Retry {
    attempts: 2,
    pause: Duration::ZERO,
};

#[tokio::test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn publish_sends_once_reuses_its_key_and_marks_the_draft_published() {
    let f = Fixture::migrated().await;
    seed(&f).await;
    let id = ops::save(&f.pool, draft()).await.unwrap()["draft_id"]
        .as_i64()
        .unwrap();

    // Not checked yet: nothing is sent and the key is never read.
    let api = FakeApi::new(vec![]);
    let err = publish(
        &f.pool,
        id,
        now(),
        &api,
        || panic!("the key was read"),
        FAST,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::Precondition);
    assert!(api.calls.lock().unwrap().is_empty());

    assert_eq!(ops::check(&f.pool, id, now()).await.unwrap()["pass"], true);
    // The first send fails after the API committed (a failed purge): the resend carries the same key
    // and bytes, and the API answers with one version.
    let api = FakeApi::new(vec![
        Err(ApiError::Retry("502 purge_failed".into())),
        live(1),
    ]);
    let out = publish(&f.pool, id, now(), &api, || Ok("the-key".into()), FAST)
        .await
        .unwrap();
    assert_eq!(out["version"], 1);
    let calls = api.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0], calls[1]);
    assert!(calls[0].0.starts_with(&format!("home-curation:{id}:")));
    assert_eq!(calls[0].1, "the-key");
    let sent: Value = serde_json::from_slice(&calls[0].2).unwrap();
    assert_eq!(sent["schema"], "curation.v1");
    assert_eq!(sent["checks"]["passed"], true);
    assert!(sent.get("rationale").is_none());
    drop(calls);

    // Published: `show` has it, and a rerun sends nothing.
    let shown = ops::show(&f.pool, now() + TimeDelta::hours(12))
        .await
        .unwrap();
    assert_eq!(shown["version"], 1);
    assert_eq!(shown["age_days"], 0.5);
    let rerun = FakeApi::new(vec![]);
    let again = publish(
        &f.pool,
        id,
        now(),
        &rerun,
        || panic!("the key was read"),
        FAST,
    )
    .await
    .unwrap();
    assert_eq!(again["already_published"], true);
    assert!(rerun.calls.lock().unwrap().is_empty());
}

#[tokio::test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn a_crash_after_approval_resends_the_same_bytes_and_a_withdrawn_page_stops_a_first_send() {
    let f = Fixture::migrated().await;
    seed(&f).await;
    let id = ops::save(&f.pool, draft()).await.unwrap()["draft_id"]
        .as_i64()
        .unwrap();
    ops::check(&f.pool, id, now()).await.unwrap();

    // The first run dies after the approval is fixed (the API gave only retryable errors).
    let api = FakeApi::new(vec![
        Err(ApiError::Retry("503".into())),
        Err(ApiError::Retry("503".into())),
    ]);
    let err = publish(&f.pool, id, now(), &api, || Ok("k".into()), FAST)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::UpstreamUnavailable);
    // Hours later, a page has been withdrawn, but the approved bytes are the same and are resent: the
    // server answers a retry of its own request before it validates the draft again (ADR 0005).
    sqlx::query("update pages set status = 'withdrawn' where page_id = 'home-p4'")
        .execute(&f.pool)
        .await
        .unwrap();
    let later = FakeApi::new(vec![live(3)]);
    let out = publish(
        &f.pool,
        id,
        now() + TimeDelta::hours(5),
        &later,
        || Ok("k".into()),
        FAST,
    )
    .await
    .unwrap();
    assert_eq!(out["version"], 3);
    assert_eq!(
        api.calls.lock().unwrap()[0].2,
        later.calls.lock().unwrap()[0].2
    );
    assert_eq!(
        api.calls.lock().unwrap()[0].0,
        later.calls.lock().unwrap()[0].0
    );

    // A draft that was never sent is checked again first: a withdrawn page stops it before any send.
    let mut other = draft();
    other["curator_model"] = json!("another-model");
    let id2 = ops::save(&f.pool, other).await.unwrap()["draft_id"]
        .as_i64()
        .unwrap();
    sqlx::query("update pages set status = 'published' where page_id = 'home-p4'")
        .execute(&f.pool)
        .await
        .unwrap();
    ops::check(&f.pool, id2, now()).await.unwrap();
    sqlx::query("update pages set status = 'withdrawn' where page_id = 'home-p5'")
        .execute(&f.pool)
        .await
        .unwrap();
    let none = FakeApi::new(vec![]);
    let err = publish(
        &f.pool,
        id2,
        now(),
        &none,
        || panic!("the key was read"),
        FAST,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::ValidationFailed);
    assert!(none.calls.lock().unwrap().is_empty());
}
