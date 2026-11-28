//! SimHash properties and candidate edge cases for the Clerk (#9), from plain inputs: no database,
//! no network.

#![allow(clippy::unwrap_used)]

use news_clerk::candidates::prepare;
use news_clerk::simhash::{as_bigint, distance, simhash};
use news_connectors::{CandidateDraft, Parsed};
use proptest::prelude::*;

fn draft(rule_key: &str, event_key: Option<&str>) -> CandidateDraft {
    CandidateDraft {
        event_key: event_key.map(str::to_owned),
        url: "https://example.test/x".into(),
        title: "A title".into(),
        published_at: None,
        rule_key: rule_key.into(),
        dp_ids: vec!["dp_aaaaaaaaaaaaaaaa".into()],
        entity: None,
    }
}

proptest! {
    /// The hash is a function of the words only: case, spacing and punctuation do not move it.
    #[test]
    fn simhash_ignores_case_spacing_and_punctuation(words in prop::collection::vec("[A-Za-z0-9]{1,9}", 0..40)) {
        let plain = words.join(" ");
        let messy = words
            .iter()
            .map(|w| w.to_uppercase())
            .collect::<Vec<_>>()
            .join(" \t\n, ... ;; ");
        prop_assert_eq!(simhash(&plain), simhash(&messy));
        prop_assert_eq!(simhash(&plain), simhash(&plain));
    }

    /// Never panics on any text, and the signed column is the same 64 bits.
    #[test]
    fn simhash_is_total_and_the_bigint_loses_nothing(text in "\\PC{0,200}") {
        let h = simhash(&text);
        let back = u64::from_ne_bytes(as_bigint(h).to_ne_bytes());
        prop_assert_eq!(back, h);
    }

    /// `distance` is a metric on 64-bit hashes.
    #[test]
    fn distance_is_a_metric(a in any::<u64>(), b in any::<u64>(), c in any::<u64>()) {
        prop_assert_eq!(distance(a, a), 0);
        prop_assert_eq!(distance(a, b), distance(b, a));
        prop_assert!(distance(a, c) <= distance(a, b) + distance(b, c));
        prop_assert!(distance(a, b) <= 64);
    }

    /// One changed word in a long text moves few bits.
    #[test]
    fn a_one_word_edit_stays_near(words in prop::collection::vec("[a-z]{3,9}", 60..90), at in 0usize..60) {
        let a = words.join(" ");
        let mut edited = words.clone();
        edited[at] = format!("{}x", edited[at]);
        let b = edited.join(" ");
        // Three shingles of ~58 change: a handful of bits at most, never half the hash.
        prop_assert!(distance(simhash(&a), simhash(&b)) <= 20, "{}", distance(simhash(&a), simhash(&b)));
    }
}

#[test]
fn degenerate_texts_hash_without_surprises() {
    // Empty, whitespace-only and punctuation-only texts all have no words: one hash, no panic.
    let empty = simhash("");
    assert_eq!(empty, simhash("   \n\t"));
    assert_eq!(empty, simhash("!!! ... ---"));
    // One and two words use the short path and still tell texts apart.
    assert_ne!(simhash("inflation"), simhash("unemployment"));
    assert_ne!(simhash("rates rose"), simhash("rates fell"));
    // Non-Latin scripts are words too.
    assert_ne!(simhash("物価 上昇 続く"), simhash("株価 下落 続く"));
}

#[test]
fn a_source_with_no_licence_emits_nothing_even_for_an_empty_parse() {
    let parsed = Parsed {
        candidates: vec![draft("family:cpi", Some("macro:us.cpi:2026-10"))],
        ..Parsed::default()
    };
    for licence in [None, Some(""), Some("bogus")] {
        let e = prepare("bls", licence, &parsed).unwrap_err();
        assert!(
            e.to_string().starts_with("licence_missing"),
            "{licence:?}: {e}"
        );
        assert!(prepare("bls", licence, &Parsed::default()).is_err());
    }
    assert!(
        prepare("bls", Some("public-domain"), &Parsed::default())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_rule_from_one_source_is_not_borrowed_by_another() {
    // `family:cpi` is a BLS and FRED rule; no other source gets its classification for free, and an
    // unmatched key keeps its event key and data points for the later (AI) classifier.
    let parsed = Parsed {
        candidates: vec![
            draft("family:cpi", Some("macro:us.cpi:2026-10")),
            draft("form:10-K", Some("earnings:0000320193:2026FY")),
            draft("form:S-1", None),
        ],
        ..Parsed::default()
    };
    let by = |source: &str| prepare(source, Some("public-domain"), &parsed).unwrap();
    let bls = by("bls");
    assert_eq!(
        (bls[0].page_type_by, bls[0].page_type.as_deref()),
        ("rule", Some("macro"))
    );
    assert_eq!(
        bls[1].page_type_by, "rule-unsure",
        "an SEC rule key under BLS"
    );
    let sec = by("sec-edgar");
    assert_eq!(
        sec[0].page_type_by, "rule-unsure",
        "a CPI rule key under SEC"
    );
    assert_eq!(
        (sec[1].page_type_by, sec[1].page_type.as_deref()),
        ("rule", Some("earnings"))
    );
    for c in bls.iter().chain(&sec) {
        if c.page_type_by == "rule-unsure" {
            assert!(c.categories.is_empty() && c.region.is_none() && c.page_type.is_none());
        } else {
            assert!(!c.categories.is_empty() && c.region.is_some());
        }
        // Classification never rewrites what the Stringer found.
        assert!(parsed.candidates.contains(&c.draft));
    }
    assert_eq!(sec[2].draft.event_key, None);
}
