//! Fixer against a real database: a story's inputs load (points, history as of the story's vintage,
//! text blocks, entities, related pages), the brief builds from them, `save` writes the files and
//! the derived points once, and `news related` lists the page that shares the story's entity.

use super::fixture::Fixture;
use super::taster::{doc, source, t0};
use serde_json::Value;

#[tokio::test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn brief_loads_builds_and_saves() {
    let f = Fixture::migrated().await;
    let p = &f.pool;
    source(p, "fred", 1).await;
    // 30 months of a percent series; the latest month is the lowest since the first.
    let mut ids = Vec::new();
    for m in 0..30u32 {
        let (y, mo) = (2024 + i32::try_from(m / 12).unwrap(), m % 12 + 1);
        let start = chrono::NaiveDate::from_ymd_opt(y, mo, 1).unwrap();
        let end = start
            .checked_add_months(chrono::Months::new(1))
            .unwrap()
            .pred_opt()
            .unwrap();
        let value = if m == 29 {
            "2.0".to_owned()
        } else {
            format!("{}.{}", 2 + m % 3, m % 10)
        };
        let id = format!("dp_t{m:02}");
        sqlx::query(
            "insert into data_points (dp_id, context_key, series_key, concept, label, value_text, value, unit,
               period_label, period_start, period_end, period_type, vintage, snapshot_sha, locator, parser,
               retrieved_at)
             values ($1, '', 'fred:T', 'c', 'Test rate', $2, $2::numeric, 'Percent', $3, $4, $5, 'duration',
               $6, sha256('fred'::bytea), '{}'::jsonb, 'test@1', now())",
        )
        .bind(&id)
        .bind(&value)
        .bind(start.format("%B %Y").to_string())
        .bind(start)
        .bind(end)
        .bind(t0())
        .execute(p)
        .await
        .unwrap();
        ids.push(id);
    }
    let entity: i64 = sqlx::query_scalar(
        "insert into entities (kind, name) values ('country', 'United States') returning entity_id",
    )
    .fetch_one(p)
    .await
    .unwrap();
    let story = |key: &'static str| {
        sqlx::query_scalar::<_, i64>(
            "insert into stories (event_key, title, first_seen, last_seen, entity_ids)
             values ($1, 'Source headline', $2, $2, $3) returning story_id",
        )
        .bind(key)
        .bind(t0())
        .bind(vec![entity])
    };
    let s = story("macro:us.t:2026-06").fetch_one(p).await.unwrap();
    let other = story("macro:us.t:2026-05").fetch_one(p).await.unwrap();
    sqlx::query(
        "insert into pages (page_id, slug, story_id, page_type, version, status, published_at)
         values ('2026/06/us-t-may-2026', 'us-t-may-2026', $1, 'macro', 1, 'published', $2)",
    )
    .bind(other)
    .bind(t0())
    .execute(p)
    .await
    .unwrap();
    let d = doc(p, "fred", "The rate fell. Officials said “it fell”.", "t").await;
    sqlx::query(
        "insert into text_blocks (doc_id, block_no, kind, text) values
           ($1, 0, 'heading', 'Test release'), ($1, 1, 'paragraph', 'The rate fell. Officials said “it fell”.')",
    )
    .bind(d)
    .execute(p)
    .await
    .unwrap();
    sqlx::query(
        "insert into candidates (source_id, url, title, event_key, story_id, doc_ids, dp_count, dp_ids,
           page_type, page_type_by, categories, region, first_seen, status)
         values ('fred', 'https://x.test/t', 't', 'macro:us.t:2026-06', $1, $2, 1, $3, 'macro', 'rule',
           '{economy,inflation}', 'us', $4, 'grouped')",
    )
    .bind(s)
    .bind(vec![d])
    .bind(vec![ids[29].clone()])
    .bind(t0())
    .execute(p)
    .await
    .unwrap();

    let inputs = news_fixer::db::load(p, "macro:us.t:2026-06").await.unwrap();
    assert_eq!(inputs.story_id, s);
    assert_eq!(inputs.page_type, "macro-release");
    assert_eq!(inputs.points.len(), 1);
    assert_eq!(inputs.history.len(), 30);
    assert_eq!(inputs.documents[0].blocks.len(), 2);
    assert_eq!(
        inputs.related[0].title, "us-t-may-2026",
        "our slug, not the source's headline"
    );
    let built = news_fixer::brief(&inputs).unwrap();
    let b = &built.brief;
    assert_eq!(b.facts[0].reference, ids[29]);
    assert!(
        b.facts
            .iter()
            .any(|f| f.derived.as_deref() == Some("pp_change"))
    );
    assert!(b.context.iter().any(|c| c.rule.starts_with("series_")));
    assert!(!b.quotes.is_empty());

    let work = std::env::temp_dir().join(format!("news-fixer-{}", f.schema));
    let saved = news_fixer::db::save(p, &work, &built).await.unwrap();
    let again = news_fixer::db::save(p, &work, &built).await.unwrap();
    assert_eq!(saved, again);
    let written: Value = serde_json::from_slice(&std::fs::read(&saved.brief).unwrap()).unwrap();
    assert_eq!(written, serde_json::to_value(b).unwrap());
    let derived: i64 = sqlx::query_scalar("select count(*) from derived_points")
        .fetch_one(p)
        .await
        .unwrap();
    assert_eq!(derived as usize, built.refs.derived.len());
    assert!(saved.condense_job.is_none());
    std::fs::remove_dir_all(&work).unwrap();

    let related = news_fixer::db::related(p, &s.to_string()).await.unwrap();
    assert_eq!(related["items"][0]["id"], "2026/06/us-t-may-2026");
    assert_eq!(related["items"][0]["kind"], "page");
}
