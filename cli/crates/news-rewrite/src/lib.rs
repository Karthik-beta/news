//! Rewrite, revision handling (Bureau Design §3.11, §8.9): keep published pages correct when sources
//! revise or restate data.
//!
//! * [`find`]: published pages whose data points a later vintage supersedes, and the derived points built
//!   from them, recomputed with the Ledger's arithmetic.
//! * [`classify`] and [`plan`]: value only, restatement or direction flip per page; the 100-page `bulk`
//!   threshold and Edition's budget (step 4) and publish (step 10) selection.
//! * [`patch`] and [`apply`]: the re-render by code, on `data.json`, the stored Copy and `page.json`,
//!   with the "restated" note for a restatement. No AI, and no number typed by code or a model.
//! * [`update`]: the A1 update job for a flip, and `news update <page>`'s output.
//!
//! Rendering (Plate), `news check` and publishing (Press) are the steps after this crate's: Followup's
//! `rerender` and `publish` steps call them. The `page.revise` job handler still fails with class `bug`
//! ("not built yet"): it needs the Bureau service's context (database pool, Plate, the work directory).

use news_core::job::{Handler, JobFailure, JobFuture, JobInput};

pub mod apply;
pub mod classify;
pub mod find;
pub mod patch;
pub mod plan;
pub mod update;

/// The issue that builds this subsystem.
pub const ISSUE: &str = "#20";

/// Job kinds this subsystem handles (see `news_dispatch::KINDS`).
pub const KINDS: &[&str] = &["page.revise"];

/// Run one job.
pub fn handle(input: JobInput) -> JobFuture {
    Box::pin(async move { Err(JobFailure::not_built(&input.kind, ISSUE)) })
}

pub const HANDLER: Handler = handle;
