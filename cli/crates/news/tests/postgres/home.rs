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

    async fn post_rollback(
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

#[tokio::test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn rollback_makes_an_earlier_curation_live_and_names_only_known_versions() {
    use news_home::rollback::rollback;
    let f = Fixture::migrated().await;
    for (version, status) in [(1, "superseded"), (2, "published")] {
        sqlx::query(
            "insert into home_curations (status, version, slots, published_at)
             values ($1, $2, '{\"lead\": {\"page_id\": \"home-p0\"}, \"side\": [], \"sections\": []}'::jsonb, $3)",
        )
        .bind(status)
        .bind(version)
        .bind(now())
        .execute(&f.pool)
        .await
        .unwrap();
    }

    // An unknown version never reads the key or calls the API.
    let api = FakeApi::new(vec![]);
    let err = rollback(&f.pool, 9, &api, || panic!("the key was read"), FAST)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
    // The live version is already live: nothing to send.
    let out = rollback(&f.pool, 2, &api, || panic!("the key was read"), FAST)
        .await
        .unwrap();
    assert_eq!(out["already_live"], true);
    assert!(api.calls.lock().unwrap().is_empty());

    // Version 1 goes live: one POST with the version, the key named for it, and the rows swap.
    let api = FakeApi::new(vec![Err(ApiError::Retry("502".into())), live(1)]);
    let out = rollback(&f.pool, 1, &api, || Ok("the-key".into()), FAST)
        .await
        .unwrap();
    assert_eq!(out["live_version"], 1);
    let calls = api.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0], calls[1], "a resend is the same request");
    assert!(calls[0].0.starts_with("home-rollback:"));
    assert_eq!(
        serde_json::from_slice::<Value>(&calls[0].2).unwrap(),
        json!({"version": 1})
    );
    let rows: Vec<(i32, String)> =
        sqlx::query_as("select version, status from home_curations order by version")
            .fetch_all(&f.pool)
            .await
            .unwrap();
    assert_eq!(
        rows,
        [(1, "published".to_owned()), (2, "superseded".to_owned())]
    );
    let shown = ops::show(&f.pool, now()).await.unwrap();
    assert_eq!(shown["version"], 1);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn home_rollback_is_accepted_only_from_board() {
    use news_hotline::{Bus, Peer, Service};
    use std::sync::Arc;
    let f = Fixture::migrated().await;
    let state = Arc::new(news_service::State::default());
    state.set(news_chief::board::ServiceState::Up);
    let bureau = Arc::new(news_service::handlers::Bureau::new(
        state,
        Arc::new(Bus::default()),
        Arc::new(news_service::log::LogRing::default()),
    ));
    bureau.set_core(Some(Arc::new(news_service::handlers::Core {
        db: f.pool.clone(),
        chief: news_chief::Chief::builder(f.pool.clone(), Arc::new(news_core::clock::SystemClock))
            .build(),
        policy: Arc::new(news_chief::AllowAll),
        stopper: Arc::new(news_chief::NoStopper),
    })));
    let call = |peer: Peer, args: Value| {
        let svc: Arc<dyn Service> = bureau.clone();
        async move {
            let (mut client, server) = tokio::io::duplex(1 << 20);
            let h = tokio::spawn(news_hotline::serve_conn(server, peer, svc));
            let env = news_hotline::call(&mut client, "t-1", "home.rollback", args)
                .await
                .unwrap();
            drop(client);
            let _ = h.await;
            env
        }
    };
    let terminal = Peer {
        argv1_is_board: false,
        ..Peer::board()
    };
    // A terminal, a script or an AI job is refused as `gated`, and the refusal is logged.
    let env = call(terminal, json!({"version": 1})).await;
    assert_eq!(env["error"]["code"], "gated", "{env}");
    assert_eq!(
        f.scalar_i64(
            "select count(*) from hotline_refusals where cmd = 'home.rollback' and refusal = 'not_board'"
        )
        .await,
        1
    );
    // Args are checked against the contract before the handler runs.
    let env = call(Peer::board(), json!({"version": 0})).await;
    assert_eq!(env["error"]["code"], "usage", "{env}");
    // Board's request reaches the handler: with no write-API origin set it stops there.
    let env = call(Peer::board(), json!({"version": 1})).await;
    assert_eq!(env["error"]["code"], "precondition", "{env}");
}

/// The write API's idempotency: a key it has seen gets the stored reply, whatever is asked now.
struct Replay {
    seen: Mutex<std::collections::HashMap<String, CurationReply>>,
    keys: Mutex<Vec<String>>,
}

