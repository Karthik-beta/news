//! The seven fixture pages (Development Plan 3.5; #17): one per page type, built from recorded public data and
//! rendered and checked like production pages. The work directory is `pipeline/fixtures/`
//! (`NEWS_WORK_DIR=pipeline/fixtures news render earnings`).
//!
//! * Code builds each page's brief, `data.json`, `sources.json` and `page.json` from recorded source files,
//!   parsed by the connectors and briefed by Fixer. They must match the committed files.
//! * The page writer's Copy (`pipeline/fixtures/copies/<page>.json`, Opus-written, references only) passes every
//!   Copy validator, and holds no digit outside a reference (the image record's fields excepted, checked equal).
//! * The committed bundles pass C1-C14 (no network: link checks answer, axe is skipped).
//! * With Bun, Plate renders each page to the committed bundle, byte for byte, and builds a templated page for a
//!   Tier 2 fixture whose Copy fails.
//!
//! `NEWS_UPDATE_GOLDEN=1` rewrites the committed files after a deliberate change (with Bun for the bundles).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use news_connectors::{Cx, ItemRef, Parsed, Snapshot, connector};
use news_core::brief::{Action, Brief, Entity};
use news_fixer::build::Built;
use news_fixer::inputs::{Block, Document, Inputs, Point};
use news_fixer::page::{Provenance, data};
use news_proof::axe::{AxeRunner, Violation};
use news_proof::checks::{self, AxeMode, Env, Markup};
use news_proof::lexicon::Lexicon;
use news_proof::links::{LinkChecker, Probe};
use news_proof::plate::Plate;
use news_proof::render;
use news_proof::validate::{OuiToken, validate};
use news_proof::work::{self, TEMPLATED, Work};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn root() -> PathBuf {
    repo().join("pipeline/fixtures")
}

fn update() -> bool {
    std::env::var_os("NEWS_UPDATE_GOLDEN").is_some_and(|v| v == "1")
}

fn read(path: impl AsRef<Path>) -> Vec<u8> {
    let p = path.as_ref();
    std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn recorded(path: &str) -> Vec<u8> {
    read(
        repo()
            .join("cli/crates/news-connectors/tests/fixtures")
            .join(path),
    )
}

fn t(y: i32, m: u32, d: u32, h: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, h, 0, 0).unwrap()
}

fn snap(url: &str, bytes: Vec<u8>, at: DateTime<Utc>) -> Snapshot {
    Snapshot {
        url: url.to_owned(),
        sha256: [7; 32],
        fetched_at: at,
        content_type: None,
        bytes,
    }
}

fn entity(id: &str, name: &str, kind: &str) -> Entity {
    Entity {
        id: id.into(),
        name: name.into(),
        kind: kind.into(),
    }
}

/// A page's source record, from the Contacts registry (`data/sources/<id>.toml`) and the document.
struct Doc {
    src: &'static str,
    registry: &'static str,
    title: &'static str,
    short: &'static str,
    kind: &'static str,
    url: String,
    licence: &'static str,
    licence_url: Option<&'static str>,
    published: Option<&'static str>,
    retrieved: DateTime<Utc>,
}

/// One fixture page: its inputs, where each value came from, its sources and its page facts.
struct Fixture {
    name: &'static str,
    inputs: Inputs,
    prov: Provenance,
    docs: Vec<Doc>,
    region: &'static str,
    tags: Vec<&'static str>,
}

/// Inputs from parses: the first candidate of each parse holds the story's points, every point is history.
fn from_parsed(story_id: i64, page_type: &str, tier: u8, parsed: &[&Parsed]) -> Inputs {
    let mut points = Vec::new();
    let mut history = Vec::new();
    for p in parsed {
        if let Some(c) = p.candidates.first() {
            for id in &c.dp_ids {
                points.push(Point::from(
                    p.points.iter().find(|d| &d.dp_id == id).unwrap(),
                ));
            }
        }
        history.extend(p.points.iter().map(Point::from));
    }
    Inputs {
        story_id,
        event_key: parsed[0]
            .candidates
            .first()
            .and_then(|c| c.event_key.clone()),
        page_type: page_type.into(),
        tier,
        action: Action::New,
        categories: Vec::new(),
        entities: Vec::new(),
        points,
        history,
        documents: Vec::new(),
        images: parsed
            .iter()
            .flat_map(|p| p.images.iter().map(|i| serde_json::to_value(i).unwrap()))
            .collect(),
        related: Vec::new(),
    }
}

