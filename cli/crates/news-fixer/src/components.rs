//! The component index (TD "Component index and details"), read by `news components`,
//! `news component <name>` and the prefix builder.
//!
//! It is `component-index.json`, written by the site build (#4, `buildComponentIndex` in
//! `site/src/components/library.ts`): `NEWS_COMPONENT_INDEX` when set, else
//! `site/dist/component-index.json` in the nearest ancestor of the working directory that has one.
//! Shape: `contracts/component-index.schema.json`,
//!
//! ```json
//! {"version": 1, "budget": {"tokens": 4000, "perEntry": 24}, "tokens": 87,
//!  "components": [{"name": "Page", "line": "Page — root; …", "tokens": 18,
//!                  "description"?: "…", "props"?: {…}, "dataShape"?: "…", "example"?: "…"}]}
//! ```
//!
//! `line` is the one-line summary. The detail fields are optional until the site build emits them.

use news_core::{NewsError, NewsResult};
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const INDEX_VAR: &str = "NEWS_COMPONENT_INDEX";
pub const INDEX_REL: &str = "site/dist/component-index.json";
/// The only index version this build reads.
pub const INDEX_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    version: u32,
    #[allow(dead_code)]
    budget: Budget,
    #[allow(dead_code)]
    tokens: u64,
    components: Vec<Component>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Budget {
    #[allow(dead_code)]
    tokens: u64,
    #[allow(dead_code)]
    per_entry: u64,
}

/// One component's entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Component {
    pub name: String,
    pub line: String,
    #[allow(dead_code)]
    tokens: u64,
    pub description: Option<String>,
    pub props: Option<Value>,
    pub data_shape: Option<String>,
    pub example: Option<String>,
}

/// Where the index is: the variable, else the nearest ancestor's site build.
pub fn index_path() -> NewsResult<PathBuf> {
    if let Some(p) = std::env::var_os(INDEX_VAR).filter(|p| !p.is_empty()) {
        return Ok(PathBuf::from(p));
    }
    let cwd = std::env::current_dir().map_err(|e| NewsError::internal(e.to_string()))?;
    cwd.ancestors()
        .map(|d| d.join(INDEX_REL))
        .find(|p| p.is_file())
        .ok_or_else(|| {
            NewsError::precondition("component-index.json not found").with_hint(format!(
                "build the site (`bun run build` in site/) or set {INDEX_VAR}"
            ))
        })
}

pub fn load(path: &Path) -> NewsResult<Vec<Component>> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        NewsError::precondition(format!("cannot read the component index: {e}"))
            .with_hint(format!("set {INDEX_VAR} or build the site"))
    })?;
    parse(&text)
}

/// The index's components, sorted by name.
pub fn parse(text: &str) -> NewsResult<Vec<Component>> {
    let index: Index = serde_json::from_str(text).map_err(|e| {
        NewsError::validation(format!(
            "component index does not match contracts/component-index.schema.json: {e}"
        ))
    })?;
    if index.version != INDEX_VERSION {
        return Err(NewsError::validation(format!(
            "component index version {} is not supported (expected {INDEX_VERSION})",
            index.version
        )));
    }
    let mut out = index.components;
    for c in &out {
        if c.name.is_empty() || c.line.trim().is_empty() || c.line.contains('\n') {
            return Err(NewsError::validation(format!(
                "component {:?} needs a name and a one-line `line`",
                c.name
            )));
        }
    }
    // Deterministic: sorted by name (the build already writes them in name order).
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    pub(crate) fn index(components: Value) -> String {
        json!({"version": 1, "budget": {"tokens": 4000, "perEntry": 24}, "tokens": 1, "components": components})
            .to_string()
    }

    #[test]
    fn reads_the_site_builds_shape_sorted_by_name() {
        let out = parse(&index(json!([
            {"name": "Page", "line": "Page — root — children[]", "tokens": 9},
            {"name": "Chart", "line": "Chart — a chart — sr_", "tokens": 8}
        ])))
        .unwrap();
        assert_eq!(
            out.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["Chart", "Page"]
        );
    }

    #[test]
    fn rejects_other_shapes_and_versions() {
        for bad in [
            r#"{"components": []}"#.to_owned(),
            r#"[{"name": "A", "line": "A"}]"#.to_owned(),
            index(json!([{"type": "A", "summary": "s"}])),
            index(json!([{"name": "A", "line": "A", "tokens": 1, "extra": 1}])),
            index(json!([{"name": "A", "line": "two\nlines", "tokens": 1}])),
            index(json!([])).replace("\"version\":1", "\"version\":2"),
        ] {
            let err = parse(&bad)
                .err()
                .unwrap_or_else(|| panic!("accepted {bad}"));
            assert_eq!(err.exit_code(), 6, "{bad}");
        }
    }
}
