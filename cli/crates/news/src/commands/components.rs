//! `news components` and `news component <type>` (TD "Component index and details").
//!
//! They read `component-index.json`, generated from the component folders by the site build (#4):
//! `NEWS_COMPONENT_INDEX` when set, else `site/dist/component-index.json` in the nearest ancestor of the
//! working directory that has one. Accepted shape (see cli/README.md):
//!
//! ```json
//! {"version": 1, "components": [{"type": "KeyFigures", "summary": "…", "data_shape": "…",
//!                                "props": {…}, "example": "…"}]}
//! ```

use super::Reply;
use news_core::{NewsError, NewsResult};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub const INDEX_VAR: &str = "NEWS_COMPONENT_INDEX";
pub const INDEX_REL: &str = "site/dist/component-index.json";

fn index_path() -> NewsResult<PathBuf> {
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

fn load(path: &Path) -> NewsResult<Vec<Value>> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        NewsError::precondition(format!("cannot read the component index: {e}"))
            .with_hint(format!("set {INDEX_VAR} or build the site"))
    })?;
    let v: Value = serde_json::from_str(&text)
        .map_err(|e| NewsError::validation(format!("component index is not valid JSON: {e}")))?;
    let list = match v {
        Value::Array(a) => a,
        Value::Object(mut o) => match o.remove("components") {
            Some(Value::Array(a)) => a,
            _ => {
                return Err(NewsError::validation(
                    "component index has no \"components\" array",
                ));
            }
        },
        _ => {
            return Err(NewsError::validation(
                "component index must be an object or an array",
            ));
        }
    };
    let mut out = Vec::with_capacity(list.len());
    for (i, c) in list.into_iter().enumerate() {
        if type_of(&c).is_none() {
            return Err(NewsError::validation(format!(
                "component {i} has no \"type\""
            )));
        }
        out.push(c);
    }
    // Deterministic: sorted by type.
    out.sort_by(|a, b| type_of(a).cmp(&type_of(b)));
    Ok(out)
}

fn type_of(c: &Value) -> Option<&str> {
    c.get("type")
        .or_else(|| c.get("name"))
        .and_then(Value::as_str)
}

fn field<'a>(c: &'a Value, names: &[&str]) -> Option<&'a Value> {
    names.iter().find_map(|n| c.get(*n))
}

fn text_of(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

/// One line: `Type: summary [data shape]`.
fn line(c: &Value) -> String {
    let t = type_of(c).unwrap_or_default();
    let summary = text_of(field(c, &["summary", "purpose", "index", "description"]));
    let shape = text_of(field(c, &["data_shape", "dataShape"]));
    match (summary.is_empty(), shape.is_empty()) {
        (true, true) => t.to_owned(),
        (false, true) => format!("{t}: {summary}"),
        (true, false) => format!("{t} [{shape}]"),
        (false, false) => format!("{t}: {summary} [{shape}]"),
    }
}

pub fn list() -> NewsResult<Reply> {
    let components = load(&index_path()?)?;
    let lines: Vec<Value> = components.iter().map(|c| Value::String(line(c))).collect();
    let text = lines
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join("\n");
    Ok(
        Reply::new(json!({ "count": lines.len(), "items": lines, "text": text }))
            // The whole index is meant for the cached prefix; never cut it by default.
            .default_limit(news_core::shape::MAX_LIMIT),
    )
}

pub fn show(component_type: &str) -> NewsResult<Reply> {
    let components = load(&index_path()?)?;
    let found = components
        .iter()
        .find(|c| type_of(c) == Some(component_type))
        .ok_or_else(|| {
            let close: Vec<&str> = components
                .iter()
                .filter_map(type_of)
                .filter(|t| {
                    t.eq_ignore_ascii_case(component_type)
                        || t.to_lowercase().contains(&component_type.to_lowercase())
                })
                .take(5)
                .collect();
            let e = NewsError::not_found(format!("no component {component_type:?}"));
            if close.is_empty() {
                e.with_hint("news components")
            } else {
                e.with_hint(format!("did you mean {}?", close.join(", ")))
            }
        })?;
    Ok(Reply::new(json!({
        "type": type_of(found),
        "summary": field(found, &["summary", "purpose", "index", "description"]),
        "props": field(found, &["props"]).cloned().unwrap_or(json!({})),
        "data_shape": field(found, &["data_shape", "dataShape"]),
        "example": field(found, &["example"]),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_degrade_gracefully() {
        assert_eq!(line(&json!({"type": "A"})), "A");
        assert_eq!(line(&json!({"type": "A", "summary": "s"})), "A: s");
        assert_eq!(line(&json!({"type": "A", "data_shape": "d"})), "A [d]");
        assert_eq!(
            line(&json!({"name": "A", "purpose": "s", "dataShape": "d"})),
            "A: s [d]"
        );
    }
}
