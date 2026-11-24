//! Ghost's PostgreSQL side (#18): `ai_calls` rows and the limits read before each call.

use super::fixture::Fixture;
use news_ghost::store::{AiCall, PgStore, Store};
use uuid::Uuid;

fn call(run: Option<Uuid>, job_id: i64, tokens: u64) -> AiCall {
    AiCall {
        ts: chrono::Utc::now(),
        run_id: run,
        job_id,
        step: "A1".into(),
        model: "sonnet".into(),
        prompt_version: "macro-release.3f9a1c.md".into(),
        tokens_in: tokens,
        tokens_out: 0,
        cache_read: 9000,
        cache_write_5m: 0,
        cache_write_1h: 0,
        turns: 1,
        duration_ms: 1234,
        subtype: "success".into(),
        validation: "valid".into(),
        repair_round: 0,
        est_cost_usd: 0.05,
        tokens,
        input_file: "work/briefs/1.json".into(),
        output_file: Some("1.1.0.out.json".into()),
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn calls_are_recorded_and_count_against_their_run() {
    let f = Fixture::migrated().await;
    let store = PgStore {
        db: f.pool.clone(),
        clickhouse: None,
    };
    let (run, other) = (Uuid::new_v4(), Uuid::new_v4());
    store.record(&call(Some(run), 1, 1500)).await;
    store.record(&call(Some(run), 2, 2500)).await;
    store.record(&call(Some(other), 3, 9999)).await;
    store.record(&call(None, 4, 7777)).await;
    let l = store.limits(Some(run), "A1").await.unwrap();
    assert_eq!(l.spent, 4000);
    assert_eq!(l.ceiling, Some(2_500_000));
    assert_eq!(l.model, "sonnet");
    let light = store.limits(None, "A3").await.unwrap();
    assert_eq!((light.spent, light.model.as_str()), (0, "haiku"));
    assert_eq!(f.scalar_i64("select count(*) from ai_calls").await, 4);
    // A re-run of the migration is a no-op.
    news_store::migrate(&f.pool, &f.schema).await.unwrap();
}
