//! The page bundle `news render` builds (Technical Design, "Page bundle"; contracts/bundle/): page.oui, data.json,
//! sources.json, meta.json, index.html, index.md and charts/<id>.csv and .json, listed in manifest.json with their
//! SHA-256 and the bundle SHA. Plate renders the HTML, Markdown, CSV and chart JSON; everything else is assembled here,
//! from the stored Copy, the brief, the page facts and the page's data. Idempotent: the same inputs and renderer version give the
//! same bytes, so the same bundle SHA (no clock is read; times come from the inputs).

use crate::validate::Copy;
use crate::work::{Stored, pretty, sha256_hex};
use chrono::{DateTime, Datelike, SecondsFormat, Utc};
use news_core::brief::Brief;
use news_core::{NewsError, NewsResult};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

/// The write API's cap per file (contracts/bundle/manifest.schema.json).
pub const MAX_FILE_BYTES: usize = 1024 * 1024;

/// `page.json`: what meta.json needs that the brief does not carry. Written with the brief by Fixer (#18) for a new
/// page, and by Press or Rewrite for an update (version and first publication).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageFacts {
    /// Defaults to the brief's `slug_hint`.
    #[serde(default)]
    pub page_id: Option<String>,
    /// The first is the primary category.
    pub categories: Vec<String>,
    pub region: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "first_version")]
    pub version: u32,
    /// First publication; none for a new page, which then takes the Copy's `saved_at`.
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default = "en_gb")]
    pub lang: String,
    #[serde(default = "noindex")]
    pub index_status: String,
}

fn first_version() -> u32 {
    1
}
fn en_gb() -> String {
    "en-GB".into()
}
fn noindex() -> String {
    "noindex".into()
}

/// Everything a render reads.
#[derive(Debug, Clone)]
pub struct Inputs {
    pub stored: Stored,
    pub copy: Copy,
    pub brief: Brief,
    pub facts: PageFacts,
    pub data: Value,
    /// data.json exactly as written: the bundle carries the source's digits byte for byte, never re-serialised.
    pub data_text: String,
    pub sources: Value,
}

impl Inputs {
    /// The one robots tag's status: a templated page is never indexed.
    pub fn index_status(&self) -> &str {
        if self.stored.templated() {
            "noindex"
        } else {
            &self.facts.index_status
        }
    }

    /// The body of Plate's `/render`.
    pub fn render_request(&self) -> Value {
        json!({
            "oui": self.copy.oui,
            "data": self.data,
            "sources": self.sources,
            "head": {"flyTitle": self.copy.fly_title, "headline": self.copy.headline, "summary": self.copy.summary},
            "indexStatus": self.index_status(),
            "disclaimer": match self.brief.rules.disclaimer.as_deref() {
                Some("not-investment-advice") => Some("investment"),
                Some("not-medical-advice") => Some("medical"),
                _ => None,
            },
        })
    }
}

/// Read a story's render inputs from its page folder (work.rs).
pub fn load_inputs(work: &crate::work::Work, story: &str) -> NewsResult<Inputs> {
    use crate::work::{read_json, read_text};
    let stored = crate::work::load(work, story)?;
    let copy: Copy = serde_json::from_value(stored.copy.clone())
        .map_err(|e| NewsError::validation(format!("the stored Copy is not page-draft.v1: {e}")))?;
    let brief: Brief = serde_json::from_value(read_json(&work.brief(story))?).map_err(|e| {
        NewsError::validation(format!("the brief for {story} is not brief.v1: {e}"))
    })?;
    let page = work.page(story);
    let facts: PageFacts = serde_json::from_value(read_json(&page.join("page.json"))?)
        .map_err(|e| NewsError::validation(format!("page.json for {story}: {e}")))?;
    let data_path = page.join("data.json");
    let data_text = read_text(&data_path)?;
    let data = serde_json::from_str(&data_text).map_err(|e| {
        NewsError::validation(format!("{} is not valid JSON: {e}", data_path.display()))
    })?;
    Ok(Inputs {
        stored,
        copy,
        brief,
        facts,
        data,
        data_text,
        sources: read_json(&page.join("sources.json"))?,
    })
}

/// Plate's `/render` reply.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rendered {
    pub renderer_version: String,
    pub files: BTreeMap<String, String>,
    pub charts: Vec<String>,
    pub refs_used: Vec<String>,
    #[serde(default)]
    pub problems: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bundle {
    /// Bundle files by path, sorted.
    pub files: BTreeMap<String, String>,
    pub manifest: Value,
    pub sha256: String,
    pub meta: Value,
}