/// Every parsed point's source and locator.
fn provenance(pairs: &[(&Parsed, &str)]) -> Provenance {
    let mut prov = Provenance::default();
    for (p, src) in pairs {
        for d in &p.points {
            prov.points
                .insert(d.dp_id.clone(), ((*src).to_owned(), d.locator.clone()));
        }
    }
    prov
}

fn cx(pairs: &[(&str, &[&str])]) -> Cx {
    Cx {
        today: NaiveDate::from_ymd_opt(2026, 11, 18).unwrap(),
        sec_ciks: vec!["320193".into()],
        fred_series: Vec::new(),
        bls_series: Vec::new(),
        tracked: pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.iter().map(|s| (*s).to_owned()).collect()))
            .collect(),
    }
}

/// A statistics Stringer's probe, item and history, as the connectors' own tests run them.
fn stat(id: &str, series: &str, probe: &str, history: &str, at: DateTime<Utc>) -> Parsed {
    let conn = connector(id).unwrap();
    let c = cx(&[(id, &[series])]);
    let req = &conn.discovery_requests(&c, None)[0];
    let d = conn
        .parse_discovery(&c, None, &[snap(&req.url, recorded(probe), at)])
        .unwrap();
    let item = &d.refs[0];
    let fetch = &conn.fetch_requests(item)[0];
    conn.parse(item, &[snap(&fetch.url, recorded(history), at)])
        .unwrap()
}

/// A BLS series, recorded 16 Nov 2026 (public domain).
fn bls(series: &str) -> Parsed {
    let conn = connector("bls").unwrap();
    let item = ItemRef {
        source_id: "bls".into(),
        kind: "bls.series".into(),
        key: series.into(),
        url: format!("https://data.bls.gov/timeseries/{series}"),
        event_key_hint: None,
        published_at: None,
        title: None,
        meta: json!({}),
    };
    let url = &conn.fetch_requests(&item)[0].url;
    conn.parse(
        &item,
        &[snap(
            url,
            recorded(&format!("bls/{series}.json")),
            t(2026, 11, 16, 14),
        )],
    )
    .unwrap()
}

fn bls_doc(src: &'static str, title: &'static str, short: &'static str, series: &str) -> Doc {
    Doc {
        src,
        registry: "bls",
        title,
        short,
        kind: "dataset",
        url: format!("https://data.bls.gov/timeseries/{series}"),
        licence: "Public domain",
        licence_url: Some("https://www.bls.gov/opub/copyright-information.htm"),
        published: None,
        retrieved: t(2026, 11, 16, 14),
    }
}

/// Apple's fiscal 2024 Form 10-K facts, recorded from SEC companyfacts (`site/tests/support/sec/`, public domain)
/// and put back into the companyfacts response shape the SEC connector reads: the same tags, values, dates,
/// form, fiscal year and filing date, under the filing's accession number. The record has no labels, so the
/// connector labels each fact with its XBRL tag.
fn earnings() -> Fixture {
    let rec: Value = serde_json::from_slice(&read(
        repo().join("site/tests/support/sec/aapl-fy2024-companyfacts.json"),
    ))
    .unwrap();
    let accn = rec["accn"].as_str().unwrap();
    let mut gaap = serde_json::Map::new();
    for (tag, f) in rec["facts"].as_object().unwrap() {
        let mut fact = f.clone();
        fact["accn"] = json!(accn);
        gaap.insert(
            tag.strip_prefix("us-gaap:").unwrap().to_owned(),
            json!({"units": {"USD": [fact]}}),
        );
    }
    let body = json!({"cik": rec["cik"], "entityName": rec["entity"], "facts": {"us-gaap": gaap}});
    let conn = connector("sec-edgar").unwrap();
    let folder = accn.replace('-', "");
    let item = ItemRef {
        source_id: "sec-edgar".into(),
        kind: "sec.filing".into(),
        key: accn.into(),
        url: format!("https://www.sec.gov/Archives/edgar/data/320193/{folder}/{accn}-index.htm"),
        event_key_hint: None,
        published_at: Some(t(2024, 11, 1, 0)),
        title: Some("Apple Inc. 10-K".into()),
        meta: json!({"cik": "0000320193", "form": "10-K", "items": ""}),
    };
    let url = &conn.fetch_requests(&item)[0].url;
    let p = conn
        .parse(
            &item,
            &[snap(
                url,
                serde_json::to_vec(&body).unwrap(),
                t(2026, 11, 21, 0),
            )],
        )
        .unwrap();
    let mut inputs = from_parsed(91001, "earnings", 1, &[&p]);
    inputs.categories = vec!["finance".into(), "earnings".into()];
    inputs.entities = vec![entity("ent_apple", "Apple Inc.", "company")];
    Fixture {
        name: "earnings",
        prov: provenance(&[(&p, "src_aapl_10k")]),
        inputs,
        docs: vec![Doc {
            src: "src_aapl_10k",
            registry: "sec-edgar",
            title: "Apple Inc. annual report on Form 10-K",
            short: "Apple Inc., annual report",
            kind: "filing",
            url: item.url.clone(),
            licence: "Public filing",
            licence_url: None,
            published: Some("2024-11-01"),
            retrieved: t(2026, 11, 21, 0),
        }],
        region: "north-america",
        tags: vec!["apple", "earnings"],
    }
}

