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

    /// Run the binary: (envelope, exit code, stdout, stderr). A successful envelope's `data` is
    /// validated against the command's `$defs.output` contract.
    pub fn call(&self, args: &[&str]) -> (serde_json::Value, i32, String, String) {
        call(&mut self.news(), args)
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

/// Run `cmd` with `args`: (envelope, exit code, stdout, stderr). Checks that stdout is one envelope
/// line and, when it is `ok`, that `data` matches the command's output contract.
pub fn call(
    cmd: &mut assert_cmd::Command,
    args: &[&str],
) -> (serde_json::Value, i32, String, String) {
    let out = cmd.args(args).output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert_eq!(
        stdout.matches('\n').count(),
        1,
        "stdout is not one line: {stdout:?} / {stderr}"
    );
    let env: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("{e}: {stdout}"));
    check_output(args, &env);
    (env, out.status.code().unwrap(), stdout, stderr)
}

/// The command a call ran, as the dotted name of its contract file (`jobs.claim`): the longest run of
/// leading words that names a file in `contracts/cli/commands/`.
fn contract_of(args: &[&str]) -> Option<(String, std::path::PathBuf)> {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../contracts/cli/commands");
    let words: Vec<&str> = args
        .iter()
        .copied()
        .take_while(|a| !a.starts_with('-'))
        .collect();
    (1..=words.len()).rev().find_map(|n| {
        let name = words[..n].join(".");
        let path = dir.join(format!("{name}.schema.json"));
        path.is_file().then_some((name, path))
    })
}

/// Validate a successful envelope's `data` against `$defs.output` of the command's contract. Skipped
/// with `--fields`, which removes keys the contract requires by design.
pub fn check_output(args: &[&str], env: &serde_json::Value) {
    if env["ok"] != true || args.contains(&"--fields") {
        return;
    }
    let (name, path) =
        contract_of(args).unwrap_or_else(|| panic!("no output contract for {args:?}"));
    let mut schema: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    schema["$ref"] = serde_json::json!("#/$defs/output");
    let validator = jsonschema::draft202012::new(&schema)
        .unwrap_or_else(|e| panic!("{name}: the contract does not compile: {e}"));
    let errors: Vec<String> = validator
        .iter_errors(&env["data"])
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect();
    assert!(
        errors.is_empty(),
        "{name}: data breaks $defs.output: {errors:?}
{env}"
    );
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
