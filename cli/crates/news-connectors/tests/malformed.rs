//! Malformed, partial and hostile responses for the three Stringers of #9 (SEC, FRED, BLS). Each is
//! an edited copy of a recorded or synthetic fixture, made inside the test, so no number is typed
//! by hand beyond what the fixture contains. Nothing touches the network.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use news_connectors::{Cx, ItemRef, ParseError, ParseErrorKind, Parsed, Snapshot, connector};
use news_core::job::ErrorClass;
use serde_json::json;

fn fixture(path: &str) -> String {
    let p = format!("{}/tests/fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    String::from_utf8(std::fs::read(&p).unwrap_or_else(|e| panic!("{p}: {e}"))).unwrap()
}

fn at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 13, 14, 0, 0).unwrap()
}

fn snap(url: &str, body: &str) -> Snapshot {
    Snapshot {
        url: url.to_owned(),
        sha256: [7; 32],
        fetched_at: at(),
        content_type: Some("application/json".into()),
        bytes: body.as_bytes().to_vec(),
    }
}

fn item(source: &str, kind: &str, key: &str, meta: serde_json::Value) -> ItemRef {
    ItemRef {
        source_id: source.into(),
        kind: kind.into(),
        key: key.into(),
        url: format!("https://example.test/{key}"),
        event_key_hint: None,
        published_at: Some(at()),
        title: None,
        meta,
    }
}

/// `body.replacen(from, to, 1)`, failing loudly when the edit did not apply (a silent no-op would
/// make the test pass for the wrong reason).
fn edit(body: &str, from: &str, to: &str) -> String {
    assert!(body.contains(from), "fixture no longer contains {from:?}");
    body.replacen(from, to, 1)
}

// ---------------------------------------------------------------------------------------- BLS

const BLS_CPI: &str = "https://api.bls.gov/publicAPI/v2/timeseries/data/CUUR0000SA0";

fn bls(body: &str) -> Result<Parsed, ParseError> {
    connector("bls").unwrap().parse(
        &item("bls", "bls.series", "CUUR0000SA0", json!({})),
        &[snap(BLS_CPI, body)],
    )
}

fn bls_fixture() -> String {
    fixture("bls/CUUR0000SA0.json")
}

#[test]
fn bls_garbage_bodies_are_typed_errors_never_panics() {
    for body in [
        "",
        "null",
        "[]",
        "{}",
        "<html>Service unavailable</html>",
        r#"{"status":"REQUEST_SUCCEEDED","Results":{"series":"x"}}"#,
        r#"{"status":"REQUEST_SUCCEEDED","Results":{"series":[{"seriesID":"CUUR0000SA0","data":"x"}]}}"#,
    ] {
        let e = bls(body).expect_err(body);
        assert!(
            matches!(
                e.class(),
                ErrorClass::UpstreamChanged | ErrorClass::InvalidData
            ),
            "{body}: {e:?}"
        );
    }
}

#[test]
fn bls_a_success_without_results_is_a_shape_error() {
    for body in [
        r#"{"status":"REQUEST_SUCCEEDED"}"#,
        r#"{"status":"REQUEST_SUCCEEDED","Results":null}"#,
    ] {
        assert!(bls(body).is_err(), "{body}");
    }
}

#[test]
fn bls_value_with_a_stray_character_is_refused_not_trimmed() {
    // A value is stored as the source printed it; one we cannot read exactly is an error with the
    // locator, never a silently repaired number.
    for bad in [" 337.511", "337.511 ", "3.3.7", "337.511*", "１２３"] {
        let body = edit(&bls_fixture(), "\"337.511\"", &format!("{bad:?}"));
        let e = bls(&body).expect_err(bad);
        assert_eq!(e.kind, ParseErrorKind::Value, "{bad:?}");
        assert!(e.locator.ends_with(".value"), "{bad:?}: {}", e.locator);
    }
}

#[test]
fn bls_impossible_periods_and_years_are_refused() {
    let month = "\"period\":\"M10\",\"periodName\":\"October\",\"latest\"";
    for (from, to) in [
        (
            month,
            "\"period\":\"M14\",\"periodName\":\"October\",\"latest\"",
        ),
        (
            month,
            "\"period\":\"M00\",\"periodName\":\"October\",\"latest\"",
        ),
        (
            "\"year\":\"2026\",\"period\":\"M10\"",
            "\"year\":\"twenty\",\"period\":\"M10\"",
        ),
    ] {
        let e = bls(&edit(&bls_fixture(), from, to)).expect_err(to);
        assert_eq!(e.kind, ParseErrorKind::Value, "{to}: {e:?}");
    }
}

#[test]
fn bls_with_no_rows_gives_no_points_and_no_candidate() {
    let body = r#"{"status":"REQUEST_SUCCEEDED","Results":{"series":[{"seriesID":"CUUR0000SA0","data":[]}]}}"#;
    let p = bls(body).unwrap();
    assert!(p.points.is_empty() && p.missing.is_empty() && p.candidates.is_empty());
}

