//! Taster against a real database: duplicates (a press release syndicated by two wires is one story),
//! event keys, update detection, the grey zone through the embedding seam, the scoring inputs Nose
//! reads, `merge` and `split`, and `news stories show|members|similar`. Detection run twice changes
//! nothing.

use super::fixture::Fixture;
use chrono::{DateTime, TimeDelta, Utc};
use news_clerk::simhash::{as_bigint, simhash};
use news_taster::config;
use news_taster::detect::{Cand, Joiner, Neighbour, NoEmbedder, detect};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};

const BOARD: &str = include_str!("../../../news-taster/tests/fixtures/fomc-2025-09-17-board.txt");
const WIRE: &str = include_str!("../../../news-taster/tests/fixtures/fomc-2025-09-17-wire.txt");
const JUNE: &str = include_str!("../../../news-taster/tests/fixtures/fomc-2025-06-18-board.txt");

fn t0() -> DateTime<Utc> {
    "2026-11-20T12:00:00Z".parse().unwrap()
}

async fn source(pool: &PgPool, id: &str, tier: i16) {
    sqlx::query(
        "insert into sources (source_id, name, publisher, connector, base_url, source_tier, licence, attribution,
           redistribution, image_policy, rate_per_sec, regions, categories)
         values ($1, $1, 'x', $1, 'https://x.test/', $2, 'public-domain', 'x', true, 'none', 1.0, '{us}', '{economy}')",
    )
    .bind(id)
    .bind(tier)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into snapshots (sha256, source_id, url, fetched_at, http_status, bytes, storage_path)
         values (sha256($1::bytea), $2, 'https://x.test/a', now(), 200, 1, 'x')",
    )
    .bind(id.as_bytes())
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
}

/// A document of `text` from `source`; `sha` is its content hash.
async fn doc(pool: &PgPool, source: &str, text: &str, sha: &str) -> i64 {
    sqlx::query_scalar(
        "insert into documents (snapshot_sha, kind, title, published_at, text_len, simhash, content_sha)
         values (sha256($1::bytea), 'press_release', 'x', $2, $3, $4, sha256($5::bytea)) returning doc_id",
    )
    .bind(source.as_bytes())
    .bind(t0())
    .bind(text.len() as i32)
    .bind(as_bigint(simhash(text)))
    .bind(sha.as_bytes())
    .fetch_one(pool)
    .await
    .unwrap()
}

/// A new `news` candidate; `at` is when it was first seen.
async fn cand(
    pool: &PgPool,
    source: &str,
    title: &str,
    key: Option<&str>,
    doc: Option<i64>,
    at: DateTime<Utc>,
) -> i64 {
    sqlx::query_scalar(
        "insert into candidates (source_id, url, title, event_key, doc_ids, page_type, page_type_by, categories,
           region, published_at, first_seen, status)
         values ($1, 'https://x.test/' || $2, $2, $3, $4, 'news', 'rule', '{economy}', 'us', $5, $5, 'new')
         returning cand_id",
    )
    .bind(source)
    .bind(title)
    .bind(key)
    .bind(doc.map_or_else(Vec::new, |d| vec![d]))
    .bind(at)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn n(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| panic!("{sql}: {e}"))
}

