//! Critic, the evaluation harness (Bureau Design §9.1), and Huddle (§8.12).
//!
//! Built: the golden sets ([`golden`]), the deterministic metrics ([`metrics`]), `news eval
//! run|report|promote` ([`eval`]), the Audit preset's steps with fresh A1 calls for the writer suite
//! ([`audit`]), `news review weekly` ([`debrief`]) and the Huddle preset's steps ([`huddle`]). The A12
//! judges are not built yet (#19); Critic has no job kinds of its own.

pub mod audit;
pub mod debrief;
pub mod eval;
pub mod golden;
pub mod huddle;
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
