//! Press against a real database and the scripted fake write API (no network): staging inside the
//! G-tier check, the drain, resends under the same key, dead rows, and the index budget.

use super::fixture::Fixture;
use chrono::{DateTime, TimeZone, Utc};
use news_chief::driver::{Handlers, StepCx, StepError, StepHandler, StepOutcome};
use news_chief::gates::{self, GRequest};
use news_chief::presets::GCommand;
use news_chief::{BoxFut, Chief, IfBusy, StartReq};
use news_core::ErrorCode;
use news_core::clock::ManualClock;
use news_press::api::{Client, page_key};
use news_press::fake::{FakeApi, Fault};
use news_press::{bundle, store};
use news_wire::pace::VirtualClock;
use serde_json::{Map, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 23, 9, 0, 0).unwrap()
}

fn work(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("news-press-it-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

const FILES: [(&str, &str); 2] = [("index.html", "<!doctype html>"), ("data.json", "{}")];

#[allow(clippy::too_many_arguments)]
/// A page row at version `v` with the bundle `sha`.
async fn page(
    f: &Fixture,
    id: &str,
    status: &str,
    index: &str,
    tier: i16,
    flags: &[&str],
    sha: &[u8],
    story: Option<i64>,
) {
    sqlx::query(
        "insert into pages (page_id, slug, story_id, tier, version, status, index_status, bundle_sha, flags, published_at)
         values ($1, $1, $2, $3, 1, $4, $5, $6, $7, now())",
    )
    .bind(id)
    .bind(story)
    .bind(tier)
    .bind(status)
    .bind(index)
    .bind(sha)
    .bind(flags.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>())
    .execute(&f.pool)
    .await
    .unwrap();
}

async fn story(f: &Fixture, key: &str, score: f64) -> i64 {
    let id: i64 = sqlx::query_scalar(
        "insert into stories (event_key, first_seen, last_seen) values ($1, now(), now()) returning story_id",
    )
    .bind(key)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into scores (story_id, computed_at, scorer_version, components, total, explain)
         values ($1, now(), 'v1', '{}', $2::numeric, 'x')",
    )
    .bind(id)
    .bind(score)
    .execute(&f.pool)
    .await
    .unwrap();
    id
}

fn client(api: &Arc<FakeApi>) -> (Client, Arc<VirtualClock>) {
    let clock = Arc::new(VirtualClock::new(t0()));
    (Client::new(api.clone(), clock.clone()), clock)
}

/// The Edition's publish step: authorise, stage a page and the index budget, in one transaction.
struct Publish {
    work: PathBuf,
}

impl StepHandler for Publish {
    fn run<'a>(&'a self, cx: &'a mut StepCx) -> BoxFut<'a, Result<StepOutcome, StepError>> {
        Box::pin(async move {
            let mut tx = cx.db.begin().await.unwrap();
            let mut auth = gates::authorize(
                &mut tx,
                GRequest {
                    run_id: cx.run_id,
                    step_seq: cx.seq,
                    command: GCommand::Publish,
                    item: None,
                },
            )
            .await
            .unwrap();
            let sha = bundle::load(&self.work, "a-page", 1).unwrap().bundle_sha;
            let first = store::stage_put(&mut auth, "a-page", 1, &sha, cx.now)
                .await
                .unwrap();
            // Staging again is the same row (a re-entered step).
            assert_eq!(
                store::stage_put(&mut auth, "a-page", 1, &sha, cx.now)
                    .await
                    .unwrap(),
                first
            );
            let picks = store::stage_index_promotions(&mut auth, 2, cx.now)
                .await
                .unwrap();
            let again = store::stage_index_promotions(&mut auth, 2, cx.now)
                .await
                .unwrap();
            assert!(again.is_empty(), "the day's cap is used up");
            tx.commit().await.unwrap();
            let ids: Vec<_> = picks.iter().map(|p| p.page_id.clone()).collect();
            Ok(StepOutcome::Done(json!({"picked": ids})))
        })
    }
}

struct Done;
impl StepHandler for Done {
    fn run<'a>(&'a self, _: &'a mut StepCx) -> BoxFut<'a, Result<StepOutcome, StepError>> {
        Box::pin(async { Ok(StepOutcome::Done(json!({}))) })
    }
}

struct Skip;
impl StepHandler for Skip {
    fn run<'a>(&'a self, _: &'a mut StepCx) -> BoxFut<'a, Result<StepOutcome, StepError>> {
        Box::pin(async { Ok(StepOutcome::Skip("review_plan is off".into())) })
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn an_authorised_step_stages_and_the_drain_publishes_and_promotes_within_the_budget() {
    let f = Fixture::migrated().await;
    let dir = work("edition");
    let sha = bundle::write(&dir, "a-page", 1, &FILES);
    page(&f, "a-page", "draft", "noindex", 2, &[], &sha, None).await;
    // Four published, not indexed: b2 is Tier 1 with a new series, b3 is templated, b1 and b4 plain.
    let (s1, s2, s3, s4) = (
        story(&f, "e1", 60.0).await,
        story(&f, "e2", 55.0).await,
        story(&f, "e3", 99.0).await,
        story(&f, "e4", 40.0).await,
    );
    for (id, tier, flags, s) in [
        ("b1", 2, vec![], s1),
        ("b2", 1, vec![], s2),
        ("b3", 1, vec!["templated"], s3),
        ("b4", 2, vec![], s4),
    ] {
        page(
            &f,
            id,
            "published",
            "noindex",
            tier,
            &flags,
            &[7u8; 32],
            Some(s),
        )
        .await;
    }
    sqlx::query("insert into page_refs (page_id, version, ref) values ('b2', 1, 'sr_gdp_growth')")
        .execute(&f.pool)
        .await
        .unwrap();

    let handlers = [
        "preflight",
        "discover",
        "stories",
        "plan",
        "brief",
        "write",
        "render",
        "spotcheck",
        "promote",
    ]
    .into_iter()
    .fold(Handlers::default(), |h, s| {
        h.with(&format!("edition.{s}"), Arc::new(Done))
    })
    .with("edition.review", Arc::new(Skip))
    .with("edition.publish", Arc::new(Publish { work: dir.clone() }));
    let clock = ManualClock::new(t0());
    let ch = Chief::builder(f.pool.clone(), Arc::new(clock))
        .handlers(handlers)
        .build();
    let run = ch
        .start(StartReq::board("edition", Map::new(), IfBusy::Offer))
        .await
        .unwrap()
        .run()
        .clone();
    ch.ensure_drivers().await.unwrap();
    for _ in 0..100 {
        let st = f
            .scalar_str(&format!(
                "select status from runs where run_id = '{}'",
                run.run_id
            ))
            .await;
        if st == "succeeded" {
            break;
        }
        assert!(st != "failed", "the run failed");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Staged in the step's transaction: the page is ready, with its row and the two promotions.
    assert_eq!(
        f.scalar_str("select status from pages where page_id = 'a-page'")
            .await,
        "ready"
    );
    assert_eq!(
        f.scalar_i64("select count(*) from publish_outbox where op = 'put_page'")
            .await,
        1
    );
    assert_eq!(
        f.scalar_str("select string_agg(page_id, ',' order by outbox_id) from publish_outbox where op = 'index_promote'").await,
        "b2,b1",
        "Tier 1 and a new series first; the templated page is skipped; the cap is 2"
    );

    let api = Arc::new(FakeApi::new());
    let (c, _) = client(&api);
    let out = store::drain(&f.pool, &c, &dir, Some(run.run_id))
        .await
        .unwrap();
    assert_eq!((out.sent, out.dead, out.unknown), (3, 0, 0));
    assert_eq!(
        f.scalar_str("select status from pages where page_id = 'a-page'")
            .await,
        "published"
    );
    assert_eq!(f.scalar_str("select string_agg(page_id, ',' order by page_id) from pages where index_status = 'index'").await, "b1,b2");
    // The first request is the page's put under page:version:sha, then the promotions.
    let reqs = api.requests();
    assert_eq!(
        reqs[0].idempotency_key,
        page_key("a-page", 1, &gates::hex(&sha), "put_page")
    );
    assert_eq!(reqs.len(), 3);
    // A second drain sends nothing; prune then removes the page's working files.
    assert_eq!(store::drain(&f.pool, &c, &dir, None).await.unwrap().sent, 0);
    let (files, _) = news_press::prune::prune(&f.pool, &dir).await.unwrap();
    assert_eq!(files, 3, "manifest, index.html and data.json");
    let stats = store::stats(&f.pool, 2, t0()).await.unwrap();
    assert!(
        stats["text"]
            .as_str()
            .unwrap()
            .contains("Index promotions today: 2 of 2")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// An outbox row as an earlier process left it, never sent.
async fn staged(f: &Fixture, id: &str, sha: &[u8; 32]) {
    page(f, id, "ready", "noindex", 2, &[], sha, None).await;
    sqlx::query(
        "insert into publish_outbox (page_id, version, bundle_sha, idempotency_key, op) values ($1, 1, $2, $3, 'put_page')",
    )
    .bind(id)
    .bind(&sha[..])
    .bind(page_key(id, 1, &gates::hex(sha), "put_page"))
    .execute(&f.pool)
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn a_crash_between_ready_and_sent_resends_the_same_key_once() {
    let f = Fixture::migrated().await;
    let dir = work("crash");
    let sha = bundle::write(&dir, "p-one", 1, &FILES);
    staged(&f, "p-one", &sha).await;
    let key = page_key("p-one", 1, &gates::hex(&sha), "put_page");

    // The API is down for the whole first drain: the row stays unsent, marked no-answer, and the page
    // is still ready, so nothing is half-published.
    let api = Arc::new(FakeApi::new());
    api.script((0..5).map(|_| Fault::Unreachable));
    let (c, _) = client(&api);
    let out = store::drain(&f.pool, &c, &dir, None).await.unwrap();
    assert_eq!((out.sent, out.unknown), (0, 1));
    assert_eq!(
        f.scalar_str("select status from pages where page_id = 'p-one'")
            .await,
        "ready"
    );
    assert_eq!(
        f.scalar_i64("select attempts::bigint from publish_outbox")
            .await,
        5
    );
    assert_eq!(f.scalar_i64("select count(*) from publish_outbox where unknown_since is not null and sent_at is null").await, 1);

    // The next drain (a new process) sends the same key; the API applies it once.
    let out = store::drain(&f.pool, &c, &dir, None).await.unwrap();
    assert_eq!(out.sent, 1);
    assert_eq!(api.applied(), std::slice::from_ref(&key));
    assert!(api.requests().iter().all(|r| r.idempotency_key == key));
    assert_eq!(
        f.scalar_str("select status from pages where page_id = 'p-one'")
            .await,
        "published"
    );

    // Sent but never recorded (the answer was lost, then the process died): the row is unsent, the API
    // replays its stored answer, and nothing is written twice.
    sqlx::query("update publish_outbox set sent_at = null, response = null")
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(store::drain(&f.pool, &c, &dir, None).await.unwrap().sent, 1);
    assert_eq!(api.applied().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn a_4xx_or_a_bad_bundle_goes_on_the_dead_list_and_the_next_row_still_goes() {
    let f = Fixture::migrated().await;
    let dir = work("dead");
    let a = bundle::write(&dir, "p-a", 1, &FILES);
    let c_sha = bundle::write(&dir, "p-c", 1, &FILES);
    staged(&f, "p-a", &a).await;
    staged(&f, "p-b", &[9u8; 32]).await; // no bundle on disk
    staged(&f, "p-c", &c_sha).await;
    let api = Arc::new(FakeApi::new());
    api.script([Fault::Status(400, "invalid_bundle", None)]);
    let (c, _) = client(&api);
    let out = store::drain(&f.pool, &c, &dir, None).await.unwrap();
    assert_eq!((out.sent, out.dead, out.unknown), (1, 2, 0));
    let err = f
        .scalar_str("select last_error from publish_outbox where page_id = 'p-a'")
        .await;
    assert!(err.contains("invalid_bundle"), "{err}");
    assert!(
        f.scalar_str("select last_error from publish_outbox where page_id = 'p-b'")
            .await
            .contains("cannot read")
    );
    assert_eq!(
        f.scalar_str("select status from pages where page_id = 'p-c'")
            .await,
        "published"
    );
    // Dead rows are not retried.
    assert_eq!(
        store::drain(&f.pool, &c, &dir, None).await.unwrap(),
        Default::default()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn staging_outside_an_authorised_step_is_gated() {
    let f = Fixture::migrated().await;
    // No run: authorize() itself refuses, so no Authorized exists to stage with.
    let mut tx = f.pool.begin().await.unwrap();
    let e = gates::authorize(
        &mut tx,
        GRequest {
            run_id: Uuid::new_v4(),
            step_seq: 1,
            command: GCommand::Publish,
            item: None,
        },
    )
    .await
    .err()
    .unwrap();
    assert_eq!(e.code, ErrorCode::Gated);
}
