//! Postgres integration tests for the CLI's database layer.
//!
//! Every test here is `#[ignore]`d, so plain `cargo test` needs no database. Run them with
//!
//! ```text
//! bun scripts/test-cli.ts integration                          # reads .env, checks the DB first
//! NEWS_TEST_DATABASE_URL=postgres://... cargo test -p news --test postgres -- --ignored
//! ```
//!
//! `bun run test:integration` runs the first line too, after the Bun workspaces' integration tests.
//!
//! The database is the pgvector container from `infra/docker-compose.yml` (`bun run infra:up`) or any
//! Postgres 17 with the `vector`, `pg_trgm` and `unaccent` extensions installed. If it cannot be
//! reached the tests fail with a one-line message; they never skip silently.
//!
//! Isolation: each test creates its own schema, puts it first on `search_path`, and drops it when the
//! test ends, including when the test panics. Nothing is created in `public`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

// The news schema, Dispatch and the CLI against a real database. Async tests use sqlx through
// `news_store`; each makes and drops its own schema (see `fixture`).
#[path = "postgres/board.rs"]
mod board;
#[path = "postgres/chief.rs"]
mod chief;
#[path = "postgres/cli_db.rs"]
mod cli_db;
#[path = "postgres/diary_roster.rs"]
mod diary_roster;
#[path = "postgres/dispatch.rs"]
mod dispatch;
#[path = "postgres/fixture.rs"]
mod fixture;
#[path = "postgres/migrations.rs"]
mod migrations;
#[path = "postgres/sources.rs"]
mod sources;
#[path = "postgres/wire.rs"]
mod wire;

use postgres::config::Host;
use postgres::{Client, Config, NoTls};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const URL_VAR: &str = "NEWS_TEST_DATABASE_URL";
const EXTENSIONS_SQL: &str = include_str!("../../../../infra/postgres/init/001_extensions.sql");
const REQUIRED_EXTENSIONS: [&str; 3] = ["vector", "pg_trgm", "unaccent"];

/// Connect as the test database user. Panics with a one-line, secret-free message when it cannot.
fn connect() -> Client {
    let url = std::env::var(URL_VAR).unwrap_or_else(|_| {
        panic!("{URL_VAR} is not set: run `bun run infra:up`, then `bun run test:integration` (CI sets it)")
    });
    let mut config: Config = url
        .parse()
        .unwrap_or_else(|_| panic!("{URL_VAR} is not a valid postgres:// URL (value not shown)"));
    config.connect_timeout(Duration::from_secs(5));
    // The URL carries the password, so only the host and port are ever printed.
    let host = match (config.get_hosts().first(), config.get_ports().first()) {
        (Some(Host::Tcp(name)), Some(port)) => format!("{name}:{port}"),
        (Some(other), _) => format!("{other:?}"),
        (None, _) => "no host".to_owned(),
    };
    config.connect(NoTls).unwrap_or_else(|e| {
        panic!("Postgres is not reachable at {host} ({e}): run `bun run infra:up`")
    })
}

/// A throwaway schema on a dedicated connection, dropped (with everything in it) on `Drop`.
struct TestDb {
    client: Client,
    schema: String,
}

impl TestDb {
    fn new() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let schema = format!(
            "news_test_{}_{}_{nanos}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let mut client = connect();
        client
            .batch_execute(&format!(
                "CREATE SCHEMA {schema}; SET search_path TO {schema}, public"
            ))
            .expect("create the test schema");
        Self { client, schema }
    }

    fn scalar(&mut self, sql: &str) -> String {
        let row = self
            .client
            .query_one(sql, &[])
            .unwrap_or_else(|e| panic!("{sql}: {e}"));
        row.get::<_, Option<String>>(0)
            .unwrap_or_else(|| "<NULL>".to_owned())
    }

    fn error_of(&mut self, sql: &str) -> String {
        match self.client.batch_execute(sql) {
            Ok(()) => panic!("expected an error from: {sql}"),
            Err(e) => e
                .as_db_error()
                .map_or_else(|| e.to_string(), |db| db.message().to_owned()),
        }
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        // Best effort: a failed drop must not mask the test's own failure.
        let _ = self
            .client
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {} CASCADE", self.schema));
    }
}

fn schema_exists(client: &mut Client, schema: &str) -> bool {
    client
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = $1)",
            &[&schema],
        )
        .unwrap()
        .get(0)
}

// --- Connection and provisioning -----------------------------------------------------------

#[test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
fn connects_to_postgres_17_or_newer() {
    let mut db = TestDb::new();
    assert_eq!(db.scalar("SELECT 1::text"), "1");
    let version: i32 = db
        .scalar("SELECT current_setting('server_version_num')")
        .parse()
        .unwrap();
    assert!(
        version >= 170_000,
        "expected Postgres 17+, server_version_num = {version}"
    );
}