#[test]
fn bls_a_missing_latest_period_makes_no_candidate_without_figures() {
    // If the newest period is the one BLS could not publish ("-"), there is no figure to write a
    // story about; a candidate with an empty `dp_ids` would carry an event key and no numbers.
    let body = edit(&bls_fixture(), "\"value\":\"337.511\"", "\"value\":\"-\"");
    let p = bls(&body).unwrap();
    assert!(
        p.candidates.iter().all(|c| !c.dp_ids.is_empty()),
        "candidate without data points: {:?}",
        p.candidates
    );
}

#[test]
fn bls_a_repeated_period_with_a_different_value_is_a_conflict_and_a_copy_collapses() {
    let row = |period: &str, name: &str, value: &str| {
        format!(
            "{{\"year\":\"2026\",\"period\":\"{period}\",\"periodName\":\"{name}\",\"value\":\"{value}\""
        )
    };
    let sept = row("M09", "September", "336.940");
    let conflict = edit(&bls_fixture(), &sept, &row("M10", "October", "999.999"));
    let e = bls(&conflict).unwrap_err();
    assert_eq!(e.kind, ParseErrorKind::Value);
    assert!(e.message.contains("conflicting observations"), "{e:?}");
    // The same row twice is one point, not two with one dp_id.
    let copy = edit(&bls_fixture(), &sept, &row("M10", "October", "337.511"));
    let p = bls(&copy).unwrap();
    let unique: std::collections::BTreeSet<_> = p.points.iter().map(|d| &d.dp_id).collect();
    assert_eq!(unique.len(), p.points.len());
}

// --------------------------------------------------------------------------------------- FRED

const FRED_META: &str = "https://api.stlouisfed.org/fred/series?series_id=CPIAUCSL&file_type=json";
const FRED_OBS: &str = "https://api.stlouisfed.org/fred/series/observations?series_id=CPIAUCSL&realtime_start=1776-07-04&file_type=json";

fn fred_with(meta: &str, obs: &str) -> Result<Parsed, ParseError> {
    connector("fred").unwrap().parse(
        &item(
            "fred",
            "fred.series",
            "CPIAUCSL",
            json!({"vintage": "2026-11-13"}),
        ),
        &[snap(FRED_META, meta), snap(FRED_OBS, obs)],
    )
}

fn fred_obs(edit_obs: impl Fn(&str) -> String) -> Result<Parsed, ParseError> {
    fred_with(
        &fixture("fred/series-CPIAUCSL.json"),
        &edit_obs(&fixture("fred/observations-CPIAUCSL.json")),
    )
}

#[test]
fn fred_api_error_bodies_are_shape_errors() {
    // FRED answers a bad request with JSON like this; the observations are simply not there.
    let e = fred_with(
        &fixture("fred/series-CPIAUCSL.json"),
        r#"{"error_code":400,"error_message":"Bad Request.  The series does not exist."}"#,
    )
    .unwrap_err();
    assert_eq!(e.class(), ErrorClass::UpstreamChanged);
    let e = fred_with(
        r#"{"error_code":400,"error_message":"Bad Request."}"#,
        &fixture("fred/observations-CPIAUCSL.json"),
    )
    .unwrap_err();
    assert_eq!(e.class(), ErrorClass::UpstreamChanged);
}

#[test]
fn fred_metadata_for_another_series_is_not_accepted() {
    let meta = edit(
        &fixture("fred/series-CPIAUCSL.json"),
        "\"id\": \"CPIAUCSL\"",
        "\"id\": \"UNRATE\"",
    );
    let e = fred_with(&meta, &fixture("fred/observations-CPIAUCSL.json")).unwrap_err();
    assert_eq!(e.kind, ParseErrorKind::Shape);
}

#[test]
fn fred_one_snapshot_alone_is_a_missing_snapshot_bug_not_a_parse() {
    let c = connector("fred").unwrap();
    let it = item("fred", "fred.series", "CPIAUCSL", json!({}));
    let only_obs = [snap(FRED_OBS, &fixture("fred/observations-CPIAUCSL.json"))];
    assert_eq!(
        c.parse(&it, &only_obs).unwrap_err().kind,
        ParseErrorKind::MissingSnapshot
    );
    let only_meta = [snap(FRED_META, &fixture("fred/series-CPIAUCSL.json"))];
    assert_eq!(
        c.parse(&it, &only_meta).unwrap_err().kind,
        ParseErrorKind::MissingSnapshot
    );
}

#[test]
fn fred_bad_values_and_dates_carry_their_locator() {
    for (from, to, path_end) in [
        ("\"value\": \"323.364\"", "\"value\": \"n/a?\"", ".value"),
        (
            "\"date\": \"2025-08-01\"",
            "\"date\": \"2025-13-01\"",
            ".date",
        ),
        (
            "\"date\": \"2025-08-01\"",
            "\"date\": \"Aug 2025\"",
            ".date",
        ),
        (
            "\"realtime_start\": \"2025-09-11\"",
            "\"realtime_start\": \"11/09/2025\"",
            ".realtime_start",
        ),
    ] {
        let e = fred_obs(|b| edit(b, from, to)).expect_err(to);
        assert_eq!(e.kind, ParseErrorKind::Value, "{to}: {e:?}");
        assert!(e.locator.ends_with(path_end), "{to}: {}", e.locator);
    }
}

