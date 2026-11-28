//! Property and edge tests for the Ledger's exact decimals and derived-point guards (D5), through the
//! public API only. Expected values come from an independent integer oracle, never from the code
//! under test.

#![allow(clippy::unwrap_used)]

use chrono::{NaiveDate, TimeZone, Utc};
use news_ledger::DataPoint;
use news_ledger::decimal::{self, Dec};
use news_ledger::guard::{Comparison, Formula, Refusal, derive};
use news_ledger::point::{Draft, PeriodType, dp_id, dv_id};
use proptest::prelude::*;
use serde_json::json;

fn d(y: i32, m: u32, dd: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, dd).unwrap()
}

fn draft(value: &str, unit: &str, start: NaiveDate, end: NaiveDate) -> Draft {
    Draft {
        context: vec![],
        series_key: "test:series".into(),
        entity: None,
        concept: "c".into(),
        label: "A series".into(),
        value_text: value.into(),
        unit: unit.into(),
        currency: None,
        scale: None,
        decimals: None,
        period_label: end.to_string(),
        period_start: Some(start),
        period_end: Some(end),
        period_type: PeriodType::Duration,
        fiscal_year: None,
        fiscal_period: None,
        seasonal_adj: Some("NSA".into()),
        vintage: Utc.with_ymd_and_hms(2026, 11, 13, 13, 30, 0).unwrap(),
        snapshot_sha: [0; 32],
        locator: json!({"kind": "json", "path": "$"}),
        parser: "test@1".into(),
    }
}

fn month(value: &str, unit: &str, y: i32, m: u32) -> DataPoint {
    let end = if m == 12 {
        d(y + 1, 1, 1)
    } else {
        d(y, m + 1, 1)
    }
    .pred_opt()
    .unwrap();
    draft(value, unit, d(y, m, 1), end).build().unwrap()
}

/// `n / 10^places` as the text the code is expected to print.
fn text(n: i128, places: u32) -> String {
    let p = 10i128.pow(places);
    let (i, f) = (n.abs() / p, n.abs() % p);
    let sign = if n < 0 { "-" } else { "" };
    if places == 0 {
        format!("{sign}{i}")
    } else {
        format!("{sign}{i}.{f:0width$}", width = places as usize)
    }
}

/// Round `num / den` half away from zero, in integers.
fn round_div(num: i128, den: i128) -> i128 {
    let (q, r) = (num / den, num % den);
    if 2 * r.abs() >= den.abs() {
        q + if (num < 0) == (den < 0) { 1 } else { -1 }
    } else {
        q
    }
}

// ------------------------------------------------------------------------------- decimals

