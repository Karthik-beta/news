//! The golden sets in `pipeline/eval/` (Bureau §9.1): loading, schema checks and stored outputs.
//!
//! A set is `pipeline/eval/<set>/set.json` (its manifest) and one case per file in `cases/`. Each case
//! is checked on load against `pipeline/eval/schemas/<set>.case.schema.json` and the set's own rules
//! (G1 briefs against `brief.v1`; G6 pages against their G1 brief). A case that breaks them is kept as
//! a problem, not dropped silently, so `news eval run` counts it as failed.

use news_core::{NewsError, NewsResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Every set Critic runs, in order.
pub const SETS: [&str; 4] = ["g1", "g2", "g5", "g6"];

/// The sets an Audit suite runs (the `suite` argument of the Audit preset).
pub fn sets_of(suite: &str) -> NewsResult<&'static [&'static str]> {
    Ok(match suite {
        "writer" => &["g1"],
        "classify" => &["g2"],
        "curator" => &["g5"],
        "spotcheck" => &["g6"],
        "all" => &SETS,
        "storymatch" => {
            return Err(NewsError::not_found(
                "suite storymatch has no golden set yet (G3 is labelled in Phase 4)",
            ));
        }
        other => {
            return Err(NewsError::usage(format!(
                "unknown suite {other:?}: writer, classify, curator, spotcheck or all"
            )));
        }
    })
}

/// A metric's bound: met when the value is at least `min` and at most `max`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bound {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

impl Bound {
    pub fn met(&self, v: f64) -> bool {
        self.min.is_none_or(|m| v >= m) && self.max.is_none_or(|m| v <= m)
    }
}

/// `set.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub set: String,
    pub name: String,
    pub suite: String,
    pub version: u32,
    pub step: String,
    pub target_cases: u32,
    pub targets: BTreeMap<String, Bound>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// A loaded set: the cases that passed their checks, and what was wrong with the others.
#[derive(Debug, Clone)]
pub struct Set {
    pub manifest: Manifest,
    pub cases: Vec<Value>,
    /// (case id, problem).
    pub problems: Vec<(String, String)>,
    /// G1 briefs by case id, for sets that refer to them (G6).
    pub briefs: BTreeMap<String, Value>,
}

/// One line of `outputs/<prompt_version>.jsonl` (`schemas/output.schema.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Output {
    pub case: String,
    pub output: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub calls: Vec<Call>,
}

/// One `ai_calls` row of a writer case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub step: String,
    pub validation: String,
    pub repair_round: u32,
    pub tokens: u64,
}

/// The repository root: the nearest directory at or above the working directory holding
/// `pipeline/eval/`.
pub fn find_root() -> NewsResult<PathBuf> {
    let here = std::env::current_dir().map_err(|e| NewsError::internal(e.to_string()))?;
    here.ancestors()
        .find(|d| d.join("pipeline/eval").is_dir())
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            NewsError::precondition("no pipeline/eval/ at or above the working directory")
                .with_hint("run news eval from inside the News repository")
        })
}

const SCHEMA_BASE: &str = "https://news.karthik.nz/pipeline/eval/schemas/";

/// Resolves `$ref`s between the eval schemas to their files.
struct EvalSchemas(PathBuf);

impl jsonschema::Retrieve for EvalSchemas {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let name = uri
            .as_str()
            .strip_prefix(SCHEMA_BASE)
            .filter(|n| !n.contains(['/', '\\']))
            .ok_or("not an eval schema")?;
        Ok(serde_json::from_str(&std::fs::read_to_string(
            self.0.join(name),
        )?)?)
    }
}

fn read_json(path: &Path) -> NewsResult<Value> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| NewsError::not_found(format!("{}: {e}", path.display())))?;
    serde_json::from_str(&text)
        .map_err(|e| NewsError::validation(format!("{}: {e}", path.display())))
}

fn validator(root: &Path, rel: &str) -> NewsResult<jsonschema::Validator> {
    let schema = read_json(&root.join(rel))?;
    jsonschema::draft202012::options()
        .with_retriever(EvalSchemas(root.join("pipeline/eval/schemas")))
        .build(&schema)
        .map_err(|e| NewsError::internal(format!("{rel} does not compile: {e}")))
}

fn errors(v: &jsonschema::Validator, value: &Value) -> Option<String> {
    let all: Vec<String> = v
        .iter_errors(value)
        .map(|e| format!("{e} at {}", e.instance_path()))
        .collect();
    (!all.is_empty()).then(|| all.join("; "))
}

