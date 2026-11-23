//! Render and Proof: rendering and code checks (Bureau Design §3.9, §3.10).
//!
//! Built so far (#17): the stored Copy (`work`), the Copy validators every AI step's `news ai accept` runs
//! (`validate`, `lint`), the templated fallback page (`fallback`, D10), and the bundle `news render` builds
//! through Plate, the warm Bun render server (`render`, `plate`). The job handlers still fail with class `bug`
//! ("not built yet") until the Bureau service gives them a context.

pub mod fallback;
pub mod lint;
pub mod plate;
pub mod render;
pub mod validate;
pub mod work;

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
