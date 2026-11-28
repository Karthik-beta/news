//! Taster, story detection (Bureau Design §3.6).
//!
//! * [`config`]: the thresholds (`data/seeds/taster-v1.toml`).
//! * [`detect`]: duplicates, event keys, update detection and grey-zone A4 jobs, with the embedding
//!   seam ([`detect::Joiner`]).
//! * [`embed`]: the embedding joiner (bge-small, pgvector); the model itself is the `embed` cargo feature.
//! * [`input`]: `stories.score_input`, the story input Nose scores.
//! * [`edit`]: H-tier `merge` and `split`. Board calls them over the Hotline; no shell command does.
//! * [`read`]: what `news stories show|members|similar` print.
//! * [`pairs`]: G3 candidates, pairs of a candidate and a story either side of the join threshold.
//!
//! The `story.detect` job handler still fails with class `bug` ("not built yet"): like Clerk's it needs
//! the service context (database pool, the embedder), which lands with the step handlers.

pub mod config;
pub mod detect;
pub mod edit;
pub mod embed;
pub mod input;
pub mod pairs;
pub mod read;

use news_core::NewsError;
use news_core::job::{Handler, JobFailure, JobFuture, JobInput};
use sqlx::PgConnection;

/// The issue that builds this subsystem.
pub const ISSUE: &str = "#15";

/// Job kinds this subsystem handles (see `news_dispatch::KINDS`).
pub const KINDS: &[&str] = &["story.detect"];

/// Run one job.
pub fn handle(input: JobInput) -> JobFuture {
    Box::pin(async move { Err(JobFailure::not_built(&input.kind, ISSUE)) })
}

pub const HANDLER: Handler = handle;

/// A database error or a domain error, mapped once at the public boundary.
pub(crate) enum Fail {
    Db(sqlx::Error),
    News(NewsError),
}

impl From<sqlx::Error> for Fail {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e)
    }
}

impl From<NewsError> for Fail {
    fn from(e: NewsError) -> Self {
        Self::News(e)
    }
}

impl From<Fail> for NewsError {
    fn from(f: Fail) -> Self {
        match f {
            Fail::Db(e) => news_store::map_err(e),
            Fail::News(e) => e,
        }
    }
}

/// Detect, merge and split take this first, so they run one at a time (released at commit).
pub(crate) async fn lock(tx: &mut PgConnection) -> Result<(), sqlx::Error> {
    sqlx::query("select pg_advisory_xact_lock(hashtext('news.taster'))")
        .execute(tx)
        .await?;
    Ok(())
}

/// A story's counts and span from its candidates.
pub(crate) async fn recount(tx: &mut PgConnection, story_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query(
        "update stories s set member_count = m.n, first_seen = coalesce(m.first_at, s.first_seen),
                              last_seen = coalesce(m.last_at, s.last_seen), entity_ids = m.entities
           from (select count(*)::int as n, min(first_seen) as first_at, max(first_seen) as last_at,
                        coalesce((select array_agg(distinct e order by e)
                                    from candidates c2, unnest(c2.entity_ids) e where c2.story_id = $1),
                                 '{}') entities
                   from candidates where story_id = $1) m
          where s.story_id = $1",
    )
    .bind(story_id)
    .execute(tx)
    .await?;
    Ok(())
}