/// America's jobs report: payrolls and the unemployment rate, BLS, recorded 16 Nov 2026.
fn macro_release() -> Fixture {
    let jobs = bls("CES0000000001");
    let rate = bls("LNS14000000");
    let mut inputs = from_parsed(91002, "macro-release", 1, &[&jobs, &rate]);
    inputs.categories = vec!["economy".into(), "jobs".into()];
    inputs.entities = vec![
        entity("ent_us", "United States", "country"),
        entity("ent_bls", "U.S. Bureau of Labor Statistics", "agency"),
    ];
    Fixture {
        name: "macro-release",
        prov: provenance(&[(&jobs, "src_bls_ces"), (&rate, "src_bls_cps")]),
        inputs,
        docs: vec![
            bls_doc(
                "src_bls_ces",
                "Current Employment Statistics: total nonfarm employment",
                "Current Employment Statistics",
                "CES0000000001",
            ),
            bls_doc(
                "src_bls_cps",
                "Current Population Survey: unemployment rate",
                "Current Population Survey",
                "LNS14000000",
            ),
        ],
        region: "north-america",
        tags: vec!["jobs", "unemployment"],
    }
}

/// The World Bank's GDP for America, World Development Indicators, recorded 18 Nov 2026 (CC BY 4.0).
fn major_report() -> Fixture {
    let p = stat(
        "worldbank",
        "GDP.USA",
        "worldbank/GDP.USA-probe.json",
        "worldbank/GDP.USA-history.json",
        t(2026, 11, 18, 6),
    );
    let mut inputs = from_parsed(91003, "major-report", 2, &[&p]);
    inputs.categories = vec!["economy".into(), "growth".into()];
    inputs.entities = vec![
        entity("ent_us", "United States", "country"),
        entity("ent_wb", "The World Bank", "institution"),
    ];
    Fixture {
        name: "major-report",
        prov: provenance(&[(&p, "src_wb_wdi")]),
        inputs,
        docs: vec![Doc {
            src: "src_wb_wdi",
            registry: "worldbank",
            title: "World Development Indicators: GDP (current US$), United States",
            short: "World Development Indicators",
            kind: "dataset",
            url: "https://data.worldbank.org/indicator/NY.GDP.MKTP.CD?locations=US".into(),
            licence: "CC BY 4.0",
            licence_url: Some("https://creativecommons.org/licenses/by/4.0/"),
            published: None,
            retrieved: t(2026, 11, 18, 6),
        }],
        region: "north-america",
        tags: vec!["gdp"],
    }
}

/// The ECB's main refinancing rate, ECB Data Portal, recorded 18 Nov 2026. The registry says the data may be
/// cited but not redistributed, so the page offers no download of it (C8).
fn policy_decision() -> Fixture {
    let p = stat(
        "ecb",
        "FM.MRR",
        "ecb/FM.MRR-probe.json",
        "ecb/FM.MRR-history.json",
        t(2026, 11, 18, 6),
    );
    let mut inputs = from_parsed(91004, "policy-decision", 1, &[&p]);
    inputs.categories = vec!["economy".into(), "monetary-policy".into()];
    inputs.entities = vec![entity("ent_ecb", "European Central Bank", "central_bank")];
    Fixture {
        name: "policy-decision",
        prov: provenance(&[(&p, "src_ecb_mrr")]),
        inputs,
        docs: vec![Doc {
            src: "src_ecb_mrr",
            registry: "ecb",
            title: "ECB Data Portal: main refinancing operations, fixed rate",
            short: "ECB Data Portal",
            kind: "dataset",
            url: "https://data.ecb.europa.eu/data/datasets/FM/FM.B.U2.EUR.4F.KR.MRR_FR.LEV".into(),
            licence: "ECB terms of use",
            licence_url: Some("https://www.ecb.europa.eu/services/disclaimer/html/index.en.html"),
            published: None,
            retrieved: t(2026, 11, 18, 6),
        }],
        region: "europe",
        tags: vec!["ecb", "interest-rates"],
    }
}

