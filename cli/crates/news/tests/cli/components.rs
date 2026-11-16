//! `news components` and `news component <name>` against fixture indexes in the site build's shape
//! (#4, `buildComponentIndex`), both validated against `contracts/component-index.schema.json`.
//!
//! * `component-index.json`: exactly what #4 emits today for its fixture library: `name`, `line`,
//!   `tokens` per component, no detail fields.
//! * `component-index.details.json`: the same, plus the optional `description`, `props`, `dataShape`
//!   and `example` on one component, as #4/#7 will emit them.

use super::contract::contracts_dir;
use super::{call_with, news};
use serde_json::{Value, json};
use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

pub fn fixture() -> String {
    fixture_path("component-index.json")
        .to_string_lossy()
        .into_owned()
}

fn with(index: &str, args: &[&str]) -> (Value, i32, String) {
    call_with(
        news()
            .args(args)
            .env("NEWS_COMPONENT_INDEX", fixture_path(index)),
    )
}

fn with_index(args: &[&str]) -> (Value, i32, String) {
    with("component-index.json", args)
}

#[test]
fn the_fixtures_match_the_component_index_contract() {
    let path = contracts_dir().join("../component-index.schema.json");
    let schema: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(
        schema["$id"],
        "https://news.karthik.nz/contracts/component-index.schema.json"
    );
    jsonschema::draft202012::meta::validate(&schema).unwrap();
    let v = jsonschema::draft202012::new(&schema).unwrap();
    for name in ["component-index.json", "component-index.details.json"] {
        let index: Value =
            serde_json::from_str(&std::fs::read_to_string(fixture_path(name)).unwrap()).unwrap();
        let errors: Vec<String> = v.iter_errors(&index).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{name}: {errors:?}");
    }
    // The old made-up shape is not a component index.
    assert!(!v.is_valid(&json!({"version": 1, "components": [{"type": "A", "summary": "s"}]})));
}

#[test]
fn components_returns_each_line_sorted_by_name() {
    let (env, code, _) = with_index(&["components"]);
    assert_eq!(code, 0, "{env}");
    let items = env["data"]["items"].as_array().unwrap();
    assert_eq!(env["data"]["count"], 4);
    assert_eq!(
        items[0],
        "FixtureFigure — fixture only: key figure — dp_/dv_ value, dv_ change"
    );
    assert!(
        items[1]
            .as_str()
            .unwrap()
            .starts_with("FixtureLineChart — ")
    );
    assert!(items[2].as_str().unwrap().starts_with("FixtureText — "));
    assert_eq!(
        items[3],
        "Page — root; blocks in reading order — children[]"
    );
    assert_eq!(
        env["data"]["text"].as_str().unwrap().lines().count(),
        4,
        "text has one line per component"
    );
    assert_eq!(env["truncated"], false);
}

#[test]
fn component_without_details_says_what_is_missing() {
    let (env, code, _) = with_index(&["component", "Page"]);
    assert_eq!(code, 0, "{env}");
    let d = &env["data"];
    assert_eq!(d["name"], "Page");
    assert_eq!(
        d["line"],
        "Page — root; blocks in reading order — children[]"
    );
    for k in ["description", "props", "data_shape", "example"] {
        assert!(d[k].is_null(), "{k}: {env}");
    }
    assert_eq!(
        d["missing"],
        json!(["description", "props", "data_shape", "example"])
    );
    let warning = env["warnings"][0].as_str().unwrap();
    assert!(
        warning.contains("no description, props, data_shape, example for Page"),
        "{warning}"
    );
}

#[test]
fn component_returns_props_data_shape_and_example_when_present() {
    let (env, code, _) = with(
        "component-index.details.json",
        &["component", "FixtureFigure"],
    );
    assert_eq!(code, 0, "{env}");
    let d = &env["data"];
    assert_eq!(d["missing"], json!([]));
    assert_eq!(env["warnings"], json!([]));
    assert_eq!(
        d["props"]["properties"]["value"]["pattern"],
        "^(dp|dv)_[a-z0-9]+(_[a-z0-9]+)*$"
    );
    assert!(d["data_shape"].as_str().unwrap().starts_with("value: dp_"));
    assert!(
        d["example"]
            .as_str()
            .unwrap()
            .starts_with("sales = FixtureFigure(")
    );
    assert!(
        d["description"]
            .as_str()
            .unwrap()
            .starts_with("Fixture-only")
    );
}

#[test]
fn unknown_component_is_not_found_with_suggestions() {
    let (env, code, _) = with_index(&["component", "fixturefigure"]);
    assert_eq!(code, 3);
    assert_eq!(env["error"]["code"], "not_found");
    assert!(
        env["error"]["hint"]
            .as_str()
            .unwrap()
            .contains("FixtureFigure")
    );
}

#[test]
fn an_index_in_another_shape_is_a_validation_error() {
    let bad = std::env::temp_dir().join(format!(
        "news-other-shape-index-{}.json",
        std::process::id()
    ));
    std::fs::write(&bad, r#"{"version": 1, "components": [{"type": "A"}]}"#).unwrap();
    let (env, code, _) = call_with(
        news()
            .args(["components"])
            .env("NEWS_COMPONENT_INDEX", &bad),
    );
    let _ = std::fs::remove_file(bad);
    assert_eq!(
        (code, env["error"]["code"].as_str()),
        (6, Some("validation_failed"))
    );
    assert!(
        env["error"]["message"]
            .as_str()
            .unwrap()
            .contains("component-index.schema.json")
    );
}

#[test]
fn missing_index_is_a_precondition_that_says_how_to_build_it() {
    let missing = std::env::temp_dir().join("news-no-such-index.json");
    let (env, code, _) = call_with(
        news()
            .args(["components"])
            .env("NEWS_COMPONENT_INDEX", &missing),
    );
    assert_eq!(code, 4);
    assert_eq!(env["error"]["code"], "precondition");
    assert!(
        env["error"]["hint"]
            .as_str()
            .unwrap()
            .contains("NEWS_COMPONENT_INDEX")
    );
}

#[test]
fn fields_and_limit_apply_to_the_index() {
    let (env, _, _) = with_index(&["components", "--limit", "2"]);
    assert_eq!(env["data"]["items"].as_array().unwrap().len(), 2);
    assert_eq!(env["truncated"], true);
    let (env, _, _) = with(
        "component-index.details.json",
        &["component", "FixtureText", "--fields", "name,line"],
    );
    assert_eq!(
        env["data"],
        json!({"name": "FixtureText", "line": "FixtureText — fixture only: paragraph — text with {ref} tokens"})
    );
}
