//! Render and Proof: rendering and code checks (Bureau Design §3.9, §3.10).
//!
//! Built (#17):
//! * Render: the stored Copy (`work`), the Copy validators every AI step's `news ai accept` runs (`validate`,
//!   `lint`), the templated fallback page (`fallback`, D10), and the bundle `news render` builds through
//!   Plate, the warm Bun render server (`render`, `plate`).
//! * Proof: [`checks::run`] runs C1-C14 on a rendered page [`bundle::Bundle`] and returns the
//!   [`checks::Markup`]. The word lists are `data/seeds/proof-v1.toml` ([`lexicon`]). Everything that reaches
//!   outside the bundle (link checks, axe, the page index, stored text blocks) comes in through
//!   [`checks::Env`], so every check is tested with a passing and a failing bundle and no network.
//!
//! The job handlers still fail with class `bug` ("not built yet") until the Bureau service gives them a context.

pub mod axe;
pub mod bundle;
pub mod checks;
pub mod copy;
pub mod fallback;
pub mod html;
pub mod lexicon;
pub mod links;
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
