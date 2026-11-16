//! PostgreSQL side of Wire: `snapshots`, `fetch_state` and source policies from `sources`.

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};

use crate::pace::BoxFut;
use crate::pipeline::{Prior, StateStore, WireError};
use crate::runner::{Fetched, SourcePolicy, Validators};

fn db(e: sqlx::Error) -> WireError {
    WireError::Db(e.to_string())
}

/// `fetch_state.url_hash`: SHA-256 of the redacted URL.
pub fn url_hash(url: &str) -> Vec<u8> {
    Sha256::digest(url.as_bytes()).to_vec()
}

/// The Runner policy for a source, from its Contacts row (synced from `data/sources/<id>.toml`).
pub async fn policy(pool: &PgPool, source_id: &str) -> Result<Option<SourcePolicy>, WireError> {
    let row = sqlx::query(
        "select source_id, rate_per_sec::float8 as rate, max_concurrency, daily_quota, robots_respect,
                requires_contact_email
           from sources where source_id = $1 and enabled",
    )
    .bind(source_id)
    .fetch_optional(pool)
    .await
    .map_err(db)?;
    Ok(row.map(|r| SourcePolicy {
        source_id: r.get("source_id"),
        rate_per_sec: r.get("rate"),
        max_concurrency: r
            .get::<i16, _>("max_concurrency")
            .try_into()
            .unwrap_or(1)
            .max(1),
        daily_quota: r
            .get::<Option<i32>, _>("daily_quota")
            .and_then(|q| u32::try_from(q).ok()),
        robots_respect: r.get("robots_respect"),
        requires_contact_email: r.get("requires_contact_email"),
    }))
}

pub struct PgState<'p> {
    pub pool: &'p PgPool,
}

impl StateStore for PgState<'_> {
    fn prior<'a>(&'a self, url: &'a str) -> BoxFut<'a, Result<Option<Prior>, WireError>> {
        Box::pin(async move {
            let row = sqlx::query(
                "select f.etag, f.last_modified, f.last_sha256, s.fetched_at, s.content_type
                   from fetch_state f join snapshots s on s.sha256 = f.last_sha256
                  where f.url_hash = $1",
            )
            .bind(url_hash(url))
            .fetch_optional(self.pool)
            .await
            .map_err(db)?;
            Ok(row.and_then(|r| {
                let sha: Vec<u8> = r.get("last_sha256");
                Some(Prior {
                    validators: Validators {
                        etag: r.get("etag"),
                        last_modified: r.get("last_modified"),
                    },
                    sha256: sha.try_into().ok()?,
                    fetched_at: r.get("fetched_at"),
                    content_type: r.get("content_type"),
                })
            }))
        })
    }

    fn record<'a>(
        &'a self,
        source_id: &'a str,
        f: &'a Fetched,
        storage_path: &'a str,
        retain_until: DateTime<Utc>,
    ) -> BoxFut<'a, Result<(), WireError>> {
        Box::pin(async move {
            let mut tx = self.pool.begin().await.map_err(db)?;
            sqlx::query(
                "insert into snapshots (sha256, source_id, url, fetched_at, http_status, content_type,
                   bytes, etag, last_modified, storage_path, retain_until)
                 values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
                 on conflict (sha256) do update set retain_until =
                   greatest(snapshots.retain_until, excluded.retain_until)",
            )
            .bind(f.sha256.as_slice())
            .bind(source_id)
            .bind(&f.url)
            .bind(f.fetched_at)
            .bind(i16::try_from(f.status).unwrap_or(200))
            .bind(f.content_type.as_deref())
            .bind(i64::try_from(f.bytes.len()).unwrap_or(i64::MAX))
            .bind(f.etag.as_deref())
            .bind(f.last_modified.as_deref())
            .bind(storage_path)
            .bind(retain_until)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
            sqlx::query(
                "insert into fetch_state (url_hash, url, source_id, etag, last_modified, last_status,
                   last_sha256, last_fetched, consecutive_errors)
                 values ($1,$2,$3,$4,$5,$6,$7,$8,0)
                 on conflict (url_hash) do update set etag = excluded.etag,
                   last_modified = excluded.last_modified, last_status = excluded.last_status,
                   last_sha256 = excluded.last_sha256, last_fetched = excluded.last_fetched,
                   consecutive_errors = 0",
            )
            .bind(url_hash(&f.url))
            .bind(&f.url)
            .bind(source_id)
            .bind(f.etag.as_deref())
            .bind(f.last_modified.as_deref())
            .bind(i16::try_from(f.status).unwrap_or(200))
            .bind(f.sha256.as_slice())
            .bind(f.fetched_at)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
            tx.commit().await.map_err(db)
        })
    }

    fn not_modified<'a>(
        &'a self,
        _source_id: &'a str,
        url: &'a str,
        at: DateTime<Utc>,
    ) -> BoxFut<'a, Result<(), WireError>> {
        Box::pin(async move {
            sqlx::query(
                "update fetch_state set last_status = 304, last_fetched = $2, consecutive_errors = 0
                  where url_hash = $1",
            )
            .bind(url_hash(url))
            .bind(at)
            .execute(self.pool)
            .await
            .map_err(db)?;
            Ok(())
        })
    }
}
