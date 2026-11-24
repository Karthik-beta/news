//! A story's page folder rendered into a bundle: the bundle files against their contracts, and, when Bun and the
//! site's packages are installed, real renders through Plate: twice for the same bundle SHA, the templated page,
//! and the parse and props validators. Without Bun (the Rust CI job) the Plate tests say so and pass.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use chrono::{DateTime, Utc};
use news_core::brief::Brief;
use news_proof::plate::Plate;
use news_proof::render::{self as bundle, Rendered};
use news_proof::validate::validate;
use news_proof::work::{self, TEMPLATED, Work};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const STORY: &str = "s1";

fn fixture(name: &str) -> Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn saved_at() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-11-14T09:00:00Z")
        .unwrap()
        .to_utc()
}

/// A work directory with the fixture brief, data, sources and page facts, and `copy` stored.
fn page_folder(tag: &str, copy: Value, flags: Vec<String>) -> (PathBuf, Work) {
    let dir = std::env::temp_dir().join(format!("news-proof-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let work = Work::new(&dir);
    let page = work.page(STORY);
    std::fs::create_dir_all(&page).unwrap();
    std::fs::create_dir_all(dir.join("briefs")).unwrap();
    std::fs::write(work.brief(STORY), fixture("brief.json").to_string()).unwrap();
    for f in ["data.json", "sources.json", "page.json"] {
        std::fs::write(page.join(f), fixture(f).to_string()).unwrap();
    }
    work::save(&work, STORY, copy, flags, "save", None, saved_at()).unwrap();
    (dir, work)
}

struct RepoContracts;

impl jsonschema::Retrieve for RepoContracts {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let rel = uri
            .as_str()
            .strip_prefix("https://news.karthik.nz/contracts/")
            .ok_or("not a News contract")?;
        Ok(serde_json::from_str(&std::fs::read_to_string(
            repo().join("contracts").join(rel),
        )?)?)
    }
}

fn assert_contract(rel: &str, instance: &Value) {
    let validator = jsonschema::draft202012::options()
        .with_retriever(RepoContracts)
        .build(&json!({"$ref": format!("https://news.karthik.nz/contracts/{rel}")}))
        .unwrap();
    let errors: Vec<String> = validator
        .iter_errors(instance)
        .map(|e| format!("{e} at {}", e.instance_path()))
        .collect();
    assert!(errors.is_empty(), "{rel}: {errors:?}");
}

fn assert_bundle_contracts(b: &bundle::Bundle) {
    assert_contract("bundle/meta.schema.json", &b.meta);
    assert_contract("bundle/manifest.schema.json", &b.manifest);
    for (file, rel) in [
        ("data.json", "bundle/data.schema.json"),
        ("sources.json", "bundle/sources.schema.json"),
    ] {
        assert_contract(rel, &serde_json::from_str(&b.files[file]).unwrap());
    }
}

#[test]
fn an_assembled_bundle_meets_the_bundle_contracts() {
    let (dir, work) = page_folder("contracts", fixture("copy.json"), vec![]);
    let inputs = bundle::load_inputs(&work, STORY).unwrap();
    let rendered: Rendered = serde_json::from_value(json!({
        "rendererVersion": "1.1.0",
        "files": {"index.html": "<!doctype html>", "index.md": "# x\n", "charts/cpi.csv": "a\r\n"},
        "charts": ["cpi"],
        "refsUsed": ["dp_7kq2", "dv_3ab8", "pd_2026_10"]
    }))
    .unwrap();
    let b = bundle::assemble(&inputs, &rendered).unwrap();
    assert_bundle_contracts(&b);
    std::fs::remove_dir_all(dir).unwrap();
}

/// A Plate for this test, or None (with a note) when Bun or the site's packages are missing.
async fn plate() -> Option<Plate> {
    let bun = std::process::Command::new("bun").arg("--version").output();
    if !bun.is_ok_and(|o| o.status.success())
        || !repo().join("node_modules/@openuidev/react-lang").is_dir()
    {
        eprintln!("skipped: Plate needs Bun and `bun install` at the repository root");
        return None;
    }
    Some(Plate::connect().await.expect("Plate starts"))
}

#[tokio::test]
async fn plate_renders_the_same_inputs_to_the_same_bundle_sha_and_checks_copies() {
    let Some(plate) = plate().await else { return };
    let (dir, work) = page_folder("plate", fixture("copy.json"), vec![]);
    let inputs = bundle::load_inputs(&work, STORY).unwrap();

    // Idempotent: the same .oui, data.json and renderer version give the same bundle SHA.
    let first = plate.render(&inputs.render_request()).await.unwrap();
    let a = bundle::assemble(&inputs, &first).unwrap();
    let b = bundle::assemble(
        &inputs,
        &plate.render(&inputs.render_request()).await.unwrap(),
    )
    .unwrap();
    assert_eq!(a.sha256, b.sha256);
    assert_eq!(a.files, b.files);
    assert!(first.problems.is_empty(), "{:?}", first.problems);
    assert_eq!(a.meta["renderer_version"], json!(first.renderer_version));
    let html = &a.files["index.html"];
    assert_eq!(html.matches("<meta name=\"robots\"").count(), 1);
    assert!(html.contains("<meta name=\"robots\" content=\"noindex\">"));
    assert_bundle_contracts(&a);
    // A different .oui gives a different bundle.
    let mut changed = inputs.clone();
    changed.copy.oui = changed.copy.oui.replace("Inflation", "Annual inflation");
    let c = bundle::assemble(
        &changed,
        &plate.render(&changed.render_request()).await.unwrap(),
    )
    .unwrap();
    assert_ne!(c.sha256, a.sha256);

    // The validators that need the library: a clean Copy passes, a wrong prop and a bad parse do not.
    let brief: Brief = serde_json::from_value(fixture("brief.json")).unwrap();
    let clean = fixture("copy.json");
    let errors = plate.check(clean["oui"].as_str().unwrap()).await.unwrap();
    assert_eq!(validate(&clean, Some(&brief), Some(&errors)), []);
    let wrong = "root = Page([figs])\nfigs = KeyFigures([a, b])\na = KeyFigure(\"Inflation\", \"two\")\nb = KeyFigure(\"Change\", \"dv_3ab8\")";
    let errors = plate.check(wrong).await.unwrap();
    let mut copy = clean.clone();
    copy["oui"] = json!(wrong);
    let rules: Vec<String> = validate(&copy, Some(&brief), Some(&errors))
        .into_iter()
        .map(|e| e.rule)
        .collect();
    assert!(rules.contains(&"props".to_string()), "{rules:?}");
    let errors = plate
        .check("root = Page([x])\nx = Nope(\"a\")")
        .await
        .unwrap();
    assert_eq!(errors.errors[0].rule, "openui_parse");
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn plate_renders_a_templated_page_for_a_failing_tier_2_item() {
    let Some(plate) = plate().await else { return };
    let brief: Brief = serde_json::from_value(fixture("brief.json")).unwrap();
    assert_eq!(brief.item.tier, 2);
    // Reporter's Copy fails the validators twice; code builds the templated page instead (D10).
    assert!(!validate(&fixture("copy-digits.json"), Some(&brief), None).is_empty());
    let copy = serde_json::to_value(news_proof::fallback::templated(&brief).unwrap()).unwrap();
    let (dir, work) = page_folder("templated", copy, vec![TEMPLATED.into()]);
    let mut inputs = bundle::load_inputs(&work, STORY).unwrap();
    inputs.facts.index_status = "index".into();
    let rendered = plate.render(&inputs.render_request()).await.unwrap();
    assert!(rendered.problems.is_empty(), "{:?}", rendered.problems);
    let b = bundle::assemble(&inputs, &rendered).unwrap();
    assert_eq!(b.meta["flags"], json!(["templated"]));
    assert_eq!(b.meta["index_status"], "noindex");
    assert!(b.files["index.html"].contains("<meta name=\"robots\" content=\"noindex\">"));
    assert!(b.files.contains_key("charts/chart.csv") && b.files.contains_key("charts/table.csv"));
    assert_bundle_contracts(&b);
    std::fs::remove_dir_all(dir).unwrap();
}
