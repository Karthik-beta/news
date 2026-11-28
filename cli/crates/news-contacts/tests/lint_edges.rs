//! Contacts lint and licence gate: boundaries, hostile values and the loader's failure modes, plus an
//! independent re-check of the repo's registry so a weakened lint cannot pass it by itself.

#![allow(clippy::unwrap_used)]

use std::path::Path;

use news_contacts::{LICENCES, Source, default_dir, licence_gate, lint, load_dir, parse};

fn dir() -> std::path::PathBuf {
    default_dir(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn repo() -> Vec<Source> {
    load_dir(&dir()).unwrap()
}

fn row(id: &str) -> Source {
    repo().into_iter().find(|s| s.source_id == id).unwrap()
}

fn rules(s: Source) -> Vec<&'static str> {
    lint(&[s]).into_iter().map(|i| i.rule).collect()
}

/// An enabled copy of the SEC row (shipped disabled), so every rule is live.
fn enabled() -> Source {
    let mut s = row("sec-edgar");
    s.enabled = true;
    s
}

#[test]
fn the_registry_meets_the_issue_without_asking_the_lint() {
    // Re-states the acceptance rule directly: every enabled source has a licence from the closed
    // list, attribution text, a tier 1-4, and a rate of at most half the documented one.
    let sources = repo();
    assert!(sources.len() >= 3);
    for s in sources.iter().filter(|s| s.enabled) {
        let id = &s.source_id;
        assert!(
            s.licence.as_deref().is_some_and(|l| LICENCES.contains(&l)),
            "{id}: licence"
        );
        assert!(!s.attribution.trim().is_empty(), "{id}: attribution");
        assert!((1..=4).contains(&s.source_tier), "{id}: tier");
        assert!(s.rate_per_sec > 0.0, "{id}: rate");
        assert!(
            s.rate_per_sec * 2.0 <= s.documented_rate_per_sec,
            "{id}: {} req/s is over half of the documented {}",
            s.rate_per_sec,
            s.documented_rate_per_sec
        );
        assert!(
            !s.documented_limit.trim().is_empty(),
            "{id}: documented limit"
        );
        if let (Some(q), Some(docq)) = (s.daily_quota, s.documented_daily_quota) {
            assert!(q <= docq, "{id}: daily quota");
        }
        // An enabled source can emit candidates.
        assert!(licence_gate(id, s.licence.as_deref()).is_ok(), "{id}");
    }
    // The three sources of #9 are all registered, each under its own file name.
    for id in ["sec-edgar", "fred", "bls"] {
        assert!(dir().join(format!("{id}.toml")).is_file(), "{id}.toml");
        assert!(sources.iter().any(|s| s.source_id == id), "{id}");
    }
}

#[test]
fn no_registry_file_carries_a_secret() {
    // The registry names the variable that holds a free key, never the key.
    for entry in std::fs::read_dir(dir()).unwrap().flatten() {
        let text = std::fs::read_to_string(entry.path()).unwrap();
        for line in text.lines().filter(|l| !l.trim_start().starts_with('#')) {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim().trim_matches('"'));
            let keyish = k.contains("key") || k.contains("token") || k.contains("secret");
            let looks_secret = v.len() >= 16 && v.bytes().all(|b| b.is_ascii_alphanumeric());
            assert!(
                !(keyish && looks_secret)
                    && !(k == "auth_env" && v.bytes().any(|b| b.is_ascii_lowercase())),
                "{}: {line}",
                entry.path().display()
            );
        }
    }
}

#[test]
fn the_rate_rule_is_exact_at_its_boundary_and_refuses_nonsense() {
    let mut s = enabled();
    s.documented_rate_per_sec = 10.0;
    for (rate, ok) in [
        (5.0, true),
        (4.999, true),
        (5.000_001, false),
        (10.0, false),
        (0.0, false),
        (-1.0, false),
        (f64::NAN, false),
        (f64::INFINITY, false),
    ] {
        s.rate_per_sec = rate;
        assert_eq!(rules(s.clone()).is_empty(), ok, "rate {rate}");
    }
    // A missing or nonsensical documented limit cannot be satisfied by any rate.
    s.rate_per_sec = 1.0;
    for doc in [0.0, -2.0, f64::NAN] {
        s.documented_rate_per_sec = doc;
        assert!(rules(s.clone()).contains(&"rate"), "documented {doc}");
    }
    s.documented_rate_per_sec = 10.0;
    s.documented_limit = "  ".into();
    assert_eq!(rules(s), vec!["rate"]);
}

