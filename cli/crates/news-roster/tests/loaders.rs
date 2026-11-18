//! The seed loaders, from recorded samples (GLEIF, ROR, Wikidata, 18 Nov 2026) and a synthetic SEC
//! file (the SEC requires a contact e-mail in the User-Agent to download the real one).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use news_roster::resolver::{Entity, Index, Step, resolve};
use news_roster::seed;

fn fixture(name: &str) -> String {
    let p = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

fn has(s: &seed::Seed, scheme: &str, value: &str) -> bool {
    s.ids.iter().any(|i| i.scheme == scheme && i.value == value)
}

#[test]
fn sec_tickers_merge_tickers_per_company_and_pad_the_cik() {
    let s = seed::sec_tickers(&fixture("sec-company-tickers-exchange.json")).unwrap();
    assert_eq!(s.len(), 4, "Alphabet's two tickers are one company");
    let apple = s.iter().find(|s| s.name == "Apple Inc.").unwrap();
    assert!(has(apple, "cik", "0000320193") && has(apple, "ticker@XNAS", "AAPL"));
    assert_eq!(
        (apple.kind.as_str(), apple.region.as_deref()),
        ("company", Some("us"))
    );
    let alpha = s.iter().find(|s| s.name == "Alphabet Inc.").unwrap();
    assert!(has(alpha, "ticker@XNAS", "GOOGL") && has(alpha, "ticker@XNAS", "GOOG"));
    assert!(has(
        s.iter().find(|s| s.name.starts_with("BERKSHIRE")).unwrap(),
        "ticker@XNYS",
        "BRK-B"
    ));
    // An exchange we do not map gets no ticker identifier (never a guessed MIC).
    let ex = s.iter().find(|s| s.name.starts_with("Example")).unwrap();
    assert_eq!(ex.ids.len(), 1);
    assert!(seed::sec_tickers("{\"fields\":[\"cik\"],\"data\":[]}").is_err());
    assert!(seed::sec_tickers("nope").is_err());
}

#[test]
fn gleif_ror_and_wikidata_samples_load() {
    let g = seed::gleif(&fixture("gleif-lei-records.json")).unwrap();
    let apple = g.iter().find(|s| s.name == "Apple Inc.").unwrap();
    assert!(has(apple, "lei", "HWUPKR0MPOU8FGXBT394"));
    assert_eq!(apple.region.as_deref(), Some("us"));
    assert!(
        apple.aliases.contains(&"Apple Computer, Inc.".to_owned()),
        "previous legal names are aliases"
    );

    let r = seed::ror(&fixture("ror-v2-organizations.json")).unwrap();
    let mit = r.iter().find(|s| has(s, "ror", "042nb2s44")).unwrap();
    assert_eq!(mit.name, "Massachusetts Institute of Technology");
    assert!(mit.aliases.contains(&"MIT".to_owned()));
    assert_eq!(
        (mit.kind.as_str(), mit.region.as_deref()),
        ("institution", Some("us"))
    );

    let c = seed::wikidata_countries(&fixture("wikidata-iso3166.json")).unwrap();
    assert_eq!(c.len(), 13);
    let au = c.iter().find(|s| has(s, "iso3166", "AU")).unwrap();
    assert_eq!(au.name, "Australia");
    assert!(
        au.ids
            .iter()
            .any(|i| i.scheme == "wikidata" && i.value == "Q408")
    );
    assert!(au.aliases.contains(&"Down Under".to_owned()));
    assert_eq!(au.region.as_deref(), Some("au"));

    let cur = seed::wikidata_currencies(&fixture("wikidata-iso4217.json")).unwrap();
    let aud = cur.iter().find(|s| has(s, "iso4217", "AUD")).unwrap();
    assert_eq!(
        aud.kind, "topic",
        "currencies are topic entities (ADR 0009)"
    );
    assert_eq!(aud.name, "Australian dollar");
    assert!(aud.region.is_none());
}

#[test]
fn loaded_seeds_resolve_through_the_resolver() {
    let mut ix = Index::default();
    let mut next = 1;
    let mut add = |s: &seed::Seed| {
        ix.add_entity(Entity {
            id: next,
            kind: s.kind.clone(),
            name: s.name.clone(),
            prominence: 3,
        });
        ix.add_alias(next, &s.name);
        for a in &s.aliases {
            ix.add_alias(next, a);
        }
        for p in &s.ids {
            ix.add_id(next, &p.scheme, &p.value);
        }
        next += 1;
    };
    for s in seed::wikidata_countries(&fixture("wikidata-iso3166.json")).unwrap() {
        add(&s);
    }
    for s in seed::sec_tickers(&fixture("sec-company-tickers-exchange.json")).unwrap() {
        add(&s);
    }
    let r = resolve(&ix, "iso3166:au", Some("country"));
    assert_eq!(
        (r.name.as_deref(), r.step),
        (Some("Australia"), Step::Identifier)
    );
    let r = resolve(&ix, "cik:320193", None);
    assert_eq!(r.name.as_deref(), Some("Apple Inc."));
    let r = resolve(&ix, "ticker@XNAS:AAPL", None);
    assert_eq!(r.name.as_deref(), Some("Apple Inc."));
    let r = resolve(&ix, "Down Under", Some("country"));
    assert_eq!(r.name.as_deref(), Some("Australia"));
}

#[test]
fn hand_written_seed_files_are_validated() {
    let ok = "[[entity]]\nkind = \"central_bank\"\nname = \"Federal Reserve\"\nids = [{ scheme = \"cb\", value = \"fed\" }]\n";
    assert_eq!(seed::parse_toml(ok).unwrap().len(), 1);
    assert!(
        seed::parse_toml(&ok.replace("central_bank", "bank")).is_err(),
        "unknown kind"
    );
    assert!(
        seed::parse_toml(&format!("{ok}extra = 1\n")).is_err(),
        "unknown field"
    );
    assert!(seed::parse_toml(&format!("{ok}prominence = 9\n")).is_err());
    let p = "[[entity]]\nscheme = \"cb\"\nvalue = \"fed\"\nprominence = 1\n";
    assert_eq!(seed::parse_prominence(p).unwrap()[0].prominence, 1);
    assert!(seed::parse_prominence(&p.replace("= 1", "= 4")).is_err());
}
