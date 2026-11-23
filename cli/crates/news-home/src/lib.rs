//! Home page curation (Bureau Design §4.5, §8.6; ADR 0005; issue #20): the Frontpage preset's code steps.
//!
//! * [`candidates`] - `home_score` and the top 40 with bucket diversity.
//! * [`draft`] - the curator's draft and the `curation.v1` body built from it.
//! * [`check`] - the TD's rules and ADR 0005's table, ported from `workers/shared/src/curation.ts`.
//! * [`store`] - `home_curations`.
//! * [`preview`] - the local SSR with the draft, and the four views.
//! * [`api`] - the write API's curation route and `publish` (G tier).
//! * [`ops`] - `save`, `check`, `preview` and `show` as the CLI runs them.
//!
//! `rollback` is H tier: Board sends it over the Hotline and the command has no CLI form.

pub mod api;
pub mod candidates;
pub mod check;
pub mod draft;
pub mod facts;
pub mod ops;
pub mod preview;
pub mod store;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_http;