/// meta.json's page types; a brief may name the page-type skill instead (`macro-release`).
fn page_type(t: &str) -> Option<&'static str> {
    Some(match t {
        "earnings" => "earnings",
        "macro" | "macro-release" => "macro",
        "policy" | "policy-decision" => "policy",
        "report" | "major-report" => "report",
        "news" | "news-story" => "news",
        "science" => "science",
        "explainer" => "explainer",
        _ => return None,
    })
}

/// meta.json's entity kinds; agencies and central banks are institutions.
fn entity_kind(k: &str) -> Option<&'static str> {
    Some(match k {
        "company" => "company",
        "person" => "person",
        "country" => "country",
        "topic" => "topic",
        "source" => "source",
        "institution" | "agency" | "central-bank" | "central_bank" | "organisation"
        | "organization" => "institution",
        _ => return None,
    })
}

fn slug_ok(s: &str, max: usize) -> bool {
    !s.is_empty()
        && s.len() <= max
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !s.starts_with('-')
        && !s.ends_with('-')
}

fn utc(s: &str, what: &str) -> NewsResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|t| t.to_utc())
        .map_err(|e| NewsError::validation(format!("{what} {s:?} is not an RFC 3339 time: {e}")))
}

/// Assemble the bundle from the inputs and Plate's render.
pub fn assemble(inputs: &Inputs, rendered: &Rendered) -> NewsResult<Bundle> {
    let bad = |m: String| NewsError::validation(m);
    let page_id = inputs
        .facts
        .page_id
        .clone()
        .or_else(|| inputs.brief.item.slug_hint.clone())
        .ok_or_else(|| {
            bad("no page ID: page.json has no page_id and the brief no slug_hint".into())
        })?;
    if !slug_ok(&page_id, 120) {
        return Err(bad(format!(
            "page ID {page_id:?} is not lower-case letters, digits and hyphens"
        )));
    }
    let page_type = page_type(&inputs.brief.item.page_type).ok_or_else(|| {
        bad(format!(
            "unknown page type {:?}",
            inputs.brief.item.page_type
        ))
    })?;
    let entities = inputs
        .brief
        .entities
        .iter()
        .map(|e| {
            entity_kind(&e.kind)
                .map(|kind| json!({"id": e.id, "name": e.name, "kind": kind}))
                .ok_or_else(|| bad(format!("entity {} has an unknown kind {:?}", e.id, e.kind)))
        })
        .collect::<NewsResult<Vec<_>>>()?;
    if !["index", "noindex"].contains(&inputs.facts.index_status.as_str()) {
        return Err(bad(format!(
            "index_status {:?} is not index or noindex",
            inputs.facts.index_status
        )));
    }

    let saved = utc(&inputs.stored.saved_at, "saved_at")?;
    let published = match &inputs.facts.published_at {
        Some(p) => utc(p, "published_at")?,
        None => saved,
    };
    let updated = saved.max(published);
    let stamp = |t: DateTime<Utc>| t.to_rfc3339_opts(SecondsFormat::Secs, true);

    let mut files = BTreeMap::new();
    files.insert("page.oui".to_string(), inputs.copy.oui.clone());
    files.insert("data.json".to_string(), inputs.data_text.clone());
    files.insert("sources.json".to_string(), pretty(&inputs.sources));
    for name in ["index.html", "index.md"] {
        let text = rendered
            .files
            .get(name)
            .ok_or_else(|| NewsError::internal(format!("Plate returned no {name}")))?;
        files.insert(name.to_string(), text.clone());
    }
    for id in &rendered.charts {
        if !slug_ok(id, 64) {
            return Err(bad(format!(
                "chart statement {id:?} cannot name charts/{id}.csv: use lower-case letters and digits"
            )));
        }
        for name in [format!("charts/{id}.csv"), format!("charts/{id}.json")] {
            let text = rendered
                .files
                .get(&name)
                .ok_or_else(|| NewsError::internal(format!("Plate returned no {name}")))?;
            files.insert(name, text.clone());
        }
    }

    let data_ids: Vec<&String> = rendered
        .refs_used
        .iter()
        .filter(|r| r.starts_with("dp_") || r.starts_with("dv_"))
        .collect();
    let mut meta = json!({
        "schema": "meta.v1",
        "id": page_id,
        "path": format!("/{:04}/{:02}/{page_id}", published.year(), published.month()),
        "version": inputs.facts.version,
        "lang": inputs.facts.lang,
        // The Copy's own text with its {ref} tokens: meta.json holds no figures (contracts/bundle/meta.schema.json).
        "title": inputs.copy.headline,
        "summary": inputs.copy.summary,
        "page_type": page_type,
        "tier": inputs.brief.item.tier,
        "categories": inputs.facts.categories,
        "entities": entities,
        "tags": inputs.facts.tags,
        "region": inputs.facts.region,
        "published_at": stamp(published),
        "updated_at": stamp(updated),
        "index_status": inputs.index_status(),
        "renderer_version": rendered.renderer_version,
        "data_ids": data_ids,
        "chart_ids": rendered.charts,
    });
    if !inputs.copy.fly_title.is_empty() {
        meta["fly_title"] = json!(inputs.copy.fly_title);
    }
    if inputs.stored.templated() {
        meta["flags"] = json!([crate::work::TEMPLATED]);
    }
    files.insert("meta.json".to_string(), pretty(&meta));

    let mut listed = Vec::new();
    let mut lines = String::new();
    for (path, text) in &files {
        if text.len() > MAX_FILE_BYTES {
            return Err(bad(format!(
                "{path} is {} bytes; the limit is {MAX_FILE_BYTES}",
                text.len()
            )));
        }
        let sha = sha256_hex(text.as_bytes());
        lines.push_str(&format!("{path} {sha}\n"));
        listed.push(json!({"path": path, "sha256": sha, "bytes": text.len()}));
    }
    let sha256 = sha256_hex(lines.as_bytes());
    let manifest = json!({
        "schema": "manifest.v1",
        "page_id": page_id,
        "version": inputs.facts.version,
        "bundle_sha256": sha256,
        "files": listed,
    });
    Ok(Bundle {
        files,
        manifest,
        sha256,
        meta,
    })
}