#[test]
fn fred_unknown_units_keep_the_values_and_flag_a_human() {
    let meta = edit(
        &fixture("fred/series-CPIAUCSL.json"),
        "\"units\": \"Index 1982-1984=100\"",
        "\"units\": \"Furlongs per fortnight\"",
    );
    let p = fred_with(&meta, &fixture("fred/observations-CPIAUCSL.json")).unwrap();
    assert_eq!(p.needs_human.len(), 1);
    assert!(p.needs_human[0].reason.contains("Furlongs per fortnight"));
    assert!(!p.points.is_empty());
    assert!(p.points.iter().all(|d| d.unit == "Furlongs per fortnight"));
}

#[test]
fn fred_every_observation_missing_is_all_missing_and_no_candidate() {
    let p = fred_obs(|b| {
        let mut v: serde_json::Value = serde_json::from_str(b).unwrap();
        for o in v["observations"].as_array_mut().unwrap() {
            o["value"] = json!(".");
        }
        v.to_string()
    })
    .unwrap();
    assert!(p.points.is_empty());
    assert!(p.candidates.is_empty());
    assert!(p.missing.len() >= 6);
    assert!(p.missing.iter().all(|m| m.value_text == "."));
}

#[test]
fn fred_discovery_with_a_garbled_cursor_starts_over_instead_of_failing() {
    let c = connector("fred").unwrap();
    let cx = Cx {
        today: NaiveDate::from_ymd_opt(2026, 11, 16).unwrap(),
        sec_ciks: vec![],
        fred_series: vec!["CPIAUCSL".into()],
        bls_series: vec![],
        tracked: Default::default(),
    };
    for cursor in ["", "not json", "[]", "{\"CPIAUCSL\":5}", "null"] {
        let reqs = c.discovery_requests(&cx, Some(cursor));
        assert!(!reqs.is_empty(), "cursor {cursor:?}");
    }
}

// ---------------------------------------------------------------------------------------- SEC

const FACTS: &str = "https://data.sec.gov/api/xbrl/companyfacts/CIK0000320193.json";
const K26: &str = "0000320193-26-000105";

fn sec_with(body: &str) -> Result<Parsed, ParseError> {
    connector("sec-edgar").unwrap().parse(
        &item(
            "sec-edgar",
            "sec.filing",
            K26,
            json!({"cik": "0000320193", "form": "10-K", "items": ""}),
        ),
        &[snap(FACTS, body)],
    )
}

fn facts() -> String {
    fixture("sec/companyfacts-CIK0000320193.json")
}

#[test]
fn sec_garbage_bodies_are_typed_errors_never_panics() {
    for body in [
        "",
        "null",
        "[]",
        "{}",
        "<html>Request Rate Threshold Exceeded</html>",
        "{\"facts\":{}}",
    ] {
        // An empty `facts` object is a valid response with nothing in it yet: the filing is not
        // there, so the job must be retried, never turned into an empty success.
        let e = sec_with(body).expect_err(body);
        assert!(
            matches!(
                e.class(),
                ErrorClass::UpstreamChanged | ErrorClass::InvalidData | ErrorClass::Transient
            ),
            "{body}: {e:?}"
        );
    }
}

#[test]
fn sec_a_value_that_is_not_a_json_number_is_refused() {
    for bad in ["\"440000000000\"", "null", "true", "[1]"] {
        let body = edit(&facts(), "\"val\":440000000000", &format!("\"val\":{bad}"));
        let e = sec_with(&body).expect_err(bad);
        assert!(
            matches!(
                e.class(),
                ErrorClass::InvalidData | ErrorClass::UpstreamChanged
            ),
            "{bad}: {e:?}"
        );
    }
}

#[test]
fn sec_numbers_stay_exactly_as_the_json_token_was_written() {
    let body = edit(&facts(), "\"val\":440000000000", "\"val\":-440000000000.00");
    let p = sec_with(&body).unwrap();
    let hit = p
        .points
        .iter()
        .find(|d| d.value_text == "-440000000000.00")
        .expect("verbatim token kept");
    assert_eq!(hit.value.as_deref(), Some("-440000000000.00"));
    // Beyond 2^53: a float round trip would corrupt the last digit.
    let body = edit(&facts(), "\"val\":440000000000", "\"val\":9007199254740993");
    let p = sec_with(&body).unwrap();
    assert!(p.points.iter().any(|d| d.value_text == "9007199254740993"));
}

#[test]
fn sec_a_filing_with_an_impossible_date_is_refused_with_its_locator() {
    for (from, to) in [
        ("\"end\":\"2026-09-26\"", "\"end\":\"2026-02-30\""),
        ("\"filed\":\"2026-10-31\"", "\"filed\":\"31 October 2026\""),
    ] {
        let e = sec_with(&edit(&facts(), from, to)).expect_err(to);
        assert_eq!(e.kind, ParseErrorKind::Value, "{to}: {e:?}");
        assert!(e.locator.starts_with("$.facts"), "{}", e.locator);
    }
}
