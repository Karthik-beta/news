//! Roster, the entity graph, and its resolver (Bureau Design §3.5).
//!
//! One identity per company, country, central bank, institution, official, topic and source;
//! mentions resolved to it by identifiers and aliases, deterministically, with no AI.
//!
//! * [`norm`]: alias normalisation and the pg_trgm-style similarity.
//! * [`resolver`]: steps 1 to 5 over an in-memory [`resolver::Index`].
//! * [`seed`]: the seed loaders (SEC, GLEIF, ROR, Wikidata) and the hand-written seed files.
//! * [`store`]: applying seeds, resolving against PostgreSQL, the unresolved queue, `show`, and the
//!   H-tier `add_alias` and `merge` (Board only: no shell command reaches them).

pub mod norm;
pub mod resolver;
pub mod seed;
pub mod store;

/// The issue that builds this subsystem.
pub const ISSUE: &str = "#12";

/// Job kinds this subsystem handles (see `news_dispatch::KINDS`): none. Roster is a library that
/// Clerk (step 1, identifiers) and Taster (mentions in text) call.
pub const KINDS: &[&str] = &[];
