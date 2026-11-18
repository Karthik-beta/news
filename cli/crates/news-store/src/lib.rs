//! PostgreSQL for the `news` CLI.
//!
//! * One schema per database (`news` by default, `NEWS_DB_SCHEMA` to change it). Every connection sets
//!   `search_path` to `<schema>, public`, so migrations and queries use unqualified names and the
//!   extensions (installed in `public` by `infra/postgres/init`) resolve. Tests point it at a
//!   throwaway schema. PostgreSQL skips a missing schema on the search path, and resolves a table the
//!   schema lacks in `public`, so commands other than `db migrate` and `db status` call
//!   [`require_schema`] first: the schema must exist and hold every migration this binary knows.
//! * Migrations live in `cli/migrations/`, numbered in blocks (cli/README.md) and embedded in the
//!   binary. Each file is idempotent, and sqlx records applied versions, so re-running is a no-op.
//! * Errors are mapped to the stable codes. Messages never contain the connection URL.

use news_core::{ErrorCode, NewsError};
use serde::Serialize;
use sqlx::migrate::Migrator;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::str::FromStr;
use std::time::Duration;

pub use sqlx::PgPool;

pub static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

pub const URL_VAR: &str = "DATABASE_URL";
pub const SCHEMA_VAR: &str = "NEWS_DB_SCHEMA";
pub const DEFAULT_SCHEMA: &str = "news";

#[derive(Clone)]
pub struct DbConfig {
    url: String,
    pub schema: String,
}

impl std::fmt::Debug for DbConfig {
    // The URL holds the password, so it is never printed.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbConfig")
            .field("url", &"<redacted>")
            .field("schema", &self.schema)
            .finish()
    }
}

impl DbConfig {
    pub fn new(url: impl Into<String>, schema: impl Into<String>) -> Result<Self, NewsError> {
        let schema = schema.into();
        if !valid_schema(&schema) {
            return Err(NewsError::usage(format!(
                "{SCHEMA_VAR} must match ^[a-z_][a-z0-9_]*$ (got {schema:?})"
            )));
        }
        Ok(Self {
            url: url.into(),
            schema,
        })
    }

    pub fn from_env() -> Result<Self, NewsError> {
        Self::from_lookup(|n| std::env::var(n).ok())
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, NewsError> {
        let url = lookup(URL_VAR).filter(|u| !u.is_empty()).ok_or_else(|| {
            NewsError::precondition(format!("{URL_VAR} is not set"))
                .with_hint("run `bun run setup`, then `bun run infra:up`; the CLI reads it from the environment")
        })?;
        let schema = lookup(SCHEMA_VAR)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_SCHEMA.to_owned());
        Self::new(url, schema)
    }

    fn options(&self) -> Result<PgConnectOptions, NewsError> {
        let opts = PgConnectOptions::from_str(&self.url).map_err(|_| {
            NewsError::usage(format!(
                "{URL_VAR} is not a valid postgres:// URL (value not shown)"
            ))
        })?;
        Ok(opts
            .options([("search_path", format!("{},public", self.schema))])
            .application_name("news-cli"))
    }

    /// Where the database is, without credentials.
    pub fn host_port(&self) -> String {
        match PgConnectOptions::from_str(&self.url) {
            Ok(o) => format!("{}:{}", o.get_host(), o.get_port()),
            Err(_) => "an unparsable URL".to_owned(),
        }
    }
}

pub fn valid_schema(s: &str) -> bool {
    let mut bytes = s.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z' | b'_'))
        && bytes.all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_'))
        && s.len() <= 63
}

/// Open a pool. `max` connections; connecting times out after 5 seconds.
pub async fn connect(cfg: &DbConfig, max: u32) -> Result<PgPool, NewsError> {
    PgPoolOptions::new()
        .max_connections(max)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(cfg.options()?)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(_) => map_err(e),
            _ => NewsError::upstream(format!(
                "PostgreSQL is not reachable at {}",
                cfg.host_port()
            ))
            .with_hint("run `bun run infra:up`"),
        })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MigrateReport {
    pub schema: String,
    /// Versions applied by this call (empty when the database was already up to date).
    pub applied: Vec<i64>,
    /// Every version now recorded.
    pub current: Vec<i64>,
}