/// Paragraphs of a text file as blocks, the first line a heading.
fn document(doc_id: i64, text: &str) -> Document {
    let text = text.replace("\r\n", "\n");
    Document {
        doc_id,
        blocks: text
            .split("\n\n")
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .enumerate()
            .map(|(i, p)| Block {
                block_no: i32::try_from(i).unwrap(),
                kind: Some(if i == 0 { "heading" } else { "paragraph" }.into()),
                text: p.to_owned(),
            })
            .collect(),
    }
}

/// The Federal Reserve's statement of 17 September 2025, as Taster's tests hold it.
fn news_story() -> Fixture {
    let text = String::from_utf8(read(
        repo().join("cli/crates/news-taster/tests/fixtures/fomc-2025-09-17-board.txt"),
    ))
    .unwrap();
    let inputs = Inputs {
        story_id: 91005,
        event_key: Some("policy:cb-fed:2025-09-17".into()),
        page_type: "news-story".into(),
        tier: 2,
        action: Action::New,
        categories: vec!["economy".into(), "monetary-policy".into()],
        entities: vec![entity("ent_fed", "Federal Reserve", "central_bank")],
        points: Vec::new(),
        history: Vec::new(),
        documents: vec![document(7001, &text)],
        images: Vec::new(),
        related: Vec::new(),
    };
    let mut prov = Provenance::default();
    prov.docs.insert(7001, "src_fed_statement".into());
    Fixture {
        name: "news-story",
        inputs,
        prov,
        docs: vec![Doc {
            src: "src_fed_statement",
            registry: "cb-fed",
            title: "Federal Reserve issues FOMC statement",
            short: "FOMC statement",
            kind: "statement",
            url: "https://www.federalreserve.gov/newsevents/pressreleases/monetary20250917a.htm"
                .into(),
            licence: "Federal Reserve website policies",
            licence_url: Some("https://www.federalreserve.gov/disclaimer.htm"),
            published: Some("2025-09-17"),
            retrieved: t(2026, 11, 18, 6),
        }],
        region: "north-america",
        tags: vec!["federal-reserve", "interest-rates"],
    }
}

/// NASA's Artemis II launch photograph and its description, NASA Image and Video Library, recorded 18 Nov 2026.
fn science() -> Fixture {
    let conn = connector("nasa").unwrap();
    let c = cx(&[("nasa", &["artemis"])]);
    let req = &conn.discovery_requests(&c, None)[0];
    let d = conn
        .parse_discovery(
            &c,
            None,
            &[snap(
                &req.url,
                recorded("nasa/search-artemis.json"),
                t(2026, 11, 18, 6),
            )],
        )
        .unwrap();
    let item = d.refs.iter().find(|r| r.key.ends_with("Launch-1")).unwrap();
    let bytes = recorded("nasa/item-KSC-04012026-Artemis II_Launch-1.json");
    let raw: Value = serde_json::from_slice(&bytes).unwrap();
    let description = raw["collection"]["items"][0]["data"][0]["description"]
        .as_str()
        .unwrap()
        .to_owned();
    let url = &conn.fetch_requests(item)[0].url;
    let p = conn
        .parse(item, &[snap(url, bytes, t(2026, 11, 18, 6))])
        .unwrap();
    let mut inputs = from_parsed(91006, "science", 2, &[&p]);
    inputs.event_key = Some("science:nasa:artemis-ii-launch".into());
    inputs.categories = vec!["science".into(), "space".into()];
    inputs.entities = vec![entity("ent_nasa", "NASA", "agency")];
    inputs.documents = vec![Document {
        doc_id: 7101,
        blocks: vec![Block {
            block_no: 0,
            kind: Some("paragraph".into()),
            text: description,
        }],
    }];
    let mut prov = provenance(&[(&p, "src_nasa_image")]);
    prov.docs.insert(7101, "src_nasa_image".into());
    Fixture {
        name: "science",
        inputs,
        prov,
        docs: vec![Doc {
            src: "src_nasa_image",
            registry: "nasa",
            title: "NASA Image and Video Library: Artemis II launch",
            short: "NASA Image and Video Library",
            kind: "other",
            url: item.url.clone(),
            licence: "NASA media usage guidelines",
            licence_url: Some("https://www.nasa.gov/nasa-brand-center/images-and-media/"),
            published: None,
            retrieved: t(2026, 11, 18, 6),
        }],
        region: "north-america",
        tags: vec!["artemis", "nasa"],
    }
}