#[test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
fn required_extensions_are_installed() {
    let mut client = connect();
    let installed: Vec<String> = client
        .query("SELECT extname::text FROM pg_extension", &[])
        .unwrap()
        .iter()
        .map(|row| row.get(0))
        .collect();
    let missing: Vec<&str> = REQUIRED_EXTENSIONS
        .iter()
        .copied()
        .filter(|e| !installed.iter().any(|i| i == e))
        .collect();
    assert!(
        missing.is_empty(),
        "missing extensions {missing:?}; run infra/postgres/init/001_extensions.sql against the test database"
    );
}

#[test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
fn extension_init_script_is_idempotent() {
    // The script runs once when the container volume is created, and CI replays it against a bare
    // service container, so running it again over an already-provisioned database must be harmless.
    // Done in a rolled-back transaction so the test never changes the shared database.
    let mut client = connect();
    let mut tx = client.transaction().unwrap();
    tx.batch_execute(EXTENSIONS_SQL).expect("first run");
    tx.batch_execute(EXTENSIONS_SQL).expect("second run");
    for ext in REQUIRED_EXTENSIONS {
        let present: bool = tx
            .query_one(
                "SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = $1)",
                &[&ext],
            )
            .unwrap()
            .get(0);
        assert!(present, "{ext} missing after running the init script");
    }
    tx.rollback().unwrap();
}

// --- The extensions do real work -----------------------------------------------------------

#[test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
fn pgvector_finds_nearest_neighbours_with_an_hnsw_index() {
    let mut db = TestDb::new();
    db.client
        .batch_execute(
            "CREATE TABLE embeddings (id int PRIMARY KEY, v vector(3) NOT NULL);
             INSERT INTO embeddings VALUES (1, '[1,0,0]'), (2, '[0,1,0]'), (3, '[0.9,0.1,0]'), (4, '[0,0,1]');
             CREATE INDEX embeddings_hnsw ON embeddings USING hnsw (v vector_cosine_ops);",
        )
        .unwrap();
    assert_eq!(
        db.scalar("SELECT id::text FROM embeddings ORDER BY v <=> '[1,0,0]' LIMIT 1"),
        "1"
    );
    // Rows 2 and 4 are both orthogonal to the query, so the id breaks the tie.
    assert_eq!(
        db.scalar("SELECT string_agg(id::text, ',' ORDER BY v <=> '[1,0,0]', id) FROM embeddings"),
        "1,3,2,4"
    );
    assert_eq!(
        db.scalar("SELECT ('[1,2,3]'::vector <-> '[1,2,4]'::vector)::text"),
        "1"
    );
    assert_eq!(
        db.scalar("SELECT vector_dims('[1,2,3]'::vector)::text"),
        "3"
    );
}

#[test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
fn pgvector_rejects_wrong_dimensions_and_malformed_vectors() {
    let mut db = TestDb::new();
    db.client
        .batch_execute("CREATE TABLE embeddings (id int, v vector(3))")
        .unwrap();
    assert!(
        db.error_of("INSERT INTO embeddings VALUES (1, '[1,2]')")
            .contains("expected 3 dimensions, not 2")
    );
    assert!(
        db.error_of("INSERT INTO embeddings VALUES (2, '[1,2,3,4]')")
            .contains("expected 3 dimensions, not 4")
    );
    assert!(
        db.error_of("INSERT INTO embeddings VALUES (3, '[1,two,3]')")
            .contains("invalid input syntax for type vector")
    );
    assert!(
        db.error_of("SELECT '[1,NaN,3]'::vector")
            .contains("NaN not allowed in vector")
    );
    assert_eq!(
        db.scalar("SELECT count(*)::text FROM embeddings"),
        "0",
        "failed inserts must leave no rows"
    );
}

#[test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
fn pg_trgm_ranks_fuzzy_entity_names_with_a_gin_index() {
    let mut db = TestDb::new();
    db.client
        .batch_execute(
            "CREATE TABLE entities (id int PRIMARY KEY, name text NOT NULL);
             INSERT INTO entities VALUES (1, 'Apple Inc.'), (2, 'Apple Incorporated'), (3, 'Microsoft'), (4, '');
             CREATE INDEX entities_name_trgm ON entities USING gin (name gin_trgm_ops);",
        )
        .unwrap();
    // A typo still finds the right entities, best match first.
    assert_eq!(
        db.scalar("SELECT string_agg(id::text, ',' ORDER BY similarity(name, 'Aple Inc') DESC, id) FROM entities WHERE name % 'Aple Inc'"),
        "1,2"
    );
    assert_eq!(
        db.scalar("SELECT count(*)::text FROM entities WHERE name % 'zzzzzz'"),
        "0"
    );
    // Edge cases: empty strings match nothing and similarity is symmetric.
    assert_eq!(db.scalar("SELECT similarity('', 'Apple')::text"), "0");
    assert_eq!(
        db.scalar("SELECT (similarity('abc', 'abd') = similarity('abd', 'abc'))::text"),
        "true"
    );
    assert_eq!(db.scalar("SELECT similarity('Apple', 'Apple')::text"), "1");
}

