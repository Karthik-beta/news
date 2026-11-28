//! Fixer, the brief builder (Bureau Design §3.8), and the cached prompt prefix (§3.9, D4).
//!
//! * [`inputs`]: what Fixer reads for a story; [`db::load`] fills it from PostgreSQL.
//! * [`build`]: the brief (`news_core::brief::Brief`, `brief.v1`), its refs and its size budget, with
//!   an A5 request when the source text needs condensing.
//! * [`context`]: context lines computed by code, each with its rule, and their re-check.
//! * [`prefix`]: page types, their skills' suggested components and the prefix file named by its hash.
//! * [`components`]: the component index the site build writes.
//! * [`db`]: loading, `news related`, and the `item.brief` job's writes ([`db::save`]).
//!
//! The `item.brief` job handler still fails with class `bug` ("not built yet"): like Clerk's and
//! Taster's it needs the service context (database pool, work directory), which lands with the step
//! handlers. `news brief` builds the same brief read-only.

pub mod build;
pub mod components;
pub mod context;
pub mod db;
pub mod inputs;
pub mod page;
pub mod prefix;

use news_core::job::{Handler, JobFailure, JobFuture, JobInput};

/// The issue that builds this subsystem.
pub const ISSUE: &str = "#18";

/// Job kinds this subsystem handles (see `news_dispatch::KINDS`).
pub const KINDS: &[&str] = &["item.brief"];

/// Run one job.
pub fn handle(input: JobInput) -> JobFuture {
    Box::pin(async move { Err(JobFailure::not_built(&input.kind, ISSUE)) })
}

pub const HANDLER: Handler = handle;

/// Build a story's brief from its inputs, with the components its page type's layout suggests.
pub fn brief(inputs: &inputs::Inputs) -> news_core::NewsResult<build::Built> {
    let suggested = prefix::suggested(&inputs.page_type, inputs.tier)?;
    build::build(inputs, &suggested)
}
