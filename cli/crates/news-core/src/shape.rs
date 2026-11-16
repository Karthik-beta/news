//! Size discipline (Bureau Design §6.2): `--limit`, `--fields` and `--cursor`, applied the same way to
//! every command.
//!
//! A command that returns a list puts it under `data.items`, in a deterministic order. Shaping then
//! skips `cursor` items, keeps `limit` of them, sets `truncated` and `data.next_cursor` when more remain,
//! and, with `--fields`, keeps only the named keys of each item (or of `data` itself when there is no
//! list). Cursors are opaque to callers: pass back exactly what `next_cursor` said.

use crate::error::NewsError;
use serde_json::{Map, Value};

/// Default number of list items: keeps output near the ~2k-token budget.
pub const DEFAULT_LIMIT: usize = 20;
/// Hard ceiling on `--limit`.
pub const MAX_LIMIT: usize = 500;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Shape {
    pub limit: Option<usize>,
    pub fields: Option<Vec<String>>,
    pub cursor: Option<String>,
}

/// The shaped `data`, plus whether anything was cut and any warnings about the request.
#[derive(Debug, Clone, PartialEq)]
pub struct Shaped {
    pub data: Value,
    pub truncated: bool,
    pub warnings: Vec<String>,
}

impl Shape {
    /// Parse the raw global options. `fields` is a comma-separated list.
    pub fn parse(
        limit: Option<usize>,
        fields: Option<&str>,
        cursor: Option<&str>,
    ) -> Result<Self, NewsError> {
        if let Some(l) = limit
            && (l == 0 || l > MAX_LIMIT)
        {
            return Err(NewsError::usage(format!(
                "--limit must be between 1 and {MAX_LIMIT}"
            )));
        }
        let fields = match fields {
            None => None,
            Some(raw) => {
                let list: Vec<String> = raw
                    .split(',')
                    .map(str::trim)
                    .filter(|f| !f.is_empty())
                    .map(str::to_owned)
                    .collect();
                if list.is_empty() {
                    return Err(NewsError::usage("--fields needs at least one field name"));
                }
                Some(list)
            }
        };
        if let Some(c) = cursor {
            decode_cursor(c)?;
        }
        Ok(Self {
            limit,
            fields,
            cursor: cursor.map(str::to_owned),
        })
    }

    /// The offset a cursor encodes (0 without one).
    pub fn offset(&self) -> usize {
        self.cursor
            .as_deref()
            .and_then(|c| decode_cursor(c).ok())
            .unwrap_or(0)
    }

    /// Apply the shape to a command's `data` with the default limit.
    pub fn apply(&self, data: Value) -> Shaped {
        self.apply_with(data, DEFAULT_LIMIT)
    }

    /// Apply the shape, with `default_limit` items when no `--limit` was given.
    pub fn apply_with(&self, mut data: Value, default_limit: usize) -> Shaped {
        let mut warnings = Vec::new();
        let mut truncated = false;
        let limit = self.limit.unwrap_or(default_limit);

        if let Some(Value::Array(items)) = data.get_mut("items") {
            let offset = self.offset().min(items.len());
            let rest: Vec<Value> = items.drain(offset..).collect();
            let more = rest.len() > limit;
            let mut kept: Vec<Value> = rest.into_iter().take(limit).collect();
            if let Some(fields) = &self.fields {
                warn_unknown(&kept, fields, &mut warnings);
                for item in &mut kept {
                    project(item, fields);
                }
            }
            *items = kept;
            if more {
                truncated = true;
                if let Some(obj) = data.as_object_mut() {
                    obj.insert(
                        "next_cursor".into(),
                        Value::String(encode_cursor(offset + limit)),
                    );
                }
            }
        } else if let Some(fields) = &self.fields {
            warn_unknown(std::slice::from_ref(&data), fields, &mut warnings);
            project(&mut data, fields);
        }
        Shaped {
            data,
            truncated,
            warnings,
        }
    }
}

fn project(value: &mut Value, fields: &[String]) {
    if let Value::Object(obj) = value {
        let kept: Map<String, Value> = fields
            .iter()
            .filter_map(|f| obj.get(f).map(|v| (f.clone(), v.clone())))
            .collect();
        *obj = kept;
    }
}

fn warn_unknown(items: &[Value], fields: &[String], warnings: &mut Vec<String>) {
    let Some(Value::Object(first)) = items.first() else {
        return;
    };
    for f in fields {
        if !first.contains_key(f) {
            warnings.push(format!("unknown field {f:?} ignored"));
        }
    }
}

/// `c<offset>`: short, opaque to callers, stable across releases.
pub fn encode_cursor(offset: usize) -> String {
    format!("c{offset}")
}

pub fn decode_cursor(cursor: &str) -> Result<usize, NewsError> {
    cursor
        .strip_prefix('c')
        .and_then(|n| n.parse::<usize>().ok())
        .ok_or_else(|| {
            NewsError::usage(format!("invalid --cursor {cursor:?}"))
                .with_hint("pass back the next_cursor value of the previous call")
        })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn items(n: usize) -> Value {
        json!({"items": (0..n).map(|i| json!({"id": i, "name": format!("n{i}"), "x": true})).collect::<Vec<_>>()})
    }

    #[test]
    fn default_limit_cuts_long_lists_and_sets_a_cursor() {
        let s = Shape::default().apply(items(25));
        assert!(s.truncated);
        assert_eq!(s.data["items"].as_array().unwrap().len(), DEFAULT_LIMIT);
        assert_eq!(s.data["next_cursor"], "c20");
    }

    #[test]
    fn cursor_continues_where_the_last_page_stopped() {
        let shape = Shape::parse(Some(10), None, Some("c20")).unwrap();
        let s = shape.apply(items(25));
        assert!(!s.truncated);
        let ids: Vec<_> = s.data["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].as_u64().unwrap())
            .collect();
        assert_eq!(ids, (20..25).collect::<Vec<u64>>());
        assert!(s.data.get("next_cursor").is_none());
    }

    #[test]
    fn short_lists_are_not_truncated() {
        let s = Shape::default().apply(items(3));
        assert!(!s.truncated);
        assert!(s.data.get("next_cursor").is_none());
    }

    #[test]
    fn fields_project_items_and_warn_about_unknown_names() {
        let shape = Shape::parse(None, Some("id, nope"), None).unwrap();
        let s = shape.apply(items(2));
        assert_eq!(s.data["items"][0], json!({"id": 0}));
        assert_eq!(s.warnings, vec!["unknown field \"nope\" ignored"]);
    }

    #[test]
    fn fields_project_plain_objects() {
        let shape = Shape::parse(None, Some("a"), None).unwrap();
        let s = shape.apply(json!({"a": 1, "b": 2}));
        assert_eq!(s.data, json!({"a": 1}));
    }

    #[test]
    fn bad_options_are_usage_errors() {
        assert!(Shape::parse(Some(0), None, None).is_err());
        assert!(Shape::parse(Some(MAX_LIMIT + 1), None, None).is_err());
        assert!(Shape::parse(None, Some(" , "), None).is_err());
        assert!(Shape::parse(None, None, Some("20")).is_err());
        assert!(Shape::parse(None, None, Some("c-1")).is_err());
    }

    #[test]
    fn cursor_round_trips() {
        for n in [0, 1, 20, 12345] {
            assert_eq!(decode_cursor(&encode_cursor(n)).unwrap(), n);
        }
    }
}