/// Load one set and check every case.
pub fn load(root: &Path, set: &str) -> NewsResult<Set> {
    if !SETS.contains(&set) {
        return Err(NewsError::not_found(format!("no golden set {set:?}")));
    }
    let dir = root.join("pipeline/eval").join(set);
    let manifest_value = read_json(&dir.join("set.json"))?;
    if let Some(e) = errors(
        &validator(root, "pipeline/eval/schemas/set.schema.json")?,
        &manifest_value,
    ) {
        return Err(NewsError::validation(format!("{set}/set.json: {e}")));
    }
    let manifest: Manifest = serde_json::from_value(manifest_value)
        .map_err(|e| NewsError::validation(format!("{set}/set.json: {e}")))?;
    if manifest.set != set {
        return Err(NewsError::validation(format!(
            "{set}/set.json names set {:?}",
            manifest.set
        )));
    }
    let case_schema = validator(
        root,
        &format!("pipeline/eval/schemas/{set}.case.schema.json"),
    )?;
    let brief_schema = validator(root, "contracts/cli/ai/brief.v1.schema.json")?;
    let briefs = if set == "g6" {
        g1_briefs(root)?
    } else {
        BTreeMap::new()
    };

    let mut files: Vec<PathBuf> = std::fs::read_dir(dir.join("cases"))
        .map_err(|e| NewsError::not_found(format!("{set}/cases: {e}")))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    let mut cases = Vec::new();
    let mut problems = Vec::new();
    for path in files {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let problem = match read_json(&path) {
            Err(e) => Some(e.to_string()),
            Ok(case) => match check_case(set, &stem, &case, &case_schema, &brief_schema, &briefs) {
                Some(p) => Some(p),
                None => {
                    cases.push(case);
                    None
                }
            },
        };
        if let Some(p) = problem {
            problems.push((stem, p));
        }
    }
    Ok(Set {
        manifest,
        cases,
        problems,
        briefs,
    })
}

fn check_case(
    set: &str,
    stem: &str,
    case: &Value,
    schema: &jsonschema::Validator,
    brief_schema: &jsonschema::Validator,
    briefs: &BTreeMap<String, Value>,
) -> Option<String> {
    if let Some(e) = errors(schema, case) {
        return Some(e);
    }
    if case["id"] != stem {
        return Some(format!("id {} differs from the file name", case["id"]));
    }
    match set {
        "g1" => errors(brief_schema, &case["brief"]).map(|e| format!("brief: {e}")),
        "g6" => {
            let Some(brief) = case["brief_case"].as_str().and_then(|b| briefs.get(b)) else {
                return Some(format!(
                    "brief_case {} is not a G1 case",
                    case["brief_case"]
                ));
            };
            let page = case["page"].as_str().unwrap_or_default();
            if let Err(e) = render(page, brief) {
                return Some(e);
            }
            if let Some(w) = case["fault"]["where"].as_str()
                && !page.contains(w)
            {
                return Some("fault.where does not occur in page".into());
            }
            None
        }
        _ => None,
    }
}

/// G1's valid briefs by case id.
fn g1_briefs(root: &Path) -> NewsResult<BTreeMap<String, Value>> {
    Ok(load(root, "g1")?
        .cases
        .into_iter()
        .filter_map(|c| Some((c["id"].as_str()?.to_owned(), c["brief"].clone())))
        .collect())
}

/// Fill a G6 page's `{ref}` tokens from its brief: a fact becomes its recorded value and unit as the
/// brief has them, a period its label. Fails on a ref the brief lacks or a digit outside a token, so no
/// value is ever typed into a case.
pub fn render(page: &str, brief: &Value) -> Result<String, String> {
    let mut out = String::with_capacity(page.len());
    let mut rest = page;
    while let Some(open) = rest.find('{') {
        let before = &rest[..open];
        if before.chars().any(|c| c.is_ascii_digit()) {
            return Err(format!("a digit outside a ref token: {before:?}"));
        }
        out.push_str(before);
        let close = rest[open..]
            .find('}')
            .ok_or_else(|| "an unclosed {".to_owned())?;
        let r = &rest[open + 1..open + close];
        out.push_str(&resolve(r, brief).ok_or_else(|| format!("ref {r} is not in the brief"))?);
        rest = &rest[open + close + 1..];
    }
    if rest.chars().any(|c| c.is_ascii_digit()) {
        return Err(format!("a digit outside a ref token: {rest:?}"));
    }
    out.push_str(rest);
    Ok(out)
}

fn resolve(r: &str, brief: &Value) -> Option<String> {
    let find = |key: &str| {
        brief[key]
            .as_array()?
            .iter()
            .find(|x| x["ref"] == r)
            .cloned()
    };
    if let Some(f) = find("facts") {
        let value = f["value"].as_str()?;
        return Some(match f["unit"].as_str().filter(|u| !u.is_empty()) {
            Some(unit) => format!("{value} {unit}"),
            None => value.to_owned(),
        });
    }
    find("periods").and_then(|p| p["label"].as_str().map(str::to_owned))
}

