//! Taster's embedding joiner (`news_taster::embed`) against pgvector, with hand-made 384-d vectors in
//! place of the model (CI never runs it): nearest-centroid join, the grey zone, a new story below the
//! floor, the 72 h window, centroids kept as the mean of the members, and ADR 0011 decision 7.
//!
//! A vector is built from two unit axes so that its cosine to another is known by construction:
//! `mix(i, j, c)` is `c·e_i + sqrt(1 - c²)·e_j`, whose cosine to `e_i` is exactly `c`.

use std::collections::HashMap;

use super::fixture::Fixture;
use super::taster::{cand, doc, n, publish_page, source, t0};
use chrono::TimeDelta;
use news_taster::config;
use news_taster::detect::{Cand, Joiner, detect};
use news_taster::embed::{DIM, EmbedJoiner, Embedder};
use sqlx::PgPool;

fn axis(i: usize) -> Vec<f32> {
    let mut v = vec![0.0; DIM];
    v[i] = 1.0;
    v
}

fn mix(i: usize, j: usize, c: f32) -> Vec<f32> {
    let mut v = vec![0.0; DIM];
    v[i] = c;
    v[j] = (1.0 - c * c).sqrt();
    v
}

/// The embedder of these tests: the vector stored for the exact text it is given.
struct Lookup(HashMap<String, Vec<f32>>);

impl Embedder for Lookup {
    fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        self.0
            .get(text)
            .cloned()
            .ok_or_else(|| format!("no fixture vector for {text:?}"))
    }
}

fn joiner(texts: &[(&str, Vec<f32>)]) -> EmbedJoiner<Lookup> {
    EmbedJoiner::new(Lookup(
        texts
            .iter()
            .map(|(t, v)| ((*t).to_owned(), v.clone()))
            .collect(),
    ))
}

async fn entities(pool: &PgPool, cand: i64, ids: &[i64]) {
    sqlx::query("update candidates set entity_ids = $2 where cand_id = $1")
        .bind(cand)
        .bind(ids)
        .execute(pool)
        .await
        .unwrap();
}

