//! Migrations 0001–0099: every TD and Bureau §14 table, the named indexes and constraints, applied to
//! an empty schema; re-running is a no-op.

use super::fixture::Fixture;

/// Every table of the TD data model and Bureau §14.
const TABLES: &[&str] = &[
    // TD
    "sources",
    "candidates",
    "clusters",
    "queue",
    "ledger",
    "entities",
    "edges",
    "embeddings",
    "taxonomy",
    "page_categories",
    "data_points",
    "home_curations",
    // Bureau §14
    "snapshots",
    "fetch_state",
    "calendar_events",
    "documents",
    "text_blocks",
    "derived_points",
    "entity_ids",
    "entity_aliases",
    "stories",
    "scores",
    "run_plans",
    "jobs",
    "pools",
    "runs",
    "run_steps",
    "approvals",
    "publish_outbox",
    "pages",
    "page_refs",
    "event_weights",
    "schedules",
    "traffic_daily",
    "start_tickets",
];

/// Everything that defines the schema, as one sorted text: tables, columns, indexes and constraints.
async fn fingerprint(f: &Fixture) -> String {
    let rows: Vec<(String,)> = sqlx::query_as(
        "select x from (
           select 'col ' || table_name || '.' || column_name || ' ' || data_type || ' ' || is_nullable
                  || ' ' || coalesce(column_default, '') as x
             from information_schema.columns where table_schema = current_schema()
           union all
           select 'idx ' || indexname || ' ' || indexdef from pg_indexes where schemaname = current_schema()
           union all
           select 'con ' || conname || ' ' || pg_get_constraintdef(c.oid)
             from pg_constraint c join pg_namespace n on n.oid = c.connamespace
            where n.nspname = current_schema()
         ) s order by x",
    )
    .fetch_all(&f.pool)
    .await
    .unwrap();
    rows.into_iter()
        .map(|(x,)| x)
        .collect::<Vec<_>>()
        .join("\n")
}

