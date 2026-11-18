//! Golden tests for the second batch of Stringers (#12), from recorded and synthetic fixtures only
//! (no network): see `tests/fixtures/README.md` for where each file came from.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use news_connectors::{
    Cx, ItemRef, ParseErrorKind, Parsed, Snapshot, abs, arxiv, bea, centralbank, connector,
    crossref, ecb, eurostat, nasa, oecd, ons, openalex, worldbank,
};
use news_core::job::ErrorClass;
use serde_json::json;

fn fixture(path: &str) -> Vec<u8> {
    let p = format!("{}/tests/fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

fn at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 18, 6, 0, 0).unwrap()
}

fn snap(url: &str, bytes: Vec<u8>) -> Snapshot {
    Snapshot {
        url: url.to_owned(),
        sha256: [7; 32],
        fetched_at: at(),
        content_type: None,
        bytes,
    }
}

fn cx(pairs: &[(&str, &[&str])]) -> Cx {
    Cx {
        today: NaiveDate::from_ymd_opt(2026, 11, 18).unwrap(),
        sec_ciks: vec![],
        fred_series: vec![],
        bls_series: vec![],
        tracked: pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.iter().map(|s| (*s).to_owned()).collect()))
            .collect::<BTreeMap<_, _>>(),
    }
}

/// Every point: exact decimal formats back to the text, provenance present, ids shaped for the table.
fn check_points(p: &Parsed, parser: &str) {
    assert!(!p.points.is_empty());
    for d in &p.points {
        let v = d.value.as_deref().unwrap();
        assert_eq!(d.style.format(v), d.value_text, "{}", d.dp_id);
        assert_eq!(d.parser, parser);
        assert!(d.locator.get("kind").is_some() && d.locator.get("path").is_some());
        assert!(d.dp_id.starts_with("dp_") && d.dp_id.len() == 19);
        assert!(d.period_start <= d.period_end);
    }
    for c in &p.candidates {
        if let Some(k) = &c.event_key {
            assert!(news_ledger::eventkey::is_valid(k), "{k}");
        }
        for id in &c.dp_ids {
            assert!(p.points.iter().any(|d| &d.dp_id == id), "unknown {id}");
        }
    }
}

fn series_item(source: &str, key: &str) -> ItemRef {
    ItemRef {
        source_id: source.into(),
        kind: format!("{source}.series"),
        key: key.into(),
        url: format!("https://example.test/{key}"),
        event_key_hint: None,
        published_at: Some(at()),
        title: None,
        meta: json!({}),
    }
}

/// The shared probe → item → history flow of the statistics Stringers.
fn stat_flow(id: &str, series: &str, probe_fx: &str, history_fx: &str) -> Parsed {
    let conn = connector(id).unwrap();
    let c = cx(&[(id, &[series])]);
    let reqs = conn.discovery_requests(&c, None);
    assert_eq!(reqs.len(), 1, "one probe per followed series");
    assert!(reqs[0].secret.is_none() || id == "bea");
    let probe = snap(&reqs[0].url, fixture(probe_fx));
    let d = conn
        .parse_discovery(&c, None, std::slice::from_ref(&probe))
        .unwrap();
    assert_eq!(d.refs.len(), 1);
    assert_eq!(d.refs[0].key, series);
    // Same bytes again: nothing new. A 304 (no snapshot): nothing either.
    let again = conn
        .parse_discovery(&c, d.next_cursor.as_deref(), std::slice::from_ref(&probe))
        .unwrap();
    assert!(again.refs.is_empty());
    let none = conn
        .parse_discovery(&c, d.next_cursor.as_deref(), &[])
        .unwrap();
    assert!(none.refs.is_empty());
    // Changed bytes: new item.
    let mut changed = probe.clone();
    changed.sha256 = [9; 32];
    let again = conn
        .parse_discovery(&c, d.next_cursor.as_deref(), &[changed])
        .unwrap();
    assert_eq!(again.refs.len(), 1);

    let item = &d.refs[0];
    let fetch = conn.fetch_requests(item);
    assert!(!fetch.is_empty());
    assert_ne!(
        fetch[0].url, reqs[0].url,
        "history must differ from the probe or fetch would 304"
    );
    let history = snap(&fetch[0].url, fixture(history_fx));
    let a = conn.parse(item, std::slice::from_ref(&history)).unwrap();
    let b = conn.parse(item, &[history]).unwrap();
    assert_eq!(a, b, "parse is pure");
    check_points(&a, &conn.parser());
    a
}