async fn story_of(pool: &PgPool, cand: i64) -> i64 {
    n(
        pool,
        &format!("select story_id from candidates where cand_id = {cand}"),
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn cosine_to_the_centroid_joins_asks_a4_or_starts_a_story() {
    let f = Fixture::migrated().await;
    let p = &f.pool;
    let t = config::v1().unwrap();
    source(p, "wire-a", 2).await;
    // Story A's first member embeds title + first paragraph, not the heading block.
    let d = doc(p, "wire-a", "Alpha text", "alpha").await;
    for (no, kind, text) in [
        (0, "heading", "A heading"),
        (1, "paragraph", "First paragraph of alpha."),
        (2, "paragraph", "Second paragraph."),
    ] {
        sqlx::query(
            "insert into text_blocks (doc_id, block_no, kind, text) values ($1, $2, $3, $4)",
        )
        .bind(d)
        .bind(no)
        .bind(kind)
        .bind(text)
        .execute(p)
        .await
        .unwrap();
    }
    let j = joiner(&[
        ("alpha\n\nFirst paragraph of alpha.", axis(0)),
        ("beta", mix(0, 1, 0.90)),
        ("gamma", mix(0, 2, 0.60)),
        ("delta", mix(0, 3, 0.78)),
    ]);

    let a = cand(p, "wire-a", "alpha", None, Some(d), t0()).await;
    entities(p, a, &[7]).await;
    detect(p, &j, &t).await.unwrap();
    let story_a = story_of(p, a).await;
    assert_eq!(
        n(
            p,
            "select count(*) from embeddings where owner_kind = 'story'"
        )
        .await,
        1
    );

    // 0.90 to A with a shared entity: joins, and the centroid becomes the mean of the two vectors.
    let b = cand(p, "wire-a", "beta", None, None, t0() + TimeDelta::hours(1)).await;
    entities(p, b, &[7, 8]).await;
    let r = detect(p, &j, &t).await.unwrap();
    assert_eq!((r.by_similarity, r.new_stories), (1, 0));
    assert_eq!(story_of(p, b).await, story_a);
    assert_eq!(
        n(
            p,
            "select count(*) from embeddings where owner_kind = 'story'"
        )
        .await,
        1
    );
    let centroid_vs_first: f64 = sqlx::query_scalar(
        "select (1 - (s.v <=> c.v))::float8 from embeddings s, embeddings c
          where s.owner_kind = 'story' and c.owner_kind = 'candidate' and c.owner_id = $1::bigint::text",
    )
    .bind(a)
    .fetch_one(p)
    .await
    .unwrap();
    // The mean of e0 and (0.9, 0.436) points at (0.95, 0.218): cosine 0.9747 to e0.
    assert!(
        (0.97..0.98).contains(&centroid_vs_first),
        "{centroid_vs_first}"
    );

    // 0.60 to A: under 0.72, a new story.
    let c = cand(p, "wire-a", "gamma", None, None, t0() + TimeDelta::hours(2)).await;
    entities(p, c, &[7]).await;
    let r = detect(p, &j, &t).await.unwrap();
    assert_eq!((r.by_similarity, r.grey_zone, r.new_stories), (0, 0, 1));
    assert_ne!(story_of(p, c).await, story_a);

    // About 0.76 to A's centroid: the grey zone. An A4 job, and the candidate stays apart.
    let dd = cand(p, "wire-a", "delta", None, None, t0() + TimeDelta::hours(3)).await;
    entities(p, dd, &[7]).await;
    let r = detect(p, &j, &t).await.unwrap();
    assert_eq!((r.by_similarity, r.grey_zone, r.new_stories), (0, 1, 1));
    assert_ne!(story_of(p, dd).await, story_a);
    assert_eq!(
        n(p, "select count(*) from jobs where kind = 'ai.storymatch'").await,
        1
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn the_nearest_story_wins_and_only_inside_the_window() {
    let f = Fixture::migrated().await;
    let p = &f.pool;
    let t = config::v1().unwrap();
    source(p, "wire-a", 2).await;
    let j = joiner(&[
        ("one", axis(0)),
        ("two", axis(1)),
        ("near two", mix(1, 0, 0.95)),
        ("late echo of one", axis(0)),
    ]);
    let one = cand(p, "wire-a", "one", None, None, t0()).await;
    let two = cand(p, "wire-a", "two", None, None, t0()).await;
    entities(p, one, &[7]).await;
    entities(p, two, &[7]).await;
    detect(p, &j, &t).await.unwrap();
    assert_ne!(story_of(p, one).await, story_of(p, two).await);

    // 0.95 to story two, 0.31 to story one: joins two.
    let x = cand(
        p,
        "wire-a",
        "near two",
        None,
        None,
        t0() + TimeDelta::hours(1),
    )
    .await;
    entities(p, x, &[7]).await;
    let r = detect(p, &j, &t).await.unwrap();
    assert_eq!((r.by_similarity, r.new_stories), (1, 0));
    assert_eq!(story_of(p, x).await, story_of(p, two).await);

    // Identical to story one, but 80 h after its last member (window: 72 h): a new story.
    let late = cand(
        p,
        "wire-a",
        "late echo of one",
        None,
        None,
        t0() + TimeDelta::hours(80),
    )
    .await;
    entities(p, late, &[7]).await;
    let r = detect(p, &j, &t).await.unwrap();
    assert_eq!((r.by_similarity, r.new_stories), (0, 1));
    assert_ne!(story_of(p, late).await, story_of(p, one).await);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn a_keyed_story_has_a_centroid_a_keyless_article_finds_but_a_keyed_one_never_matches() {
    let f = Fixture::migrated().await;
    let p = &f.pool;
    let t = config::v1().unwrap();
    source(p, "wire-a", 2).await;
    let j = joiner(&[
        ("ECB holds", axis(0)),
        ("same words", axis(0)),
        ("ECB holds again", axis(0)),
    ]);
    // A keyed candidate is embedded too, so its story gets a centroid (decision 7 only stops it being
    // matched by similarity).
    let keyed = cand(p, "wire-a", "ECB holds", Some("ecb:2026-10"), None, t0()).await;
    entities(p, keyed, &[7]).await;
    detect(p, &j, &t).await.unwrap();
    let story = story_of(p, keyed).await;
    assert_eq!(
        n(
            p,
            "select count(*) from embeddings where owner_kind = 'story'"
        )
        .await,
        1
    );

    // A keyless article with the same vector and a shared entity finds the keyed story (the FOMC case).
    let keyless = cand(
        p,
        "wire-a",
        "same words",
        None,
        None,
        t0() + TimeDelta::hours(1),
    )
    .await;
    entities(p, keyless, &[7]).await;
    let r = detect(p, &j, &t).await.unwrap();
    assert_eq!((r.by_similarity, r.new_stories), (1, 0));
    assert_eq!(story_of(p, keyless).await, story);

    // A keyed candidate with a different key and an identical vector is a new story: no cosine match.
    let other = cand(
        p,
        "wire-a",
        "ECB holds again",
        Some("ecb:2026-11"),
        None,
        t0() + TimeDelta::hours(2),
    )
    .await;
    entities(p, other, &[7]).await;
    let r = detect(p, &j, &t).await.unwrap();
    assert_eq!(
        (r.by_similarity, r.grey_zone, r.by_event_key, r.new_stories),
        (0, 0, 0, 1)
    );
    assert_ne!(story_of(p, other).await, story);

    // Its page makes the story one that needs the update threshold (0.85); it stays a neighbour 10 days on.
    publish_page(p, story).await;
    let mut conn = p.acquire().await.unwrap();
    let late = Cand {
        cand_id: keyless,
        title: Some("same words".into()),
        event_key: None,
        entity_ids: vec![],
        first_seen: t0() + TimeDelta::days(10),
        doc: None,
    };
    let ns = j.neighbours(&mut conn, &late, &t).await.unwrap();
    assert_eq!(ns.len(), 1);
    assert_eq!(ns[0].story_id, story);
    assert!(ns[0].has_page);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn merge_and_split_keep_centroids_and_a_merged_away_story_is_never_matched() {
    let f = Fixture::migrated().await;
    let p = &f.pool;
    let t = config::v1().unwrap();
    source(p, "wire-a", 2).await;
    let j = joiner(&[
        ("three", axis(0)),
        ("five", axis(1)),
        ("echo of five", axis(1)),
    ]);
    let c3 = cand(p, "wire-a", "three", None, None, t0()).await;
    let c5 = cand(p, "wire-a", "five", None, None, t0()).await;
    entities(p, c3, &[7]).await;
    entities(p, c5, &[7]).await;
    detect(p, &j, &t).await.unwrap();
    let (s3, s5) = (story_of(p, c3).await, story_of(p, c5).await);
    assert_ne!(s3, s5);
    let centroids = |s: i64| async move {
        n(
            p,
            &format!(
                "select count(*) from embeddings where owner_kind = 'story' and owner_id = '{s}'"
            ),
        )
        .await
    };

    // Merge 5 into 3: 5's centroid goes, 3's becomes the mean of both (cosine 0.707 to each axis).
    news_taster::edit::merge(p, s5, s3).await.unwrap();
    assert_eq!((centroids(s5).await, centroids(s3).await), (0, 1));
    let cos_to = |c: i64| {
        sqlx::query_scalar::<_, f64>(
            "select (1 - (s.v <=> c.v))::float8 from embeddings s, embeddings c
              where s.owner_kind = 'story' and s.owner_id = $1::bigint::text
                and c.owner_kind = 'candidate' and c.owner_id = $2::bigint::text",
        )
        .bind(s3)
        .bind(c)
        .fetch_one(p)
    };
    assert!((0.70..0.72).contains(&cos_to(c5).await.unwrap()));

    // A new keyless article exactly like 5's old centroid never joins 5 (closed, redirected): 0.707 to
    // 3's mean is under the 0.72 floor, so it is a new story.
    let x = cand(
        p,
        "wire-a",
        "echo of five",
        None,
        None,
        t0() + TimeDelta::hours(1),
    )
    .await;
    entities(p, x, &[7]).await;
    let r = detect(p, &j, &t).await.unwrap();
    assert_eq!((r.by_similarity, r.grey_zone, r.new_stories), (0, 0, 1));
    assert_ne!(story_of(p, x).await, s5);

    // Split 5's candidate back out of 3: 3's centroid is the first candidate's vector again, and the
    // new story has one of its own.
    let new = news_taster::edit::split(p, s3, &[c5]).await.unwrap();
    assert!(cos_to(c3).await.unwrap() > 0.999);
    assert_eq!(centroids(new).await, 1);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn an_embedding_failure_is_no_neighbours_not_an_error() {
    let f = Fixture::migrated().await;
    let p = &f.pool;
    let t = config::v1().unwrap();
    source(p, "wire-a", 2).await;
    // The lookup embedder has no vector for this text and returns an error.
    cand(p, "wire-a", "unknown", None, None, t0()).await;
    let r = detect(p, &joiner(&[]), &t).await.unwrap();
    assert_eq!((r.candidates, r.new_stories), (1, 1));
    assert_eq!(n(p, "select count(*) from embeddings").await, 0);
}
