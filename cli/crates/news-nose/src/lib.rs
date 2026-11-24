//! Nose and Desk: scoring, ranking and selection (Bureau Design §3.7, §4; ADR 0012).
//!
//! * [`config`]: scorer config v1 and the seeds in `data/seeds/`.
//! * [`score`]: the signals, the score S and the Pitch.
//! * [`desk`]: the Budget, greedy with diminishing returns per bucket under hard limits.
//! * [`store`]: `stories.score_input`, `scores` and `run_plans` in PostgreSQL.
//! * [`calibrate`] and [`huddle`]: Huddle's ranking calibration (Bradley–Terry fit, labels, scorer versions).
//!
//! `news plan --run` and `news scores explain|weights` call these. The Dispatch job handlers
//! (`story.score`, `run.select`) still fail with class `bug` ("not built yet"): Chief's Edition step
//! that calls Desk lands with the step handlers.

pub mod calibrate;
pub mod config;
pub mod desk;
pub mod huddle;
pub mod score;
pub mod store;

use news_core::job::{Handler, JobFailure, JobFuture, JobInput};

/// The issue that builds this subsystem.
pub const ISSUE: &str = "#15";

/// Job kinds this subsystem handles (see `news_dispatch::KINDS`).
pub const KINDS: &[&str] = &["story.score", "run.select"];

/// Run one job.
pub fn handle(input: JobInput) -> JobFuture {
    Box::pin(async move { Err(JobFailure::not_built(&input.kind, ISSUE)) })
}

pub const HANDLER: Handler = handle;