/// Write the bundle into the page folder `dir` (where `news check` reads it), with manifest.json beside the files.
/// Charts from an earlier render are removed; the folder's other files (copy.json, page.json) stay.
pub fn write(bundle: &Bundle, dir: &Path) -> NewsResult<()> {
    let io =
        |e: std::io::Error| NewsError::internal(format!("cannot write {}: {e}", dir.display()));
    match std::fs::remove_dir_all(dir.join("charts")) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io(e)),
    }
    for (path, text) in &bundle.files {
        let to = dir.join(path);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        std::fs::write(to, text).map_err(io)?;
    }
    std::fs::write(dir.join("manifest.json"), pretty(&bundle.manifest)).map_err(io)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
pub(crate) mod tests {
    use super::*;
    use crate::validate::tests::{brief, clean};

    pub(crate) fn inputs() -> Inputs {
        let copy = clean();
        Inputs {
            stored: Stored {
                draft_id: "s1-abc".into(),
                story: "s1".into(),
                saved_at: "2026-11-20T10:00:00Z".into(),
                source: "save".into(),
                job: None,
                flags: vec![],
                copy: copy.clone(),
            },
            copy: serde_json::from_value(copy).unwrap(),
            brief: brief(),
            facts: serde_json::from_value(
                json!({"categories": ["economy/inflation"], "region": "north-america"}),
            )
            .unwrap(),
            data: json!({"version": 1, "points": {}}),
            data_text: "{\"version\": 1, \"points\": {}}
"
            .into(),
            sources: json!({"version": 1, "sources": {}}),
        }
    }

    pub(crate) fn rendered() -> Rendered {
        serde_json::from_value(json!({
            "rendererVersion": "1.1.0",
            "files": {"index.html": "<!doctype html>", "index.md": "# x\n", "charts/cpi.csv": "a\r\n", "charts/cpi.json": "{}"},
            "charts": ["cpi"],
            "refsUsed": ["dp_7kq2", "dv_3ab8", "pd_2026_10"],
            "sourcesUsed": [],
            "problems": []
        }))
        .unwrap()
    }