#[test]
fn toml_floats_that_are_not_numbers_cannot_slip_past_the_rate_rule() {
    let text = std::fs::read_to_string(dir().join("bls.toml")).unwrap();
    assert!(text.contains("rate_per_sec = 0.5"));
    for value in ["nan", "inf", "-0.0", "0"] {
        let s = parse(&text.replacen("rate_per_sec = 0.5", &format!("rate_per_sec = {value}"), 1))
            .unwrap();
        assert!(rules(s).contains(&"rate"), "{value}");
    }
}

#[test]
fn only_the_closed_list_of_licences_passes_and_the_gate_agrees() {
    let mut s = enabled();
    for (licence, lint_rule) in [
        (None, Some("licence_missing")),
        (Some(""), Some("licence_missing")),
        (Some(" "), Some("licence")),
        (Some("Public-Domain"), Some("licence")),
        (Some("public-domain "), Some("licence")),
        (Some("CC0"), Some("licence")),
        (Some("public-domain"), None),
        (Some("cc-by-4.0"), None),
        (Some("custom"), None),
    ] {
        s.licence = licence.map(str::to_owned);
        assert_eq!(
            rules(s.clone()),
            lint_rule.into_iter().collect::<Vec<_>>(),
            "{licence:?}"
        );
        // Whatever the lint refuses, the gate refuses too: a source cannot emit without a licence.
        if lint_rule.is_some() {
            assert!(licence_gate("x", licence).is_err(), "{licence:?}");
        } else {
            assert!(licence_gate("x", licence).is_ok(), "{licence:?}");
        }
    }
    // `unclear` is a licence the gate lets through, but only with redistribution off.
    s.licence = Some("unclear".into());
    s.redistribution = true;
    assert_eq!(rules(s.clone()), vec!["redistribution"]);
    s.redistribution = false;
    assert!(rules(s).is_empty());
    assert!(licence_gate("x", Some("unclear")).is_ok());
}

#[test]
fn a_disabled_source_is_held_to_the_structural_rules_but_not_the_licence_rules() {
    let mut s = enabled();
    s.enabled = false;
    s.licence = None;
    s.attribution.clear();
    s.rate_per_sec = 99.0;
    assert!(rules(s.clone()).is_empty());
    s.source_tier = 9;
    s.base_url = "http://insecure.example/".into();
    s.regions.clear();
    assert_eq!(rules(s), vec!["base_url", "tier", "taxonomy"]);
}

#[test]
fn ids_must_be_unique_kebab_case() {
    let (a, mut b) = (enabled(), enabled());
    assert!(
        lint(&[a.clone(), b.clone()])
            .iter()
            .any(|i| i.rule == "duplicate_id")
    );
    for bad in ["", "SEC", "sec_edgar", "sec edgar", "sec-edgar\n", "séc"] {
        b.source_id = bad.into();
        assert!(rules(b.clone()).contains(&"source_id"), "{bad:?}");
    }
}

#[test]
fn the_loader_names_the_file_it_cannot_read() {
    let tmp = std::env::temp_dir().join(format!("news-contacts-edges-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    // An empty directory is an empty registry, not an error; other files are ignored.
    assert!(load_dir(&tmp).unwrap().is_empty());
    std::fs::write(tmp.join("README.md"), "not a source").unwrap();
    assert!(load_dir(&tmp).unwrap().is_empty());
    let good = std::fs::read_to_string(dir().join("bls.toml")).unwrap();
    for (name, text) in [
        ("truncated.toml", good[..good.len() / 2].to_owned()),
        ("typo.toml", good.replace("rate_per_sec", "rate_per_second")),
        ("missing.toml", good.replace("rate_per_sec = 0.5\n", "")),
        (
            "wrongtype.toml",
            good.replace("source_tier = 1", "source_tier = \"one\""),
        ),
        ("empty.toml", String::new()),
    ] {
        std::fs::write(tmp.join(name), text).unwrap();
        let e = load_dir(&tmp).unwrap_err().to_string();
        assert!(e.contains(name), "{name}: {e}");
        std::fs::remove_file(tmp.join(name)).unwrap();
    }
    // Two files claiming one id load, and the lint is what refuses them.
    std::fs::write(tmp.join("a.toml"), &good).unwrap();
    std::fs::write(tmp.join("b.toml"), &good).unwrap();
    let both = load_dir(&tmp).unwrap();
    assert_eq!(both.len(), 2);
    assert!(lint(&both).iter().any(|i| i.rule == "duplicate_id"));
    let _ = std::fs::remove_dir_all(&tmp);
}
