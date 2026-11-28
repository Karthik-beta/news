//! C6 (quotes verbatim), C8 (licence), C9 (required fields), C12 (size and structure), C14 (duplicate page).

use serde_json::Value;
use std::collections::BTreeSet;

use super::{Ctx, Finding, Run};
use crate::copy;
use crate::html;

/// The `(document, block)` a quote's locator points at: `{"kind": "text_block", "doc": 5521, "block": 3}`.
pub fn quote_block(q: &Value) -> Option<(i64, i64)> {
    Some((
        q["locator"]["doc"].as_i64()?,
        q["locator"]["block"].as_i64()?,
    ))
}

pub(super) fn c6(cx: &Ctx) -> Vec<Finding> {
    let mut out = Vec::new();
    for (id, q) in cx.b.data_map("quotes").into_iter().flatten() {
        let at = format!("data.json:quotes.{id}");
        let text = q["text"].as_str().unwrap_or_default();
        let Some(key) = quote_block(q) else {
            out.push(Finding::block(
                "C6",
                at,
                "the locator names no stored text block (needs `doc` and `block`)",
            ));
            continue;
        };
        match cx.env.text_blocks.get(&key) {
            None => out.push(Finding::block(
                "C6",
                at,
                format!(
                    "text block {}/{} is not stored, so the quote cannot be checked",
                    key.0, key.1
                ),
            )),
            Some(block) if !block.contains(text) => {
                out.push(Finding::block(
                    "C6",
                    at,
                    format!(
                        "the quote is not an exact substring of text block {}/{}",
                        key.0, key.1
                    ),
                ));
            }
            Some(_) => {}
        }
    }
    out
}

/// The sources the page's cited data comes from: each used reference, followed through series, derived
/// values and context lines to the data points and quotes underneath.
fn used_sources(cx: &Ctx) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut todo: Vec<String> = cx.refs.iter().map(|(_, r)| r.clone()).collect();
    let mut sources = BTreeSet::new();
    while let Some(r) = todo.pop() {
        if !seen.insert(r.clone()) {
            continue;
        }
        let kind = r.split_once('_').map_or("", |p| p.0);
        let map = match kind {
            "dp" => "points",
            "dv" => "derived",
            "sr" => "series",
            "q" => "quotes",
            "cx" => "context",
            _ => continue,
        };
        let Some(v) = cx.b.data[map].get(&r) else {
            continue;
        };
        if let Some(s) = v["source"].as_str() {
            sources.insert(s.to_owned());
        }
        for i in v["inputs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            todo.push(i.to_owned());
        }
        for p in v["points"].as_array().into_iter().flatten() {
            if let Some(d) = p["point"].as_str() {
                todo.push(d.to_owned());
            }
        }
    }
    sources
}

pub(super) fn c8(cx: &Ctx) -> Vec<Finding> {
    let mut out = Vec::new();
    let all = cx.b.sources["sources"].as_object();
    let used = used_sources(cx);
    for id in &used {
        let at = format!("sources.json:{id}");
        let Some(s) = all.and_then(|m| m.get(id)) else {
            continue;
        };
        for (k, v) in [
            ("licence.name", &s["licence"]["name"]),
            ("publisher", &s["publisher"]),
            ("title", &s["title"]),
            ("url", &s["url"]),
        ] {
            if v.as_str().is_none_or(|t| t.trim().is_empty()) {
                out.push(Finding::block(
                    "C8",
                    &at,
                    format!("no attribution: `{k}` is empty"),
                ));
            }
        }
        if !cx.b.html.is_empty() && !cx.b.html.contains(&format!("id=\"source-{id}\"")) {
            out.push(Finding::block(
                "C8",
                &at,
                "the page does not list this source (no attribution on the page)",
            ));
        }
    }
    // A source that does not allow redistribution may be cited but its data is not offered as a download.
    let restricted: BTreeSet<&str> = all
        .into_iter()
        .flatten()
        .filter(|(_, s)| s["licence"]["redistribution"] == false)
        .map(|(id, _)| id.as_str())
        .collect();
    if !restricted.is_empty() {
        let values: BTreeSet<&str> =
            cx.b.data_map("points")
                .into_iter()
                .flatten()
                .filter(|(_, p)| p["source"].as_str().is_some_and(|s| restricted.contains(s)))
                .filter_map(|(_, p)| p["value"].as_str())
                .collect();
        for (path, text) in &cx.b.downloads {
            // CSV fields and JSON strings alike: split on the separators and quotes of both.
            let hit = text
                .split(|c: char| {
                    matches!(c, ',' | '"' | ':' | '[' | ']' | '{' | '}') || c.is_whitespace()
                })
                .find(|f| values.contains(f));
            if let Some(v) = hit {
                out.push(Finding::block(
                    "C8",
                    path,
                    format!("offers {v}, a value of a source that does not allow redistribution"),
                ));
            }
        }
    }
    out
}

