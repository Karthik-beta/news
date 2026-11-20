//! G7: the labelled mentions. Precision on resolved mentions must be at least 0.99 (a CI gate);
//! recall is reported. Ambiguous names (margin below 0.1) must stay unresolved.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;

use news_roster::resolver::{Entity, Index, Reason, Step, canonical_id, resolve};
use news_roster::seed::{self, Seed};

fn universe() -> (Index, BTreeMap<String, i64>) {
    let text = std::fs::read_to_string(format!(
        "{}/tests/g7/universe.toml",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let seeds: Vec<Seed> = seed::parse_toml(&text).unwrap();
    let mut ix = Index::default();
    let mut by_ref = BTreeMap::new();
    for (i, s) in seeds.iter().enumerate() {
        let id = i64::try_from(i).unwrap() + 1;
        ix.add_entity(Entity {
            id,
            kind: s.kind.clone(),
            name: s.name.clone(),
            prominence: 3,
        });
        ix.add_alias(id, &s.name);
        for a in &s.aliases {
            ix.add_alias(id, a);
        }
        for p in &s.ids {
            ix.add_id(id, &p.scheme, &p.value);
            by_ref.insert(
                format!("{}:{}", p.scheme, canonical_id(&p.scheme, &p.value)),
                id,
            );
        }
    }
    (ix, by_ref)
}

fn mentions() -> Vec<(String, Option<String>, Option<String>)> {
    let text = std::fs::read_to_string(format!(
        "{}/tests/g7/mentions.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    v["mentions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            let a = m.as_array().unwrap();
            (
                a[0].as_str().unwrap().to_owned(),
                a[1].as_str().map(str::to_owned),
                a[2].as_str().map(str::to_owned),
            )
        })
        .collect()
}

#[test]
fn g7_precision_is_at_least_099_and_recall_is_reported() {
    let (ix, by_ref) = universe();
    let labelled = mentions();
    let (mut resolved, mut correct, mut resolvable, mut found) = (0u32, 0u32, 0u32, 0u32);
    let mut wrong = Vec::new();
    let mut missed = Vec::new();
    for (text, kind, expect) in &labelled {
        let want = expect.as_ref().map(|e| {
            let (s, v) = e.split_once(':').unwrap();
            *by_ref
                .get(&format!("{s}:{}", canonical_id(s, v)))
                .unwrap_or_else(|| panic!("label {e:?} is not in the universe"))
        });
        let r = resolve(&ix, text, kind.as_deref());
        // Re-running changes nothing.
        assert_eq!(r, resolve(&ix, text, kind.as_deref()), "{text:?}");
        if want.is_some() {
            resolvable += 1;
        }
        if r.step == Step::Unresolved {
            assert!(r.entity_id.is_none() && r.reason.is_some());
            if want.is_some() {
                missed.push(format!("{text:?} ({kind:?}) -> {:?}", r.reason));
            }
        } else {
            resolved += 1;
            if r.entity_id == want {
                correct += 1;
                found += 1;
            } else {
                wrong.push(format!(
                    "{text:?} ({kind:?}) -> {:?} via {:?}, wanted {expect:?}",
                    r.name, r.step
                ));
            }
        }
    }
    let precision = f64::from(correct) / f64::from(resolved);
    let recall = f64::from(found) / f64::from(resolvable);
    println!(
        "G7: {} mentions, {resolved} resolved, precision {precision:.4}, recall {recall:.4}",
        labelled.len()
    );
    for m in &missed {
        println!("  unresolved: {m}");
    }
    assert!(wrong.is_empty(), "wrong resolutions: {wrong:#?}");
    assert!(precision >= 0.99, "precision {precision}");
    assert!(recall >= 0.85, "recall {recall} fell: {missed:#?}");
}

#[test]
fn ambiguous_names_stay_unresolved_with_their_candidates() {
    let (ix, by_ref) = universe();
    let r = resolve(&ix, "Delta", None);
    assert_eq!(r.reason, Some(Reason::Ambiguous));
    let ids: Vec<i64> = r.candidates.iter().map(|c| c.0).collect();
    assert!(ids.contains(&by_ref["cik:0000027904"]) && ids.contains(&by_ref["cik:0001101396"]));
    let r = resolve(&ix, "Korean", Some("country"));
    assert_eq!(r.reason, Some(Reason::Ambiguous));
    assert_eq!(r.candidates.len(), 2);
    // The resolver never invents an entity: nothing it returns is outside the index.
    let r = resolve(&ix, "Acme Corporation", Some("company"));
    assert!(r.entity_id.is_none());
}
