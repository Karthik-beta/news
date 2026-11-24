//! `news save` and `news render` with a temporary work directory and no database. Rendering through a real Plate
//! is tested in news-proof (tests/render.rs); here, `render` only reaches its input checks.

use super::{call_with, news};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../news-proof/tests/fixtures")
        .join(name)
}

fn work(tag: &str, with_brief: bool) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("news-cli-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("briefs")).unwrap();
    if with_brief {
        std::fs::copy(fixture("brief.json"), dir.join("briefs/s1.json")).unwrap();
    }
    dir
}

fn run(dir: &Path, args: &[&str]) -> (Value, i32, String) {
    call_with(news().env("NEWS_WORK_DIR", dir).args(args))
}

#[test]
fn save_stores_a_clean_copy_once() {
    let dir = work("save", true);
    let copy = fixture("copy.json");
    let (env, code, stderr) = run(&dir, &["save", "s1", copy.to_str().unwrap()]);
    assert_eq!(code, 0, "{env}");
    assert!(stderr.is_empty(), "{stderr}");
    let id = env["data"]["draft_id"].as_str().unwrap().to_owned();
    assert!(id.starts_with("s1-"), "{id}");
    assert_eq!(env["warnings"], serde_json::json!([]));
    let stored: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("pages/s1/copy.json")).unwrap())
            .unwrap();
    assert_eq!(stored["source"], "save");
    assert_eq!(stored["flags"], serde_json::json!([]));
    // The same Copy again: the same draft, the first save time kept.
    let (again, _, _) = run(&dir, &["save", "s1", copy.to_str().unwrap()]);
    assert_eq!(again["data"]["draft_id"], id.as_str());
    let restored: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("pages/s1/copy.json")).unwrap())
            .unwrap();
    assert_eq!(restored["saved_at"], stored["saved_at"]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn save_refuses_typed_numbers_unknown_refs_and_bad_stories() {
    let dir = work("refuse", true);
    let (env, code, _) = run(
        &dir,
        &["save", "s1", fixture("copy-digits.json").to_str().unwrap()],
    );
    assert_eq!(code, 6, "{env}");
    assert_eq!(env["error"]["code"], "validation_failed");
    let rules: Vec<&str> = env["error"]["details"]["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["rule"].as_str().unwrap())
        .collect();
    assert!(
        rules.contains(&"numeric_literal") && rules.contains(&"refs"),
        "{rules:?}"
    );
    assert!(!dir.join("pages/s1/copy.json").exists());

    let (env, code, _) = run(
        &dir,
        &["save", "../s1", fixture("copy.json").to_str().unwrap()],
    );
    assert_eq!(
        (code, env["error"]["code"].as_str()),
        (2, Some("usage")),
        "{env}"
    );
    let (env, code, _) = run(&dir, &["save", "s1", "no-such-copy.json"]);
    assert_eq!(
        (code, env["error"]["code"].as_str()),
        (3, Some("not_found")),
        "{env}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn save_without_a_brief_warns_that_refs_were_not_checked() {
    let dir = work("nobrief", false);
    let (env, code, _) = run(
        &dir,
        &["save", "s1", fixture("copy.json").to_str().unwrap()],
    );
    assert_eq!(code, 0, "{env}");
    assert!(
        env["warnings"][0].as_str().unwrap().contains("no brief"),
        "{env}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn render_needs_a_stored_copy_and_the_page_folder() {
    let dir = work("render", true);
    let (env, code, _) = run(&dir, &["render", "s1"]);
    assert_eq!(
        (code, env["error"]["code"].as_str()),
        (3, Some("not_found")),
        "{env}"
    );
    assert!(
        env["error"]["hint"]
            .as_str()
            .unwrap()
            .contains("news save s1"),
        "{env}"
    );
    run(
        &dir,
        &["save", "s1", fixture("copy.json").to_str().unwrap()],
    );
    // Fixer writes data.json, sources.json and page.json beside the brief; without them there is nothing to render.
    let (env, code, _) = run(&dir, &["render", "s1"]);
    assert_eq!(
        (code, env["error"]["code"].as_str()),
        (3, Some("not_found")),
        "{env}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn render_validates_the_stored_copy_again() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let bun = std::process::Command::new("bun").arg("--version").output();
    if !bun.is_ok_and(|o| o.status.success())
        || !repo.join("node_modules/@openuidev/react-lang").is_dir()
    {
        eprintln!("skipped: Plate needs Bun and `bun install` at the repository root");
        return;
    }
    let dir = work("tampered", true);
    let (env, code, _) = run(
        &dir,
        &["save", "s1", fixture("copy.json").to_str().unwrap()],
    );
    assert_eq!(code, 0, "{env}");
    for f in ["data.json", "sources.json", "page.json"] {
        std::fs::copy(fixture(f), dir.join("pages/s1").join(f)).unwrap();
    }
    // The saved file is edited by hand after `news save` accepted it.
    let path = dir.join("pages/s1/copy.json");
    let mut stored: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    stored["copy"]["headline"] = "Annual inflation in America fell to 2.4% in {pd_2026_10}".into();
    std::fs::write(&path, stored.to_string()).unwrap();
    let (env, code, _) = run(&dir, &["render", "s1"]);
    assert_eq!(
        (code, env["error"]["code"].as_str()),
        (6, Some("validation_failed")),
        "{env}"
    );
    assert_eq!(env["error"]["details"]["errors"][0]["where"], "headline");
    assert!(!dir.join("pages/s1/index.html").exists());
    std::fs::remove_dir_all(dir).unwrap();
}