/// Every taster-owned row as one text, to compare before and after a re-run.
async fn dump(pool: &PgPool) -> String {
    sqlx::query_scalar(
        "select coalesce((select string_agg(s::text, E'\n' order by story_id) from stories s), '') || '#' ||
                coalesce((select string_agg(c::text, E'\n' order by cand_id) from candidates c), '') || '#' ||
                (select count(*) from jobs)::text",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn publish_page(pool: &PgPool, story: i64) {
    let page = format!("2026/11/p{story}");
    sqlx::query(
        "insert into pages (page_id, slug, story_id, page_type, version, status, published_at)
         values ($1, $1, $2, 'news', 1, 'published', $3)",
    )
    .bind(&page)
    .bind(story)
    .bind(t0())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("update stories set status = 'covered', page_id = $2 where story_id = $1")
        .bind(story)
        .bind(&page)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn a_release_syndicated_by_two_wires_is_one_story_and_a_rerun_changes_nothing() {
    let f = Fixture::migrated().await;
    let p = &f.pool;
    source(p, "fed", 1).await;
    source(p, "wire-a", 2).await;
    source(p, "wire-b", 3).await;
    let board = doc(p, "fed", BOARD, "board").await;
    let wire = doc(p, "wire-a", WIRE, "wire").await;
    // The same bytes arrive again from another wire: an exact duplicate.
    let same = doc(p, "wire-b", WIRE, "wire").await;
    let june = doc(p, "fed", JUNE, "june").await;
    let late = doc(p, "wire-b", WIRE, "late").await;
    cand(p, "fed", "Fed statement", None, Some(board), t0()).await;
    cand(
        p,
        "wire-a",
        "Fed holds",
        None,
        Some(wire),
        t0() + TimeDelta::minutes(5),
    )
    .await;
    cand(
        p,
        "wire-b",
        "Fed holds again",
        None,
        Some(same),
        t0() + TimeDelta::minutes(9),
    )
    .await;
    // The June statement reads alike but is another event: its own story.
    cand(
        p,
        "fed",
        "Fed statement June",
        None,
        Some(june),
        t0() + TimeDelta::minutes(12),
    )
    .await;
    // The September text again eight days later, with another hash: outside the 7-day SimHash window.
    cand(
        p,
        "wire-b",
        "Fed late",
        None,
        Some(late),
        t0() + TimeDelta::days(8),
    )
    .await;

    let t = config::v1().unwrap();
    let report = detect(p, &NoEmbedder, &t).await.unwrap();
    assert_eq!(report.candidates, 5);
    // wire-a joined through SimHash and wire-b's copy through its content hash.
    assert_eq!(
        (report.near_duplicates, report.exact_duplicates),
        (1, 1),
        "{report:?}"
    );
    assert_eq!(report.new_stories, 3, "{report:?}");
    assert_eq!(n(p, "select count(*) from stories").await, 3);
    assert_eq!(
        n(
            p,
            "select member_count::bigint from stories order by story_id limit 1"
        )
        .await,
        3
    );
    // Scoring input for the Fed story: Nose's shape, the best (tier 1) member leading.
    let input: Value =
        sqlx::query_scalar("select score_input from stories order by story_id limit 1")
            .fetch_one(p)
            .await
            .unwrap();
    assert_eq!(input["event_type"], "news_story");
    assert_eq!(input["page"], json!({"type": "news", "primary_sources": 1}));
    assert_eq!(
        (input["source_tier"].as_i64(), input["action"].as_str()),
        (Some(1), Some("new"))
    );
    assert_eq!(input["regions"], json!(["north-america"]));
    serde_json::from_value::<news_nose::score::StoryInput>(input).unwrap();

    let before = dump(p).await;
    assert_eq!(
        detect(p, &NoEmbedder, &t).await.unwrap(),
        Default::default()
    );
    assert_eq!(dump(p).await, before, "a re-run must change nothing");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn a_candidate_for_a_covered_story_is_an_update_nose_can_score() {
    let f = Fixture::migrated().await;
    let p = &f.pool;
    source(p, "fed", 1).await;
    source(p, "wire-a", 2).await;
    cand(p, "fed", "Fed", Some("news:fed:2026-11-19"), None, t0()).await;
    cand(
        p,
        "wire-a",
        "Fed wire",
        Some("news:fed:2026-11-19"),
        None,
        t0(),
    )
    .await;
    let t = config::v1().unwrap();
    let r = detect(p, &NoEmbedder, &t).await.unwrap();
    assert_eq!(
        (r.by_event_key, r.new_stories, r.updates, r.scoreable),
        // by_event_key counts joins to an existing keyed story; the first candidate creates it.
        (1, 1, 0, 1),
        "{r:?}"
    );
    let story = n(p, "select story_id from stories").await;

    publish_page(p, story).await;
    // Covered stories are not in Nose's open list ...
    assert!(news_nose::store::open_inputs(p).await.unwrap().is_empty());
    // ... until a new candidate for the same key arrives.
    cand(
        p,
        "wire-a",
        "Fed follow-up",
        Some("news:fed:2026-11-19"),
        None,
        t0() + TimeDelta::hours(3),
    )
    .await;
    let r = detect(p, &NoEmbedder, &t).await.unwrap();
    assert_eq!((r.updates, r.new_stories), (1, 0), "{r:?}");
    assert_eq!(
        sqlx::query_scalar::<_, String>("select status from stories")
            .fetch_one(p)
            .await
            .unwrap(),
        "open"
    );
    let open = news_nose::store::open_inputs(p).await.unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].1["action"], "update");
    assert_eq!(open[0].1["novelty"], json!({"kind": "follow_up"}));
    // It scores: the stored input is what Nose's `score` reads.
    let input: news_nose::score::StoryInput = serde_json::from_value(open[0].1.clone()).unwrap();
    let cfg = news_nose::config::v1().unwrap();
    news_nose::score::score(
        &input,
        t0() + TimeDelta::hours(4),
        &Default::default(),
        &cfg,
    )
    .unwrap();
    assert_eq!(n(p, "select member_count::bigint from stories").await, 3);
}

/// Neighbours fixed by the test, standing in for the embedder.
struct Fixed(Vec<Neighbour>);

impl Joiner for Fixed {
    async fn neighbours(
        &self,
        _: &mut PgConnection,
        _: &Cand,
        _: &config::Thresholds,
    ) -> Result<Vec<Neighbour>, sqlx::Error> {
        Ok(self.0.clone())
    }
}

fn near(story_id: i64, cos: f64, shared_entity: bool, has_page: bool) -> Fixed {
    Fixed(vec![Neighbour {
        story_id,
        cos,
        shared_entity,
        has_page,
    }])
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn the_grey_zone_asks_a4_and_stays_separate_without_an_answer() {
    let f = Fixture::migrated().await;
    let p = &f.pool;
    source(p, "wire-a", 2).await;
    let t = config::v1().unwrap();
    cand(p, "wire-a", "Chip export curbs widen", None, None, t0()).await;
    detect(p, &NoEmbedder, &t).await.unwrap();
    let first = n(p, "select story_id from stories").await;

    // 0.78 is in the grey zone: an A4 job, and the candidate is its own story.
    let c2 = cand(
        p,
        "wire-a",
        "Export controls on chips expand",
        None,
        None,
        t0() + TimeDelta::hours(1),
    )
    .await;
    let r = detect(p, &near(first, 0.78, true, false), &t)
        .await
        .unwrap();
    assert_eq!(
        (r.grey_zone, r.by_similarity, r.new_stories),
        (1, 0, 1),
        "{r:?}"
    );
    assert_eq!(n(p, "select count(*) from stories").await, 2);
    let (kind, pool, args): (String, String, Value) =
        sqlx::query_as("select kind, pool, args from jobs")
            .fetch_one(p)
            .await
            .unwrap();
    assert_eq!((kind.as_str(), pool.as_str()), ("ai.storymatch", "ai"));
    assert_eq!(args, json!({"cand": c2, "story": first, "cos": 0.78}));
    let input: Value = sqlx::query_scalar("select score_input from stories where story_id <> $1")
        .bind(first)
        .fetch_one(p)
        .await
        .unwrap();
    assert_eq!(
        input["novelty"],
        json!({"kind": "similar", "max_cos": 0.78})
    );

    // The same pair is asked once: the job's unique key holds if the candidate is detected again.
    sqlx::query("update candidates set status = 'new', story_id = null where cand_id = $1")
        .bind(c2)
        .execute(p)
        .await
        .unwrap();
    detect(p, &near(first, 0.78, true, false), &t)
        .await
        .unwrap();
    assert_eq!(n(p, "select count(*) from jobs").await, 1);

    // Over the join bar with a shared entity: joins. Over it without one: grey. Under 0.72: new.
    cand(p, "wire-a", "c3", None, None, t0() + TimeDelta::hours(2)).await;
    let r = detect(p, &near(first, 0.9, true, false), &t).await.unwrap();
    assert_eq!((r.by_similarity, r.grey_zone), (1, 0), "{r:?}");
    cand(p, "wire-a", "c4", None, None, t0() + TimeDelta::hours(3)).await;
    let r = detect(p, &near(first, 0.9, false, false), &t)
        .await
        .unwrap();
    assert_eq!((r.grey_zone, r.by_similarity), (1, 0), "{r:?}");
    cand(p, "wire-a", "c5", None, None, t0() + TimeDelta::hours(4)).await;
    let r = detect(p, &near(first, 0.5, true, false), &t).await.unwrap();
    assert_eq!(
        (r.grey_zone, r.by_similarity, r.new_stories),
        (0, 0, 1),
        "{r:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn a_close_match_to_a_story_with_a_page_is_an_update() {
    let f = Fixture::migrated().await;
    let p = &f.pool;
    source(p, "wire-a", 2).await;
    let t = config::v1().unwrap();
    cand(p, "wire-a", "Fed holds", None, None, t0()).await;
    detect(p, &NoEmbedder, &t).await.unwrap();
    let story = n(p, "select story_id from stories").await;
    publish_page(p, story).await;
    // 0.84 to a page is grey; 0.86 with a shared entity joins it as an update.
    cand(
        p,
        "wire-a",
        "Fed holds rates",
        None,
        None,
        t0() + TimeDelta::hours(1),
    )
    .await;
    let r = detect(p, &near(story, 0.84, true, true), &t).await.unwrap();
    assert_eq!((r.updates, r.grey_zone), (0, 1), "{r:?}");
    cand(
        p,
        "wire-a",
        "Fed holds rates steady",
        None,
        None,
        t0() + TimeDelta::hours(2),
    )
    .await;
    let r = detect(p, &near(story, 0.86, true, true), &t).await.unwrap();
    assert_eq!((r.updates, r.by_similarity), (1, 1), "{r:?}");
    assert_eq!(
        n(
            p,
            &format!("select count(*) from stories where story_id = {story} and status = 'open'")
        )
        .await,
        1
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn merge_and_split_move_candidates_and_keep_event_keys_pointing_home() {
    let f = Fixture::migrated().await;
    let p = &f.pool;
    source(p, "wire-a", 2).await;
    let t = config::v1().unwrap();
    cand(p, "wire-a", "A one", Some("news:a"), None, t0()).await;
    let b1 = cand(
        p,
        "wire-a",
        "B one",
        Some("news:b"),
        None,
        t0() + TimeDelta::hours(1),
    )
    .await;
    let b2 = cand(
        p,
        "wire-a",
        "B two",
        Some("news:b"),
        None,
        t0() + TimeDelta::hours(2),
    )
    .await;
    detect(p, &NoEmbedder, &t).await.unwrap();
    let a = n(p, "select story_id from stories where event_key = 'news:a'").await;
    let b = n(p, "select story_id from stories where event_key = 'news:b'").await;

    // Merge A into B: B has all three, A is closed, and a later 'news:a' candidate lands in B.
    news_taster::edit::merge(p, a, b).await.unwrap();
    assert_eq!(
        n(
            p,
            &format!("select member_count::bigint from stories where story_id = {b}")
        )
        .await,
        3
    );
    assert_eq!(
        n(
            p,
            &format!("select count(*) from stories where story_id = {a} and status = 'closed'")
        )
        .await,
        1
    );
    cand(
        p,
        "wire-a",
        "A two",
        Some("news:a"),
        None,
        t0() + TimeDelta::hours(3),
    )
    .await;
    detect(p, &NoEmbedder, &t).await.unwrap();
    assert_eq!(
        n(
            p,
            &format!("select member_count::bigint from stories where story_id = {b}")
        )
        .await,
        4
    );
    assert_eq!(n(p, "select count(*) from stories").await, 2);
    // Refusals: itself, a merged-away story, and a story with a page folded into another.
    assert!(news_taster::edit::merge(p, b, b).await.is_err());
    assert!(news_taster::edit::merge(p, a, b).await.is_err());
    cand(
        p,
        "wire-a",
        "C",
        Some("news:c"),
        None,
        t0() + TimeDelta::hours(4),
    )
    .await;
    detect(p, &NoEmbedder, &t).await.unwrap();
    let cs = n(p, "select story_id from stories where event_key = 'news:c'").await;
    publish_page(p, cs).await;
    let err = news_taster::edit::merge(p, cs, b).await.unwrap_err();
    assert!(err.message.contains("has a page"), "{}", err.message);
    // Merging into a story with a page reopens it as an update.
    news_taster::edit::merge(p, b, cs).await.unwrap();
    assert_eq!(
        n(
            p,
            &format!("select count(*) from stories where story_id = {cs} and status = 'open'")
        )
        .await,
        1
    );
    let input: Value = sqlx::query_scalar("select score_input from stories where story_id = $1")
        .bind(cs)
        .fetch_one(p)
        .await
        .unwrap();
    assert_eq!(input["action"], "update");

    // Split two candidates off C's story; at least one must stay, and they must belong to it.
    assert!(news_taster::edit::split(p, cs, &[]).await.is_err());
    assert!(news_taster::edit::split(p, cs, &[9999]).await.is_err());
    let all: Vec<i64> = sqlx::query_scalar("select cand_id from candidates where story_id = $1")
        .bind(cs)
        .fetch_all(p)
        .await
        .unwrap();
    assert!(news_taster::edit::split(p, cs, &all).await.is_err());
    let new = news_taster::edit::split(p, cs, &[b1, b2]).await.unwrap();
    assert_eq!(
        n(
            p,
            &format!("select member_count::bigint from stories where story_id = {new}")
        )
        .await,
        2
    );
    assert_eq!(
        n(
            p,
            &format!("select member_count::bigint from stories where story_id = {cs}")
        )
        .await,
        all.len() as i64 - 2
    );
    assert_eq!(
        n(p, &format!("select count(*) from stories where story_id = {new} and event_key is null and score_input is not null")).await,
        1
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn stories_show_members_and_similar_print_their_contracts() {
    let f = Fixture::migrated().await;
    let p = &f.pool;
    source(p, "fed", 1).await;
    source(p, "bls", 1).await;
    let board = doc(p, "fed", BOARD, "board").await;
    let c1 = cand(
        p,
        "fed",
        "Fed statement",
        Some("news:fomc:2025-09-17"),
        Some(board),
        t0(),
    )
    .await;
    let c2 = cand(p, "bls", "No text", None, None, t0() + TimeDelta::hours(1)).await;
    detect(p, &NoEmbedder, &config::v1().unwrap())
        .await
        .unwrap();

    let (env, code, ..) = f.call(&["stories", "show", "news:fomc:2025-09-17"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(
        env["data"],
        json!({"story_id": 1, "event_key": "news:fomc:2025-09-17", "title": "Fed statement",
               "member_count": 1, "status": "open", "page_id": null})
    );
    let (env, code, ..) = f.call(&["stories", "show", "1"]);
    assert_eq!((code, env["data"]["story_id"].as_i64()), (0, Some(1)));
    let (env, code, ..) = f.call(&["stories", "show", "99"]);
    assert_eq!(
        (code, env["error"]["code"].as_str()),
        (3, Some("not_found")),
        "{env}"
    );

    let (env, code, ..) = f.call(&["stories", "members", "1"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(
        env["data"]["items"],
        json!([{"cand_id": c1, "source_id": "fed", "title": "Fed statement",
                "published_at": t0().to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)}])
    );

    // The wire's text, not the Board's, finds the story by SimHash (cos 1 - Hamming/64).
    let (env, code, ..) = f.call(&["stories", "similar", "--text", WIRE]);
    assert_eq!(code, 0, "{env}");
    let top = &env["data"]["items"][0];
    assert_eq!(top["story_id"], 1);
    assert!(top["cos"].as_f64().unwrap() >= 1.0 - 3.0 / 64.0, "{top}");
    let (env, code, ..) = f.call(&[
        "stories",
        "similar",
        "--text",
        "inflation expectations eased",
    ]);
    assert_eq!(
        (code, env["data"]["items"].as_array().unwrap().len()),
        (0, 0),
        "{env}"
    );
    // A candidate with no text has no SimHash neighbours: an empty list and a warning.
    let (env, code, ..) = f.call(&["stories", "similar", "--cand", &c2.to_string()]);
    assert_eq!(
        (code, env["data"]["items"].as_array().unwrap().len()),
        (0, 0),
        "{env}"
    );
    assert_eq!(env["warnings"].as_array().unwrap().len(), 1, "{env}");
    let (env, code, ..) = f.call(&["stories", "similar", "--cand", "9999"]);
    assert_eq!(
        (code, env["error"]["code"].as_str()),
        (3, Some("not_found")),
        "{env}"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn the_sql_hamming_distance_is_the_clerks() {
    let f = Fixture::migrated().await;
    for (a, b) in [(BOARD, WIRE), (BOARD, JUNE), (WIRE, WIRE)] {
        let (x, y) = (as_bigint(simhash(a)), as_bigint(simhash(b)));
        let sql: i64 = sqlx::query_scalar("select bit_count(($1::bigint # $2::bigint)::bit(64))")
            .bind(x)
            .bind(y)
            .fetch_one(&f.pool)
            .await
            .unwrap();
        assert_eq!(
            sql,
            i64::from(news_clerk::simhash::distance(simhash(a), simhash(b)))
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn two_detect_runs_at_once_group_each_candidate_once_and_leave_no_empty_story() {
    let f = Fixture::migrated().await;
    let p = &f.pool;
    source(p, "wire-a", 2).await;
    let t = config::v1().unwrap();
    for (i, key) in [Some("news:x"), Some("news:x"), Some("news:y"), None]
        .into_iter()
        .enumerate()
    {
        cand(
            p,
            "wire-a",
            &format!("Item {i}"),
            key,
            None,
            t0() + TimeDelta::minutes(i as i64),
        )
        .await;
    }
    let (a, b) = tokio::join!(detect(p, &NoEmbedder, &t), detect(p, &NoEmbedder, &t));
    let (a, b) = (a.unwrap(), b.unwrap());
    // Whichever ran first did all the work; the other found nothing new.
    assert_eq!(a.candidates + b.candidates, 4, "{a:?} {b:?}");
    assert_eq!(n(p, "select count(*) from stories").await, 3);
    assert_eq!(
        n(p, "select count(*) from stories where member_count = 0").await,
        0
    );
    assert_eq!(
        n(
            p,
            "select count(*) from candidates where status = 'grouped' and story_id is not null"
        )
        .await,
        4
    );
}
