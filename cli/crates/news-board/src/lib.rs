//! Board: the local analytics dashboard (Technical Design, "Telemetry and analytics"; Bureau Design
//! §3.13). The dashboard itself is a Bun app (`dashboard/`); this crate owns its data jobs, starting
//! with the daily Cloudflare Web Analytics pull into `traffic_daily` (TD "Event sources": cloud usage
//! pulled through their APIs).
//!
//! Skeleton only: Board is built in #16. Its job handlers are registered with Dispatch now, so the
//! registry is complete; until then they fail with class `bug` ("not built yet").

use news_core::job::{Handler, JobFailure, JobFuture, JobInput};

/// The issue that builds this subsystem.
pub const ISSUE: &str = "#16";

/// Job kinds this subsystem handles (see `news_dispatch::KINDS`).
pub const KINDS: &[&str] = &["traffic.pull"];

/// Run one job.
pub fn handle(input: JobInput) -> JobFuture {
    Box::pin(async move { Err(JobFailure::not_built(&input.kind, ISSUE)) })
}

pub const HANDLER: Handler = handle;
