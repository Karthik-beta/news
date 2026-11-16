//! The library half of the `news` package: what the binary's commands, the Bureau service
//! (`news service run`, #10) and the integration tests share.
//!
//! * [`registry`]: binds each Dispatch job kind to its subsystem's handler. The worker loop
//!   (`news_dispatch::worker::run`) takes `registry::resolve`.

pub mod registry;
