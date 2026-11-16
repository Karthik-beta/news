//! What ADR 0007 retired: start tickets, the prompt hook, the `news work` command and every CLI form
//! of an H-tier action. H-tier actions are Board's, sent over the Hotline; the terminal reads status
//! and runs the R, W and G tiers (G forms answer `gated` until Chief, #10).

use super::call;
use super::contract::contracts_dir;
use serde_json::Value;

/// (argv, the word clap does not recognise).
const RETIRED: &[(&str, &str)] = &[
    // The ticket and hook commands.
    ("run intent run daily", "intent"),
    ("run hook-prompt", "hook-prompt"),
    ("run ticket edition", "ticket"),
    // The H-tier forms of Bureau Design §6.4.
    ("run start edition", "start"),
    ("run approve r_7f3c plan", "approve"),
    ("run changes r_7f3c plan --note again", "changes"),
    ("run reject", "reject"),
    ("run resume r_7f3c", "resume"),
    ("run pause r_7f3c", "pause"),
    ("run cancel r_7f3c", "cancel"),
    ("home rollback 2", "rollback"),
    ("jobs pause net --until +1h", "pause"),
    ("jobs unpause net", "unpause"),
    ("sources enable fred", "enable"),
    ("sources disable fred", "disable"),
    ("scores adopt v2", "adopt"),
    ("stories merge 1 2", "merge"),
    ("stories split 1 7 8", "split"),
    ("entities alias add Apple 1", "alias"),
    ("entities alias merge 2 1", "alias"),
    ("eval label g1", "label"),
    // The worker is hosted by the service.
    ("work --once", "work"),
];

#[test]
fn retired_commands_are_unknown_subcommands() {
    for (argv, word) in RETIRED {
        let args: Vec<&str> = argv.split(' ').collect();
        let (env, code, _) = call(&args);
        assert_eq!(code, 2, "{argv}: {env}");
        assert_eq!(env["error"]["code"], "usage", "{argv}");
        assert!(
            env["error"]["message"]
                .as_str()
                .unwrap()
                .contains(&format!("unrecognized subcommand '{word}'")),
            "{argv}: {env}"
        );
    }
}

#[test]
fn no_contract_is_h_tier_and_none_is_left_for_a_retired_command() {
    let dir = contracts_dir().join("commands");
    let mut names = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        let name = p
            .file_name()
            .unwrap()
            .to_string_lossy()
            .replace(".schema.json", "");
        let tier = v["x-news-tier"].as_str().unwrap();
        assert!(
            matches!(tier, "R" | "W" | "G"),
            "{name}: tier {tier:?}; H-tier actions have no CLI form"
        );
        names.push(name);
    }
    for (argv, _) in RETIRED {
        let words: Vec<&str> = argv
            .split(' ')
            .take_while(|w| !w.starts_with('-'))
            .collect();
        // `run start edition` names the contract `run.start`; the arguments after the command are not
        // part of the name, so try every prefix.
        for n in 1..=words.len() {
            let name = words[..n].join(".");
            assert!(!names.contains(&name), "{name}: a contract for {argv:?}");
        }
    }
}
