//! The call contract (Bureau §6.2): stdout carries exactly one JSON envelope, logs go to stderr, and
//! each error code maps to its exit code. Every envelope here is also validated against
//! contracts/cli/envelope.schema.json.

use super::{call, call_with, news};
use serde_json::{Value, json};
use std::path::PathBuf;

pub fn contracts_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../contracts/cli")
}

fn envelope_validator() -> jsonschema::Validator {
    let text = std::fs::read_to_string(contracts_dir().join("envelope.schema.json")).unwrap();
    let schema: Value = serde_json::from_str(&text).unwrap();
    jsonschema::draft202012::new(&schema).expect("envelope schema compiles")
}

/// Exit code per error code, as the contract states it.
const EXIT: &[(&str, i32)] = &[
    ("usage", 2),
    ("not_found", 3),
    ("conflict", 4),
    ("precondition", 4),
    ("gated", 7),
    ("validation_failed", 6),
    ("rate_limited", 5),
    ("upstream_unavailable", 5),
    ("licence_blocked", 4),
    ("budget_exceeded", 4),
    ("internal", 10),
];

fn exit_for(code: &str) -> i32 {
    EXIT.iter().find(|(c, _)| *c == code).map(|(_, e)| *e).unwrap_or_else(|| panic!("unknown code {code}"))
}

fn check(env: &Value, code: i32) {
    let v = envelope_validator();
    let errors: Vec<String> = v.iter_errors(env).map(|e| e.to_string()).collect();
    assert!(errors.is_empty(), "{env} breaks envelope.schema.json: {errors:?}");
    match env["ok"].as_bool() {
        Some(true) => assert_eq!(code, 0, "{env}"),
        Some(false) => {
            let c = env["error"]["code"].as_str().unwrap();
            assert_eq!(code, exit_for(c), "{env}");
            assert_eq!(env["error"]["retryable"], code == 5, "{env}");
        }
        None => panic!("no ok: {env}"),
    }
}

#[test]
fn the_contract_table_matches_the_published_exit_codes() {
    let text = std::fs::read_to_string(contracts_dir().join("envelope.schema.json")).unwrap();
    let schema: Value = serde_json::from_str(&text).unwrap();
    let published = &schema["x-news-exit-codes"];
    assert_eq!(published.as_object().unwrap().len(), EXIT.len());
    for (code, exit) in EXIT {
        assert_eq!(published[code], json!(exit), "{code}");
    }
}

#[test]
fn each_error_code_the_binary_can_reach_offline_exits_with_its_code() {
    let fixture = super::components::fixture();
    let bad_index = std::env::temp_dir().join(format!("news-bad-index-{}.json", std::process::id()));
    std::fs::write(&bad_index, "{not json").unwrap();
    let cases: Vec<(&str, Vec<&str>, Vec<(&str, String)>)> = vec![
        ("usage", vec!["frobnicate"], vec![]),
        ("usage", vec!["work", "--pools", "ai"], vec![]),
        ("usage", vec!["version", "--limit", "0"], vec![]),
        ("usage", vec!["version", "--cursor", "nope"], vec![]),
        ("not_found", vec!["schema", "no", "such", "command"], vec![]),
        ("not_found", vec!["ai", "schema", "A99"], vec![]),
        (
            "not_found",
            vec!["component", "NoSuchThing"],
            vec![("NEWS_COMPONENT_INDEX", fixture.clone())],
        ),
        ("precondition", vec!["discover"], vec![]),
        ("precondition", vec!["jobs", "list"], vec![]), // DATABASE_URL unset
        (
            "validation_failed",
            vec!["components"],
            vec![("NEWS_COMPONENT_INDEX", bad_index.to_string_lossy().into_owned())],
        ),
        (
            "upstream_unavailable",
            vec!["jobs", "list"],
            vec![("DATABASE_URL", "postgres://news:x@127.0.0.1:1/news".to_owned())],
        ),
        (
            "gated",
            vec!["jobs", "pause", "net", "--until", "+1h"],
            vec![("CLAUDECODE", "1".to_owned())],
        ),
    ];
    for (want, args, env) in cases {
        let mut cmd = news();
        cmd.args(&args);
        for (k, v) in &env {
            cmd.env(k, v);
        }
        let (got, code, _) = call_with(&mut cmd);
        assert_eq!(got["error"]["code"], want, "{args:?}: {got}");
        check(&got, code);
    }
    let _ = std::fs::remove_file(bad_index);
}

#[test]
fn success_and_failure_envelopes_both_match_the_schema() {
    for args in [
        vec!["version"],
        vec!["schema"],
        vec!["schema", "jobs", "claim"],
        vec!["ai", "schema", "A1"],
        vec!["discover"],
        vec!["nope"],
    ] {
        let (env, code, _) = call(&args);
        check(&env, code);
    }
}

#[test]
fn logs_go_to_stderr_and_never_to_stdout() {
    // Debug logging on: stderr gets log lines, stdout still carries exactly the one envelope.
    let mut cmd = news();
    cmd.args(["jobs", "list"])
        .env("NEWS_LOG", "debug")
        .env("DATABASE_URL", "postgres://news:x@127.0.0.1:1/news");
    let (env, code, stderr) = call_with(&mut cmd);
    check(&env, code);
    assert!(!stderr.is_empty(), "expected debug logs on stderr");
    assert!(!stderr.contains("\"ok\""), "envelope leaked to stderr: {stderr}");
}

#[test]
fn usage_errors_put_clap_text_on_stderr_and_the_envelope_on_stdout() {
    let (env, code, stderr) = call(&["jobs", "claim", "--pool", "net"]);
    check(&env, code);
    assert!(env["error"]["message"].as_str().unwrap().contains("--worker"), "{env}");
    assert!(stderr.contains("Usage: news jobs claim"), "{stderr}");
}

#[test]
fn shaping_options_are_accepted_by_every_command() {
    // Global options: valid anywhere, validated before the command runs.
    let (env, code, _) = call(&["schema", "--limit", "3"]);
    check(&env, code);
    assert_eq!(env["data"]["items"].as_array().unwrap().len(), 3);
    assert_eq!(env["truncated"], true);
    let next = env["data"]["next_cursor"].as_str().unwrap().to_owned();
    let (env2, _, _) = call(&["schema", "--limit", "3", "--cursor", &next]);
    assert_ne!(env["data"]["items"], env2["data"]["items"]);

    let (env, _, _) = call(&["version", "--fields", "news,nope"]);
    assert_eq!(env["data"], json!({"news": env!("CARGO_PKG_VERSION")}));
    assert_eq!(env["warnings"], json!(["unknown field \"nope\" ignored"]));
}

#[test]
fn output_is_deterministic() {
    for args in [vec!["schema"], vec!["schema", "jobs", "list"], vec!["ai", "schema", "brief"]] {
        assert_eq!(call(&args).0, call(&args).0, "{args:?}");
    }
}

#[test]
fn no_command_prints_a_secret_from_the_environment() {
    let secret = "pw-4f9c-never-print-me";
    let url = format!("postgres://news:{secret}@127.0.0.1:1/news");
    for args in [vec!["jobs", "list"], vec!["db", "status"], vec!["work", "--pools", "net", "--once"]] {
        let out = news().args(&args).env("DATABASE_URL", &url).output().unwrap();
        let all = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(!all.contains(secret), "{args:?}: {all}");
    }
}
