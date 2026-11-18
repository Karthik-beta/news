//! Every file in `contracts/cli/` is a JSON Schema 2020-12 document that validates against the
//! 2020-12 meta-schema, and the binary serves exactly these files.

use super::call;
use super::contract::contracts_dir;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const META: &str = "https://json-schema.org/draft/2020-12/schema";
const BASE: &str = "https://news.karthik.nz/contracts/cli/";

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            files(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn all() -> Vec<(String, Value)> {
    let root = contracts_dir();
    let mut paths = Vec::new();
    files(&root, &mut paths);
    paths.sort();
    paths
        .into_iter()
        .map(|p| {
            let rel = p
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let text = std::fs::read_to_string(&p).unwrap();
            let v: Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{rel}: {e}"));
            (rel, v)
        })
        .collect()
}

fn schema_of(rel: &str) -> Value {
    all().into_iter().find(|(r, _)| r == rel).unwrap().1
}

#[test]
fn every_contract_validates_against_the_2020_12_meta_schema() {
    let contracts = all();
    assert!(contracts.len() > 60, "only {} contracts", contracts.len());
    for (rel, v) in &contracts {
        assert!(
            rel.ends_with(".schema.json"),
            "{rel}: not a .schema.json file"
        );
        assert_eq!(v["$schema"], META, "{rel}");
        assert_eq!(v["$id"], format!("{BASE}{rel}"), "{rel}");
        if let Err(e) = jsonschema::draft202012::meta::validate(v) {
            panic!("{rel} breaks the 2020-12 meta-schema: {e}");
        }
    }
}

#[test]
fn command_contracts_have_input_output_tier_and_status() {
    for (rel, v) in all().iter().filter(|(r, _)| r.starts_with("commands/")) {
        assert!(v["$defs"]["input"].is_object(), "{rel}: no $defs.input");
        assert!(v["$defs"]["output"].is_object(), "{rel}: no $defs.output");
        let tier = v["x-news-tier"].as_str().unwrap_or_default();
        assert!(["R", "W", "G", "H"].contains(&tier), "{rel}: tier {tier:?}");
        let status = v["x-news-status"].as_str().unwrap_or_default();
        assert!(
            status == "built" || status.starts_with("stub until #"),
            "{rel}: {status:?}"
        );
        for code in v["x-news-errors"].as_array().unwrap() {
            assert!(
                schema_of("envelope.schema.json")["x-news-exit-codes"][code.as_str().unwrap()]
                    .is_number(),
                "{rel}: {code}"
            );
        }
    }
}

#[test]
fn the_ai_contracts_compile_and_accept_the_design_examples() {
    let brief = jsonschema::draft202012::new(&schema_of("ai/brief.v1.schema.json")).unwrap();
    // The example brief from Bureau §3.8.
    let example = json!({
      "brief_version": 1,
      "item": {"story_id": 81234, "event_key": "macro:us.cpi:2026-10", "page_type": "macro-release", "tier": 1,
               "action": "new", "slug_hint": "us-cpi-october-2026", "data_as_of": "2026-11-13"},
      "entities": [{"id": "ent_us", "name": "United States", "kind": "country"}],
      "facts": [
        {"ref": "dp_7kq2", "label": "CPI-U, all items, 12-month change, NSA", "value": "2.4", "unit": "Percent", "period": "October 2026"},
        {"ref": "dv_3ab8", "label": "Change in the 12-month rate", "value": "-0.3", "unit": "pp", "derived": "pp_change"}
      ],
      "series": [{"ref": "sr_cpi_yoy_5y", "label": "CPI-U 12-month change, monthly, 2021–2026", "points": 60}],
      "periods": [{"ref": "pd_2026_10", "label": "October 2026"}],
      "context": [{"id": "cx_1", "text": "Lowest 12-month rate since March 2021", "rule": "series_min_since"}],
      "quotes": [{"id": "q_1", "doc": 5521, "block": 3, "text": "The index for shelter rose 0.2 percent in October"}],
      "source_text": {"untrusted": true, "points": ["…"]},
      "media": {"images": [], "video": null},
      "related_pages": [{"page_id": "2026/10/us-cpi-september-2026", "title": "…"}],
      "components": {"suggested": ["KeyFigures", "LineChart"]},
      "rules": {"disclaimer": "not-investment-advice", "fiscal_note": null}
    });
    let errs: Vec<String> = brief.iter_errors(&example).map(|e| e.to_string()).collect();
    assert!(errs.is_empty(), "{errs:?}");
    let mut bad = example.clone();
    bad["source_text"]["untrusted"] = json!(false);
    assert!(
        !brief.is_valid(&bad),
        "source text must be fenced as untrusted"
    );

    let draft = jsonschema::draft202012::new(&schema_of("ai/page-draft.v1.schema.json")).unwrap();
    let copy = json!({
        "headline": "Annual inflation in America fell to {dp_7kq2} in October",
        "fly_title": "American inflation",
        "summary": "Consumer prices rose more slowly in {pd_2026_10}; the 12-month rate fell by {dv_3ab8} to its lowest since {pd_2021_03}.",
        "oui": "KeyFigures(figures=[dp_7kq2])",
        "refs_used": ["dp_7kq2", "dv_3ab8", "pd_2026_10", "pd_2021_03", "sr_cpi_yoy_5y", "q_1", "cx_1"],
        "components_used": ["KeyFigures"],
        "angle": "change"
    });
    assert!(draft.is_valid(&copy));
    for (field, value) in [
        ("refs_used", json!(["2.4"])),
        ("refs_used", json!(["dp_7KQ2"])),
        ("refs_used", json!(["xx_1"])),
        ("angle", json!("opinion")),
        ("headline", json!("short")),
    ] {
        let mut c = copy.clone();
        c[field] = value.clone();
        assert!(!draft.is_valid(&c), "{field} = {value} should be rejected");
    }
    let mut extra = copy.clone();
    extra["numbers"] = json!([2.4]);
    assert!(!draft.is_valid(&extra), "no extra properties");

    let a5 = jsonschema::draft202012::new(&schema_of("ai/a5.condense.v1.schema.json")).unwrap();
    assert!(a5.is_valid(
        &json!({"points": [{"text": "Shelter costs slowed", "support": [3]}], "quotes": []})
    ));
    assert!(
        !a5.is_valid(
            &json!({"points": [{"text": "Rose 2 percent", "support": [3]}], "quotes": []})
        ),
        "digits in A5 text"
    );
}

#[test]
fn every_ai_contract_compiles() {
    for (rel, v) in all().iter().filter(|(r, _)| r.starts_with("ai/")) {
        jsonschema::draft202012::new(v).unwrap_or_else(|e| panic!("{rel}: {e}"));
    }
}

#[test]
fn news_schema_serves_the_files_verbatim() {
    for (rel, v) in all().iter().filter(|(r, _)| r.starts_with("commands/")) {
        let words: Vec<&str> = rel
            .trim_start_matches("commands/")
            .trim_end_matches(".schema.json")
            .split('.')
            .collect();
        let mut args = vec!["schema"];
        args.extend(&words);
        let (env, code, _) = call(&args);
        assert_eq!(code, 0, "{rel}: {env}");
        assert_eq!(&env["data"]["schema"], v, "{rel}");
    }
    let (env, _, _) = call(&["schema", "--limit", "500"]);
    assert_eq!(
        env["data"]["count"],
        all()
            .iter()
            .filter(|(r, _)| r.starts_with("commands/"))
            .count()
    );
    assert_eq!(env["data"]["envelope"], schema_of("envelope.schema.json"));
}

#[test]
fn news_ai_schema_serves_each_step() {
    for (step, rel) in [
        ("A1", "ai/page-draft.v1.schema.json"),
        ("a2", "ai/page-draft.v1.schema.json"),
        ("A3", "ai/a3.classify.v1.schema.json"),
        ("A4", "ai/a4.storymatch.v1.schema.json"),
        ("A5", "ai/a5.condense.v1.schema.json"),
        ("A6", "ai/a6.entity-pick.v1.schema.json"),
        ("A7", "ai/a7.spotcheck.v1.schema.json"),
        ("A9", "ai/a9.triage.v1.schema.json"),
        ("A12", "ai/a12.judge.v1.schema.json"),
        ("brief", "ai/brief.v1.schema.json"),
    ] {
        let (env, code, _) = call(&["ai", "schema", step]);
        assert_eq!(code, 0, "{step}");
        assert_eq!(env["data"]["schema"], schema_of(rel), "{step}");
    }
}

#[test]
fn the_ai_schema_input_contract_accepts_what_the_cli_accepts() {
    let mut schema = schema_of("commands/ai.schema.schema.json");
    schema["$ref"] = json!("#/$defs/input");
    let input = jsonschema::draft202012::new(&schema).unwrap();
    // The CLI matches step names in any letter case.
    for step in ["A1", "a2", "A12", "a12", "brief", "BRIEF", "Brief"] {
        assert!(input.is_valid(&json!({ "step": step })), "{step}");
        let (env, code, _) = call(&["ai", "schema", step]);
        assert_eq!(code, 0, "{step}: {env}");
    }
    for step in ["A8", "A99", "a", "brief2", " A1", "A1 ", ""] {
        assert!(!input.is_valid(&json!({ "step": step })), "{step:?}");
        let (_, code, _) = call(&["ai", "schema", step]);
        assert_ne!(code, 0, "{step:?}");
    }
    // The response names the step in its canonical form.
    let (env, _, _) = call(&["ai", "schema", "a2"]);
    assert_eq!(env["data"]["step"], "A2");
}

#[test]
fn the_reference_pattern_is_the_one_from_the_design() {
    let draft = schema_of("ai/page-draft.v1.schema.json");
    assert_eq!(
        draft["properties"]["refs_used"]["items"]["pattern"],
        "^(dp|dv|sr|pd|q|cx)_[a-z0-9]+(_[a-z0-9]+)*$"
    );
}
