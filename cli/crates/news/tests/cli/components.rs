//! `news components` and `news component <type>` against a fixture index (the real one is generated
//! by the site build, #4).

use super::{call_with, news};
use serde_json::json;
use std::path::PathBuf;

pub fn fixture() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/component-index.json")
        .to_string_lossy()
        .into_owned()
}

fn with_index(args: &[&str]) -> (serde_json::Value, i32, String) {
    call_with(news().args(args).env("NEWS_COMPONENT_INDEX", fixture()))
}

#[test]
fn components_returns_one_line_per_component_sorted_by_type() {
    let (env, code, _) = with_index(&["components"]);
    assert_eq!(code, 0, "{env}");
    let items = env["data"]["items"].as_array().unwrap();
    assert_eq!(env["data"]["count"], 3);
    assert_eq!(items.len(), 3);
    assert_eq!(
        items[0],
        "DataTable: A table of data points with sources [rows[] of dp_*]"
    );
    assert!(items[1].as_str().unwrap().starts_with("KeyFigures: "));
    assert!(items[2].as_str().unwrap().starts_with("LineChart: "));
    for line in items {
        assert!(!line.as_str().unwrap().contains('\n'));
    }
    assert_eq!(
        env["data"]["text"].as_str().unwrap().lines().count(),
        3,
        "text has one line per component"
    );
    assert_eq!(env["truncated"], false);
}

#[test]
fn component_returns_props_data_shape_and_example() {
    let (env, code, _) = with_index(&["component", "KeyFigures"]);
    assert_eq!(code, 0, "{env}");
    let d = &env["data"];
    assert_eq!(d["type"], "KeyFigures");
    assert_eq!(d["props"]["figures"]["minItems"], 2);
    assert_eq!(
        d["data_shape"],
        "figures[] of {ref: dp_*|dv_*, period: pd_*}"
    );
    assert!(d["example"].as_str().unwrap().starts_with("KeyFigures("));
}

#[test]
fn unknown_component_is_not_found_with_suggestions() {
    let (env, code, _) = with_index(&["component", "keyfigures"]);
    assert_eq!(code, 3);
    assert_eq!(env["error"]["code"], "not_found");
    assert!(
        env["error"]["hint"]
            .as_str()
            .unwrap()
            .contains("KeyFigures")
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
    let (env, _, _) = with_index(&["component", "DataTable", "--fields", "type,example"]);
    assert_eq!(
        env["data"],
        json!({"type": "DataTable", "example": "DataTable(rows=[dp_7kq2, dp_9mz1])"})
    );
}