impl Replay {
    fn new() -> Self {
        Self {
            seen: Mutex::new(Default::default()),
            keys: Mutex::new(Vec::new()),
        }
    }
}

impl WriteApi for Replay {
    async fn put_curation(&self, _: &str, _: &str, _: Vec<u8>) -> Result<CurationReply, ApiError> {
        unreachable!("rollback never publishes")
    }

    async fn post_rollback(
        &self,
        key: &str,
        _: &str,
        body: Vec<u8>,
    ) -> Result<CurationReply, ApiError> {
        self.keys.lock().unwrap().push(key.into());
        let asked = serde_json::from_slice::<Value>(&body).unwrap()["version"]
            .as_i64()
            .unwrap() as i32;
        Ok(self
            .seen
            .lock()
            .unwrap()
            .entry(key.into())
            .or_insert(CurationReply {
                version: 2,
                live_version: asked,
                purged: vec![],
            })
            .clone())
    }
}

async fn two_curations(f: &Fixture) {
    for (version, status) in [(1, "superseded"), (2, "published")] {
        sqlx::query(
            "insert into home_curations (status, version, slots, published_at)
             values ($1, $2, '{\"lead\": {\"page_id\": \"home-p0\"}, \"side\": [], \"sections\": []}'::jsonb, $3)",
        )
        .bind(status)
        .bind(version)
        .bind(now())
        .execute(&f.pool)
        .await
        .unwrap();
    }
}

async fn live_version(f: &Fixture) -> i32 {
    sqlx::query_scalar("select version from home_curations where status = 'published'")
        .fetch_one(&f.pool)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn v1_then_v2_then_v1_again_sends_a_new_key_each_time() {
    use news_home::rollback::rollback;
    let f = Fixture::migrated().await;
    two_curations(&f).await;
    let api = Replay::new();
    for (to, live_after) in [(1, 1), (2, 2), (1, 1)] {
        let out = rollback(&f.pool, to, &api, || Ok("the-key".into()), FAST)
            .await
            .unwrap();
        assert_eq!(out["live_version"], live_after);
        assert_eq!(
            live_version(&f).await,
            live_after,
            "the local record agrees"
        );
    }
    let keys = api.keys.lock().unwrap().clone();
    assert_eq!(keys.len(), 3);
    let distinct: std::collections::HashSet<_> = keys.iter().collect();
    assert_eq!(distinct.len(), 3, "a key names one attempt: {keys:?}");
    let open: i64 = sqlx::query_scalar("select count(*) from home_rollbacks where done_at is null")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(open, 0);
}

#[tokio::test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn a_crash_between_the_api_call_and_the_local_record_resends_the_same_key() {
    use news_home::rollback::{Begun, begin, rollback};
    let f = Fixture::migrated().await;
    two_curations(&f).await;
    let api = Replay::new();
    // The process dies after the API took the rollback and before `mark_live`.
    let Begun::Send { key, .. } = begin(&f.pool, 1).await.unwrap() else {
        panic!("v1 is not live");
    };
    api.post_rollback(&key, "b", br#"{"version":1}"#.to_vec())
        .await
        .unwrap();
    assert_eq!(live_version(&f).await, 2, "the local record is behind");
    // Board asks again: the same attempt, so the same key, and the stored reply finishes the record.
    let out = rollback(&f.pool, 1, &api, || Ok("the-key".into()), FAST)
        .await
        .unwrap();
    assert_eq!(out["live_version"], 1);
    assert_eq!(live_version(&f).await, 1);
    let keys = api.keys.lock().unwrap().clone();
    assert_eq!(keys, [key.clone(), key]);
    let rows: i64 =
        sqlx::query_scalar("select count(*) from home_rollbacks where done_at is not null")
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(rows, 1);
}

#[tokio::test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn a_reply_for_another_version_is_a_conflict_and_changes_nothing_locally() {
    use news_home::rollback::rollback;
    let f = Fixture::migrated().await;
    two_curations(&f).await;
    // A replayed reply from an earlier attempt: the API says v2 is live though v1 was asked for.
    let api = FakeApi::new(vec![live(2), live(1)]);
    let err = rollback(&f.pool, 1, &api, || Ok("k".into()), FAST)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::Conflict);
    assert_eq!(live_version(&f).await, 2);
    // The attempt is over: asking again is a new request with a new key.
    rollback(&f.pool, 1, &api, || Ok("k".into()), FAST)
        .await
        .unwrap();
    assert_eq!(live_version(&f).await, 1);
    let calls = api.calls.lock().unwrap().clone();
    assert_ne!(calls[0].0, calls[1].0);
}