// ------------------------------------------------------------------------------------------ ECB

#[test]
fn ecb_policy_rate_keeps_the_ecb_dates_and_units() {
    let p = stat_flow(
        "ecb",
        "FM.MRR",
        "ecb/FM.MRR-probe.json",
        "ecb/FM.MRR-history.json",
    );
    assert_eq!(p.points.len(), 12);
    assert!(p.needs_human.is_empty());
    let first = &p.points[0];
    assert_eq!(first.series_key, "ecb:FM:B.U2.EUR.4F.KR.MRR_FR.LEV");
    assert_eq!(first.unit, "Percent per annum");
    assert_eq!(first.value_text, "4.25");
    assert_eq!(first.period_label, "2023-08-02");
    assert_eq!(
        first.period_start, first.period_end,
        "a rate set on one day is a one-day period"
    );
    assert_eq!(first.vintage, at());
    // The rate is published only when it changes: irregular dates stay as published.
    let labels: Vec<&str> = p.points.iter().map(|d| d.period_label.as_str()).collect();
    assert_eq!(&labels[9..], ["2025-06-11", "2026-06-17", "2026-09-16"]);
    let c = &p.candidates[0];
    assert_eq!(
        c.event_key.as_deref(),
        Some("macro:ea.policy_rate:2026-09-16")
    );
    assert_eq!(c.rule_key, "family:policy_rate");
    assert_eq!(p.entities[0].value, "EA");
}

#[test]
fn ecb_exchange_rate_unit_is_not_guessed() {
    let p = stat_flow(
        "ecb",
        "EXR.USD",
        "ecb/FM.MRR-probe.json",
        "ecb/EXR.USD-history.json",
    );
    assert_eq!(p.points.len(), 12);
    assert_eq!(p.points[0].unit, "US dollar");
    assert_eq!(p.points[0].value_text, "1.1216");
    assert_eq!(
        p.needs_human.len(),
        1,
        "an unknown unit is flagged once, never converted"
    );
    assert!(p.needs_human[0].reason.contains("US dollar"));
    assert!(p.points.iter().all(|d| d.currency.is_none()));
}

