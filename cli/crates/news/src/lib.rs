//! The library half of the `news` package: what the binary's commands, the Bureau service
//! (`news service run`, #10) and the integration tests share.
//!
//! * [`steps`]: the preset step bodies Chief runs (Bureau §8.3–§8.13).
//! * [`ingest`]: one source through Wire and Clerk, shared by `news discover|fetch|parse` and the ingest job handlers (#9).
//! * [`registry`]: binds each Dispatch job kind to its subsystem's handler. The worker loop
//!   (`news_dispatch::worker::run`) takes `registry::resolve`.

pub mod ingest;
pub mod registry;
pub mod steps;