#[test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
fn unaccent_folds_diacritics_and_handles_edge_cases() {
    let mut db = TestDb::new();
    assert_eq!(
        db.scalar("SELECT unaccent('Café Zürich São Paulo Nestlé')"),
        "Cafe Zurich Sao Paulo Nestle"
    );
    assert_eq!(
        db.scalar("SELECT unaccent('plain ascii 123')"),
        "plain ascii 123"
    );
    assert_eq!(db.scalar("SELECT unaccent('')"), "");
    assert_eq!(db.scalar("SELECT unaccent(NULL::text)"), "<NULL>");
    // Accent-insensitive entity matching: what the de-duplication step relies on.
    assert_eq!(
        db.scalar("SELECT similarity(unaccent('Nestlé'), 'Nestle')::text"),
        "1"
    );
}

// --- Test isolation ------------------------------------------------------------------------

#[test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
fn objects_land_in_the_test_schema_not_public() {
    let mut db = TestDb::new();
    let schema = db.schema.clone();
    // Compare public before and after rather than assuming it has no `probe`: another suite or a
    // developer's own table must not fail this test, and only this test's schema is inspected.
    // Compare OIDs, not regclass text: the text form gains a `public.` prefix once our own
    // `probe` shadows it in search_path, even though public.probe itself is unchanged.
    let public_before = db.scalar("SELECT to_regclass('public.probe')::oid::text");
    db.client
        .batch_execute("CREATE TABLE probe (id int)")
        .unwrap();
    assert_eq!(
        db.scalar(&format!(
            "SELECT count(*)::text FROM information_schema.tables \
             WHERE table_schema = '{schema}' AND table_name = 'probe'"
        )),
        "1"
    );
    assert_eq!(
        db.scalar("SELECT to_regclass('public.probe')::oid::text"),
        public_before
    );
}

#[test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
fn schemas_are_isolated_from_each_other() {
    let mut a = TestDb::new();
    let mut b = TestDb::new();
    assert_ne!(a.schema, b.schema);
    a.client
        .batch_execute("CREATE TABLE t (n int); INSERT INTO t VALUES (1), (2)")
        .unwrap();
    // The same table name in the other schema is independent, and is invisible until created.
    // Schema-qualified: an unqualified 't' could resolve to a `public.t` through search_path.
    let b_t = format!("SELECT to_regclass('{}.t')::text", b.schema);
    assert_eq!(b.scalar(&b_t), "<NULL>");
    b.client
        .batch_execute("CREATE TABLE t (n int); INSERT INTO t VALUES (10)")
        .unwrap();
    assert_eq!(a.scalar("SELECT count(*)::text FROM t"), "2");
    assert_eq!(b.scalar("SELECT count(*)::text FROM t"), "1");
}

#[test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
fn schema_is_dropped_with_its_contents_when_the_test_ends() {
    let mut observer = connect();
    let schema = {
        let mut db = TestDb::new();
        db.client
            .batch_execute("CREATE TABLE leftovers (id int)")
            .unwrap();
        assert!(schema_exists(&mut observer, &db.schema));
        db.schema.clone()
    };
    assert!(
        !schema_exists(&mut observer, &schema),
        "schema {schema} was not cleaned up"
    );
}

#[test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
fn schema_is_dropped_even_when_the_test_panics() {
    let mut observer = connect();
    let mut schema = String::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = TestDb::new();
        schema.clone_from(&db.schema);
        panic!("simulated test failure");
    }));
    assert!(result.is_err());
    assert!(!schema.is_empty());
    assert!(
        !schema_exists(&mut observer, &schema),
        "schema {schema} leaked after a panic"
    );
}

#[test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
fn a_failed_statement_does_not_poison_later_ones_or_cleanup() {
    let mut observer = connect();
    let schema = {
        let mut db = TestDb::new();
        assert!(
            db.error_of("SELECT * FROM does_not_exist")
                .contains("does not exist")
        );
        assert_eq!(db.scalar("SELECT 'still usable'"), "still usable");
        db.schema.clone()
    };
    assert!(!schema_exists(&mut observer, &schema));
}
