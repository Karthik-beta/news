//! Wire: Stringers, the Runner and the Morgue (Bureau Design §3.2).
//!
//! * [`runner`]: the one polite HTTP client (quota, robots.txt, token buckets in the `net` pool,
//!   conditional GET, `Retry-After`, content-type allow-list, size caps, streaming SHA-256).
//! * [`morgue`]: content-addressed zstd files under `work/raw/sha256/…` and their retention.
//! * [`pipeline`]: discover and fetch for any connector, recording `snapshots` and `fetch_state`.
//! * [`store`]: the PostgreSQL side.
//!
//! The connectors themselves (SEC EDGAR, FRED, BLS) live in `news-connectors`.

use news_core::job::{ErrorClass, Handler, JobFailure, JobFuture, JobInput};

pub mod fake;
pub mod morgue;
pub mod pace;
pub mod pipeline;
pub mod robots;
pub mod runner;
pub mod store;

/// The issue that builds this subsystem.
pub const ISSUE: &str = "#9";

/// Job kinds this subsystem handles (see `news_dispatch::KINDS`).
pub const KINDS: &[&str] = &["source.discover", "doc.fetch", "work.prune"];

/// Run one job.
///
/// TODO(#10, #29): the handlers need a long-lived context (database pool, one shared [`runner::Runner`]
/// so per-host buckets and robots caches are shared, the [`morgue::Morgue`] root) that the Bureau
/// service provides. Until it exists the registered handler refuses with class `bug`, and the work is
/// done by [`pipeline::Wire::discover`] / [`pipeline::Wire::fetch`], which the service will call:
/// `source.discover` → `discover`, enqueue `doc.fetch` per new ref; `doc.fetch` → `fetch`, enqueue
/// `doc.parse` only when `changed`; `work.prune` → delete Morgue files past `retain_until`.
pub fn handle(input: JobInput) -> JobFuture {
    Box::pin(async move {
        Err(JobFailure::new(
            ErrorClass::Bug,
            format!(
                "handler for {} needs the Bureau service context (#10, #29); Wire is built in {ISSUE}",
                input.kind
            ),
        ))
    })
}

pub const HANDLER: Handler = handle;
