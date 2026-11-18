//! The binary against a real database: `news db migrate|status` and `news jobs list|claim`.

use super::fixture::Fixture;
use news_dispatch::queue::{self, NewJob};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn db_migrate_is_idempotent_from_the_cli() {
    let f = Fixture::new().await;
    let (env, code, _, _) = f.call(&["db", "status"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["up_to_date"], false);

    let (env, code, _, _) = f.call(&["db", "migrate"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(
        env["data"]["applied"],
        serde_json::json!(news_store::known_versions())
    );
    let (env, code, _, _) = f.call(&["db", "migrate"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["applied"], serde_json::json!([]));
    let (env, _, _, _) = f.call(&["db", "status"]);
    assert_eq!(env["data"]["up_to_date"], true);
    assert_eq!(env["data"]["pending"], serde_json::json!([]));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn db_migrate_sets_the_ai_pool_from_news_parallel() {
    let f = Fixture::new().await;
    let mut cmd = f.news();
    cmd.env("NEWS_PARALLEL", "5");
    let (env, code, _, _) = super::fixture::call(&mut cmd, &["db", "migrate"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["ai_max_running"], 5);
    assert_eq!(
        f.scalar_i64("select max_running::bigint from pools where pool = 'ai'")
            .await,
        5
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn a_command_on_an_unmigrated_schema_says_to_migrate() {
    let f = Fixture::new().await;
    let (env, code, _, _) = f.call(&["jobs", "list"]);
    assert_eq!(code, 4, "{env}");
    assert_eq!(env["error"]["code"], "precondition");
    assert_eq!(env["error"]["hint"], "run `news db migrate`");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn jobs_list_is_deterministic_paged_and_filterable() {
    let f = Fixture::migrated().await;
    for i in 0..25 {
        let pool = if i % 2 == 0 { "net" } else { "cpu" };
        queue::enqueue(&f.pool, &NewJob::new("test.fetch").pool(pool))
            .await
            .unwrap();
    }
    let (env, code, _, _) = f.call(&["jobs", "list"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["truncated"], true);
    let items = env["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 20);
    let ids: Vec<i64> = items
        .iter()
        .map(|j| j["job_id"].as_i64().unwrap())
        .collect();
    assert!(ids.windows(2).all(|w| w[0] > w[1]), "newest first: {ids:?}");
    assert_eq!(
        env["data"]["counts"],
        serde_json::json!([{"pool":"cpu","state":"available","count":12},{"pool":"net","state":"available","count":13}])
    );
    assert!(
        env["data"]["text"]
            .as_str()
            .unwrap()
            .contains("net      available  13")
    );
    let cursor = env["data"]["next_cursor"].as_str().unwrap().to_owned();
    let (page2, _, _, _) = f.call(&["jobs", "list", "--cursor", &cursor]);
    assert_eq!(page2["data"]["items"].as_array().unwrap().len(), 5);
    assert_eq!(page2["truncated"], false);
    assert_eq!(f.call(&["jobs", "list"]).0, env, "same state, same output");

    let (env, _, _, _) = f.call(&[
        "jobs",
        "list",
        "--pool",
        "cpu",
        "--fields",
        "job_id,state",
        "--limit",
        "2",
    ]);
    assert_eq!(env["data"]["items"][0].as_object().unwrap().len(), 2);
    let (env, code, _, _) = f.call(&["jobs", "list", "--state", "finished"]);
    assert_eq!((code, env["error"]["code"].as_str()), (2, Some("usage")));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn jobs_claim_returns_args_and_respects_limit() {
    let f = Fixture::migrated().await;
    for i in 0..4 {
        queue::enqueue(
            &f.pool,
            &NewJob::new("ai.write")
                .args(serde_json::json!({"input_path": format!("work/briefs/{i}.json")})),
        )
        .await
        .unwrap();
    }
    let (env, code, _, _) = f.call(&[
        "jobs", "claim", "--pool", "ai", "--worker", "ghost", "--limit", "2",
    ]);
    assert_eq!(code, 0, "{env}");
    let items = env["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["input_path"], "work/briefs/0.json");
    assert_eq!(items[0]["attempt"], 1);
    // ai's max_running is 3: one more fits, then nothing.
    let (env, _, _, _) = f.call(&[
        "jobs", "claim", "--pool", "ai", "--worker", "ghost", "--limit", "5",
    ]);
    assert_eq!(env["data"]["items"].as_array().unwrap().len(), 1);
    let (env, _, _, _) = f.call(&["jobs", "claim", "--pool", "ai", "--worker", "ghost"]);
    assert_eq!(env["data"]["items"].as_array().unwrap().len(), 0);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn a_missing_schema_is_refused_instead_of_falling_back_to_public() {
    let f = Fixture::new().await;
    // PostgreSQL skips a missing schema on the search_path, so queries would reach `public`.
    sqlx::query(sqlx::AssertSqlSafe(format!("drop schema {}", f.schema)))
        .execute(&f.pool)
        .await
        .unwrap();
    for args in [vec!["jobs", "list"], vec!["jobs", "retry", "1"]] {
        let (env, code, _, _) = f.call(&args);
        assert_eq!(code, 4, "{args:?}: {env}");
        assert_eq!(env["error"]["code"], "precondition", "{args:?}");
        assert_eq!(
            env["error"]["message"],
            format!("schema {:?} does not exist", f.schema),
            "{args:?}"
        );
        assert_eq!(env["error"]["hint"], "run `news db migrate`", "{args:?}");
    }
    let (env, code, _, _) = f.call(&["db", "status"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["applied"], serde_json::json!([]));
    assert_eq!(env["data"]["up_to_date"], false);
    // `db migrate` still creates it.
    let (env, code, _, _) = f.call(&["db", "migrate"]);
    assert_eq!(code, 0, "{env}");
    let (env, code, _, _) = f.call(&["jobs", "list"]);
    assert_eq!(code, 0, "{env}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn a_schema_named_like_a_keyword_migrates() {
    // `select` passes valid_schema; the statements must quote it. The name is fixed, so the test
    // refuses to touch a `select` schema it did not create, and drops only the one it created.
    let cfg = news_store::DbConfig::new(super::fixture::url(), "select").unwrap();
    let pool = news_store::connect(&cfg, 2).await.unwrap();
    assert!(
        !news_store::schema_exists(&pool, "select").await.unwrap(),
        "schema \"select\" already exists in the test database; this test creates and drops it,          so it will not touch one it did not create (drop it by hand if an earlier run left it)"
    );
    let migrated = news_store::migrate(&pool, "select").await;
    let pools: Result<i64, _> = sqlx::query_scalar(r#"select count(*) from "select".pools"#)
        .fetch_one(&pool)
        .await;
    if news_store::schema_exists(&pool, "select").await.unwrap() {
        sqlx::query(r#"drop schema "select" cascade"#)
            .execute(&pool)
            .await
            .unwrap();
    }
    assert_eq!(migrated.unwrap().schema, "select");
    assert_eq!(pools.unwrap(), 6);
}

/// Drops the decoy `public.jobs` when the test ends, panicking or not.
struct DropPublicJobs;

impl Drop for DropPublicJobs {
    fn drop(&mut self) {
        let _ = std::thread::spawn(|| {
            let _ = super::connect().batch_execute("drop table if exists public.jobs");
        })
        .join();
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn an_existing_but_unmigrated_schema_never_falls_back_to_public() {
    let source = Fixture::migrated().await;
    let target = Fixture::new().await;
    let mut lock = source.pool.acquire().await.unwrap();
    sqlx::query("select pg_advisory_lock($1)")
        .bind(super::fixture::PUBLIC_LOCK)
        .execute(&mut *lock)
        .await
        .unwrap();
    let in_public: bool = sqlx::query_scalar("select to_regclass('public.jobs') is not null")
        .fetch_one(&mut *lock)
        .await
        .unwrap();
    assert!(
        !in_public,
        "public.jobs already exists in the test database; this test creates and drops its own"
    );
    let _cleanup = DropPublicJobs;
    // A populated public.jobs, and a target schema that exists but has no tables.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "create table public.jobs (like {}.jobs including defaults including constraints);
         insert into public.jobs (job_id, kind, pool, state, error_class, finished_at)
         values (1, 'doc.fetch', 'net', 'discarded', 'parse_error', now());",
        source.schema
    )))
    .execute(&mut *lock)
    .await
    .unwrap();
    for args in [vec!["jobs", "retry", "1"], vec!["jobs", "list"]] {
        let (env, code, _, _) = target.call(&args);
        assert_eq!(code, 4, "{args:?}: {env}");
        assert_eq!(env["error"]["code"], "precondition", "{args:?}");
        assert_eq!(env["error"]["hint"], "run `news db migrate`", "{args:?}");
    }
    let state: String = sqlx::query_scalar("select state from public.jobs where job_id = 1")
        .fetch_one(&mut *lock)
        .await
        .unwrap();
    assert_eq!(state, "discarded", "jobs retry changed public.jobs");
    // `db migrate` still initialises the target, and then it is used.
    let (env, code, _, _) = target.call(&["db", "migrate"]);
    assert_eq!(code, 0, "{env}");
    let (env, code, _, _) = target.call(&["jobs", "retry", "1"]);
    assert_eq!(
        (code, env["error"]["code"].as_str()),
        (3, Some("not_found")),
        "{env}"
    );
    drop(_cleanup);
    sqlx::query("select pg_advisory_unlock($1)")
        .bind(super::fixture::PUBLIC_LOCK)
        .execute(&mut *lock)
        .await
        .unwrap();
}