/// How America measures inflation: the CPI-U index, BLS, recorded 16 Nov 2026.
fn explainer() -> Fixture {
    let p = bls("CUUR0000SA0");
    let mut inputs = from_parsed(91007, "explainer", 1, &[&p]);
    inputs.categories = vec!["economy".into(), "inflation".into()];
    inputs.entities = vec![
        entity("ent_us", "United States", "country"),
        entity("ent_bls", "U.S. Bureau of Labor Statistics", "agency"),
    ];
    Fixture {
        name: "explainer",
        prov: provenance(&[(&p, "src_bls_cpi")]),
        inputs,
        docs: vec![bls_doc(
            "src_bls_cpi",
            "Consumer Price Index for All Urban Consumers: all items, U.S. city average",
            "Consumer Price Index",
            "CUUR0000SA0",
        )],
        region: "north-america",
        tags: vec!["inflation", "cpi"],
    }
}

fn all() -> Vec<Fixture> {
    vec![
        earnings(),
        macro_release(),
        major_report(),
        policy_decision(),
        news_story(),
        science(),
        explainer(),
    ]
}

/// The page's `sources.json`: the document's own fields, and publisher, connector and redistribution from the
/// Contacts registry.
fn sources(docs: &[Doc]) -> Value {
    let registry = news_contacts::load_dir(&repo().join("data/sources")).unwrap();
    let mut out = serde_json::Map::new();
    for d in docs {
        let r = registry
            .iter()
            .find(|s| s.source_id == d.registry)
            .unwrap_or_else(|| panic!("{} is not in data/sources", d.registry));
        out.insert(
            d.src.into(),
            json!({
                "title": d.title, "short": d.short, "publisher": r.publisher, "connector": r.connector,
                "type": d.kind, "primary": true, "url": d.url,
                "licence": {"name": d.licence, "url": d.licence_url, "redistribution": r.redistribution},
                "published_at": d.published, "retrieved_at": d.retrieved.date_naive().to_string(),
            }),
        );
    }
    json!({"version": 1, "sources": out})
}

/// When code says a fixture page was saved: noon on the day its data is as of.
fn saved_at(brief: &Brief) -> DateTime<Utc> {
    let day = brief
        .item
        .data_as_of
        .unwrap_or_else(|| NaiveDate::from_ymd_opt(2026, 11, 18).unwrap());
    Utc.from_utc_datetime(&day.and_hms_opt(12, 0, 0).unwrap())
}

fn copy(name: &str) -> Value {
    serde_json::from_slice(&read(root().join("copies").join(format!("{name}.json")))).unwrap()
}

/// Everything code writes for a fixture page before render, by path under the work directory: the brief,
/// data.json, sources.json, page.json and the stored Copy.
fn built_files(f: &Fixture) -> (Built, BTreeMap<String, String>) {
    let built = news_fixer::brief(&f.inputs).unwrap();
    let data = data(&f.inputs, &built, &f.prov).unwrap();
    let page = json!({
        "categories": [f.inputs.categories.join("/")],
        "region": f.region,
        "tags": f.tags,
    });
    let n = f.name;
    // The stored Copy, through `news save`'s own code, in a scratch work directory.
    let scratch = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("news-fixture-{n}-{}", std::process::id()));
    let stored = work::save(
        &Work::new(&scratch),
        n,
        copy(n),
        Vec::new(),
        "save",
        None,
        saved_at(&built.brief),
    )
    .unwrap();
    let _ = std::fs::remove_dir_all(&scratch);
    let files = BTreeMap::from([
        (format!("briefs/{n}.json"), work::pretty(&built.brief)),
        (format!("pages/{n}/data.json"), work::pretty(&data)),
        (
            format!("pages/{n}/sources.json"),
            work::pretty(&sources(&f.docs)),
        ),
        (format!("pages/{n}/page.json"), work::pretty(&page)),
        (format!("pages/{n}/copy.json"), work::pretty(&stored)),
    ]);
    (built, files)
}

