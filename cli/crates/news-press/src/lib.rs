//! Press, the publisher (Bureau Design §3.12, ADR 0004 "Write order", ADR 0006).
//!
//! * [`store`]: staging into the outbox (inside the G-tier check's transaction), [`store::drain`], the
//!   index budget's staging and `news stats`.
//! * [`api`]: the write-API client: `Idempotency-Key` on every write, a token bucket of 5 a second with
//!   bursts of 10, 5xx retried with backoff, any other 4xx a `bug`.
//! * [`budget`]: the index budget's ranking (Edition step 11).
//! * [`sync`]: taxonomy and redirects. [`prune`]: `news prune`. [`bundle`]: the rendered bundle on disk.
//! * [`fake`]: a scripted write API for tests.
//!
//! The G-tier commands (`news publish`, `news home publish`) have no CLI form: Chief calls these
//! functions in process, as the authorised step of a live run (Bureau §5.5). Only [`api::ReqwestApi`]
//! reads the write-API key. The Dispatch handlers (`page.publish`, `index.promote`) still fail with
//! class `bug` ("not built yet"): they need the Bureau service's context (database pool, one shared
//! [`api::Client`], the work directory) that Chief's step handlers provide.

use news_core::job::{Handler, JobFailure, JobFuture, JobInput};

pub mod api;
pub mod budget;
pub mod bundle;
pub mod fake;
pub mod prune;
pub mod store;
pub mod sync;

/// The issue that builds this subsystem.
pub const ISSUE: &str = "#20";

/// Job kinds this subsystem handles (see `news_dispatch::KINDS`).
pub const KINDS: &[&str] = &["page.publish", "index.promote"];

/// Run one job.
pub fn handle(input: JobInput) -> JobFuture {
    Box::pin(async move { Err(JobFailure::not_built(&input.kind, ISSUE)) })
}

pub const HANDLER: Handler = handle;
