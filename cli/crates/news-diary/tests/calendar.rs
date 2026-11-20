//! Calendar fixture tests: ICS (BLS, BEA), HTML (FOMC), the seed file and derived earnings windows
//! produce the expected event keys; the capture schedule runs on a virtual clock. Fixtures were
//! recorded on 18 Nov 2026 and trimmed (see `tests/fixtures/README.md`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use chrono::{NaiveDate, TimeZone, Utc};
use news_diary::{earnings, fomc, ics, rules, schedule, seed};

fn fixture(name: &str) -> Vec<u8> {
    let p = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

#[test]
fn bls_ics_maps_cpi_and_the_jobs_report_to_event_keys() {
    let events = ics::parse(&fixture("bls-schedule.ics")).unwrap();
    assert_eq!(events.len(), 23);
    let entries = rules::entries_from_ics("bls", &events);
    let keys: Vec<(&str, String, Option<String>)> = entries
        .iter()
        .map(|e| {
            (
                e.event_key.as_str(),
                e.expected_day.to_string(),
                e.expected_at.map(|t| t.to_rfc3339()),
            )
        })
        .collect();
    // CPI on 10 Nov is for October (the month before the release day).
    assert!(keys.contains(&(
        "macro:us.cpi:2026-10",
        "2026-11-10".into(),
        Some("2026-11-10T13:30:00+00:00".into())
    )));
    // 08:30 US-Eastern is 13:30 UTC in winter (EST) and the same rule holds on 4 December.
    assert!(keys.contains(&(
        "macro:us.payrolls:2026-11",
        "2026-12-04".into(),
        Some("2026-12-04T13:30:00+00:00".into())
    )));
    assert!(keys.iter().any(|k| k.0 == "macro:us.unemployment:2026-11"));
    assert!(keys.iter().any(|k| k.0 == "macro:us.cpi:2026-11"));
    // Releases no Stringer captures (JOLTS, PPI, …) are not calendar events.
    assert_eq!(entries.len(), 6, "{keys:?}");
    assert!(
        entries
            .iter()
            .all(|e| e.precision == "exact" && e.origin == "ics" && e.source_id == "bls")
    );
    // Deterministic: re-running changes nothing.
    assert_eq!(entries, rules::entries_from_ics("bls", &events));
}

#[test]
fn bea_ics_keeps_the_advance_gdp_estimate_only() {
    let events = ics::parse(&fixture("bea-release-calendar.ics")).unwrap();
    let entries = rules::entries_from_ics("bea", &events);
    let keys: Vec<&str> = entries.iter().map(|e| e.event_key.as_str()).collect();
    assert_eq!(keys, ["macro:us.gdp:2026-Q3", "macro:us.gdp:2026-Q4"]);
    assert_eq!(entries[0].expected_day, d(2026, 10, 29));
    assert_eq!(
        entries[0].expected_at.unwrap().to_rfc3339(),
        "2026-10-29T12:30:00+00:00"
    );
    assert_eq!(entries[1].expected_day, d(2027, 1, 28));
}

#[test]
fn fomc_page_gives_the_statement_day_of_each_meeting() {
    let html = String::from_utf8(fixture("fomccalendars-2026-2027.htm")).unwrap();
    let meetings = fomc::meetings(&html).unwrap();
    assert_eq!(meetings.len(), 16);
    assert_eq!(meetings[0], (d(2026, 1, 27), d(2026, 1, 28)));
    // `8-9*` is the December meeting with projections: the asterisk does not change the date.
    assert!(meetings.contains(&(d(2026, 12, 8), d(2026, 12, 9))));
    let e = fomc::entries(&html, d(2026, 11, 18)).unwrap();
    let keys: Vec<&str> = e.iter().map(|e| e.event_key.as_str()).collect();
    assert_eq!(keys[0], "policy:fed:2026-12-09");
    assert_eq!(keys.len(), 9, "December 2026 and the eight 2027 meetings");
    assert_eq!(*keys.last().unwrap(), "policy:fed:2027-11-17");
    assert!(
        e.iter()
            .all(|e| e.source_id == "cb-fed" && e.origin == "html")
    );
}

#[test]
fn fomc_cross_month_meetings_and_a_changed_page() {
    let html = r#"<a id="1">2027 FOMC Meetings</a>
        <div class="fomc-meeting__month col"><strong>Apr/May</strong></div>
        <div class="fomc-meeting__date col">28-1</div>"#;
    assert_eq!(
        fomc::meetings(html).unwrap(),
        [(d(2027, 4, 28), d(2027, 5, 1))]
    );
    assert!(fomc::meetings("<html>redesigned</html>").is_err());
    let cut = r#"<a id="1">2027 FOMC Meetings</a><div class="fomc-meeting__date col">28-29</div>"#;
    assert!(
        fomc::meetings(cut).is_err(),
        "a date before any month is a shape error"
    );
}

#[test]
fn the_repo_seed_file_is_hand_checked_and_builds_policy_keys() {
    let path = format!(
        "{}/../../../data/calendar/seed.toml",
        env!("CARGO_MANIFEST_DIR")
    );
    let s = seed::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(!s.events.is_empty());
    let e = seed::entries(&s, d(2026, 11, 18));
    assert_eq!(e[0].event_key, "policy:boe:2026-12-17");
    assert!(e.iter().any(|e| e.event_key == "policy:boe:2027-02-04"));
    assert_eq!(
        e.iter()
            .filter(|e| e.event_key.starts_with("policy:boe:"))
            .count(),
        9
    );
    assert!(seed::stale(&s, d(2026, 12, 1)).is_empty());
}

#[test]
fn derived_earnings_use_the_companys_own_period_labels() {
    let history = [
        earnings::Filing {
            cik: "320193".into(),
            fiscal_year: 2026,
            fiscal_period: "Q4".into(),
            filed: d(2026, 10, 30),
        },
        earnings::Filing {
            cik: "320193".into(),
            fiscal_year: 2026,
            fiscal_period: "Q1".into(),
            filed: d(2026, 1, 30),
        },
    ];
    let e = earnings::derive(&history, d(2026, 11, 18));
    assert_eq!(e[0].event_key, "earnings:0000320193:2027Q1");
    assert_eq!(e[0].expected_day, d(2027, 1, 30));
    assert_eq!(e[0].window_days, 7);
    assert_eq!(e[1].event_key, "earnings:0000320193:2027Q4");
    assert!(news_ledger::eventkey::is_valid(&e[1].event_key));
}

/// On a virtual clock: a CPI release at 13:30 UTC is pre-warmed the day before, captured from
/// 13:32, retried every 5 minutes for 2 hours, then hourly, and ends `late` 26 hours after the start.
#[test]
fn capture_follows_the_schedule_on_a_virtual_clock_and_ends_late() {
    let entries =
        rules::entries_from_ics("bls", &ics::parse(&fixture("bls-schedule.ics")).unwrap());
    let cpi = entries
        .iter()
        .find(|e| e.event_key == "macro:us.cpi:2026-11")
        .unwrap();
    let t = schedule::Timing {
        expected_day: cpi.expected_day,
        expected_at: cpi.expected_at,
        window_days: cpi.window_days,
    };
    assert_eq!(cpi.expected_day, d(2026, 12, 10));
    assert_eq!(
        schedule::prewarm_at(&t),
        Utc.with_ymd_and_hms(2026, 12, 9, 0, 0, 0).unwrap()
    );
    let start = schedule::capture_start(&t);
    assert_eq!(
        start,
        Utc.with_ymd_and_hms(2026, 12, 10, 13, 32, 0).unwrap()
    );

    // Drive a virtual clock: each attempt "fails" (the release never shows up).
    let mut now = start;
    let mut attempts = vec![now];
    let mut n = 1;
    let final_state = loop {
        match schedule::attempt(start, n) {
            schedule::Next::At(next) => {
                assert!(next > now, "time moves forward");
                now = next;
                attempts.push(now);
                n += 1;
            }
            schedule::Next::Late => break "late",
        }
    };
    assert_eq!(final_state, "late");
    assert_eq!(attempts.len(), 49);
    let gap = |i: usize| (attempts[i + 1] - attempts[i]).num_minutes();
    assert!(
        (0..24).all(|i| gap(i) == 5),
        "5-minute retries for the first 2 hours"
    );
    assert!((24..48).all(|i| gap(i) == 60), "then hourly");
    assert_eq!(now, schedule::late_after(&t));
    assert_eq!(now, Utc.with_ymd_and_hms(2026, 12, 11, 15, 32, 0).unwrap());
}
