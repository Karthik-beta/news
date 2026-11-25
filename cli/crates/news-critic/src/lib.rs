//! Critic, the evaluation harness (Bureau Design §9.1), and Huddle's Debrief (§8.12).
//!
//! The harness is still a skeleton (#19, later PRs): its job handlers are registered with Dispatch, so the
//! registry is complete, and until then they fail with class `bug` ("not built yet"). [`debrief`] is built:
//! `news review weekly`.

pub mod debrief;

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
