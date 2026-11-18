//! Ledger (Bureau Design §3.4, ADR 0018): the types and rules behind every published figure.
//!
//! * [`point`]: a data point exactly as published, its `dp_id` and locator.
//! * [`decimal`]: exact decimals parsed from `value_text`, and the style that writes them back.
//! * [`units`]: the unit vocabulary; an unknown unit is flagged, never guessed.
//! * [`guard`]: derived points (changes, shares) and the guards that refuse unlike comparisons.
//! * [`eventkey`]: story identity for structured items.
//!
//! Pure: no database, network or clock. Storage lives in `news-clerk`.

pub mod decimal;
pub mod eventkey;
pub mod guard;
pub mod point;
pub mod units;

pub use point::{DataPoint, Draft, Missing, PeriodType};
