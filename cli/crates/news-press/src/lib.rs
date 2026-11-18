//! Press, the publisher (Bureau Design §3.12).
//!
//! Skeleton only: Press is built in #20. Its job handlers are registered with Dispatch now, so the
//! registry is complete; until then they fail with class `bug` ("not built yet").

use news_core::job::{Handler, JobFailure, JobFuture, JobInput};

/// The issue that builds this subsystem.
pub const ISSUE: &str = "#20";

/// Job kinds this subsystem handles (see `news_dispatch::KINDS`).
pub const KINDS: &[&str] = &["page.publish", "index.promote"];

/// Run one job.
pub fn handle(input: JobInput) -> JobFuture {
    Box::pin(async move { Err(JobFailure::not_built(&input.kind, ISSUE)) })
}

pub const HANDLER: Handler = handle;