/// Whether `schema` exists in the database.
pub async fn schema_exists(pool: &PgPool, schema: &str) -> Result<bool, NewsError> {
    sqlx::query_scalar("select exists (select 1 from pg_namespace where nspname = $1)")
        .bind(schema)
        .fetch_one(pool)
        .await
        .map_err(map_err)
}

/// Refuse a schema that does not exist or is not fully migrated. PostgreSQL skips a missing schema
/// on the search path and looks up a table the schema lacks in `public`, so unqualified queries
/// would read and change tables there instead. Once every migration this binary knows is recorded
/// in the schema's own `_sqlx_migrations`, every table it queries exists in the schema.
pub async fn require_schema(pool: &PgPool, schema: &str) -> Result<(), NewsError> {
    if !valid_schema(schema) {
        return Err(NewsError::usage("invalid schema name"));
    }
    if !schema_exists(pool, schema).await? {
        return Err(
            NewsError::precondition(format!("schema {schema:?} does not exist"))
                .with_hint("run `news db migrate`"),
        );
    }
    let tracked: bool = sqlx::query_scalar(
        "select exists (select 1 from information_schema.tables
          where table_schema = $1 and table_name = '_sqlx_migrations')",
    )
    .bind(schema)
    .fetch_one(pool)
    .await
    .map_err(map_err)?;
    let applied: Vec<i64> = if tracked {
        // Qualified, so it never reads `public._sqlx_migrations`. valid_schema keeps `"` out.
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "select version from \"{schema}\"._sqlx_migrations where success"
        )))
        .fetch_all(pool)
        .await
        .map_err(map_err)?
    } else {
        Vec::new()
    };
    let pending: Vec<i64> = known_versions()
        .into_iter()
        .filter(|v| !applied.contains(v))
        .collect();
    if pending.is_empty() {
        Ok(())
    } else {
        Err(NewsError::precondition(format!(
            "the database is not migrated (schema {schema:?} lacks migrations {pending:?})"
        ))
        .with_hint("run `news db migrate`"))
    }
}

/// Create the schema if needed, then apply pending migrations. Idempotent.
pub async fn migrate(pool: &PgPool, schema: &str) -> Result<MigrateReport, NewsError> {
    if !valid_schema(schema) {
        return Err(NewsError::usage("invalid schema name"));
    }
    // valid_schema above keeps `"` out of the name; quoting lets a keyword such as `select` through.
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "create schema if not exists \"{schema}\""
    )))
    .execute(pool)
    .await
    .map_err(map_err)?;
    let before = applied_versions(pool).await?;
    MIGRATOR.run(pool).await.map_err(map_migrate_err)?;
    let current = applied_versions(pool).await?;
    let applied = current
        .iter()
        .copied()
        .filter(|v| !before.contains(v))
        .collect();
    Ok(MigrateReport {
        schema: schema.to_owned(),
        applied,
        current,
    })
}

/// Map a migration error: a database error, between migrations or inside one, keeps its stable code
/// (a lost connection is retryable); anything else is `internal`.
fn map_migrate_err(e: sqlx::migrate::MigrateError) -> NewsError {
    use sqlx::migrate::MigrateError;
    match e {
        MigrateError::Execute(inner) | MigrateError::ExecuteMigration(inner, _) => map_err(inner),
        other => NewsError::internal(format!("migration failed: {other}")),
    }
}

/// Versions recorded in `_sqlx_migrations` in the current schema, ascending. Empty before the first run.
pub async fn applied_versions(pool: &PgPool) -> Result<Vec<i64>, NewsError> {
    let exists: bool = sqlx::query_scalar(
        "select exists (select 1 from information_schema.tables
          where table_schema = current_schema() and table_name = '_sqlx_migrations')",
    )
    .fetch_one(pool)
    .await
    .map_err(map_err)?;
    if !exists {
        return Ok(Vec::new());
    }
    sqlx::query_scalar("select version from _sqlx_migrations where success order by version")
        .fetch_all(pool)
        .await
        .map_err(map_err)
}

