//! A throwaway schema for async tests: created on `new`, migrated on `migrated`, dropped on `Drop`
//! (from a plain thread, so it works inside or outside a Tokio runtime, and after a panic).

use news_store::{DbConfig, PgPool};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn url() -> String {
    std::env::var(super::URL_VAR).unwrap_or_else(|_| {
        panic!(
            "{} is not set: run `bun run infra:up`, then `bun run test:integration` (CI sets it)",
            super::URL_VAR
        )
    })
}

pub struct Fixture {
    pub schema: String,
    pub cfg: DbConfig,
    pub pool: PgPool,
}

fn unique_schema() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!(
        "news_it_{}_{}_{nanos}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

impl Fixture {
    /// An empty schema (search_path = schema, public). Nothing is migrated.
    pub async fn new() -> Self {
        let schema = unique_schema();
        let cfg = DbConfig::new(url(), schema.clone()).unwrap();
        let pool = news_store::connect(&cfg, 10)
            .await
            .unwrap_or_else(|e| panic!("Postgres is not reachable: {e}"));
        sqlx::query(sqlx::AssertSqlSafe(format!("create schema {schema}")))
            .execute(&pool)
            .await
            .unwrap();
        Self { schema, cfg, pool }
    }

    /// A schema with every migration applied.
    pub async fn migrated() -> Self {
        let f = Self::new().await;
        news_store::migrate(&f.pool, &f.schema).await.unwrap();
        f
    }

    /// Another pool on the same schema (one per simulated worker).
    pub async fn pool(&self, max: u32) -> PgPool {
        news_store::connect(&self.cfg, max).await.unwrap()
    }

    /// The `news` binary pointed at this schema.
    pub fn news(&self) -> assert_cmd::Command {
        let mut cmd = assert_cmd::cargo_bin_cmd!("news");
        cmd.env("DATABASE_URL", url())
            .env("NEWS_DB_SCHEMA", &self.schema)
            .env_remove("NEWS_CH_HTTP")
            .env_remove("NEWS_OTEL_HTTP")
            .env_remove("NEWS_LOG")
            .env_remove("CLAUDECODE")
            .env_remove("NEWS_WRITE_API_KEY")
            .env("NO_COLOR", "1");
        cmd
    }

    /// Run the binary: (envelope, exit code, stdout, stderr).
    pub fn call(&self, args: &[&str]) -> (serde_json::Value, i32, String, String) {
        call(self.news().args(args))
    }

    pub async fn scalar_i64(&self, sql: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.to_owned()))
            .fetch_one(&self.pool)
            .await
            .unwrap_or_else(|e| panic!("{sql}: {e}"))
    }

    pub async fn scalar_str(&self, sql: &str) -> String {
        sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql.to_owned()))
            .fetch_one(&self.pool)
            .await
            .unwrap_or_else(|e| panic!("{sql}: {e}"))
    }
}

pub fn call(cmd: &mut assert_cmd::Command) -> (serde_json::Value, i32, String, String) {
    let out = cmd.output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert_eq!(
        stdout.matches('\n').count(),
        1,
        "stdout is not one line: {stdout:?} / {stderr}"
    );
    let env = serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("{e}: {stdout}"));
    (env, out.status.code().unwrap(), stdout, stderr)
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let schema = self.schema.clone();
        // The sync client cannot run on a Tokio worker thread; a plain thread can.
        let _ = std::thread::spawn(move || {
            let mut c = super::connect();
            let _ = c.batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"));
        })
        .join();
    }
}
