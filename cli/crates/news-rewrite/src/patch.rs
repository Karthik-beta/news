//! Re-rendering by code (Bureau §3.11, step 3): the stored Copy and `data.json` with the new vintages'
//! refs and values. Pure JSON and text edits. No number is typed by a model or by this code: values are
//! the Ledger's own text, and derived values are the Ledger's arithmetic ([`crate::classify::recompute`]).

use crate::find::PageChange;
use serde_json::{Value, json};

/// The statement name of the note this module adds.
const NOTE_NAME: &str = "restated";

fn is_id_byte(b: u8) -> bool {
    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'
}

/// `text` with every whole-token occurrence of the ref `old` replaced by `new` (`dp_a` is not found
/// inside `dp_ab` or `xdp_a`).
pub fn replace_ref(text: &str, old: &str, new: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(found) = text[at..].find(old) {
        let (start, end) = (at + found, at + found + old.len());
        let whole = (start == 0 || !is_id_byte(bytes[start - 1]))
            && (end >= bytes.len() || !is_id_byte(bytes[end]));
        out.push_str(&text[at..start]);
        out.push_str(if whole { new } else { old });
        at = end;
    }
    out.push_str(&text[at..]);
    out
}

/// Rename refs in every string of `v` (text, refs lists, series points, derived inputs), not in keys.
pub fn rename_in(v: &mut Value, renames: &[(String, String)]) {
    match v {
        Value::String(s) => {
            for (old, new) in renames {
                if s.contains(old.as_str()) {
                    *s = replace_ref(s, old, new);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|i| rename_in(i, renames)),
        Value::Object(map) => map.values_mut().for_each(|i| rename_in(i, renames)),
        _ => {}
    }
}

/// `data.json` with the page's new vintages: each moved point is re-keyed to the new `dp_` id with the
/// new value, text and locator, and says what it supersedes; each moved derived point is re-keyed with
/// its recomputed value. Everything else (labels, units, periods, sources) is the old entry's.
pub fn patch_data(data: &mut Value, change: &PageChange) {
    let renames = change.renames();
    if let Some(points) = data.get_mut("points").and_then(Value::as_object_mut) {
        for m in &change.points {
            if let Some(mut entry) = points.remove(&m.old_id) {
                entry["value"] = json!(m.new_numeric);
                entry["value_text"] = json!(m.new_text);
                if m.new_locator.get("kind").is_some() {
                    entry["locator"] = m.new_locator.clone();
                }
                points.insert(m.new_id.clone(), entry);
            }
        }
    }
    if let Some(derived) = data.get_mut("derived").and_then(Value::as_object_mut) {
        for d in &change.derived {
            if let (Some(mut entry), Some(value)) = (derived.remove(&d.old_id), &d.new_value) {
                entry["value"] = json!(value);
                entry["inputs"] = json!(d.new_inputs);
                derived.insert(d.new_id.clone(), entry);
            }
        }
    }
    // Series points and anything else that names an old id.
    for (key, value) in data.as_object_mut().into_iter().flatten() {
        if key != "points" && key != "derived" {
            rename_in(value, &renames);
        }
    }
    if let Some(points) = data.get_mut("points").and_then(Value::as_object_mut) {
        for m in &change.points {
            if let Some(entry) = points.get_mut(&m.new_id) {
                entry["supersedes"] = json!(m.old_id);
            }
        }
    }
}

/// Add the "restated" note to a Copy's OpenUI source: a `Note` statement, listed last in the page's
/// root. The text has no digits (every number on a page is a ref). Returns `Ok(None)` when the page
/// already has the note or already has two notes (the design allows two), and `Err` when the root is
/// not the one-line `root = Page([…])` form.
pub fn add_restated_note(oui: &str) -> Result<Option<String>, String> {
    if oui
        .lines()
        .any(|l| l.trim_start().starts_with(&format!("{NOTE_NAME} =")))
        || oui.matches("= Note(").count() >= 2
    {
        return Ok(None);
    }
    let mut lines: Vec<String> = oui.lines().map(str::to_owned).collect();
    let root = lines
        .iter()
        .position(|l| l.starts_with("root = Page(["))
        .ok_or("the Copy has no one-line `root = Page([…])`")?;
    let line = lines[root].trim_end();
    let inner = line
        .strip_suffix("])")
        .ok_or("the Copy's root is not one line")?;
    let sep = if inner.ends_with('[') { "" } else { ", " };
    lines[root] = format!("{inner}{sep}{NOTE_NAME}])");
    lines.push(format!(
        "{NOTE_NAME} = Note(\"Restated figures\", [\"The source has restated figures for an earlier period. This page shows the restated values.\"])"
    ));
    let mut out = lines.join("\n");
    if oui.ends_with('\n') {
        out.push('\n');
    }
    Ok(Some(out))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::find::{DvMove, PointMove};

    fn change() -> PageChange {
        PageChange {
            page_id: "us-cpi".into(),
            version: 2,
            story_id: Some(7),
            points: vec![PointMove {
                old_id: "dp_old".into(),
                new_id: "dp_new".into(),
                old_text: "2.7".into(),
                new_text: "2.6".into(),
                new_numeric: Some("2.6".into()),
                new_locator: json!({"kind": "json", "path": "$.v2"}),
                restated: false,
            }],
            derived: vec![DvMove {
                old_id: "dv_old".into(),
                new_id: "dv_new".into(),
                formula: "pp_change(a,b)".into(),
                old_value: "-0.3".into(),
                new_value: Some("-0.4".into()),
                new_inputs: vec!["dp_new".into(), "dp_keep".into()],
            }],
        }
    }

    #[test]
    fn refs_are_replaced_as_whole_tokens() {
        assert_eq!(
            replace_ref(
                "{dp_a} and dp_ab and xdp_a and dp_a_2 {dp_a}.",
                "dp_a",
                "dp_z"
            ),
            "{dp_z} and dp_ab and xdp_a and dp_a_2 {dp_z}."
        );
    }

    #[test]
    fn data_is_rekeyed_with_the_new_value_and_what_it_supersedes() {
        let mut data = json!({
            "version": 1,
            "points": {
                "dp_old": {"label": "CPI", "value": "2.7", "value_text": "2.7", "unit": "Percent",
                           "period": "pd_1", "source": "src_a", "locator": {"kind": "json", "path": "$.v1"}},
                "dp_keep": {"label": "CPI", "value": "2.4", "value_text": "2.4", "unit": "Percent",
                            "period": "pd_0", "source": "src_a", "locator": {"kind": "json"}}
            },
            "derived": {"dv_old": {"label": "Change", "value": "-0.3", "unit": "pp", "inputs": ["dp_old", "dp_keep"], "rounding": 1}},
            "series": {"sr_a": {"points": [{"period": "pd_1", "point": "dp_old"}]}}
        });
        patch_data(&mut data, &change());
        let p = &data["points"]["dp_new"];
        assert_eq!(
            (
                p["value"].as_str(),
                p["value_text"].as_str(),
                p["label"].as_str()
            ),
            (Some("2.6"), Some("2.6"), Some("CPI"))
        );
        assert_eq!(p["supersedes"], "dp_old");
        assert_eq!(p["locator"]["path"], "$.v2");
        assert!(data["points"].get("dp_old").is_none());
        assert_eq!(data["points"]["dp_keep"]["value"], "2.4");
        assert_eq!(data["derived"]["dv_new"]["value"], "-0.4");
        assert_eq!(
            data["derived"]["dv_new"]["inputs"],
            json!(["dp_new", "dp_keep"])
        );
        assert_eq!(data["series"]["sr_a"]["points"][0]["point"], "dp_new");
        // Running it again changes nothing.
        let once = data.clone();
        patch_data(&mut data, &change());
        assert_eq!(data, once);
    }

    #[test]
    fn the_copy_follows_the_new_ids_in_text_and_refs_lists() {
        let mut copy = json!({
            "headline": "Inflation {dp_old}",
            "oui": "root = Page([a])\na = KeyFigure(\"CPI\", \"dp_old\")\nb = KeyFigure(\"Change\", \"dv_old\")",
            "refs_used": ["dp_old", "dv_old"]
        });
        rename_in(&mut copy, &change().renames());
        assert_eq!(copy["headline"], "Inflation {dp_new}");
        assert!(copy["oui"].as_str().unwrap().contains("\"dv_new\""));
        assert_eq!(copy["refs_used"], json!(["dp_new", "dv_new"]));
    }

    #[test]
    fn the_restated_note_is_added_once_and_holds_no_digit() {
        let oui = "root = Page([a, b])\na = Prose([\"x\"])\nb = Prose([\"y\"])\n";
        let out = add_restated_note(oui).unwrap().unwrap();
        assert!(out.starts_with("root = Page([a, b, restated])\n"));
        assert!(out.contains("restated = Note(\"Restated figures\""));
        assert!(out.ends_with('\n'));
        let note = out.lines().last().unwrap();
        assert!(!note.chars().any(|c| c.is_ascii_digit()), "{note}");
        // Again: nothing to add. Two notes already: nothing to add either.
        assert_eq!(add_restated_note(&out).unwrap(), None);
        let two = "root = Page([a])\nn1 = Note(\"A\", [\"x\"])\nn2 = Note(\"B\", [\"y\"])";
        assert_eq!(add_restated_note(two).unwrap(), None);
        assert!(add_restated_note("a = Prose([\"x\"])").is_err());
        assert_eq!(
            add_restated_note("root = Page([])")
                .unwrap()
                .unwrap()
                .lines()
                .next(),
            Some("root = Page([restated])")
        );
    }
}
