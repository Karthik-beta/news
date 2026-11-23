//! Every command that is not built yet returns the envelope with `ok:false`, `precondition`, exit 4,
//! and names the issue that builds it. The list covers every stub leaf of the command tree; the unit
//! test `every_command_that_is_not_built_is_a_precondition_stub` in main.rs walks the tree itself.

use super::call;
use std::collections::BTreeSet;

/// (argv, issue).
pub const STUBS: &[(&str, &str)] = &[
    ("discover", "#9"),
    ("discover --source fred", "#9"),
    ("fetch 81234", "#9"),
    ("prune", "#9"),
    ("parse 81234", "#9"),
    ("related 81234", "#15"),
    ("stats", "#16"),
    ("save 81234 copy.json", "#17"),
    ("render 81234", "#17"),
    ("check 81234", "#17"),
    ("brief 81234", "#18"),
    ("ai input 1", "#18"),
    ("ai accept 1 out.json", "#18"),
    ("ai fallback 1", "#18"),
    ("eval run g1", "#19"),
    ("eval report e1", "#19"),
    ("update 2026/11/us-cpi-october-2026", "#20"),
    ("home candidates", "#20"),
    ("home save curation.json", "#20"),
    ("home check 3", "#20"),
    ("home preview 3", "#20"),
    ("home show", "#20"),
    ("corrections pull", "#21"),
    ("corrections show c1", "#21"),
    ("corrections resolve c1 --action dismiss", "#21"),
];

#[test]
fn every_stub_returns_precondition_with_exit_4() {
    for (argv, issue) in STUBS {
        let args: Vec<&str> = argv.split(' ').collect();
        let (env, code, stderr) = call(&args);
        assert_eq!(code, 4, "{argv}: {env}");
        assert_eq!(env["ok"], false, "{argv}");
        assert_eq!(env["error"]["code"], "precondition", "{argv}");
        assert_eq!(env["error"]["retryable"], false, "{argv}");
        assert!(
            env["error"]["message"]
                .as_str()
                .unwrap()
                .contains("not built yet"),
            "{argv}: {env}"
        );
        assert!(
            env["error"]["hint"].as_str().unwrap().contains(issue),
            "{argv}: {env}"
        );
        assert!(stderr.is_empty(), "{argv}: {stderr}");
    }
}

/// The CLI forms of G-tier commands (ADR 0008 §6.3): always `gated`, exit 7, with no database.
pub const G_TIER: &[&str] = &[
    "publish 81234",
    "home publish 3",
    "eval promote A1 v3",
    "scores adopt v2",
    "corrections resolve c1 --action update",
];

#[test]
fn every_g_tier_cli_form_is_gated() {
    for argv in G_TIER {
        let args: Vec<&str> = argv.split(' ').collect();
        let (env, code, _) = call(&args);
        assert_eq!(code, 7, "{argv}: {env}");
        assert_eq!(env["error"]["code"], "gated", "{argv}");
        assert!(
            env["error"]["message"]
                .as_str()
                .unwrap()
                .contains("authorised step of a live run"),
            "{argv}: {env}"
        );
    }
}

#[test]
fn stubs_cover_every_leaf_that_has_a_stub_contract() {
    // A stub contract (x-news-status "stub until #N") exists for every stub leaf except home and publish.
    let dir = super::contract::contracts_dir().join("commands");
    let mut stub_contracts = BTreeSet::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        if v["x-news-status"].as_str().unwrap().starts_with("stub") {
            let name = p
                .file_name()
                .unwrap()
                .to_string_lossy()
                .replace(".schema.json", "");
            stub_contracts.insert(name);
        }
    }
    let covered: BTreeSet<String> = STUBS
        .iter()
        .map(|(argv, _)| {
            // The command path is the leading words that name subcommands.
            let words: Vec<&str> = argv.split(' ').collect();
            let mut path = Vec::new();
            for w in words {
                let candidate = if path.is_empty() {
                    w.to_owned()
                } else {
                    format!("{}.{w}", path.join("."))
                };
                if stub_contracts
                    .iter()
                    .any(|c| c == &candidate || c.starts_with(&format!("{candidate}.")))
                {
                    path.push(w);
                } else {
                    break;
                }
            }
            path.join(".")
        })
        .collect();
    for c in &stub_contracts {
        assert!(covered.contains(c), "no e2e stub case for {c}");
    }
}