pub(super) fn c9(cx: &Ctx) -> Vec<Finding> {
    let b = cx.b;
    let mut out = Vec::new();
    for k in ["title", "summary"] {
        if b.meta_str(k).is_empty() {
            out.push(Finding::block("C9", format!("meta.json:{k}"), "is missing"));
        }
    }
    let cats: Vec<&str> = b.meta["categories"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if cats.is_empty() {
        out.push(Finding::block(
            "C9",
            "meta.json:categories",
            "needs at least one category",
        ));
    }
    if !b.html.is_empty() {
        let tags = html::tags(&b.html);
        if let Some(t) = tags.iter().find(|t| t.name == "title") {
            if html::inner_text(&b.html, t).is_empty() {
                out.push(Finding::block("C9", "index.html", "the <title> is empty"));
            }
        } else {
            out.push(Finding::block("C9", "index.html", "no <title>"));
        }
        for img in tags.iter().filter(|t| t.name == "img") {
            if img.attr("alt").is_none_or(|a| a.trim().is_empty()) {
                out.push(Finding::block(
                    "C9",
                    "index.html",
                    format!("image {} has no alt text", img.attr("src").unwrap_or("?")),
                ));
            }
        }
        let text = html::visible_text(&b.html);
        for d in &cx.lex.c9.disclaimer {
            let applies = cats.iter().any(|c| {
                d.categories
                    .iter()
                    .any(|x| c == x || c.starts_with(&format!("{x}/")))
            });
            if applies && !text.contains(&d.text) {
                out.push(Finding::block(
                    "C9",
                    "index.html",
                    format!(
                        "a page in {} needs the disclaimer {:?}",
                        cats.join(", "),
                        d.text
                    ),
                ));
            }
        }
    }
    out
}

pub(super) fn c12(cx: &Ctx) -> Vec<Finding> {
    let b = cx.b;
    let mut out = Vec::new();
    let max = cx.lex.c12.bundle_bytes_max;
    if b.total_bytes() > max {
        out.push(Finding::warn(
            "C12",
            "bundle",
            format!("{} bytes is over the budget of {max}", b.total_bytes()),
        ));
    }
    if let Some([lo, hi]) = b.meta["page_type"]
        .as_str()
        .and_then(|t| cx.lex.c12.components.get(t))
        .copied()
    {
        let n = copy::statements(&b.oui)
            .iter()
            .filter(|(_, n)| n != "root")
            .count();
        if n < lo || n > hi {
            out.push(Finding::warn(
                "C12",
                "page.oui",
                format!(
                    "{n} components, but a {} page has {lo} to {hi}",
                    b.meta_str("page_type")
                ),
            ));
        }
    }
    out
}

pub(super) fn c14(cx: &Ctx) -> Run {
    let Some(dups) = cx.env.duplicates else {
        return Err("the page index could not be read".into());
    };
    Ok(dups
        .iter()
        .map(|d| {
            Finding::block(
                "C14",
                "meta.json:path",
                format!(
                    "page {} already exists with the same {} ({}): publish this as an update of it",
                    d.page_id, d.reason, d.slug
                ),
            )
        })
        .collect())
}