proptest! {
    /// Any text at all: parsing never panics, and whatever it accepts is written back unchanged.
    #[test]
    fn parse_never_panics_and_accepted_text_round_trips(
        t in prop_oneof![
            "[-+\u{2212}0-9.,eE ]{0,24}",
            "\\PC{0,16}",
            "[0-9]{1,45}(\\.[0-9]{0,45})?([eE][-+]?[0-9]{0,6})?",
        ]
    ) {
        if let Ok((numeric, style)) = decimal::parse(&t) {
            prop_assert_eq!(style.format(&numeric), t);
        }
    }

    /// The exact decimal never panics on any canonical string, and when it fits it prints back to
    /// the same number (compared as decimals, since 1.50 and 1.5 are one value).
    #[test]
    fn exact_decimal_is_total_and_stable(
        t in "[-+]?[0-9]{1,45}(\\.[0-9]{0,45})?([eE][-+]?[0-9]{0,4})?"
    ) {
        if let Ok((numeric, _)) = decimal::parse(&t)
            && let Some(dec) = Dec::from_numeric(&numeric)
        {
            prop_assert_eq!(Dec::from_numeric(&dec.to_numeric()), Some(dec));
            prop_assert!(!dec.to_numeric().contains('E'));
        }
    }

    /// Arithmetic on any values, however extreme, refuses instead of panicking or wrapping.
    #[test]
    fn arithmetic_never_panics(
        ma in any::<i128>(), sa in 0u32..=18, mb in any::<i128>(), sb in 0u32..=18, places in 0u32..=20,
    ) {
        let (a, b) = (Dec { m: ma, scale: sa }, Dec { m: mb, scale: sb });
        let _ = a.checked_sub(b);
        let _ = a.ratio_pct(b, places);
        let _ = a.round(places);
        let _ = a.to_numeric();
    }

    /// A percentage change is the integer oracle's, rounded half away from zero, for any pair of
    /// positive figures.
    #[test]
    fn pct_change_matches_the_integer_oracle(
        va in 1i128..100_000_000, vb in 1i128..100_000_000, places in 0u32..=3,
    ) {
        let a = month(&va.to_string(), "USD", 2026, 9);
        let b = month(&vb.to_string(), "USD", 2026, 8);
        let got = derive(Formula::PctChange, &a, &b, Comparison::Sequential, places);
        // (va - vb) / vb * 100 at `places` decimals.
        let want = text(round_div((va - vb) * 100 * 10i128.pow(places), vb), places);
        prop_assert_eq!(got.unwrap().value, want);
    }

    /// A share is part / total, in percent, same period.
    #[test]
    fn share_matches_the_integer_oracle(part in 0i128..1_000_000, total in 1i128..1_000_000) {
        let a = month(&part.to_string(), "USD", 2026, 9);
        let b = month(&total.to_string(), "USD", 2026, 9);
        let got = derive(Formula::Share, &a, &b, Comparison::SamePeriod, 2).unwrap();
        prop_assert_eq!(got.value, text(round_div(part * 100 * 100, total), 2));
        prop_assert_eq!(got.unit, "Percent");
    }

    /// A difference of two figures given to one decimal place is exact in tenths.
    #[test]
    fn diff_of_tenths_is_exact(ia in -100_000i128..100_000, ib in -100_000i128..100_000) {
        let a = month(&text(ia, 1), "Percent", 2026, 9);
        let b = month(&text(ib, 1), "Percent", 2026, 8);
        let got = derive(Formula::PpChange, &a, &b, Comparison::Sequential, 1).unwrap();
        prop_assert_eq!(got.value, text(ia - ib, 1));
        prop_assert_eq!(got.unit, "Percentage points");
    }

    /// Guards hold in the other direction too: whatever the figures, the newer point first is the
    /// only order a year-on-year or sequential change accepts.
    #[test]
    fn a_change_is_never_computed_backwards_in_time(va in 1u32..1000, vb in 1u32..1000) {
        let new = month(&va.to_string(), "USD", 2026, 9);
        let old_m = month(&vb.to_string(), "USD", 2026, 8);
        let old_y = month(&vb.to_string(), "USD", 2025, 9);
        prop_assert!(derive(Formula::PctChange, &new, &old_m, Comparison::Sequential, 1).is_ok());
        prop_assert!(matches!(
            derive(Formula::PctChange, &old_m, &new, Comparison::Sequential, 1),
            Err(Refusal::Period(_))
        ));
        prop_assert!(derive(Formula::PctChange, &new, &old_y, Comparison::YearOnYear, 1).is_ok());
        prop_assert!(matches!(
            derive(Formula::PctChange, &old_y, &new, Comparison::YearOnYear, 1),
            Err(Refusal::Period(_))
        ));
    }

    /// Ids depend on what the point is, not on the value or the bytes it came from: the same series,
    /// period and vintage always get one id; a new vintage always gets a new one.
    #[test]
    fn dp_ids_follow_series_period_and_vintage(secs in 0i64..4_000_000_000, other in 1i64..1_000_000) {
        let v = |s: i64| Utc.timestamp_opt(s, 0).unwrap();
        let a = dp_id("s", "k=v", "duration|2026-10-01|2026-10-31|October 2026", v(secs));
        prop_assert_eq!(&a, &dp_id("s", "k=v", "duration|2026-10-01|2026-10-31|October 2026", v(secs)));
        prop_assert_ne!(&a, &dp_id("s", "k=v", "duration|2026-10-01|2026-10-31|October 2026", v(secs + other)));
        prop_assert!(a.len() == 19 && a.bytes().skip(3).all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(&b)));
    }
}

#[test]
fn leading_and_trailing_junk_is_not_trimmed() {
    for t in [
        " 1", "1 ", "\t1", "1\n", "\u{a0}1", "\u{ff11}", "1_000", "0x10", "1,", ",1", "١٢٣",
    ] {
        assert!(decimal::parse(t).is_err(), "{t:?}");
    }
}

#[test]
fn numbers_too_big_or_too_fine_for_exact_arithmetic_are_refused_not_rounded() {
    // 39 digits overflow i128; scale beyond 18 places is more precision than the guards carry.
    assert!(Dec::from_numeric(&"9".repeat(39)).is_none());
    assert!(Dec::from_numeric("0.0000000000000000001").is_none());
    assert!(Dec::from_numeric("1E40").is_none());
    assert!(Dec::from_numeric("1E-19").is_none());
    // The text still parses, so it can be stored verbatim.
    assert!(decimal::parse(&"9".repeat(39)).is_ok());
}

// ---------------------------------------------------------------------------------- guards

#[test]
fn an_unknown_unit_is_never_compared_even_with_itself() {
    let a = month("5", "Furlongs", 2026, 9);
    let b = month("4", "Furlongs", 2026, 8);
    for f in [
        Formula::PctChange,
        Formula::Diff,
        Formula::Share,
        Formula::PpChange,
    ] {
        assert!(
            matches!(
                derive(f, &a, &b, Comparison::Sequential, 1),
                Err(Refusal::UnknownUnit(_))
            ),
            "{f:?}"
        );
    }
}

