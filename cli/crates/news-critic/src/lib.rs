//! Critic, the evaluation harness (Bureau Design §9.1), and Huddle (§8.12).
//!
//! Built: the golden sets ([`golden`]), the deterministic metrics ([`metrics`]), `news eval
//! run|report|promote` ([`eval`]), the Audit preset's steps with fresh A1 calls for the writer suite
//! ([`audit`]), `news review weekly` ([`debrief`]) and the Huddle preset's steps ([`huddle`]). Karthik's
//! labels ([`labels`]: the A12 judges' items and rates; [`g3`]: story pairs and the join threshold) are
//! drafted by `news eval draft`, stored through Board and written to `pipeline/eval/` by `news eval
//! export`. The A12 judges themselves are not built yet (#19); Critic has no job kinds of its own.

pub mod audit;
pub mod debrief;
pub mod eval;
pub mod g3;
pub mod golden;
pub mod huddle;
pub mod labels;
pub mod metrics;

use news_core::job::{Handler, JobFailure, JobFuture, JobInput};

/// The issue that builds this subsystem.
pub const ISSUE: &str = "#19";

/// Job kinds this subsystem handles (see `news_dispatch::KINDS`).
pub const KINDS: &[&str] = &[];

/// Run one job.
pub fn handle(input: JobInput) -> JobFuture {
    Box::pin(async move { Err(JobFailure::not_built(&input.kind, ISSUE)) })
}

pub const HANDLER: Handler = handle;
