//! Taster, story detection (Bureau Design §3.6).
//!
//! Skeleton only: Taster is built in #15. Its job handlers are registered with Dispatch now, so the
//! registry is complete; until then they fail with class `bug` ("not built yet").

use news_core::job::{Handler, JobFailure, JobFuture, JobInput};

/// The issue that builds this subsystem.
pub const ISSUE: &str = "#15";

/// Job kinds this subsystem handles (see `news_dispatch::KINDS`).
pub const KINDS: &[&str] = &["story.detect"];

/// Run one job.
pub fn handle(input: JobInput) -> JobFuture {
    Box::pin(async move { Err(JobFailure::not_built(&input.kind, ISSUE)) })
}

pub const HANDLER: Handler = handle;
