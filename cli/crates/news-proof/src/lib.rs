//! Render and Proof: rendering and code checks (Bureau Design §3.10).
//!
//! * Proof is built here (#17): [`checks::run`] runs C1-C14 on a rendered page [`bundle::Bundle`] and returns
//!   the [`checks::Markup`]. The word lists are `data/seeds/proof-v1.toml` ([`lexicon`]). Everything that
//!   reaches outside the bundle (link checks, axe, the page index, stored text blocks) comes in through
//!   [`checks::Env`], so every check is tested with a passing and a failing bundle and no network.
//! * Render and save are built in a separate branch of #17. Their job handlers are registered with Dispatch
//!   now, so the registry is complete; until then they fail with class `bug` ("not built yet").

pub mod axe;
pub mod bundle;
pub mod checks;
pub mod copy;
pub mod html;
pub mod lexicon;
pub mod links;

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
