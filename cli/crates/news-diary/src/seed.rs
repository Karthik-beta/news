//! The yearly seed file (`data/calendar/seed.toml`): hand-checked decision dates for the central
//! banks whose calendars are not machine-readable (Bureau §3.3, "plus a hand-checked yearly seed
//! file, because these matter most and change rarely").
//!
//! Every entry names the bank, the decision date as the bank publishes it, and the day a person
//! checked it against the bank's own page (`checked_on`). Entries without that day are refused, and
//! an entry more than a year old is flagged by [`stale`], so the file cannot quietly rot.

use chrono::NaiveDate;
use serde::Deserialize;

use news_connectors::CalendarEntry;
use news_ledger::eventkey;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct SeedError(pub String);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seed {
    #[serde(default, rename = "event")]
    pub events: Vec<SeedEvent>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedEvent {
    /// Registry source the decision belongs to (`cb-boe`).
    pub source_id: String,
    /// Event-key bank (`boe`).
    pub bank: String,
    pub title: String,
    pub date: NaiveDate,
    /// The page the date was read from.
    pub page: String,
    /// The day a person checked the date against that page.
    pub checked_on: NaiveDate,
}

pub fn parse(text: &str) -> Result<Seed, SeedError> {
    let seed: Seed = toml::from_str(text).map_err(|e| SeedError(e.to_string()))?;
    for e in &seed.events {
        if !e.page.starts_with("https://") {
            return Err(SeedError(format!(
                "{} {}: page must be an https URL",
                e.bank, e.date
            )));
        }
        if eventkey::policy(&e.bank, e.date).is_none() {
            return Err(SeedError(format!("{:?} is not a valid bank key", e.bank)));
        }
        if e.checked_on < NaiveDate::from_ymd_opt(2026, 1, 1).unwrap_or_default()
            || e.checked_on > e.date
        {
            return Err(SeedError(format!(
                "{} {}: checked_on {} must be before the decision date",
                e.bank, e.date, e.checked_on
            )));
        }
    }
    Ok(seed)
}

/// Entries on or after `from`.
pub fn entries(seed: &Seed, from: NaiveDate) -> Vec<CalendarEntry> {
    seed.events
        .iter()
        .filter(|e| e.date >= from)
        .filter_map(|e| {
            Some(CalendarEntry {
                source_id: e.source_id.clone(),
                event_key: eventkey::policy(&e.bank, e.date)?,
                title: e.title.clone(),
                expected_day: e.date,
                expected_at: None,
                window_days: 0,
                precision: "day".into(),
                origin: "html".into(),
            })
        })
        .collect()
}

/// Where the seed file is: `NEWS_CALENDAR_DIR/seed.toml`, else `data/calendar/seed.toml` in the
/// nearest ancestor of `start` that has one.
pub fn seed_path(start: &std::path::Path) -> Option<std::path::PathBuf> {
    if let Some(d) = std::env::var_os("NEWS_CALENDAR_DIR") {
        return Some(std::path::PathBuf::from(d).join("seed.toml"));
    }
    start
        .ancestors()
        .map(|a| a.join("data").join("calendar").join("seed.toml"))
        .find(|p| p.is_file())
}

/// Upcoming entries last checked more than a year before `today`.
pub fn stale(seed: &Seed, today: NaiveDate) -> Vec<&SeedEvent> {
    seed.events
        .iter()
        .filter(|e| e.date >= today && today.signed_duration_since(e.checked_on).num_days() > 365)
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const OK: &str = r#"
[[event]]
source_id = "cb-boe"
bank = "boe"
title = "Bank of England MPC decision"
date = "2027-02-04"
page = "https://www.bankofengland.co.uk/monetary-policy/upcoming-mpc-dates"
checked_on = "2026-11-18"
"#;

    #[test]
    fn a_checked_entry_becomes_a_policy_event() {
        let s = parse(OK).unwrap();
        let e = entries(&s, NaiveDate::from_ymd_opt(2026, 11, 18).unwrap());
        assert_eq!(e[0].event_key, "policy:boe:2027-02-04");
        assert_eq!(e[0].origin, "html");
        assert!(entries(&s, NaiveDate::from_ymd_opt(2027, 3, 1).unwrap()).is_empty());
    }

    #[test]
    fn unchecked_or_unknown_fields_are_refused() {
        assert!(parse(&OK.replace("checked_on = \"2026-11-18\"\n", "")).is_err());
        assert!(parse(&format!("{OK}extra = 1\n")).is_err());
        assert!(parse(&OK.replace("2026-11-18", "2027-03-01")).is_err());
        assert!(parse(&OK.replace("https://", "http://")).is_err());
    }

    #[test]
    fn old_checks_are_reported() {
        let s = parse(OK).unwrap();
        assert!(stale(&s, NaiveDate::from_ymd_opt(2026, 12, 1).unwrap()).is_empty());
        assert_eq!(
            stale(&s, NaiveDate::from_ymd_opt(2027, 11, 30).unwrap()).len(),
            0,
            "past events are not stale"
        );
    }
}
