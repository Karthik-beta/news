//! Render and Proof: rendering and code checks (Bureau Design §3.10).
//!
//! Skeleton only: Proof is built in #17. Its job handlers are registered with Dispatch now, so the
//! registry is complete; until then they fail with class `bug` ("not built yet").

use news_core::job::{Handler, JobFailure, JobFuture, JobInput};

/// The issue that builds this subsystem.
pub const ISSUE: &str = "#17";

/// Job kinds this subsystem handles (see `news_dispatch::KINDS`).
pub const KINDS: &[&str] = &["item.render", "item.check"];

/// Run one job.
pub fn handle(input: JobInput) -> JobFuture {
    Box::pin(async move { Err(JobFailure::not_built(&input.kind, ISSUE)) })
}

pub const HANDLER: Handler = handle;