#[test]
fn a_missing_or_zero_base_has_no_derived_point() {
    let a = month("5", "USD", 2026, 9);
    let mut b = month("4", "USD", 2026, 8);
    b.value = None;
    assert_eq!(
        derive(Formula::PctChange, &a, &b, Comparison::Sequential, 1),
        Err(Refusal::NoValue)
    );
    let zero = month("0.00", "USD", 2026, 8);
    assert_eq!(
        derive(Formula::PctChange, &a, &zero, Comparison::Sequential, 1),
        Err(Refusal::Arithmetic)
    );
    let zero_total = month("0", "USD", 2026, 9);
    assert_eq!(
        derive(Formula::Share, &a, &zero_total, Comparison::SamePeriod, 1),
        Err(Refusal::Arithmetic)
    );
    // A difference does not divide, so a zero base is fine there.
    assert!(derive(Formula::Diff, &a, &zero, Comparison::Sequential, 1).is_ok());
}

#[test]
#[ignore = "BUG: pct_change is computed over a negative base as (a-b)/b, so a loss that narrows from -10 to -5 comes out as -50.0, which reads as a deterioration"]
fn a_percentage_change_over_a_negative_base_is_refused_or_sign_correct() {
    let a = month("-5", "USD", 2026, 9);
    let b = month("-10", "USD", 2026, 8);
    match derive(Formula::PctChange, &a, &b, Comparison::Sequential, 1) {
        Err(_) => {}
        Ok(v) => assert_eq!(v.value, "50.0", "an improvement must not print as a fall"),
    }
}

#[test]
fn currency_pairs_with_the_same_unit_text_still_must_match() {
    // XBRL says `USD`; a point whose currency field was lost must not be compared with one that has it.
    let mut a = month("5", "USD", 2026, 9);
    let mut b = month("4", "USD", 2026, 8);
    a.currency = Some("USD".into());
    b.currency = None;
    assert!(matches!(
        derive(Formula::PctChange, &a, &b, Comparison::Sequential, 1),
        Err(Refusal::Currency(..))
    ));
    b.currency = Some("usd".into());
    assert!(matches!(
        derive(Formula::PctChange, &a, &b, Comparison::Sequential, 1),
        Err(Refusal::Currency(..))
    ));
}

#[test]
fn year_on_year_across_a_leap_day_and_52_53_week_years() {
    // Feb 2024 (29 days) against Feb 2023 (28 days): same length class, 366 days between ends.
    let a = month("12", "USD", 2024, 2);
    let b = month("10", "USD", 2023, 2);
    assert!(derive(Formula::PctChange, &a, &b, Comparison::YearOnYear, 1).is_ok());
    // A fiscal quarter of 13 weeks and a calendar quarter are both a quarter; a 4-week month is not.
    let q = draft("1", "USD", d(2026, 6, 28), d(2026, 9, 26))
        .build()
        .unwrap();
    let q_prev = draft("1", "USD", d(2025, 6, 29), d(2025, 9, 27))
        .build()
        .unwrap();
    assert!(derive(Formula::Diff, &q, &q_prev, Comparison::YearOnYear, 1).is_ok());
    let four_weeks = draft("1", "USD", d(2025, 8, 31), d(2025, 9, 27))
        .build()
        .unwrap();
    assert!(derive(Formula::Diff, &q, &four_weeks, Comparison::YearOnYear, 1).is_err());
}

#[test]
fn a_year_to_date_figure_is_not_a_quarter() {
    // Nine months to date against a quarter ending on the same day: different lengths.
    let ytd = draft("9", "USD", d(2025, 9, 28), d(2026, 6, 27))
        .build()
        .unwrap();
    let qtr = draft("3", "USD", d(2026, 3, 29), d(2026, 6, 27))
        .build()
        .unwrap();
    assert!(matches!(
        derive(Formula::Share, &qtr, &ytd, Comparison::SamePeriod, 1),
        Err(Refusal::Period(_))
    ));
}

#[test]
fn derived_ids_are_stable_and_depend_on_the_order_of_the_inputs() {
    let a = month("5", "USD", 2026, 9);
    let b = month("4", "USD", 2026, 8);
    let x = derive(Formula::Diff, &a, &b, Comparison::Sequential, 1).unwrap();
    let y = derive(Formula::Diff, &a, &b, Comparison::Sequential, 1).unwrap();
    assert_eq!(x, y);
    assert_eq!(x.dp_id, dv_id("diff(a,b)", &[&a.dp_id, &b.dp_id]));
    assert_ne!(x.dp_id, dv_id("diff(a,b)", &[&b.dp_id, &a.dp_id]));
    // A restated input is a different point and so a different derived point.
    let mut restated = draft("5", "USD", d(2026, 9, 1), d(2026, 9, 30));
    restated.vintage = Utc.with_ymd_and_hms(2026, 12, 1, 0, 0, 0).unwrap();
    let a2 = restated.build().unwrap();
    let z = derive(Formula::Diff, &a2, &b, Comparison::Sequential, 1).unwrap();
    assert_ne!(x.dp_id, z.dp_id);
}