/// A committed file, byte for byte (`.gitattributes` keeps `pages/` out of line-ending conversion).
fn committed(rel: &str) -> String {
    String::from_utf8(read(root().join(rel))).unwrap()
}

#[test]
fn page_inputs_are_built_by_code_from_recorded_data() {
    assert_eq!(all().len(), 7, "one page per page type");
    for f in all() {
        let (built, files) = built_files(&f);
        assert_eq!(built.brief.item.page_type, f.name, "{}", f.name);
        assert!(built.trimmed.is_empty(), "{}: {:?}", f.name, built.trimmed);
        for (rel, text) in &files {
            if update() {
                work::write_atomic(&root().join(rel), text).unwrap();
            } else {
                assert!(
                    &committed(rel) == text,
                    "{rel} differs from what code builds; NEWS_UPDATE_GOLDEN=1 rewrites it"
                );
            }
        }
    }
}

/// The page writer's own text: the Copy's head and every string literal of its `.oui`.
fn copy_texts(copy: &Value) -> Vec<String> {
    let mut out: Vec<String> = ["headline", "fly_title", "summary"]
        .iter()
        .map(|k| copy[k].as_str().unwrap().to_owned())
        .collect();
    out.extend(
        news_proof::validate::oui_tokens(copy["oui"].as_str().unwrap())
            .into_iter()
            .map(|t| match t {
                OuiToken::Text { text, .. } | OuiToken::Number { text, .. } => text,
            }),
    );
    out
}

#[test]
fn copies_are_references_only_and_pass_the_validators() {
    for f in all() {
        let (built, _) = built_files(&f);
        let c = copy(f.name);
        assert_eq!(validate(&c, Some(&built.brief), None), [], "{}", f.name);
        // Image records are copied verbatim from the brief (URL, width, height): the only digits allowed.
        let media: Vec<String> = built
            .brief
            .media
            .images
            .iter()
            .flat_map(|i| {
                [
                    i["url"].to_string(),
                    i["width"].to_string(),
                    i["height"].to_string(),
                ]
            })
            .map(|s| s.trim_matches('"').to_owned())
            .collect();
        for text in copy_texts(&c) {
            if media.contains(&text) {
                continue;
            }
            let left = without_refs(&text);
            assert!(
                !left.chars().any(|ch| ch.is_ascii_digit()),
                "{}: a digit outside a reference in {text:?}",
                f.name
            );
        }
    }
}

/// `text` with every reference (`{dp_…}` tokens and bare refs) taken out.
fn without_refs(text: &str) -> String {
    let mut out = String::new();
    for word in text.split_inclusive(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
        let (w, tail) = word.split_at(
            word.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(word.len()),
        );
        if !news_core::refs::is_ref(w) {
            out.push_str(w);
        }
        out.push_str(tail);
    }
    out
}

struct Answering;
impl LinkChecker for Answering {
    fn check(&self, _: &str) -> Probe {
        Probe {
            ok: true,
            note: "not probed in tests".into(),
        }
    }
}

struct NoAxe;
impl AxeRunner for NoAxe {
    fn run(&self, _: &Path) -> Result<Vec<Violation>, String> {
        Err("axe does not run in this test".into())
    }
}

/// C1-C14 on a page folder, with the fixture's stored text blocks, no other page, links that answer and no axe.
fn proof(f: &Fixture, dir: &Path) -> Markup {
    let blocks: BTreeMap<(i64, i64), String> = f
        .inputs
        .documents
        .iter()
        .flat_map(|d| {
            d.blocks
                .iter()
                .map(|b| ((d.doc_id, i64::from(b.block_no)), b.text.clone()))
        })
        .collect();
    let lex = Lexicon::v1().unwrap();
    let env = Env {
        lexicon: &lex,
        text_blocks: &blocks,
        duplicates: Some(&[]),
        links: Some(&Answering),
        axe: &NoAxe,
        axe_mode: AxeMode::Off,
    };
    checks::run(&news_proof::bundle::Bundle::load(dir), &env)
}

