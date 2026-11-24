//! `news components` and `news component <name>` (TD "Component index and details"). The index and
//! where it is found are Fixer's (`news_fixer::components`), which also puts it in the cached prefix.

use super::Reply;
use news_core::{NewsError, NewsResult};
use news_fixer::components::{Component as Entry, index_path, load};
use serde_json::json;

pub fn list() -> NewsResult<Reply> {
    let components = load(&index_path()?)?;
    let lines: Vec<&str> = components.iter().map(|c| c.line.as_str()).collect();
    Ok(Reply::new(json!({
        "count": lines.len(),
        "items": lines,
        "text": lines.join("\n"),
    }))
    // The whole index is meant for the cached prefix; never cut it by default.
    .default_limit(news_core::shape::MAX_LIMIT))
}

pub fn show(name: &str) -> NewsResult<Reply> {
    let components = load(&index_path()?)?;
    let found = components.iter().find(|c| c.name == name).ok_or_else(|| {
        let close: Vec<&str> = components
            .iter()
            .map(|c| c.name.as_str())
            .filter(|n| {
                n.eq_ignore_ascii_case(name) || n.to_lowercase().contains(&name.to_lowercase())
            })
            .take(5)
            .collect();
        let e = NewsError::not_found(format!("no component {name:?}"));
        if close.is_empty() {
            e.with_hint("news components")
        } else {
            e.with_hint(format!("did you mean {}?", close.join(", ")))
        }
    })?;
    Ok(details(found))
}

/// One component's entry, with every detail field present (null when the index lacks it) and a
/// warning naming what is missing.
fn details(c: &Entry) -> Reply {
    let missing: Vec<&str> = [
        ("description", c.description.is_none()),
        ("props", c.props.is_none()),
        ("data_shape", c.data_shape.is_none()),
        ("example", c.example.is_none()),
    ]
    .into_iter()
    .filter_map(|(k, gone)| gone.then_some(k))
    .collect();
    let mut reply = Reply::new(json!({
        "name": c.name,
        "line": c.line,
        "description": c.description,
        "props": c.props,
        "data_shape": c.data_shape,
        "example": c.example,
        "missing": missing,
    }));
    if !missing.is_empty() {
        reply.warnings.push(format!(
            "component-index.json has no {} for {} yet (the site build adds them, #4/#7); use its line until then",
            missing.join(", "),
            c.name
        ));
    }
    reply
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use news_fixer::components::parse;

    #[test]
    fn details_name_what_is_missing() {
        let index = json!({"version": 1, "budget": {"tokens": 4000, "perEntry": 24}, "tokens": 1,
            "components": [{"name": "A", "line": "A — x — y", "tokens": 3, "example": "A()"}]});
        let out = parse(&index.to_string()).unwrap();
        let reply = details(&out[0]);
        assert_eq!(
            reply.data["missing"],
            json!(["description", "props", "data_shape"])
        );
        assert_eq!(reply.data["example"], "A()");
        assert_eq!(reply.warnings.len(), 1);
    }
}
