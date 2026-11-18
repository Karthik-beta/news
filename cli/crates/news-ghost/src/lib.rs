//! Ghost, the AI runner's contracts and validators (Bureau Design §3.9, §7).
//!
//! Skeleton only: Ghost is built in #18. Its job handlers are registered with Dispatch now, so the
//! registry is complete; until then they fail with class `bug` ("not built yet").

use news_core::job::{Handler, JobFailure, JobFuture, JobInput};

/// The issue that builds this subsystem.
pub const ISSUE: &str = "#18";

/// Job kinds this subsystem handles (see `news_dispatch::KINDS`).
pub const KINDS: &[&str] = &[
    "ai.write",
    "ai.repair",
    "ai.classify",
    "ai.storymatch",
    "ai.condense",
    "ai.spotcheck",
];

/// Run one job.
pub fn handle(input: JobInput) -> JobFuture {
    Box::pin(async move { Err(JobFailure::not_built(&input.kind, ISSUE)) })
}

pub const HANDLER: Handler = handle;