fn assert_passes(name: &str, m: &Markup) {
    assert!(m.pass, "{name}: {:#?}", m.failures);
    assert!(m.warnings.is_empty(), "{name}: {:#?}", m.warnings);
    for c in &m.checks {
        let axe_skipped = c.check == "C13" && c.status == "skipped";
        assert!(c.status == "pass" || axe_skipped, "{name}: {c:?}");
    }
}

#[test]
fn the_committed_bundles_pass_c1_to_c14() {
    for f in all() {
        let dir = root().join("pages").join(f.name);
        assert!(
            dir.join("index.html").is_file(),
            "{}: not rendered; run with Bun and NEWS_UPDATE_GOLDEN=1",
            f.name
        );
        assert_passes(f.name, &proof(&f, &dir));
    }
}

/// A Plate for this test, or None (with a note) when Bun or the site's packages are missing.
async fn plate() -> Option<Plate> {
    let bun = std::process::Command::new("bun").arg("--version").output();
    if !bun.is_ok_and(|o| o.status.success())
        || !repo().join("node_modules/@openuidev/react-lang").is_dir()
    {
        eprintln!("skipped: Plate needs Bun and `bun install` at the repository root");
        return None;
    }
    // The `news` binary installs the TLS provider in `main`; a test builds Plate's client without it.
    let _ = rustls::crypto::ring::default_provider().install_default();
    Some(Plate::connect().await.expect("Plate starts"))
}

#[tokio::test]
async fn plate_renders_each_page_to_its_committed_bundle() {
    let Some(plate) = plate().await else { return };
    let work = Work::new(root());
    for f in all() {
        let inputs = render::load_inputs(&work, f.name).unwrap();
        let rendered = plate.render(&inputs.render_request()).await.unwrap();
        assert!(
            rendered.problems.is_empty(),
            "{}: {:?}",
            f.name,
            rendered.problems
        );
        let bundle = render::assemble(&inputs, &rendered).unwrap();
        let dir = work.page(f.name);
        if update() {
            render::write(&bundle, &dir).unwrap();
            continue;
        }
        let manifest: Value =
            serde_json::from_str(&committed(&format!("pages/{}/manifest.json", f.name))).unwrap();
        assert_eq!(
            manifest["bundle_sha256"],
            json!(bundle.sha256),
            "{}: render differs from the committed bundle",
            f.name
        );
        for (path, text) in &bundle.files {
            assert!(
                &committed(&format!("pages/{}/{path}", f.name)) == text,
                "{}: {path} differs",
                f.name
            );
        }
    }
}

#[tokio::test]
async fn a_failing_tier_2_fixture_gets_a_templated_page() {
    let Some(plate) = plate().await else { return };
    let f = major_report();
    let (built, files) = built_files(&f);
    assert_eq!(built.brief.item.tier, 2);
    // Reporter's Copy types a number: the validators refuse it (twice, in a run), so code builds the page (D10).
    let mut bad = copy(f.name);
    bad["summary"] = json!("The economy grew by five per cent in a year.");
    assert!(!validate(&bad, Some(&built.brief), None).is_empty());
    let templated =
        serde_json::to_value(news_proof::fallback::templated(&built.brief).unwrap()).unwrap();
    assert_eq!(validate(&templated, Some(&built.brief), None), []);

    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("news-fixture-templated-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let work = Work::new(&dir);
    for (rel, text) in &files {
        work::write_atomic(&dir.join(rel), text).unwrap();
    }
    std::fs::write(
        work.page(f.name).join("page.json"),
        json!({"categories": ["economy/growth"], "region": f.region, "index_status": "index"})
            .to_string(),
    )
    .unwrap();
    work::save(
        &work,
        f.name,
        templated,
        vec![TEMPLATED.into()],
        "fallback",
        None,
        saved_at(&built.brief),
    )
    .unwrap();
    let inputs = render::load_inputs(&work, f.name).unwrap();
    let rendered = plate.render(&inputs.render_request()).await.unwrap();
    assert!(rendered.problems.is_empty(), "{:?}", rendered.problems);
    let bundle = render::assemble(&inputs, &rendered).unwrap();
    assert_eq!(bundle.meta["flags"], json!(["templated"]));
    assert_eq!(bundle.meta["index_status"], "noindex");
    assert!(bundle.files["index.html"].contains("<meta name=\"robots\" content=\"noindex\">"));
    render::write(&bundle, &work.page(f.name)).unwrap();
    assert_passes("templated major-report", &proof(&f, &work.page(f.name)));
    std::fs::remove_dir_all(dir).unwrap();
}