/// The versions embedded in this binary.
pub fn known_versions() -> Vec<i64> {
    MIGRATOR.iter().map(|m| m.version).collect()
}

/// Map a database error onto the stable codes.
pub fn map_err(e: sqlx::Error) -> NewsError {
    match &e {
        sqlx::Error::RowNotFound => NewsError::not_found("no such row"),
        sqlx::Error::PoolTimedOut | sqlx::Error::Io(_) | sqlx::Error::PoolClosed => {
            NewsError::upstream("PostgreSQL is busy or unreachable")
        }
        sqlx::Error::Database(db) => match db.code().as_deref() {
            // undefined_table, undefined_column: the schema is older than the binary.
            Some("42P01" | "42703") => {
                NewsError::precondition(format!("the database is not migrated ({})", db.message()))
                    .with_hint("run `news db migrate`")
            }
            Some("23505") => NewsError::conflict(format!("duplicate: {}", db.message())),
            Some("23503" | "23514" | "23502" | "22P02") => {
                NewsError::validation(db.message().to_owned())
            }
            // serialization_failure, deadlock_detected, lock_not_available, too_many_connections
            Some("40001" | "40P01" | "55P03" | "53300") => {
                NewsError::new(ErrorCode::UpstreamUnavailable, "database busy; retry")
            }
            _ => NewsError::internal(format!("database error: {}", db.message())),
        },
        _ => NewsError::internal(format!("database error: {e}")),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn schema_names_are_plain_identifiers() {
        for ok in ["news", "news_test_1_2", "_x"] {
            assert!(valid_schema(ok), "{ok}");
        }
        for bad in [
            "",
            "News",
            "1news",
            "news;drop",
            "news-test",
            "a b",
            &"a".repeat(64),
        ] {
            assert!(!valid_schema(bad), "{bad}");
        }
    }

    #[test]
    fn config_needs_a_url_and_defaults_the_schema() {
        let err = DbConfig::from_lookup(|_| None).unwrap_err();
        assert_eq!(err.code, ErrorCode::Precondition);
        let cfg = DbConfig::from_lookup(|n| (n == URL_VAR).then(|| "postgres://u:p@h:1/d".into()))
            .unwrap();
        assert_eq!(cfg.schema, DEFAULT_SCHEMA);
        assert_eq!(cfg.host_port(), "h:1");
    }

    #[test]
    fn migration_execution_errors_keep_their_database_code() {
        use sqlx::migrate::MigrateError;
        // sqlx 0.9 wraps a failing migration script in ExecuteMigration; a lost connection there is
        // retryable, like one between migrations.
        for e in [
            MigrateError::ExecuteMigration(sqlx::Error::PoolTimedOut, 4),
            MigrateError::Execute(sqlx::Error::PoolTimedOut),
        ] {
            let mapped = map_migrate_err(e);
            assert_eq!(mapped.code, ErrorCode::UpstreamUnavailable, "{mapped:?}");
        }
        assert_eq!(
            map_migrate_err(MigrateError::VersionMissing(3)).code,
            ErrorCode::Internal
        );
    }

    #[test]
    fn debug_never_shows_the_url() {
        let cfg = DbConfig::new("postgres://u:hunter2@h:1/d", "news").unwrap();
        assert!(!format!("{cfg:?}").contains("hunter2"));
    }

    #[test]
    fn migrations_are_numbered_in_their_blocks_and_ascending() {
        // Blocks of 100 per owner (cli/README.md): 0001–0099 core, 0100–0199 #9, 0200–0299 #10, …
        let versions = known_versions();
        assert!(!versions.is_empty());
        assert!(versions.windows(2).all(|w| w[0] < w[1]));
        assert!(
            versions.iter().any(|v| (1..=99).contains(v)),
            "{versions:?}"
        );
        assert!(
            versions.iter().all(|v| (1..=999).contains(v)),
            "{versions:?}"
        );
    }

    /// SQL statements, lower-cased, without `--` comments, whitespace collapsed to single spaces.
    fn statements(sql: &str) -> Vec<String> {
        let code: String = sql
            .lines()
            .map(|l| l.split_once("--").map_or(l, |(code, _)| code))
            .collect::<Vec<_>>()
            .join(" ");
        code.to_lowercase()
            .split(';')
            .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// True unless `stmt` is an `ALTER TABLE` with an action that fails or changes something on a
    /// second run. Guarded forms re-run as no-ops: `add column if not exists`, `drop column if exists`,
    /// `drop constraint if exists`, `alter column … set default` / `drop default`.
    fn alter_is_guarded(stmt: &str) -> bool {
        const GUARDED: [&str; 3] = [
            "add column if not exists ",
            "drop column if exists ",
            "drop constraint if exists ",
        ];
        let Some(rest) = stmt.strip_prefix("alter table ") else {
            return true;
        };
        let rest = rest.strip_prefix("if exists ").unwrap_or(rest);
        let rest = rest.strip_prefix("only ").unwrap_or(rest);
        let Some((_table, actions)) = rest.split_once(' ') else {
            return false;
        };
        // Split on the commas between actions, not those inside a type such as numeric(10, 2).
        let mut parts: Vec<String> = Vec::new();
        for piece in actions.split(',') {
            let p = piece.trim();
            match parts.last_mut() {
                Some(last)
                    if !["add ", "drop ", "alter ", "rename ", "set ", "validate "]
                        .iter()
                        .any(|k| p.starts_with(k)) =>
                {
                    last.push(',');
                    last.push_str(p);
                }
                _ => parts.push(p.to_owned()),
            }
        }
        parts.iter().all(|a| {
            GUARDED.iter().any(|g| a.starts_with(g))
                || (a.starts_with("alter column ")
                    && (a.contains(" set default ") || a.ends_with(" drop default")))
        })
    }

    #[test]
    fn guarded_alters_pass_and_others_fail() {
        for ok in [
            "alter table jobs add column if not exists lease_token uuid",
            "alter table if exists jobs add column if not exists a numeric(10, 2), add column if not exists b text",
            "alter table only jobs drop column if exists a, drop constraint if exists jobs_x",
            "alter table jobs alter column priority set default 2",
            "alter table jobs alter column priority drop default",
            "create table if not exists x (a int)",
        ] {
            assert!(alter_is_guarded(ok), "{ok}");
        }
        for bad in [
            "alter table jobs add column a int",
            "alter table jobs add constraint c check (a > 0)",
            "alter table jobs add column if not exists a int, add column b int",
            "alter table jobs rename column a to b",
            "alter table jobs drop column a",
            "alter table jobs",
        ] {
            assert!(!alter_is_guarded(bad), "{bad}");
        }
        assert_eq!(
            statements("ALTER TABLE x\n  ADD COLUMN IF NOT EXISTS y int; -- note; here\nselect 1;"),
            ["alter table x add column if not exists y int", "select 1"]
        );
    }

    #[test]
    fn every_migration_statement_is_idempotent() {
        // Plain CREATE TABLE / INDEX without IF NOT EXISTS, and INSERT without ON CONFLICT, would fail
        // or duplicate on a second run.
        for m in MIGRATOR.iter() {
            let sql = m.sql.as_str().to_lowercase();
            for (i, line) in sql.lines().enumerate() {
                let t = line.trim_start();
                if t.starts_with("create table ")
                    || t.starts_with("create index ")
                    || t.starts_with("create unique index ")
                {
                    assert!(
                        t.contains("if not exists"),
                        "{}:{}: {t}",
                        m.description,
                        i + 1
                    );
                }
            }
            for stmt in statements(&sql) {
                assert!(
                    alter_is_guarded(&stmt),
                    "{}: unguarded {stmt:?}; use CREATE … IF NOT EXISTS or a guarded ALTER (add column if not exists, drop … if exists, set/drop default)",
                    m.description
                );
            }
            let inserts = sql.matches("insert into").count();
            assert_eq!(
                inserts,
                sql.matches("on conflict").count(),
                "{}",
                m.description
            );
        }
    }
}