#[test]
fn sdmx_failures_are_typed() {
    let conn = connector("ecb").unwrap();
    let item = series_item("ecb", "FM.MRR");
    let url = conn.fetch_requests(&item)[0].url.clone();
    for (bytes, what) in [
        (b"not json".to_vec(), "garbage"),
        (br#"{"header":{}}"#.to_vec(), "no dataSets"),
        (br#"{"dataSets":[{"series":{}}]}"#.to_vec(), "no structure"),
    ] {
        let e = conn.parse(&item, &[snap(&url, bytes)]).unwrap_err();
        assert_eq!(e.kind, ParseErrorKind::Shape, "{what}");
        assert_eq!(e.class(), ErrorClass::UpstreamChanged);
    }
    let e = conn.parse(&item, &[]).unwrap_err();
    assert_eq!(e.kind, ParseErrorKind::MissingSnapshot);
}

#[test]
fn sdmx_null_observation_is_missing_not_zero() {
    let conn = connector("ecb").unwrap();
    let item = series_item("ecb", "FM.MRR");
    let url = conn.fetch_requests(&item)[0].url.clone();
    let text = String::from_utf8(fixture("ecb/FM.MRR-history.json")).unwrap();
    let edited = text.replacen("[4.25,", "[null,", 1);
    assert_ne!(text, edited);
    let p = conn
        .parse(&item, &[snap(&url, edited.into_bytes())])
        .unwrap();
    assert_eq!(p.points.len(), 11);
    assert_eq!(p.missing.len(), 1);
    assert_eq!(p.missing[0].period_label, "2023-08-02");
}

// ----------------------------------------------------------------------------- OECD, ABS, Eurostat

#[test]
fn oecd_cpi_has_a_real_gap_and_six_decimals() {
    let p = stat_flow(
        "oecd",
        "CPI.USA",
        "oecd/CPI.USA-probe.json",
        "oecd/CPI.USA-history.json",
    );
    assert_eq!(p.points.len(), 12);
    let sep = p
        .points
        .iter()
        .find(|d| d.period_label == "2026-09")
        .unwrap();
    assert_eq!(sep.value_text, "3.737685");
    assert_eq!(sep.unit, "Percent per annum");
    assert_eq!(
        sep.seasonal_adj.as_deref(),
        Some("Neither seasonally adjusted nor calendar adjusted")
    );
    assert_eq!(sep.period_start.unwrap().to_string(), "2026-09-01");
    assert_eq!(sep.period_end.unwrap().to_string(), "2026-09-30");
    // October 2025 was never published (the US federal shutdown): the series simply skips it.
    assert!(p.points.iter().all(|d| d.period_label != "2025-10"));
    assert_eq!(
        p.candidates[0].event_key.as_deref(),
        Some("macro:us.cpi_yoy:2026-09")
    );
}

#[test]
fn abs_quarters_keep_the_abs_labels() {
    let p = stat_flow(
        "abs",
        "CPI.AUS",
        "abs/CPI.AUS-probe.json",
        "abs/CPI.AUS-history.json",
    );
    assert_eq!(p.points.len(), 8);
    let q3 = &p.points[0];
    assert_eq!(q3.period_label, "2026-Q3");
    assert_eq!(q3.value_text, "103.64");
    assert_eq!(q3.unit, "Index numbers");
    assert_eq!(q3.seasonal_adj.as_deref(), Some("Original"));
    assert_eq!(q3.period_start.unwrap().to_string(), "2026-07-01");
    assert_eq!(q3.period_end.unwrap().to_string(), "2026-09-30");
    assert!(p.needs_human.is_empty());
    assert_eq!(
        p.candidates[0].event_key.as_deref(),
        Some("macro:au.cpi:2026-Q3")
    );
}

#[test]
fn eurostat_vintage_is_the_datasets_updated_stamp() {
    let p = stat_flow(
        "eurostat",
        "HICP.EA",
        "eurostat/HICP.EA-probe.json",
        "eurostat/HICP.EA-history.json",
    );
    assert_eq!(p.points.len(), 12);
    assert_eq!(
        p.points[0].vintage,
        Utc.with_ymd_and_hms(2026, 2, 6, 22, 0, 0).unwrap(),
        "2026-02-06T23:00:00+0100"
    );
    // Eurostat publishes `2.0`: the trailing zero is part of the value and stays.
    assert!(
        p.points
            .iter()
            .any(|d| d.value_text == "2.0" && d.period_label == "2025-12")
    );
    assert_eq!(p.points[0].unit, "Annual rate of change");
    assert_eq!(
        p.points[0].series_key,
        "eurostat:prc_hicp_manr:M.RCH_A.CP00.EA"
    );
}

// -------------------------------------------------------------------------------------- World Bank

#[test]
fn worldbank_unit_comes_from_the_indicator_name_and_values_are_exact() {
    let p = stat_flow(
        "worldbank",
        "GDP.USA",
        "worldbank/GDP.USA-probe.json",
        "worldbank/GDP.USA-history.json",
    );
    assert_eq!(p.points.len(), 5);
    let y = p.points.iter().find(|d| d.period_label == "2024").unwrap();
    assert_eq!(y.value_text, "29298013000000");
    assert_eq!(y.unit, "current US$");
    assert_eq!(
        y.currency, None,
        "the currency is stated in the unit text, never added"
    );
    assert_eq!(y.period_end.unwrap().to_string(), "2024-12-31");
    assert_eq!(
        y.vintage,
        Utc.with_ymd_and_hms(2026, 11, 11, 0, 0, 0).unwrap()
    );
    assert!(p.needs_human.is_empty());
    assert_eq!(
        p.candidates[0].event_key.as_deref(),
        Some("macro:us.gdp_wb:2025")
    );
}

#[test]
fn worldbank_null_value_is_missing() {
    let conn = connector("worldbank").unwrap();
    let item = series_item("worldbank", "GDP.USA");
    let url = conn.fetch_requests(&item)[0].url.clone();
    let text = String::from_utf8(fixture("worldbank/GDP.USA-history.json")).unwrap();
    let edited = text.replacen("30769700000000", "null", 1);
    let p = conn
        .parse(&item, &[snap(&url, edited.into_bytes())])
        .unwrap();
    assert_eq!(p.points.len(), 4);
    assert_eq!(p.missing[0].period_label, "2025");
}

// ---------------------------------------------------------------------------------------------- ONS

#[test]
fn ons_discovers_a_new_dataset_version_and_parses_its_months() {
    let conn = connector("ons").unwrap();
    let c = cx(&[("ons", &["cpih01"])]);
    let reqs = conn.discovery_requests(&c, None);
    assert_eq!(
        reqs[0].url,
        "https://api.beta.ons.gov.uk/v1/datasets/cpih01"
    );
    let listing = snap(&reqs[0].url, fixture("ons/dataset-cpih01.json"));
    let d = conn
        .parse_discovery(&c, None, std::slice::from_ref(&listing))
        .unwrap();
    assert_eq!(d.refs.len(), 1);
    assert_eq!(d.refs[0].meta["version"], "67");
    let again = conn
        .parse_discovery(&c, d.next_cursor.as_deref(), &[listing])
        .unwrap();
    assert!(again.refs.is_empty(), "same version: nothing new");

    let item = &d.refs[0];
    let f = conn.fetch_requests(item);
    assert!(f[0].url.contains("/versions/67/observations?"));
    let p = conn
        .parse(
            item,
            &[snap(&f[0].url, fixture("ons/observations-cpih01.json"))],
        )
        .unwrap();
    check_points(&p, "ons@1");
    assert_eq!(p.points.len(), 8);
    let jan = p
        .points
        .iter()
        .find(|d| d.period_label == "Jan-26")
        .unwrap();
    assert_eq!(jan.unit, "Index: 2015=100");
    assert_eq!(jan.period_start.unwrap().to_string(), "2026-01-01");
    assert_eq!(jan.period_end.unwrap().to_string(), "2026-01-31");
    assert!(p.needs_human.is_empty());
    assert_eq!(
        p.candidates[0].event_key.as_deref(),
        Some("macro:gb.cpih:2026-01")
    );
}

#[test]
fn ons_shape_change_is_upstream_changed() {
    let conn = connector("ons").unwrap();
    let c = cx(&[("ons", &["cpih01"])]);
    let url = conn.discovery_requests(&c, None)[0].url.clone();
    let e = conn
        .parse_discovery(&c, None, &[snap(&url, br#"{"id":"cpih01"}"#.to_vec())])
        .unwrap_err();
    assert_eq!(e.class(), ErrorClass::UpstreamChanged);
}

// ---------------------------------------------------------------------------------------------- BEA

#[test]
fn bea_synthetic_fixture_covers_commas_scale_and_missing() {
    let p = stat_flow(
        "bea",
        "NIPA.T10101",
        "bea/NIPA.T10101-probe.json",
        "bea/NIPA.T10101-history.json",
    );
    assert_eq!(p.points.len(), 4);
    assert_eq!(p.missing.len(), 1, "(NA) is a missing observation");
    let level = p
        .points
        .iter()
        .find(|d| d.series_key.ends_with("A191RC"))
        .unwrap();
    assert_eq!(
        level.value_text, "31,184,900",
        "thousands separator kept as published"
    );
    assert_eq!(level.value.as_deref(), Some("31184900"));
    assert_eq!(level.scale.as_deref(), Some("UNIT_MULT=6"));
    assert_eq!(level.unit, "Level");
    assert_eq!(
        p.needs_human.len(),
        1,
        "unit `Level` is not in the vocabulary"
    );
    let growth = p
        .points
        .iter()
        .find(|d| d.period_label == "2026Q3")
        .unwrap();
    assert_eq!(growth.value_text, "2.4");
    assert_eq!(growth.unit, "Percent change, annualized");
    assert_eq!(growth.period_start.unwrap().to_string(), "2026-07-01");
    assert_eq!(
        p.candidates[0].event_key.as_deref(),
        Some("macro:us.gdp:2026-Q3")
    );
}

#[test]
fn bea_key_is_named_never_embedded() {
    let conn = connector("bea").unwrap();
    let c = cx(&[("bea", &["NIPA.T10101"])]);
    let r = &conn.discovery_requests(&c, None)[0];
    let s = r.secret.as_ref().unwrap();
    assert_eq!(
        (s.param.as_str(), s.env.as_str()),
        ("UserID", "BEA_API_KEY")
    );
    assert!(!r.url.contains("UserID"));
}

// ---------------------------------------------------------------------------------------------- NASA

#[test]
fn nasa_images_carry_credit_licence_alt_and_a_rehost_decision() {
    let conn = connector("nasa").unwrap();
    let c = cx(&[("nasa", &["artemis"])]);
    let reqs = conn.discovery_requests(&c, None);
    assert!(reqs[0].url.contains("q=artemis") && reqs[0].url.contains("year_start=2026"));
    let listing = snap(&reqs[0].url, fixture("nasa/search-artemis.json"));
    let d = conn
        .parse_discovery(&c, None, std::slice::from_ref(&listing))
        .unwrap();
    assert_eq!(d.refs.len(), 3);
    let again = conn
        .parse_discovery(&c, d.next_cursor.as_deref(), &[listing])
        .unwrap();
    assert!(again.refs.is_empty());

    let item = d.refs.iter().find(|r| r.key.ends_with("Launch-1")).unwrap();
    let f = conn.fetch_requests(item);
    assert!(
        f[0].url
            .ends_with("nasa_id=KSC-04012026-Artemis%20II_Launch-1")
    );
    let bytes = fixture("nasa/item-KSC-04012026-Artemis II_Launch-1.json");
    let p = conn.parse(item, &[snap(&f[0].url, bytes.clone())]).unwrap();
    let img = &p.images[0];
    assert!(img.url.ends_with("~large.jpg") && !img.url.contains(' '));
    assert_eq!(img.credit.as_deref(), Some("NASA/Brandon Hancock"));
    assert_eq!(img.licence, "custom");
    assert!(
        img.rehost,
        "a NASA-credited image may be copied to our storage"
    );
    assert!(!img.alt.is_empty());
    assert_eq!((img.width, img.height), (Some(1920), Some(1280)));
    assert!(p.points.is_empty());

    // A third-party credit stays a link to NASA's file.
    let text = String::from_utf8(bytes).unwrap();
    let third = text.replace("NASA/Brandon Hancock", "Jane Doe / Example Press");
    let p = conn
        .parse(item, &[snap(&f[0].url, third.into_bytes())])
        .unwrap();
    assert!(!p.images[0].rehost);
    // No credit at all: no rehost, no invented credit.
    let none = text.replace("\"photographer\":\"NASA/Brandon Hancock\",", "");
    let p = conn
        .parse(item, &[snap(&f[0].url, none.into_bytes())])
        .unwrap();
    assert!(!p.images[0].rehost && p.images[0].credit.is_none());
}

// ----------------------------------------------------------------------------------- Papers family

#[test]
fn openalex_lists_works_since_the_cursor_and_keys_them_by_doi() {
    let conn = connector("openalex").unwrap();
    let c = cx(&[("openalex", &["primary_topic.field.id:20"])]);
    let reqs = conn.discovery_requests(&c, None);
    assert!(reqs[0].url.contains("from_publication_date:2026-11-15"));
    let listing = snap(&reqs[0].url, fixture("openalex/list-field20.json"));
    let d = conn.parse_discovery(&c, None, &[listing]).unwrap();
    assert_eq!(d.refs.len(), 3);
    // All three recorded works are dated 2027 (data deposits pre-date their records): a future date
    // must not carry the cursor past today, or the next run would list nothing.
    let cursor: BTreeMap<String, String> =
        serde_json::from_str(d.next_cursor.as_deref().unwrap()).unwrap();
    assert!(cursor.is_empty());
    let next = conn.discovery_requests(&c, d.next_cursor.as_deref());
    assert!(next[0].url.contains("from_publication_date:2026-11-15"));
    // A work dated today moves the cursor to today.
    let today = String::from_utf8(fixture("openalex/list-field20.json"))
        .unwrap()
        .replacen("2027-11-14", "2026-11-17", 1);
    let d2 = conn
        .parse_discovery(&c, None, &[snap(&reqs[0].url, today.into_bytes())])
        .unwrap();
    let cursor: BTreeMap<String, String> =
        serde_json::from_str(d2.next_cursor.as_deref().unwrap()).unwrap();
    assert_eq!(cursor["primary_topic.field.id:20"], "2026-11-17");

    let item = d.refs.iter().find(|r| r.key == "W7234566083").unwrap();
    assert!(
        item.event_key_hint
            .as_deref()
            .unwrap()
            .starts_with("paper:doi:10.")
    );
    let f = conn.fetch_requests(item);
    let p = conn
        .parse(
            item,
            &[snap(&f[0].url, fixture("openalex/work-W7234566083.json"))],
        )
        .unwrap();
    assert!(p.points.is_empty());
    let cand = &p.candidates[0];
    assert_eq!(cand.event_key, item.event_key_hint);
    assert_eq!(cand.rule_key, "family:paper");
    assert!(!cand.title.is_empty());
    // Re-running parse changes nothing.
    assert_eq!(
        p,
        conn.parse(
            item,
            &[snap(&f[0].url, fixture("openalex/work-W7234566083.json"))]
        )
        .unwrap()
    );
}

#[test]
fn crossref_needs_the_contact_email_registry_flag_and_parses_dois() {
    let conn = connector("crossref").unwrap();
    let c = cx(&[]);
    let reqs = conn.discovery_requests(&c, None);
    assert!(reqs[0].url.contains("from-index-date:2026-11-17"));
    let d = conn
        .parse_discovery(
            &c,
            None,
            &[snap(&reqs[0].url, fixture("crossref/list.json"))],
        )
        .unwrap();
    assert_eq!(d.refs.len(), 3);
    let item = d
        .refs
        .iter()
        .find(|r| r.key == "10.1007/s00408-021-00426-w")
        .unwrap();
    assert_eq!(
        item.event_key_hint.as_deref(),
        Some("paper:doi:10.1007/s00408-021-00426-w")
    );
    let f = conn.fetch_requests(item);
    let p = conn
        .parse(
            item,
            &[snap(
                &f[0].url,
                fixture("crossref/work-10.1007_s00408-021-00426-w.json"),
            )],
        )
        .unwrap();
    assert_eq!(p.candidates[0].event_key, item.event_key_hint);
    // A work with no title fails cleanly.
    let e = conn
        .parse(
            item,
            &[snap(
                &f[0].url,
                br#"{"status":"ok","message":{"DOI":"10.1007/s00408-021-00426-w","title":[]}}"#
                    .to_vec(),
            )],
        )
        .unwrap_err();
    assert_eq!(e.kind, ParseErrorKind::Shape);
}

#[test]
fn arxiv_uses_the_oai_host_and_one_record_per_item() {
    let conn = connector("arxiv").unwrap();
    let c = cx(&[("arxiv", &["econ"])]);
    let reqs = conn.discovery_requests(&c, None);
    assert!(
        reqs[0]
            .url
            .starts_with("https://oaipmh.arxiv.org/oai?verb=ListRecords")
    );
    assert!(reqs[0].url.contains("set=econ&from=2026-11-17"));
    // The recorded listing was asked from 16 November.
    let url = "https://oaipmh.arxiv.org/oai?verb=ListRecords&metadataPrefix=arXiv&set=econ&from=2026-11-16";
    let mut c16 = c.clone();
    c16.today = NaiveDate::from_ymd_opt(2026, 11, 17).unwrap();
    assert_eq!(conn.discovery_requests(&c16, None)[0].url, url);
    let d = conn
        .parse_discovery(&c16, None, &[snap(url, fixture("arxiv/list-econ.xml"))])
        .unwrap();
    assert_eq!(d.refs.len(), 3);
    assert_eq!(d.refs[0].key, "2112.14514");
    assert_eq!(
        d.refs[0].event_key_hint.as_deref(),
        Some("paper:arxiv:2112.14514")
    );

    let f = conn.fetch_requests(&d.refs[0]);
    assert!(f[0].url.contains("verb=GetRecord") && f[0].url.ends_with("oai:arXiv.org:2112.14514"));
    let p = conn
        .parse(
            &d.refs[0],
            &[snap(&f[0].url, fixture("arxiv/record-2112.14514.xml"))],
        )
        .unwrap();
    assert!(
        p.candidates[0]
            .title
            .starts_with("Coal Mining, Women's Work")
    );
    assert_eq!(
        p.candidates[0]
            .published_at
            .unwrap()
            .date_naive()
            .to_string(),
        "2026-11-13"
    );

    // `noRecordsMatch` is an empty day, not a failure; any other OAI error is.
    let empty = br#"<?xml version="1.0"?><OAI-PMH xmlns="http://www.openarchives.org/OAI/2.0/"><error code="noRecordsMatch"/></OAI-PMH>"#;
    let d = conn
        .parse_discovery(&c16, None, &[snap(url, empty.to_vec())])
        .unwrap();
    assert!(d.refs.is_empty());
    let bad = br#"<?xml version="1.0"?><OAI-PMH xmlns="http://www.openarchives.org/OAI/2.0/"><error code="badArgument">x</error></OAI-PMH>"#;
    assert!(
        conn.parse_discovery(&c16, None, &[snap(url, bad.to_vec())])
            .is_err()
    );
}

// ---------------------------------------------------------------------------------- Central banks

fn bank_items(id: &str) -> Vec<ItemRef> {
    let b = centralbank::bank(id).unwrap();
    let c = cx(&[]);
    let reqs = connector(id).unwrap().discovery_requests(&c, None);
    assert_eq!(reqs[0].url, b.feed);
    let file = format!("centralbank/{}-feed.xml", id.trim_start_matches("cb-"));
    connector(id)
        .unwrap()
        .parse_discovery(&c, None, &[snap(b.feed, fixture(&file))])
        .unwrap()
        .refs
}

#[test]
fn fed_statements_get_policy_keys_from_their_publication_day() {
    let refs = bank_items("cb-fed");
    assert!(!refs.is_empty());
    assert!(
        refs.iter()
            .all(|r| r.title.as_deref().unwrap().contains("FOMC statement"))
    );
    let oct = refs
        .iter()
        .find(|r| r.url.ends_with("monetary20261028a.htm"))
        .unwrap();
    assert_eq!(oct.event_key_hint.as_deref(), Some("policy:fed:2026-10-28"));
    let conn = connector("cb-fed").unwrap();
    let f = conn.fetch_requests(oct);
    let p = conn
        .parse(
            oct,
            &[snap(
                &f[0].url,
                fixture("centralbank/fed-statement-20261028.html"),
            )],
        )
        .unwrap();
    assert_eq!(
        p.candidates[0].event_key.as_deref(),
        Some("policy:fed:2026-10-28")
    );
    assert_eq!(p.candidates[0].rule_key, "kind:statement");
    assert!(p.candidates[0].title.contains("FOMC statement"));
    assert!(
        p.points.is_empty(),
        "statements carry no numbers extracted by code"
    );
    assert_eq!(p.entities[0].kind, "central_bank");
}

#[test]
fn boe_and_boc_statements_are_found_by_their_own_wording() {
    let boe = bank_items("cb-boe");
    assert!(
        boe.iter()
            .any(|r| r.event_key_hint.as_deref() == Some("policy:boe:2026-11-05"))
            || boe.iter().all(|r| r
                .event_key_hint
                .as_deref()
                .unwrap()
                .starts_with("policy:boe:"))
    );
    assert!(boe.iter().all(|r| {
        r.title
            .as_deref()
            .unwrap()
            .contains("Monetary Policy Summary")
    }));
    let boc = bank_items("cb-boc");
    let oct = boc
        .iter()
        .find(|r| r.url.contains("fad-press-release-2026-10-28"))
        .unwrap();
    assert_eq!(oct.event_key_hint.as_deref(), Some("policy:boc:2026-10-28"));
    let conn = connector("cb-boc").unwrap();
    let f = conn.fetch_requests(oct);
    let p = conn
        .parse(
            oct,
            &[snap(
                &f[0].url,
                fixture("centralbank/boc-statement-20261028.html"),
            )],
        )
        .unwrap();
    assert!(p.candidates[0].title.contains("policy rate"));
}

#[test]
fn ecb_speeches_and_rba_notices_are_not_statements() {
    // The recorded ECB feed lists speeches and data releases only; the RBA feed one unrelated notice.
    assert!(bank_items("cb-ecb").is_empty());
    assert!(bank_items("cb-rba").is_empty());
    // With a decision in the feed the ECB's wording matches and the date is the publication day.
    let conn = connector("cb-ecb").unwrap();
    let feed = r#"<rss version="2.0"><channel><item><title>Monetary policy decisions</title>
        <link>https://www.ecb.europa.eu/press/pr/date/2026/html/ecb.mp261210~abc.en.html</link>
        <guid>g1</guid><pubDate>Thu, 10 Dec 2026 13:45:00 +0100</pubDate></item></channel></rss>"#;
    let d = conn
        .parse_discovery(
            &cx(&[]),
            None,
            &[snap(
                centralbank::bank("cb-ecb").unwrap().feed,
                feed.as_bytes().to_vec(),
            )],
        )
        .unwrap();
    assert_eq!(
        d.refs[0].event_key_hint.as_deref(),
        Some("policy:ecb:2026-12-10")
    );
}

#[test]
fn feed_failures_are_typed_and_an_empty_page_is_refused() {
    let conn = connector("cb-fed").unwrap();
    let url = centralbank::bank("cb-fed").unwrap().feed;
    let e = conn
        .parse_discovery(&cx(&[]), None, &[snap(url, b"<html>oops".to_vec())])
        .unwrap_err();
    assert_eq!(e.kind, ParseErrorKind::Shape);
    let item = bank_items("cb-fed").remove(0);
    let e = conn
        .parse(&item, &[snap(&item.url, b"<html></html>".to_vec())])
        .unwrap_err();
    assert_eq!(e.kind, ParseErrorKind::Value);
}

#[test]
fn the_cursor_stops_a_statement_being_discovered_twice() {
    let b = centralbank::bank("cb-fed").unwrap();
    let conn = connector("cb-fed").unwrap();
    let s = snap(b.feed, fixture("centralbank/fed-feed.xml"));
    let first = conn
        .parse_discovery(&cx(&[]), None, std::slice::from_ref(&s))
        .unwrap();
    let second = conn
        .parse_discovery(&cx(&[]), first.next_cursor.as_deref(), &[s])
        .unwrap();
    assert!(!first.refs.is_empty() && second.refs.is_empty());
}

// -------------------------------------------------------------------------------------- registry

#[test]
fn every_built_connector_is_registered_once() {
    let mut seen = std::collections::BTreeSet::new();
    for id in news_connectors::IDS {
        let c = connector(id).unwrap_or_else(|| panic!("{id} not registered"));
        assert_eq!(c.id(), *id);
        assert!(seen.insert(*id), "{id} twice");
    }
    assert!(connector("imf").is_none(), "IMF is not built");
    let _ = (
        abs::ID,
        arxiv::ID,
        bea::ID,
        crossref::ID,
        ecb::ID,
        eurostat::ID,
        nasa::ID,
    );
    let _ = (oecd::ID, ons::ID, openalex::ID, worldbank::ID);
}
