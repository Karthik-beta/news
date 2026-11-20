//! Diary, the release calendar (Bureau Design §3.3): know what is due and when, so work is scheduled
//! ahead. The calendar is an accelerator, never the only route: a wrong date only delays capture,
//! because feed discovery still finds the release.
//!
//! * [`ics`] and [`rules`]: iCalendar files (BLS, BEA) and the per-source table that maps event
//!   summaries to event keys.
//! * [`fomc`]: the Fed's FOMC calendar page (HTML).
//! * [`seed`]: the hand-checked yearly seed file (`data/calendar/seed.toml`) for the central banks
//!   whose calendars are not machine-readable.
//! * [`earnings`]: expected earnings windows derived from a company's own filing history.
//! * [`schedule`]: the pre-warm and capture schedule, and when an event becomes `late`.
//! * [`store`]: `calendar_events` (upsert, upcoming, late, captured).
//!
//! Everything but [`store`] is pure: no clock, network or database.

use news_core::job::{Handler, JobFailure, JobFuture, JobInput};

pub mod earnings;
pub mod fomc;
pub mod ics;
pub mod rules;
pub mod schedule;
pub mod seed;
pub mod store;

/// The issue that builds this subsystem.
pub const ISSUE: &str = "#12";

/// Job kinds this subsystem handles (see `news_dispatch::KINDS`).
pub const KINDS: &[&str] = &["calendar.sync", "calendar.capture"];

/// Run one job.
///
/// TODO(#10, #29): both kinds need the Bureau service context. `calendar.sync` fetches each
/// calendar source through the shared Runner (BLS and BEA ICS, the FOMC page, FRED `release/dates`
/// via discovery), maps them with [`rules`] / [`fomc`], adds [`seed`] and [`earnings`] entries and
/// calls [`store::upsert`]; `calendar.capture` runs the connector's discovery for the event's source
/// on the [`schedule`] and calls [`store::mark_captured`] when the event key appears. Until the
/// service exists the handlers refuse with class `bug`, like Wire's and Clerk's; `news calendar
/// sync` does the offline part (seed file and derived windows) by hand.
pub fn handle(input: JobInput) -> JobFuture {
    Box::pin(async move {
        Err(JobFailure::new(
            news_core::job::ErrorClass::Bug,
            format!(
                "handler for {} needs the Bureau service context (#10, #29); Diary is built in {ISSUE}",
                input.kind
            ),
        ))
    })
}

pub const HANDLER: Handler = handle;
