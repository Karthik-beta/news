//! `news check` on a bundle directory, with no database, network or browser: C10 and C13 are switched off,
//! C14 is skipped, and quotes cannot be verified without stored text blocks (C6 fails closed).

use super::{call_with, news};
use std::path::{Path, PathBuf};

fn good() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../news-proof/tests/fixtures/good")
}

fn copy_good() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("news-check-e2e-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(dir.join("charts")).unwrap();
    for rel in [
        "page.oui",
        "data.json",
        "sources.json",
        "meta.json",
        "index.html",
        "index.md",
        "charts/sr-cpi.csv",
    ] {
        std::fs::copy(good().join(rel), dir.join(rel)).unwrap();
    }
    dir
}

fn check(dir: &Path) -> (serde_json::Value, i32) {
    let mut cmd = news();
    cmd.args(["check", dir.to_str().unwrap()])
        .env("NEWS_PROOF_LINKS", "off")
        .env("NEWS_PROOF_AXE", "off");
    let (env, code, _) = call_with(&mut cmd);
    (env, code)
}

#[test]
fn a_failing_page_is_validation_failed_with_the_markup_in_details() {
    let dir = copy_good();
    let (env, code) = check(&dir);
    assert_eq!(code, 6, "{env}");
    assert_eq!(env["error"]["code"], "validation_failed");
    let markup = &env["error"]["details"];
    assert_eq!(markup["pass"], false);
    let failures = markup["failures"].as_array().unwrap();
    assert_eq!(failures.len(), 1, "{markup}");
    assert_eq!(failures[0]["check"], "C6");
    let status = |id: &str| {
        markup["checks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["check"] == id)
            .unwrap()["status"]
            .clone()
    };
    assert_eq!(status("C10"), "skipped");
    assert_eq!(status("C13"), "skipped");
    assert_eq!(status("C14"), "skipped");
    assert_eq!(status("C4"), "pass");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_clean_page_passes_and_matches_the_contract() {
    let dir = copy_good();
    // No quote, so nothing needs a stored text block.
    let data = std::fs::read_to_string(dir.join("data.json")).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&data).unwrap();
    v["quotes"] = serde_json::json!({});
    std::fs::write(dir.join("data.json"), v.to_string()).unwrap();
    let oui = std::fs::read_to_string(dir.join("page.oui")).unwrap();
    std::fs::write(dir.join("page.oui"), oui.replace(": {q_shelter}", ".")).unwrap();
    let (env, code) = check(&dir);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["ok"], true);
    assert_eq!(env["data"]["pass"], true);
    assert_eq!(env["data"]["page_id"], "us-cpi-october-2026");
    assert_eq!(env["data"]["checks"].as_array().unwrap().len(), 14);
    assert!(env["warnings"][0].as_str().unwrap().contains("no database"));
    let mut contract: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            super::contract::contracts_dir().join("commands/check.schema.json"),
        )
        .unwrap(),
    )
    .unwrap();
    contract["$ref"] = serde_json::json!("#/$defs/output");
    let schema = jsonschema::draft202012::new(&contract).unwrap();
    assert!(schema.is_valid(&env["data"]), "{}", env["data"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_story_without_a_bundle_is_not_found_and_junk_is_a_usage_error() {
    let (env, code, _) = call_with(&mut {
        let mut c = news();
        c.args(["check", "81234"]).env(
            "NEWS_WORK_DIR",
            std::env::temp_dir().join("news-no-such-work"),
        );
        c
    });
    assert_eq!(code, 3, "{env}");
    assert_eq!(env["error"]["code"], "not_found");
    let (env, code, _) = call_with(news().args(["check", "not-a-story"]));
    assert_eq!(code, 2, "{env}");
}
