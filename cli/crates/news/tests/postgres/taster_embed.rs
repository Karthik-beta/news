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
use news_taster::embed::{DIM, EmbedJoiner, Embedder, MODEL};
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

fn lit(v: &[f32]) -> String {
    let parts: Vec<String> = v.iter().map(f32::to_string).collect();
    format!("[{}]", parts.join(","))
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
async fn a_cosine_never_joins_two_event_keys_and_a_story_with_a_page_is_flagged() {
    let f = Fixture::migrated().await;
    let p = &f.pool;
    let t = config::v1().unwrap();
    source(p, "wire-a", 2).await;
    // A keyed story, and a stored centroid for it (a keyed candidate is not embedded itself).
    cand(p, "wire-a", "ECB holds", Some("ecb:2026-10"), None, t0()).await;
    detect(p, &joiner(&[]), &t).await.unwrap();
    let story = n(p, "select story_id from stories").await;
    sqlx::query("insert into embeddings (owner_kind, owner_id, model, v) values ('story', $1::bigint::text, $2, $3::text::halfvec)")
        .bind(story)
        .bind(MODEL)
        .bind(lit(&axis(0)))
        .execute(p)
        .await
        .unwrap();

    let j = joiner(&[("same words", axis(0))]);
    let mut conn = p.acquire().await.unwrap();
    let probe = |event_key: Option<&str>, id: i64| Cand {
        cand_id: id,
        title: Some("same words".into()),
        event_key: event_key.map(str::to_owned),
        entity_ids: vec![],
        first_seen: t0() + TimeDelta::hours(1),
        doc: None,
    };
    let keyless = cand(
        p,
        "wire-a",
        "same words",
        None,
        None,
        t0() + TimeDelta::hours(1),
    )
    .await;
    let keyed = cand(
        p,
        "wire-a",
        "same words",
        Some("ecb:2026-11"),
        None,
        t0() + TimeDelta::hours(1),
    )
    .await;

    // Identical vectors: with no key on the candidate the keyed story is a neighbour; with a key of
    // its own it is not (only key equality joins those).
    let ns = j
        .neighbours(&mut conn, &probe(None, keyless), &t)
        .await
        .unwrap();
    assert_eq!(ns.len(), 1);
    assert_eq!(ns[0].story_id, story);
    assert!(ns[0].cos > 0.999, "{}", ns[0].cos);
    assert!(!ns[0].shared_entity && !ns[0].has_page);
    let ns = j
        .neighbours(&mut conn, &probe(Some("ecb:2026-11"), keyed), &t)
        .await
        .unwrap();
    assert!(ns.is_empty());

    // Its page makes it a story that needs the update threshold (0.85); it stays a neighbour 10 days on.
    publish_page(p, story).await;
    let mut late = probe(None, keyless);
    late.first_seen = t0() + TimeDelta::days(10);
    let ns = j.neighbours(&mut conn, &late, &t).await.unwrap();
    assert_eq!(ns.len(), 1);
    assert!(ns[0].has_page);
}
