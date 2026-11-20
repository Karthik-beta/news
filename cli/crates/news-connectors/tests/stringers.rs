//! Golden tests for the Stringers, from recorded and synthetic fixtures only (no network): see
//! `tests/fixtures/README.md` for where each file came from.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use news_connectors::{Cx, ItemRef, ParseErrorKind, Parsed, Snapshot, bls, connector, fred, sec};
use news_core::job::ErrorClass;
use serde_json::json;

fn fixture(path: &str) -> Vec<u8> {
    let p = format!("{}/tests/fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

fn snap(url: &str, bytes: Vec<u8>, fetched_at: DateTime<Utc>) -> Snapshot {
    Snapshot {
        url: url.to_owned(),
        sha256: [0; 32],
        fetched_at,
        content_type: Some("application/json".into()),
        bytes,
    }
}

fn t(y: i32, m: u32, d: u32, h: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, h, 0, 0).unwrap()
}

fn cx() -> Cx {
    Cx {
        today: NaiveDate::from_ymd_opt(2026, 11, 16).unwrap(),
        sec_ciks: vec!["320193".into()],
        fred_series: vec!["CPIAUCSL".into()],
        bls_series: vec!["CUUR0000SA0".into()],
        tracked: Default::default(),
    }
}

fn ids(p: &Parsed) -> Vec<String> {
    p.points.iter().map(|d| d.dp_id.clone()).collect()
}

/// Every point: exact decimal formats back to the text, provenance present, dp_id shaped for the table.
fn check_points(p: &Parsed, parser: &str) {
    for d in &p.points {
        let v = d.value.as_deref().unwrap();
        assert_eq!(d.style.format(v), d.value_text, "{}", d.dp_id);
        assert_eq!(d.parser, parser);
        assert!(d.locator.get("kind").is_some() && d.locator.get("path").is_some());
        assert!(d.dp_id.starts_with("dp_") && d.dp_id.len() == 19);
    }
    for c in &p.candidates {
        if let Some(k) = &c.event_key {
            assert!(news_ledger::eventkey::is_valid(k), "{k}");
        }
        for id in &c.dp_ids {
            assert!(
                p.points.iter().any(|d| &d.dp_id == id),
                "candidate cites unknown {id}"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------- BLS

const BLS_CPI: &str = "https://api.bls.gov/publicAPI/v2/timeseries/data/CUUR0000SA0";
const BLS_CES: &str = "https://api.bls.gov/publicAPI/v2/timeseries/data/CES0000000001";

fn bls_item(id: &str) -> ItemRef {
    ItemRef {
        source_id: "bls".into(),
        kind: "bls.series".into(),
        key: id.into(),
        url: format!("https://data.bls.gov/timeseries/{id}"),
        event_key_hint: None,
        published_at: None,
        title: None,
        meta: json!({}),
    }
}

#[test]
fn bls_cpi_parses_verbatim_with_a_missing_observation() {
    let c = connector("bls").unwrap();
    let s = snap(
        BLS_CPI,
        fixture("bls/CUUR0000SA0.json"),
        t(2026, 11, 13, 14),
    );
    let p = c
        .parse(&bls_item("CUUR0000SA0"), std::slice::from_ref(&s))
        .unwrap();
    check_points(&p, "bls@1");
    assert_eq!(p.points.len(), 35);
    let oct = p
        .points
        .iter()
        .find(|d| d.period_label == "October 2026")
        .unwrap();
    assert_eq!(oct.value_text, "337.511");
    assert_eq!(oct.value.as_deref(), Some("337.511"));
    assert_eq!(oct.unit, "Index 1982-1984=100");
    assert_eq!(oct.seasonal_adj.as_deref(), Some("NSA"));
    assert_eq!(oct.series_key, "bls:CUUR0000SA0");
    assert_eq!(oct.period_start.unwrap().to_string(), "2026-10-01");
    assert_eq!(oct.period_end.unwrap().to_string(), "2026-10-31");
    assert_eq!(oct.vintage, s.fetched_at);
    assert_eq!(oct.locator["path"], "$.Results.series[0].data[0].value");
    // Missing observation: the 2025 lapse in appropriations, kept with its footnote.
    assert_eq!(p.missing.len(), 1);
    let m = &p.missing[0];
    assert_eq!(
        (m.period_label.as_str(), m.value_text.as_str()),
        ("October 2025", "-")
    );
    assert_eq!(m.locator["footnotes"][0]["code"], "X");
    // The annual average row keeps its own label and spans the year.
    let annual = p
        .points
        .iter()
        .find(|d| d.period_label == "Annual 2025")
        .unwrap();
    assert_eq!(annual.period_end.unwrap().to_string(), "2025-12-31");
    let cand = &p.candidates[0];
    assert_eq!(cand.event_key.as_deref(), Some("macro:us.cpi:2026-10"));
    assert_eq!(cand.rule_key, "family:cpi");
    assert_eq!(cand.dp_ids, vec![oct.dp_id.clone()]);
}

#[test]
fn bls_footnoted_preliminary_values_and_revisions() {
    let c = connector("bls").unwrap();
    let first = snap(
        BLS_CES,
        fixture("bls/CES0000000001.json"),
        t(2026, 11, 6, 14),
    );
    let p1 = c.parse(&bls_item("CES0000000001"), &[first]).unwrap();
    check_points(&p1, "bls@1");
    let oct = p1
        .points
        .iter()
        .find(|d| d.period_label == "October 2026")
        .unwrap();
    assert_eq!(oct.value_text, "159293");
    assert_eq!(oct.locator["footnotes"][0]["code"], "P");
    assert_eq!(oct.locator["footnotes"][0]["text"], "preliminary");
    assert_eq!(oct.unit, "All employees, thousands");
    assert!(
        p1.points
            .iter()
            .find(|d| d.period_label == "August 2026")
            .unwrap()
            .locator
            .get("footnotes")
            .is_none()
    );

    // A month later September is revised (edited copy of the recorded response): a new vintage, a new
    // dp_id, the new value; the Ledger links it to the old one through `supersedes`.
    let revised = String::from_utf8(fixture("bls/CES0000000001.json"))
        .unwrap()
        .replace("\"value\":\"159128\"", "\"value\":\"159101\"");
    let later = snap(BLS_CES, revised.into_bytes(), t(2026, 12, 4, 14));
    let p2 = c.parse(&bls_item("CES0000000001"), &[later]).unwrap();
    let sep1 = p1
        .points
        .iter()
        .find(|d| d.period_label == "September 2026")
        .unwrap();
    let sep2 = p2
        .points
        .iter()
        .find(|d| d.period_label == "September 2026")
        .unwrap();
    assert_eq!(
        (sep1.value_text.as_str(), sep2.value_text.as_str()),
        ("159128", "159101")
    );
    assert_ne!(sep1.dp_id, sep2.dp_id);
    assert_eq!(sep1.series_key, sep2.series_key);
    assert!(
        sep2.supersedes.is_none(),
        "parsers never link; the Ledger store does"
    );
}

#[test]
fn bls_reparse_gives_identical_dp_ids() {
    let c = connector("bls").unwrap();
    let s = snap(
        BLS_CPI,
        fixture("bls/CUUR0000SA0.json"),
        t(2026, 11, 13, 14),
    );
    let a = c
        .parse(&bls_item("CUUR0000SA0"), std::slice::from_ref(&s))
        .unwrap();
    let b = c.parse(&bls_item("CUUR0000SA0"), &[s]).unwrap();
    assert_eq!(ids(&a), ids(&b));
}

#[test]
fn bls_discovery_finds_new_periods_and_skips_known_ones() {
    let c = connector("bls").unwrap();
    let reqs = c.discovery_requests(&cx(), None);
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].url, BLS_CPI);
    assert_eq!(reqs[0].secret.as_ref().unwrap().env, "BLS_API_KEY");
    let s = snap(
        BLS_CPI,
        fixture("bls/CUUR0000SA0.json"),
        t(2026, 11, 13, 14),
    );
    let d = c
        .parse_discovery(
            &cx(),
            Some(r#"{"CUUR0000SA0":"2026-M09"}"#),
            std::slice::from_ref(&s),
        )
        .unwrap();
    assert_eq!(d.refs.len(), 1);
    assert_eq!(
        d.refs[0].event_key_hint.as_deref(),
        Some("macro:us.cpi:2026-10")
    );
    let next = d.next_cursor.unwrap();
    assert_eq!(next, r#"{"CUUR0000SA0":"2026-M10"}"#);
    let again = c.parse_discovery(&cx(), Some(&next), &[s]).unwrap();
    assert!(again.refs.is_empty());
}

#[test]
fn bls_failures_are_typed() {
    let c = connector("bls").unwrap();
    let item = bls_item("CUUR0000SA0");
    let bad = |body: &str| {
        c.parse(
            &item,
            &[snap(BLS_CPI, body.as_bytes().to_vec(), t(2026, 11, 13, 14))],
        )
        .unwrap_err()
    };
    let truncated = String::from_utf8(fixture("bls/CUUR0000SA0.json")).unwrap()[..400].to_owned();
    assert_eq!(bad(&truncated).class(), ErrorClass::UpstreamChanged);
    let refused =
        bad(r#"{"status":"REQUEST_NOT_PROCESSED","message":["daily threshold"],"Results":{}}"#);
    assert_eq!(refused.kind, ParseErrorKind::Upstream);
    assert!(refused.message.contains("daily threshold"));
    // BLS declined the request (threshold, overload): snooze, never the dead list.
    assert_eq!(refused.class(), ErrorClass::RateLimited);
    assert_eq!(
        refused.retry_after(),
        Some(std::time::Duration::from_secs(3600))
    );
    let odd = bad(
        r#"{"status":"REQUEST_SUCCEEDED","Results":{"series":[{"seriesID":"CUUR0000SA0","data":[{"year":"2026","period":"X01","periodName":"?","value":"1.0","footnotes":[{}]}]}]}}"#,
    );
    assert_eq!(odd.kind, ParseErrorKind::Value);
    assert!(odd.locator.ends_with(".period"), "{}", odd.locator);
    let junk = bad(
        r#"{"status":"REQUEST_SUCCEEDED","Results":{"series":[{"seriesID":"CUUR0000SA0","data":[{"year":"2026","period":"M01","periodName":"January","value":"1,23","footnotes":[{}]}]}]}}"#,
    );
    assert_eq!(junk.class(), ErrorClass::InvalidData);
    // No snapshot at all.
    assert_eq!(
        c.parse(&item, &[]).unwrap_err().kind,
        ParseErrorKind::MissingSnapshot
    );
}

#[test]
fn bls_unknown_series_is_flagged_not_guessed() {
    let c = connector("bls").unwrap();
    let body = String::from_utf8(fixture("bls/CUUR0000SA0.json"))
        .unwrap()
        .replace("CUUR0000SA0", "CUUR0000SAF");
    let url = "https://api.bls.gov/publicAPI/v2/timeseries/data/CUUR0000SAF";
    let p = c
        .parse(
            &bls_item("CUUR0000SAF"),
            &[snap(url, body.into_bytes(), t(2026, 11, 13, 14))],
        )
        .unwrap();
    assert!(p.points.is_empty() && p.candidates.is_empty());
    assert_eq!(p.needs_human.len(), 1);
}

// --------------------------------------------------------------------------------------- FRED

const FRED_META: &str = "https://api.stlouisfed.org/fred/series?series_id=CPIAUCSL&file_type=json";

fn fred_item() -> ItemRef {
    ItemRef {
        source_id: "fred".into(),
        kind: "fred.series".into(),
        key: "CPIAUCSL".into(),
        url: "https://fred.stlouisfed.org/series/CPIAUCSL".into(),
        event_key_hint: None,
        published_at: Some(t(2026, 11, 13, 0)),
        title: None,
        meta: json!({"vintage": "2026-11-13"}),
    }
}

fn fred_snaps() -> Vec<Snapshot> {
    let c = connector("fred").unwrap();
    let reqs = c.fetch_requests(&fred_item());
    assert_eq!(reqs.len(), 2);
    assert_eq!(reqs[0].url, FRED_META);
    assert!(reqs[1].url.contains(
        "realtime_start=1776-07-04&realtime_end=9999-12-31&observation_start=2024-11-13"
    ));
    vec![
        snap(
            &reqs[0].url,
            fixture("fred/series-CPIAUCSL.json"),
            t(2026, 11, 13, 15),
        ),
        snap(
            &reqs[1].url,
            fixture("fred/observations-CPIAUCSL.json"),
            t(2026, 11, 13, 15),
        ),
    ]
}

#[test]
fn fred_requests_never_carry_the_key() {
    let c = connector("fred").unwrap();
    for r in c
        .discovery_requests(&cx(), None)
        .iter()
        .chain(c.fetch_requests(&fred_item()).iter())
    {
        assert!(!r.url.contains("api_key"), "{}", r.url);
        assert_eq!(r.secret.as_ref().unwrap().param, "api_key");
        assert_eq!(r.secret.as_ref().unwrap().env, "FRED_API_KEY");
    }
}

#[test]
fn fred_parses_every_vintage_and_the_missing_month() {
    let c = connector("fred").unwrap();
    let p = c.parse(&fred_item(), &fred_snaps()).unwrap();
    check_points(&p, "fred@1");
    assert_eq!(p.points.len(), 6);
    assert_eq!(p.missing.len(), 1);
    assert_eq!(p.missing[0].period_label, "2025-10-01");
    assert_eq!(p.missing[0].value_text, ".");
    // A restatement: August 2026 first printed 331.000, revised to 331.112 on 2026-10-15.
    let aug: Vec<_> = p
        .points
        .iter()
        .filter(|d| d.period_label == "2026-08-01")
        .collect();
    assert_eq!(aug.len(), 2);
    assert_eq!(aug[0].value_text, "331.000", "trailing zeros kept verbatim");
    assert_eq!(aug[0].vintage, t(2026, 9, 11, 0));
    assert_eq!(aug[1].value_text, "331.112");
    assert_eq!(aug[1].vintage, t(2026, 10, 15, 0));
    assert_ne!(aug[0].dp_id, aug[1].dp_id);
    let oct = p
        .points
        .iter()
        .find(|d| d.period_label == "2026-10-01")
        .unwrap();
    assert_eq!(oct.unit, "Index 1982-1984=100");
    assert_eq!(oct.seasonal_adj.as_deref(), Some("SA"));
    assert_eq!(oct.locator["realtime_start"], "2026-11-13");
    let cand = &p.candidates[0];
    assert_eq!(cand.event_key.as_deref(), Some("macro:us.cpi:2026-10"));
    assert_eq!(cand.dp_ids, vec![oct.dp_id.clone()]);
    assert_eq!(ids(&p), ids(&c.parse(&fred_item(), &fred_snaps()).unwrap()));
}

#[test]
fn fred_discovery_uses_vintage_dates_and_feeds_the_calendar() {
    let c = connector("fred").unwrap();
    let reqs = c.discovery_requests(&cx(), Some(r#"{"CPIAUCSL":"2026-10-15"}"#));
    assert_eq!(reqs.len(), 2);
    assert!(
        reqs[0]
            .url
            .contains("series/vintagedates?series_id=CPIAUCSL&realtime_start=2026-10-15")
    );
    assert!(
        reqs[1]
            .url
            .contains("release/dates?release_id=10&realtime_start=2026-11-16")
    );
    let snaps = [
        snap(
            &reqs[0].url,
            fixture("fred/vintagedates-CPIAUCSL.json"),
            t(2026, 11, 16, 8),
        ),
        snap(
            &reqs[1].url,
            fixture("fred/release-dates-10.json"),
            t(2026, 11, 16, 8),
        ),
    ];
    let d = c
        .parse_discovery(&cx(), Some(r#"{"CPIAUCSL":"2026-10-15"}"#), &snaps)
        .unwrap();
    assert_eq!(d.refs.len(), 1);
    assert_eq!(d.refs[0].meta["vintage"], "2026-11-13");
    let keys: Vec<_> = d
        .calendar
        .iter()
        .map(|e| (e.event_key.as_str(), e.expected_day.to_string()))
        .collect();
    assert_eq!(
        keys,
        vec![
            ("macro:us.cpi:2026-11", "2026-12-10".to_owned()),
            ("macro:us.cpi:2026-12", "2027-01-13".to_owned())
        ]
    );
    let again = c
        .parse_discovery(&cx(), d.next_cursor.as_deref(), &snaps[..1])
        .unwrap();
    assert!(again.refs.is_empty(), "known vintage skipped");
}

// ---------------------------------------------------------------------------------------- SEC

const K26: &str = "0000320193-26-000105";
const Q3: &str = "0000320193-26-000077";
const FACTS: &str = "https://data.sec.gov/api/xbrl/companyfacts/CIK0000320193.json";

fn sec_item(accn: &str, form: &str, items: &str) -> ItemRef {
    ItemRef {
        source_id: "sec-edgar".into(),
        kind: "sec.filing".into(),
        key: accn.into(),
        url: format!(
            "https://www.sec.gov/Archives/edgar/data/320193/{}/doc.htm",
            accn.replace('-', "")
        ),
        event_key_hint: None,
        published_at: Some(t(2026, 10, 31, 6)),
        title: Some(format!("Apple Inc. {form}")),
        meta: json!({"cik": "0000320193", "form": form, "items": items}),
    }
}

fn sec_parse(accn: &str, form: &str) -> Parsed {
    let c = connector("sec-edgar").unwrap();
    let item = sec_item(accn, form, "");
    assert_eq!(c.fetch_requests(&item)[0].url, FACTS);
    c.parse(
        &item,
        &[snap(
            FACTS,
            fixture("sec/companyfacts-CIK0000320193.json"),
            t(2026, 10, 31, 7),
        )],
    )
    .unwrap()
}

#[test]
fn sec_earnings_rule_is_structured() {
    assert!(sec::is_earnings("10-K", ""));
    assert!(sec::is_earnings("10-Q/A", ""));
    assert!(sec::is_earnings("8-K", "2.02,9.01"));
    assert!(!sec::is_earnings("8-K", "5.07"));
    assert!(!sec::is_earnings("8-K", "2.021"));
    assert!(!sec::is_earnings("4", ""));
}

#[test]
fn sec_discovery_lists_earnings_filings_since_the_cursor() {
    let c = connector("sec-edgar").unwrap();
    let reqs = c.discovery_requests(&cx(), None);
    assert_eq!(
        reqs[0].url,
        "https://data.sec.gov/submissions/CIK0000320193.json"
    );
    let s = [snap(
        &reqs[0].url,
        fixture("sec/submissions-CIK0000320193.json"),
        t(2026, 11, 16, 8),
    )];
    let d = c
        .parse_discovery(
            &cx(),
            Some(r#"{"0000320193":{"date":"2026-08-01","seen":["0000320193-26-000077"]}}"#),
            &s,
        )
        .unwrap();
    let got: Vec<_> = d
        .refs
        .iter()
        .map(|r| (r.key.as_str(), r.meta["form"].as_str().unwrap()))
        .collect();
    assert_eq!(
        got,
        vec![
            ("0000320193-26-000104", "8-K"),
            (K26, "10-K"),
            ("0001193125-26-245117", "8-K/A")
        ]
    );
    assert_eq!(
        d.refs[1].url,
        "https://www.sec.gov/Archives/edgar/data/320193/000032019326000105/aapl-20260926.htm"
    );
    assert_eq!(
        d.refs[1].published_at,
        Some(Utc.with_ymd_and_hms(2026, 10, 31, 6, 1, 36).unwrap())
    );
    let next = d.next_cursor.unwrap();
    assert_eq!(
        next,
        r#"{"0000320193":{"date":"2026-10-31","seen":["0000320193-26-000105","0001193125-26-245117"]}}"#
    );
    assert!(
        c.parse_discovery(&cx(), Some(&next), &s)
            .unwrap()
            .refs
            .is_empty()
    );
    // First run: the last 30 days only.
    let first = c.parse_discovery(&cx(), None, &s).unwrap();
    assert_eq!(first.refs.len(), 3);
}

/// The submissions fixture without the rows of these accessions (as if not yet disseminated).
fn submissions_without(accns: &[&str]) -> Vec<u8> {
    let mut v: serde_json::Value =
        serde_json::from_slice(&fixture("sec/submissions-CIK0000320193.json")).unwrap();
    let recent = v["filings"]["recent"].as_object_mut().unwrap();
    let keep: Vec<bool> = recent["accessionNumber"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| !accns.contains(&a.as_str().unwrap()))
        .collect();
    for col in recent.values_mut() {
        let arr = col.as_array_mut().unwrap();
        let mut i = 0;
        arr.retain(|_| {
            i += 1;
            keep[i - 1]
        });
    }
    serde_json::to_vec(&v).unwrap()
}

#[test]
fn sec_discovery_keeps_same_day_filings_whatever_their_accession() {
    // Accession prefixes are the filer's (or its filing agent's) CIK, not a clock: a filing agent's
    // 8-K/A (0001193125-…) can be seen before Apple's own 10-K of the same filing date, whose
    // accession sorts lower. The 10-K must still be discovered on the next run.
    const AGENT: &str = "0001193125-26-245117";
    let c = connector("sec-edgar").unwrap();
    let url = "https://data.sec.gov/submissions/CIK0000320193.json";
    let early = [snap(url, submissions_without(&[K26]), t(2026, 10, 31, 9))];
    let keys = |d: &news_connectors::Discovery| -> Vec<String> {
        d.refs.iter().map(|r| r.key.clone()).collect()
    };
    let d1 = c.parse_discovery(&cx(), None, &early).unwrap();
    assert_eq!(keys(&d1), vec!["0000320193-26-000104", AGENT]);

    let full = [snap(
        url,
        fixture("sec/submissions-CIK0000320193.json"),
        t(2026, 11, 16, 8),
    )];
    let d2 = c
        .parse_discovery(&cx(), d1.next_cursor.as_deref(), &full)
        .unwrap();
    assert_eq!(keys(&d2), vec![K26]);
    let d3 = c
        .parse_discovery(&cx(), d2.next_cursor.as_deref(), &full)
        .unwrap();
    assert!(d3.refs.is_empty());
}

#[test]
fn sec_10k_with_restatement_and_non_calendar_fiscal_year() {
    let p = sec_parse(K26, "10-K");
    check_points(&p, "sec-edgar@1");
    // 8 facts in this filing (one listed twice under two frames); history of the same concepts too.
    assert_eq!(p.candidates[0].dp_ids.len(), 8);
    assert_eq!(p.points.len(), 16);
    assert_eq!(
        p.candidates[0].event_key.as_deref(),
        Some("earnings:0000320193:2026FY")
    );
    assert_eq!(p.candidates[0].rule_key, "form:10-K");
    assert_eq!(p.entities[0].name, "Apple Inc.");

    let ni = |accn: &str, end: &str| {
        p.points
            .iter()
            .find(|d| {
                d.concept == "us-gaap:NetIncomeLoss"
                    && d.locator["accn"] == accn
                    && d.period_end.unwrap().to_string() == end
            })
            .unwrap()
    };
    // Restatement: FY2025 net income as first filed, and as restated in the FY2026 10-K.
    let original = ni("0000320193-25-000079", "2025-09-27");
    let restated = ni(K26, "2025-09-27");
    assert_eq!(
        (original.value_text.as_str(), restated.value_text.as_str()),
        ("112010000000", "111990000000")
    );
    assert_ne!(original.dp_id, restated.dp_id);
    assert_eq!(original.series_key, restated.series_key);
    assert!(restated.vintage > original.vintage);
    // Both carry Apple's own fiscal year: the comparative is the same period a year before the filing's.
    assert_eq!(
        (original.fiscal_year, original.fiscal_period.as_deref()),
        (Some(2025), Some("FY"))
    );
    assert_eq!(
        (restated.fiscal_year, restated.fiscal_period.as_deref()),
        (Some(2025), Some("FY"))
    );

    // Non-calendar fiscal year: FY2026 ran 28 Sep 2025 – 26 Sep 2026; no calendarisation.
    let fy26 = ni(K26, "2026-09-26");
    assert_eq!(fy26.period_start.unwrap().to_string(), "2025-09-28");
    assert_eq!(fy26.period_label, "2025-09-28/2026-09-26");
    assert_eq!(
        (fy26.fiscal_year, fy26.fiscal_period.as_deref()),
        (Some(2026), Some("FY"))
    );
    assert_eq!(
        (fy26.currency.as_deref(), fy26.unit.as_str()),
        (Some("USD"), "USD")
    );
    assert_eq!(fy26.locator["tag"], "us-gaap:NetIncomeLoss");
    assert_eq!(fy26.context_key, format!("accn={K26};unit=USD"));

    let eps = p
        .points
        .iter()
        .find(|d| d.unit == "USD/shares" && d.locator["accn"] == K26 && d.fiscal_year == Some(2026))
        .unwrap();
    assert_eq!(eps.value_text, "8.10", "the JSON number token, verbatim");
    assert_eq!(eps.currency.as_deref(), Some("USD"));

    // The cover-page share count is an instant after the period end: no fiscal label.
    let shares = p.points.iter().find(|d| d.unit == "shares").unwrap();
    assert_eq!(shares.fiscal_year, None);

    // An unknown unit is stored verbatim and flagged for a human.
    assert_eq!(p.needs_human.len(), 1);
    assert!(p.needs_human[0].reason.contains("USD/Contract"));
    let odd = p.points.iter().find(|d| d.unit == "USD/Contract").unwrap();
    assert_eq!(
        (odd.value_text.as_str(), odd.currency.as_deref()),
        ("1.0E3", None)
    );

    assert_eq!(ids(&p), ids(&sec_parse(K26, "10-K")));
}

#[test]
fn sec_10q_quarter_and_year_to_date() {
    let p = sec_parse(Q3, "10-Q");
    assert_eq!(
        p.candidates[0].event_key.as_deref(),
        Some("earnings:0000320193:2026Q3")
    );
    let rev = |start: &str| {
        p.points
            .iter()
            .find(|d| d.locator["accn"] == Q3 && d.period_start.unwrap().to_string() == start)
            .unwrap()
    };
    assert_eq!(
        (
            rev("2026-03-29").fiscal_year,
            rev("2026-03-29").fiscal_period.as_deref()
        ),
        (Some(2026), Some("Q3"))
    );
    assert_eq!(
        (
            rev("2025-03-30").fiscal_year,
            rev("2025-03-30").fiscal_period.as_deref()
        ),
        (Some(2025), Some("Q3"))
    );
    // Nine months to date is not "Q3": no fiscal period rather than a wrong one.
    assert_eq!(rev("2025-09-28").fiscal_period, None);
}

#[test]
fn sec_filing_not_yet_in_companyfacts_is_retried_not_dead() {
    let c = connector("sec-edgar").unwrap();
    let err = c
        .parse(
            &sec_item("0000320193-26-999999", "10-Q", ""),
            &[snap(
                FACTS,
                fixture("sec/companyfacts-CIK0000320193.json"),
                t(2026, 11, 1, 0),
            )],
        )
        .unwrap_err();
    assert_eq!(err.class(), ErrorClass::Transient);
}

#[test]
fn sec_8k_item_202_becomes_a_candidate_without_numbers() {
    let c = connector("sec-edgar").unwrap();
    let item = sec_item("0000320193-26-000104", "8-K", "2.02,9.01");
    let reqs = c.fetch_requests(&item);
    assert_eq!(reqs[0].url, item.url);
    assert_eq!(reqs[0].accept, vec!["text/html", "application/xhtml+xml"]);
    let p = c
        .parse(
            &item,
            &[snap(
                &item.url,
                fixture("sec/primary-8k.htm"),
                t(2026, 10, 30, 21),
            )],
        )
        .unwrap();
    assert!(p.points.is_empty());
    assert_eq!(p.candidates[0].rule_key, "form:8-K/2.02");
    assert_eq!(p.candidates[0].event_key, None);
}

#[test]
fn sec_malformed_companyfacts_fail_cleanly() {
    let c = connector("sec-edgar").unwrap();
    let item = sec_item(K26, "10-K", "");
    let body = String::from_utf8(fixture("sec/companyfacts-CIK0000320193.json")).unwrap();
    let broken = body.replacen("\"end\":\"2026-09-26\"", "\"end\":\"26/09/2026\"", 1);
    let err = c
        .parse(
            &item,
            &[snap(FACTS, broken.into_bytes(), t(2026, 10, 31, 7))],
        )
        .unwrap_err();
    assert_eq!(err.class(), ErrorClass::InvalidData);
    assert!(
        err.locator.contains("$.facts['us-gaap']"),
        "{}",
        err.locator
    );
    let err = c
        .parse(
            &item,
            &[snap(FACTS, b"{\"facts\": 3}".to_vec(), t(2026, 10, 31, 7))],
        )
        .unwrap_err();
    assert_eq!(err.class(), ErrorClass::UpstreamChanged);
}

#[test]
fn every_connector_is_registered_once() {
    for id in news_connectors::IDS {
        let c = connector(id).unwrap();
        assert_eq!(c.id(), *id);
        assert_eq!(c.parser(), format!("{id}@1"));
    }
    assert!(connector("nope").is_none());
    let _ = (bls::SERIES, fred::SERIES);
}