async fn table_count(f: &Fixture, table: &str) -> i64 {
    f.scalar_i64(&format!("select count(*) from {table}")).await
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn migrations_apply_to_an_empty_schema_and_rerunning_is_a_no_op() {
    let f = Fixture::new().await;
    assert!(
        news_store::applied_versions(&f.pool)
            .await
            .unwrap()
            .is_empty()
    );

    let first = news_store::migrate(&f.pool, &f.schema).await.unwrap();
    assert_eq!(first.applied, news_store::known_versions());
    assert_eq!(first.current, news_store::known_versions());
    for t in TABLES {
        let exists: bool = sqlx::query_scalar(
            "select exists (select 1 from information_schema.tables where table_schema = current_schema() and table_name = $1)",
        )
        .bind(t)
        .fetch_one(&f.pool)
        .await
        .unwrap();
        assert!(exists, "table {t} missing");
    }
    let before = fingerprint(&f).await;
    let seeded = (
        table_count(&f, "pools").await,
        table_count(&f, "schedules").await,
        table_count(&f, "event_weights").await,
    );

    // Re-running through the migrator applies nothing.
    let second = news_store::migrate(&f.pool, &f.schema).await.unwrap();
    assert!(second.applied.is_empty(), "{:?}", second.applied);
    assert_eq!(fingerprint(&f).await, before);

    // Each file is idempotent on its own too: executing it again changes nothing, seeds included.
    for m in news_store::MIGRATOR.iter() {
        sqlx::raw_sql(sqlx::AssertSqlSafe(m.sql.as_str().to_owned()))
            .execute(&f.pool)
            .await
            .unwrap_or_else(|e| panic!("re-running {} {}: {e}", m.version, m.description));
    }
    assert_eq!(fingerprint(&f).await, before);
    assert_eq!(
        seeded,
        (
            table_count(&f, "pools").await,
            table_count(&f, "schedules").await,
            table_count(&f, "event_weights").await
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn nothing_lands_in_public() {
    let f = Fixture::migrated().await;
    // Shared: waits while the fallback test has its decoy `public.jobs`.
    let mut lock = f.pool.acquire().await.unwrap();
    sqlx::query("select pg_advisory_lock_shared($1)")
        .bind(super::fixture::PUBLIC_LOCK)
        .execute(&mut *lock)
        .await
        .unwrap();
    let in_public: i64 = sqlx::query_scalar(
        "select count(*) from information_schema.tables where table_schema = 'public' and table_name = any($1)",
    )
    .bind(TABLES)
    .fetch_one(&mut *lock)
    .await
    .unwrap();
    sqlx::query("select pg_advisory_unlock_shared($1)")
        .bind(super::fixture::PUBLIC_LOCK)
        .execute(&mut *lock)
        .await
        .unwrap();
    assert_eq!(in_public, 0);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn named_indexes_exist_with_their_definitions() {
    let f = Fixture::migrated().await;
    let def = |name: &'static str| {
        let pool = f.pool.clone();
        async move {
            sqlx::query_scalar::<_, String>("select indexdef from pg_indexes where schemaname = current_schema() and indexname = $1")
                .bind(name)
                .fetch_optional(&pool)
                .await
                .unwrap()
                .unwrap_or_else(|| panic!("index {name} missing"))
        }
    };
    let unique_live = def("jobs_unique_live").await;
    assert!(
        unique_live.starts_with("CREATE UNIQUE INDEX"),
        "{unique_live}"
    );
    assert!(
        unique_live.contains("(unique_key)") && unique_live.contains("WHERE"),
        "{unique_live}"
    );
    let ready = def("jobs_ready").await;
    assert!(
        ready.contains("(pool, priority, run_after)") && ready.contains("WHERE"),
        "{ready}"
    );
    assert!(def("runs_one_per_preset").await.contains("UNIQUE"));
    assert!(def("runs_one_heavy").await.contains("UNIQUE"));
    let hnsw = def("embeddings_v_hnsw").await;
    assert!(
        hnsw.contains("USING hnsw") && hnsw.contains("halfvec_cosine_ops"),
        "{hnsw}"
    );
    let trgm = def("entity_aliases_trgm").await;
    assert!(trgm.contains("gin_trgm_ops"), "{trgm}");
}

async fn ticket(f: &Fixture) -> i64 {
    sqlx::query_scalar(
        "insert into start_tickets (preset, args, canonical, prompt, match, state)
         values ('edition', '{}', '/bureau run daily', '/bureau run daily', 'grammar', 'issued') returning ticket_id",
    )
    .fetch_one(&f.pool)
    .await
    .unwrap()
}

async fn insert_run(
    f: &Fixture,
    preset: &str,
    heavy: bool,
    status: &str,
    summary: Option<&str>,
) -> Result<(), String> {
    let t = ticket(f).await;
    sqlx::query(
        "insert into runs (run_id, short_id, preset, args, heavy, origin, ticket_id, status, config, budget,
                           started_at, last_seen_at, summary)
         values (gen_random_uuid(), 'r_' || substr(md5(random()::text), 1, 6), $1, '{}', $2, 'test', $3, $4,
                 '{}', '{}', now(), now(), $5::jsonb)",
    )
    .bind(preset)
    .bind(heavy)
    .bind(t)
    .bind(status)
    .bind(summary)
    .execute(&f.pool)
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn every_finished_run_needs_a_wrap() {
    let f = Fixture::migrated().await;
    for status in ["succeeded", "failed", "cancelled"] {
        let err = insert_run(&f, &format!("p_{status}"), false, status, None)
            .await
            .unwrap_err();
        assert!(err.contains("runs_wrap_required"), "{status}: {err}");
        insert_run(
            &f,
            &format!("p_{status}"),
            false,
            status,
            Some(r#"{"wrap":true}"#),
        )
        .await
        .unwrap();
    }
    insert_run(&f, "p_live", false, "running", None)
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn one_live_run_per_preset_and_one_heavy_run_at_a_time() {
    let f = Fixture::migrated().await;
    insert_run(&f, "edition", true, "running", None)
        .await
        .unwrap();
    let dup = insert_run(&f, "edition", false, "paused", None)
        .await
        .unwrap_err();
    assert!(dup.contains("runs_one_per_preset"), "{dup}");
    let heavy = insert_run(&f, "catchup", true, "waiting_ai", None)
        .await
        .unwrap_err();
    assert!(heavy.contains("runs_one_heavy"), "{heavy}");
    // A heavy run that is waiting for approval or paused does not hold the slot.
    insert_run(&f, "catchup", true, "waiting_approval", None)
        .await
        .unwrap();
    // Light presets are not limited by the heavy slot.
    insert_run(&f, "frontpage", false, "running", None)
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn pools_are_seeded_as_in_the_design() {
    let f = Fixture::migrated().await;
    let pools: Vec<(String, i16)> =
        sqlx::query_as("select pool, max_running from pools order by pool")
            .fetch_all(&f.pool)
            .await
            .unwrap();
    assert_eq!(
        pools,
        [
            ("ai", 3),
            ("cpu", 2),
            ("docling", 1),
            ("net", 6),
            ("publish", 1),
            ("render", 2)
        ]
        .map(|(p, n)| (p.to_owned(), n))
    );
    let enabled: i64 = f
        .scalar_i64("select count(*) from schedules where enabled")
        .await;
    assert_eq!(
        enabled, 0,
        "schedules are seeded disabled until their handlers exist"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn ledger_keeps_values_verbatim_and_refs_are_checked() {
    let f = Fixture::migrated().await;
    sqlx::raw_sql(
        "insert into sources (source_id, name, publisher, connector, base_url, source_tier, licence, attribution,
                              redistribution, image_policy, rate_per_sec, regions, categories)
         values ('bls', 'BLS', 'U.S. Bureau of Labor Statistics', 'bls', 'https://api.bls.gov', 1, 'public-domain',
                 'U.S. Bureau of Labor Statistics', true, 'none', 0.5, '{us}', '{economy}');
         insert into snapshots (sha256, source_id, url, fetched_at, http_status, bytes, storage_path)
         values (decode(repeat('ab', 32), 'hex'), 'bls', 'https://api.bls.gov/x', now(), 200, 10, 'work/raw/x');",
    )
    .execute(&f.pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into data_points (dp_id, context_key, series_key, concept, label, value_text, value, unit,
                                  period_label, period_type, vintage, snapshot_sha, locator, parser, retrieved_at)
         values ('dp_7kq2', '', 'bls:CUUR0000SA0', 'CPI', 'CPI-U, all items', '1,234.50', 1234.50, 'Index 1982-84=100',
                 'October 2026', 'duration', now(), decode(repeat('ab', 32), 'hex'), '{}', 'bls@1', now())",
    )
    .execute(&f.pool)
    .await
    .unwrap();
    let (text, unit): (String, String) =
        sqlx::query_as("select value_text, unit from data_points where dp_id = 'dp_7kq2'")
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(
        (text.as_str(), unit.as_str()),
        ("1,234.50", "Index 1982-84=100")
    );
    // A malformed ref is refused by the schema; a well-formed one is stored.
    sqlx::query("insert into pages (page_id, slug, version, status) values ('p', 'p', 1, 'draft')")
        .execute(&f.pool)
        .await
        .unwrap();
    for (r, ok) in [
        ("dp_7kq2", true),
        ("pd_2026_10", true),
        ("DP_x", false),
        ("dp_", false),
        ("xx_1", false),
    ] {
        let res = sqlx::query("insert into page_refs (page_id, version, ref) values ('p', 1, $1)")
            .bind(r)
            .execute(&f.pool)
            .await;
        assert_eq!(res.is_ok(), ok, "{r}: {res:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn a_page_belongs_to_several_categories() {
    let f = Fixture::migrated().await;
    sqlx::raw_sql(
        "insert into taxonomy (tax_id, slug, name) values (1, 'economy', 'Economy'), (2, 'markets', 'Markets');
         insert into taxonomy (tax_id, parent_id, slug, name) values (3, 1, 'inflation', 'Inflation');
         insert into pages (page_id, slug, version, status) values ('p', 'p', 1, 'draft');
         insert into page_categories (page_id, tax_id) values ('p', 1), ('p', 2), ('p', 3);",
    )
    .execute(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        f.scalar_i64("select count(*) from page_categories where page_id = 'p'")
            .await,
        3
    );
    for bad in [
        "insert into page_categories values ('p', 1)",
        "insert into page_categories values ('p', 99)",
        "insert into page_categories values ('nope', 1)",
    ] {
        assert!(
            sqlx::query(sqlx::AssertSqlSafe(bad.to_owned()))
                .execute(&f.pool)
                .await
                .is_err(),
            "{bad}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn embeddings_take_page_keys_as_well_as_story_and_candidate_ids() {
    let f = Fixture::migrated().await;
    // Pages are keyed by text (`pages.page_id`); stories and candidates by number, stored as text.
    sqlx::raw_sql(
        "insert into embeddings (owner_kind, owner_id, model, v) values
           ('page', '2026/10/us-cpi-september-2026', 'bge-small-en-v1.5@int8', array_fill(0.1::real, array[384])::halfvec(384)),
           ('story', '42', 'bge-small-en-v1.5@int8', array_fill(0.2::real, array[384])::halfvec(384))",
    )
    .execute(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        f.scalar_str("select owner_id from embeddings where owner_kind = 'page'")
            .await,
        "2026/10/us-cpi-september-2026"
    );
    assert!(
        sqlx::query(
            "insert into embeddings (owner_kind, owner_id, model, v)
             values ('snippet', '1', 'm', array_fill(0.1::real, array[384])::halfvec(384))"
        )
        .execute(&f.pool)
        .await
        .is_err()
    );
}
