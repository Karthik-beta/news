//! Dispatch: the job engine (Bureau Design §5.3–§5.9, decision D1).
//!
//! A small custom queue on PostgreSQL with River's semantics: unique jobs, states, attempt-based
//! full-jitter backoff, snooze without spending an attempt, and a rescuer for stuck jobs. A claim locks
//! the pool's row `FOR UPDATE` before taking jobs with `SKIP LOCKED`, so `max_running` is exact under
//! any number of concurrent workers. ADR: `docs/adr/0003-dispatch-job-engine.md`.

pub mod backoff;
pub mod kinds;
pub mod queue;
pub mod schedule;
pub mod worker;

pub use kinds::{KINDS, KindSpec};
pub use queue::{Claimed, Enqueued, Job, NewJob};

/// Pools, in the order of §5.7.
pub const POOLS: [&str; 6] = ["net", "cpu", "docling", "render", "ai", "publish"];

/// Pools `news work` may run. `ai`, `render` and `publish` are driven only inside preset runs (Ghost,
/// the renderer, Press), so the background worker never needs the write-API key (§5.6, §9.3).
pub const WORK_POOLS: [&str; 3] = ["net", "cpu", "docling"];

/// Advisory lock held by the rescuer (§5.4). Taken as the pair `(42, hashtext(current_schema()))`, so
/// queues in different schemas (the test schemas) never keep each other's rescuer out.
pub const RESCUER_LOCK: i32 = 42;
/// Advisory lock held while evaluating schedules (§5.6), per schema like [`RESCUER_LOCK`].
pub const SCHEDULER_LOCK: i32 = 43;

/// SQL that tries the transaction-scoped advisory lock `($1, this schema)`.
pub(crate) const TRY_SCHEMA_LOCK: &str =
    "select pg_try_advisory_xact_lock($1, hashtext(current_schema()))";

/// Live states: a unique key is taken while its job is in one of these.
pub const LIVE_STATES: [&str; 5] = ["available", "scheduled", "running", "retryable", "snoozed"];
pub const STATES: [&str; 8] = [
    "available",
    "scheduled",
    "running",
    "retryable",
    "snoozed",
    "completed",
    "discarded",
    "cancelled",
];

pub fn is_pool(p: &str) -> bool {
    POOLS.contains(&p)
}
