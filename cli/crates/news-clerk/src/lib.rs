//! Clerk and Ledger: parsing snapshots into data points, text blocks and candidates (Bureau Design
//! §3.4, ADR 0018).
//!
//! * Parsing itself is each Stringer's pure `parse` (`news-connectors`); the Ledger's rules (exact
//!   decimals, `dp_id`, units, guards, event keys) are in `news-ledger`.
//! * [`candidates`]: the licence gate and rule-based classification.
//! * [`rules`]: the rules table and the taxonomy seed.
//! * [`ledger`]: storing points (vintages, `supersedes`), Roster step 1 entities and candidates.
//! * [`simhash`]: 64-bit SimHash for `documents`.

use news_core::job::{ErrorClass, Handler, JobFailure, JobFuture, JobInput};

pub mod candidates;
pub mod ledger;
pub mod rules;
pub mod simhash;

pub use news_ledger;

/// The issue that builds this subsystem.
pub const ISSUE: &str = "#9";

/// Job kinds this subsystem handles (see `news_dispatch::KINDS`).
pub const KINDS: &[&str] = &["doc.parse", "doc.parse_docling"];

/// Run one job.
///
/// TODO(#10, #29): `doc.parse` needs the service context (database pool, Morgue root). The service
/// will load the item's snapshots from the Morgue, call the connector's `parse`, then
/// [`candidates::prepare`] and [`ledger::store`]; a `ParseError` maps to its class (`invalid_data` or
/// `upstream_changed` → dead list, `NotYet` → retry, a source's refusal → snooze for its
/// `retry_after`). `doc.parse_docling` is #12 (Docling).
pub fn handle(input: JobInput) -> JobFuture {
    Box::pin(async move {
        Err(JobFailure::new(
            ErrorClass::Bug,
            format!(
                "handler for {} needs the Bureau service context (#10, #29); Clerk is built in {ISSUE}",
                input.kind
            ),
        ))
    })
}

pub const HANDLER: Handler = handle;