    #[test]
    fn the_same_inputs_give_the_same_bundle_sha() {
        let a = assemble(&inputs(), &rendered()).unwrap();
        let b = assemble(&inputs(), &rendered()).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.sha256.len(), 64);
        let mut other = inputs();
        other.copy.oui.push('\n');
        assert_ne!(assemble(&other, &rendered()).unwrap().sha256, a.sha256);
        let mut newer = rendered();
        newer.renderer_version = "1.2.0".into();
        assert_ne!(assemble(&inputs(), &newer).unwrap().sha256, a.sha256);
    }

    #[test]
    fn lists_the_contract_files_and_hashes_them_like_the_write_api() {
        let b = assemble(&inputs(), &rendered()).unwrap();
        let paths: Vec<&str> = b.files.keys().map(String::as_str).collect();
        assert_eq!(
            paths,
            [
                "charts/cpi.csv",
                "charts/cpi.json",
                "data.json",
                "index.html",
                "index.md",
                "meta.json",
                "page.oui",
                "sources.json"
            ]
        );
        let lines: String = b.manifest["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                format!(
                    "{} {}\n",
                    f["path"].as_str().unwrap(),
                    f["sha256"].as_str().unwrap()
                )
            })
            .collect();
        assert_eq!(sha256_hex(lines.as_bytes()), b.manifest["bundle_sha256"]);
        assert_eq!(b.manifest["files"][0]["bytes"], 3);
    }

    #[test]
    fn data_json_keeps_the_source_digits_byte_for_byte() {
        let text = r#"{"version": 1, "points": {"dp_x": {"value": "1.50", "n": 1.50, "big": 12345678901234567890, "e": 1e3}}}"#;
        let mut i = inputs();
        i.data = serde_json::from_str(text).unwrap();
        i.data_text = text.into();
        assert_eq!(assemble(&i, &rendered()).unwrap().files["data.json"], text);
    }

    #[test]
    fn writes_meta_from_the_inputs() {
        let m = assemble(&inputs(), &rendered()).unwrap().meta;
        assert_eq!(m["id"], "us-cpi-october-2026");
        assert_eq!(m["path"], "/2026/11/us-cpi-october-2026");
        assert_eq!(m["page_type"], "macro");
        assert_eq!(m["tier"], 2);
        assert_eq!(
            m["title"],
            "Annual inflation in America fell to {dp_7kq2} in {pd_2026_10}"
        );
        assert_eq!(m["published_at"], "2026-11-20T10:00:00Z");
        assert_eq!(m["index_status"], "noindex");
        assert_eq!(m["renderer_version"], "1.1.0");
        assert_eq!(m["data_ids"], json!(["dp_7kq2", "dv_3ab8"]));
        assert_eq!(m["entities"][0]["kind"], "country");
        assert!(m.get("flags").is_none());
    }

    #[test]
    fn a_templated_page_is_flagged_and_never_indexed() {
        let mut i = inputs();
        i.facts.index_status = "index".into();
        assert_eq!(i.index_status(), "index");
        i.stored.flags = vec!["templated".into()];
        assert_eq!(i.render_request()["indexStatus"], "noindex");
        let m = assemble(&i, &rendered()).unwrap().meta;
        assert_eq!(m["index_status"], "noindex");
        assert_eq!(m["flags"], json!(["templated"]));
    }

    #[test]
    fn the_brief_rule_names_the_disclaimer_plate_adds() {
        let mut i = inputs();
        for (rule, kind) in [
            (Some("not-investment-advice"), json!("investment")),
            (Some("not-medical-advice"), json!("medical")),
            (None, Value::Null),
        ] {
            i.brief.rules.disclaimer = rule.map(str::to_owned);
            assert_eq!(i.render_request()["disclaimer"], kind);
        }
    }

    #[test]
    fn an_update_keeps_its_first_publication() {
        let mut i = inputs();
        i.facts.published_at = Some("2026-10-15T08:30:00Z".into());
        i.facts.version = 2;
        let b = assemble(&i, &rendered()).unwrap();
        assert_eq!(b.meta["path"], "/2026/10/us-cpi-october-2026");
        assert_eq!(b.meta["updated_at"], "2026-11-20T10:00:00Z");
        assert_eq!(b.manifest["version"], 2);
    }

    #[test]
    fn refuses_what_the_write_api_would() {
        let mut r = rendered();
        r.charts = vec!["revenueChart".into()];
        assert!(assemble(&inputs(), &r).is_err());
        let mut i = inputs();
        i.brief.entities[0].kind = "planet".into();
        assert!(assemble(&i, &rendered()).is_err());
        let mut i = inputs();
        i.brief.item.page_type = "blog".into();
        assert!(assemble(&i, &rendered()).is_err());
    }

    #[test]
    fn writes_files_and_the_manifest() {
        let dir = std::env::temp_dir().join(format!("news-proof-bundle-{}", std::process::id()));
        let b = assemble(&inputs(), &rendered()).unwrap();
        write(&b, &dir).unwrap();
        write(&b, &dir).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("charts/cpi.csv")).unwrap(),
            "a\r\n"
        );
        let m: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(m["bundle_sha256"], b.sha256);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