/// `pipeline/eval/<set>/outputs/<prompt_version>.jsonl`; `usage` when the version could name a path.
pub fn outputs_path(root: &Path, set: &str, prompt_version: &str) -> NewsResult<PathBuf> {
    if prompt_version.is_empty()
        || !prompt_version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        || prompt_version.starts_with('.')
    {
        return Err(NewsError::usage(format!(
            "prompt version {prompt_version:?} must be letters, digits, '.', '-' or '_'"
        )));
    }
    Ok(root
        .join("pipeline/eval")
        .join(set)
        .join("outputs")
        .join(format!("{prompt_version}.jsonl")))
}

/// The stored outputs of `prompt_version` for a set, checked against `schemas/output.schema.json`.
pub fn outputs(root: &Path, set: &str, prompt_version: &str) -> NewsResult<Vec<Output>> {
    let path = outputs_path(root, set, prompt_version)?;
    let text = std::fs::read_to_string(&path).map_err(|e| {
        NewsError::not_found(format!("no stored outputs {}: {e}", path.display()))
            .with_hint("Audit stores each version's outputs there")
    })?;
    let schema = validator(root, "pipeline/eval/schemas/output.schema.json")?;
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(i, line)| {
            let at = || format!("{}:{}", path.display(), i + 1);
            let v: Value = serde_json::from_str(line)
                .map_err(|e| NewsError::validation(format!("{}: {e}", at())))?;
            if let Some(e) = errors(&schema, &v) {
                return Err(NewsError::validation(format!("{}: {e}", at())));
            }
            serde_json::from_value(v).map_err(|e| NewsError::validation(format!("{}: {e}", at())))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn repo() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
    }

    /// The golden sets in the repository: every case passes its checks and ids are unique.
    #[test]
    fn every_golden_case_validates() {
        for set in SETS {
            let s = load(&repo(), set).unwrap_or_else(|e| panic!("{set}: {e}"));
            assert!(s.problems.is_empty(), "{set}: {:?}", s.problems);
            assert!(!s.cases.is_empty(), "{set} has no cases");
            let mut ids: Vec<&str> = s.cases.iter().filter_map(|c| c["id"].as_str()).collect();
            ids.dedup();
            assert_eq!(ids.len(), s.cases.len(), "{set}: duplicate ids");
            // Every case names recorded files that exist (G5 lists several, joined by "; ").
            for c in &s.cases {
                let from = c["provenance"]["from"].as_str().unwrap_or_default();
                for f in from.split("; ") {
                    assert!(repo().join(f).is_file(), "{}: {f} does not exist", c["id"]);
                }
            }
        }
    }

    #[test]
    fn g6_faults_are_in_the_rendered_page() {
        let s = load(&repo(), "g6").unwrap_or_else(|e| panic!("{e}"));
        for c in &s.cases {
            let brief = &s.briefs[c["brief_case"].as_str().unwrap_or_default()];
            let page = render(c["page"].as_str().unwrap_or_default(), brief)
                .unwrap_or_else(|e| panic!("{}: {e}", c["id"]));
            if let Some(w) = c["fault"]["where"].as_str() {
                let w = render(w, brief).unwrap_or_else(|e| panic!("{}: {e}", c["id"]));
                assert!(page.contains(&w), "{}", c["id"]);
            }
        }
    }

    #[test]
    fn render_fills_refs_and_refuses_typed_digits() {
        let brief = json!({
            "facts": [{"ref": "dp_a", "value": "3.1", "unit": "percent"},
                      {"ref": "dp_b", "value": "7", "unit": ""}],
            "periods": [{"ref": "pd_x", "label": "2026-10-01"}]
        });
        assert_eq!(
            render("Rate {dp_a} in {pd_x}, count {dp_b}.", &brief).as_deref(),
            Ok("Rate 3.1 percent in 2026-10-01, count 7.")
        );
        assert!(render("Rate 3 {dp_a}", &brief).is_err());
        assert!(render("Rate {dp_a} then 4", &brief).is_err());
        assert!(render("Rate {dp_z}", &brief).is_err());
        assert!(render("Rate {dp_a", &brief).is_err());
    }

    #[test]
    fn suites_map_to_sets() {
        assert_eq!(sets_of("writer").ok(), Some(&["g1"][..]));
        assert_eq!(sets_of("all").ok().map(<[_]>::len), Some(4));
        assert!(sets_of("storymatch").is_err());
        assert!(sets_of("nope").is_err());
    }

    #[test]
    fn bounds() {
        let b = Bound {
            min: Some(0.9),
            max: None,
        };
        assert!(b.met(0.9) && !b.met(0.89));
        let b = Bound {
            min: None,
            max: Some(0.0),
        };
        assert!(b.met(0.0) && !b.met(1.0));
    }

    #[test]
    fn outputs_refuse_path_like_versions() {
        for v in ["", "../x", ".hidden", "a/b", "a\\b"] {
            assert!(outputs(&repo(), "g1", v).is_err(), "{v}");
        }
    }
}
