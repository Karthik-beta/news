//! C1 (parse and render), C2 (references resolve), C3 (values match source), C7 (period and unit labels).

use serde_json::Value;
use std::collections::BTreeSet;

use super::{Ctx, Finding};
use crate::copy;
use crate::html;
use news_core::refs::is_ref;
use news_ledger::decimal::{self, Dec};

const MAPS: [&str; 6] = [
    "points", "derived", "series", "periods", "quotes", "context",
];

/// The `data.json` map a reference lives in.
fn map_of(r: &str) -> Option<&'static str> {
    Some(match r.split_once('_')?.0 {
        "dp" => "points",
        "dv" => "derived",
        "sr" => "series",
        "pd" => "periods",
        "q" => "quotes",
        "cx" => "context",
        _ => return None,
    })
}

fn lookup<'a>(cx: &'a Ctx, r: &str) -> Option<&'a Value> {
    cx.b.data[map_of(r)?].get(r)
}

fn has_source(cx: &Ctx, id: &str) -> bool {
    cx.b.sources["sources"].get(id).is_some()
}

fn str_of<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// Used references once each, with the first place they appear.
fn unique_refs<'a>(cx: &'a Ctx) -> Vec<(&'a str, &'a str)> {
    let mut seen = BTreeSet::new();
    cx.refs
        .iter()
        .filter(|(_, r)| seen.insert(r.as_str()))
        .map(|(at, r)| (at.as_str(), r.as_str()))
        .collect()
}

pub(super) fn c1(cx: &Ctx) -> Vec<Finding> {
    let b = cx.b;
    let mut out: Vec<Finding> = b
        .problems
        .iter()
        .map(|(f, m)| Finding::block("C1", f, m))
        .collect();
    for p in copy::structure_problems(&b.oui) {
        out.push(Finding::block("C1", "page.oui", p));
    }
    if !b.data.is_null() {
        if b.data["version"] != 1 {
            out.push(Finding::block("C1", "data.json", "`version` must be 1"));
        }
        for m in MAPS {
            if !b.data[m].is_object() {
                out.push(Finding::block(
                    "C1",
                    "data.json",
                    format!("`{m}` is missing"),
                ));
            }
        }
    }
    if !b.sources.is_null() && !(b.sources["version"] == 1 && b.sources["sources"].is_object()) {
        out.push(Finding::block(
            "C1",
            "sources.json",
            "needs `version` 1 and a `sources` map",
        ));
    }
    if !b.meta.is_null() && b.meta["schema"] != "meta.v1" {
        out.push(Finding::block(
            "C1",
            "meta.json",
            "`schema` must be \"meta.v1\"",
        ));
    }
    if !b.html.is_empty() && !html::tags(&b.html).iter().any(|t| t.name == "article") {
        out.push(Finding::block(
            "C1",
            "index.html",
            "the renderer wrote no <article>",
        ));
    }
    if !b.md.is_empty() && !b.md.starts_with("---") {
        out.push(Finding::block("C1", "index.md", "no front matter"));
    }
    out
}

pub(super) fn c2(cx: &Ctx) -> Vec<Finding> {
    let mut out = Vec::new();
    for t in &cx.copy {
        if !t.is_prose() {
            continue;
        }
        let s = copy::split(&t.text);
        for brace in s.stray_braces {
            out.push(Finding::block(
                "C2",
                &t.at,
                format!("`{{{brace}}}` is not a reference token"),
            ));
        }
        let bare = s
            .prose
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .find(|w| is_ref(w));
        if let Some(w) = bare {
            out.push(Finding::block(
                "C2",
                &t.at,
                format!("reference `{w}` is written without braces, so it would show as text"),
            ));
        }
    }
    for (at, r) in unique_refs(cx) {
        let Some(v) = lookup(cx, r) else {
            out.push(Finding::block(
                "C2",
                at,
                format!("`{r}` is not in data.json"),
            ));
            continue;
        };
        let need_source = |id: Option<&str>, out: &mut Vec<Finding>| match id {
            Some(id) if has_source(cx, id) => {}
            other => out.push(Finding::block(
                "C2",
                at,
                format!("`{r}` cites source {other:?}, which is not in sources.json"),
            )),
        };
        match r.split_once('_').map(|p| p.0) {
            Some("dp") => {
                need_source(str_of(v, "source"), &mut out);
                if v["value"].is_null() {
                    out.push(Finding::block(
                        "C2",
                        at,
                        format!("`{r}` has no value (the source published none)"),
                    ));
                }
                if let Some(p) =
                    str_of(v, "period").filter(|p| cx.b.data["periods"].get(*p).is_none())
                {
                    out.push(Finding::block(
                        "C2",
                        at,
                        format!("`{r}` has period `{p}`, which is not in data.json"),
                    ));
                }
            }
            Some("dv") => {
                for i in v["inputs"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    match cx.b.data["points"].get(i) {
                        None => out.push(Finding::block(
                            "C2",
                            at,
                            format!("`{r}` is calculated from `{i}`, which is not in data.json"),
                        )),
                        Some(dp) => need_source(str_of(dp, "source"), &mut out),
                    }
                }
            }
            Some("sr") => {
                for p in v["points"].as_array().into_iter().flatten() {
                    if let Some(pd) =
                        str_of(p, "period").filter(|pd| cx.b.data["periods"].get(*pd).is_none())
                    {
                        out.push(Finding::block(
                            "C2",
                            at,
                            format!("`{r}` has period `{pd}`, which is not in data.json"),
                        ));
                    }
                    if let Some(dp) = str_of(p, "point") {
                        match cx.b.data["points"].get(dp) {
                            None => out.push(Finding::block(
                                "C2",
                                at,
                                format!("`{r}` has point `{dp}`, which is not in data.json"),
                            )),
                            Some(d) => need_source(str_of(d, "source"), &mut out),
                        }
                    }
                }
            }
            Some("q") => need_source(str_of(v, "source"), &mut out),
            Some("cx") => {
                for i in v["inputs"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    if lookup(cx, i).is_none() {
                        out.push(Finding::block(
                            "C2",
                            at,
                            format!("`{r}` is computed from `{i}`, which is not in data.json"),
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    // The renderer shows "no data" for a reference it could not fill.
    for t in html::tags(&cx.b.html)
        .iter()
        .filter(|t| t.has_class("source-ref--missing"))
    {
        out.push(Finding::block(
            "C2",
            "index.html",
            format!(
                "the page shows \"no data\" for `{}`",
                t.attr("data-ref").unwrap_or("?")
            ),
        ));
    }
    out
}

fn dec(v: &Value) -> Option<Dec> {
    Dec::from_numeric(v.as_str()?)
}

fn same(a: Dec, b: Dec) -> bool {
    a.checked_sub(b).is_some_and(|d| d.m == 0)
}

/// Digits of a value as the page shows them: no sign, no point, no trailing zeros of a fraction.
fn digits(value: &str) -> String {
    let v = value.trim_start_matches('-');
    let v = if v.contains('.') {
        v.trim_end_matches('0').trim_end_matches('.')
    } else {
        v
    };
    v.chars().filter(char::is_ascii_digit).collect()
}

pub(super) fn c3(cx: &Ctx) -> Vec<Finding> {
    let mut out = Vec::new();
    // `value` is the exact decimal of `value_text`.
    for (id, p) in cx.b.data_map("points").into_iter().flatten() {
        let at = format!("data.json:points.{id}");
        match (p["value"].as_str(), p["value_text"].as_str()) {
            (Some(v), Some(t)) => match decimal::parse(t) {
                Ok((n, _)) => {
                    let ok = Dec::from_numeric(&n)
                        .zip(Dec::from_numeric(v))
                        .is_some_and(|(a, b)| same(a, b));
                    if !ok {
                        out.push(Finding::block(
                            "C3",
                            at,
                            format!("value {v} is not the value of the source's text {t:?}"),
                        ));
                    }
                }
                Err(e) => out.push(Finding::block(
                    "C3",
                    at,
                    format!("value_text {t:?} does not parse: {e}"),
                )),
            },
            (Some(v), None) => out.push(Finding::block(
                "C3",
                at,
                format!("value {v} has no source text"),
            )),
            _ => {}
        }
    }
    // A derived value is recomputed from its inputs: `a` is the newer value or the part, `b` the base or total.
    for (id, d) in cx.b.data_map("derived").into_iter().flatten() {
        let at = format!("data.json:derived.{id}");
        let inputs: Vec<&Value> = d["inputs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(|i| cx.b.data["points"].get(i))
            .collect();
        if inputs.len() != 2 || d["inputs"].as_array().map(Vec::len) != Some(2) {
            out.push(Finding::block(
                "C3",
                at,
                "needs exactly two inputs that are in data.json",
            ));
            continue;
        }
        for k in ["unit", "currency", "scale"] {
            if inputs[0][k] != inputs[1][k] {
                out.push(Finding::block(
                    "C3",
                    &at,
                    format!("inputs differ in {k}: a derived value never converts"),
                ));
            }
        }
        let (Some(a), Some(b), Some(shown)) = (
            dec(&inputs[0]["value"]),
            dec(&inputs[1]["value"]),
            dec(&d["value"]),
        ) else {
            out.push(Finding::block(
                "C3",
                at,
                "an input or the value has no number",
            ));
            continue;
        };
        let places = d["rounding"]
            .as_u64()
            .and_then(|r| u32::try_from(r).ok())
            .unwrap_or(0);
        let computed = match d["formula"].as_str() {
            Some("pct_change") => a.checked_sub(b).and_then(|x| x.ratio_pct(b, places)),
            Some("share") => a.ratio_pct(b, places),
            Some("pp_change" | "diff") => a.checked_sub(b).and_then(|x| x.round(places)),
            other => {
                out.push(Finding::block(
                    "C3",
                    at,
                    format!("unknown formula {other:?}"),
                ));
                continue;
            }
        };
        match computed {
            Some(c) if same(c, shown) => {}
            Some(c) => out.push(Finding::block(
                "C3",
                at,
                format!(
                    "value {} but recomputing {} from its inputs gives {}",
                    d["value"].as_str().unwrap_or("?"),
                    d["formula"].as_str().unwrap_or("?"),
                    c.to_numeric()
                ),
            )),
            None => out.push(Finding::block(
                "C3",
                at,
                "the inputs cannot be divided or overflow",
            )),
        }
    }
    // Each figure the Copy cites is on the page with the data point's digits (the as-reported figure in its
    // citation popover, or the cited span). The shared formatter is the renderer's; this catches a page that
    // shows a different number.
    if !cx.b.html.is_empty() {
        let tags = html::tags(&cx.b.html);
        let shown: String = tags
            .iter()
            .filter(|t| t.has_class("popover__value") || t.has_class("cited"))
            .map(|t| html::inner_text(&cx.b.html, t))
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .filter(char::is_ascii_digit)
            .collect();
        for (at, r) in unique_refs(cx) {
            if !matches!(r.split_once('_').map(|p| p.0), Some("dp" | "dv")) {
                continue;
            }
            let Some(v) = lookup(cx, r).and_then(|v| v["value"].as_str()) else {
                continue;
            };
            let needle = digits(v);
            if !needle.is_empty() && !shown.contains(&needle) {
                out.push(Finding::block(
                    "C3",
                    at,
                    format!("`{r}` is {v}, but the page does not show that figure"),
                ));
            }
        }
    }
    out
}

pub(super) fn c7(cx: &Ctx) -> Vec<Finding> {
    let mut out = Vec::new();
    // A period label shows as the source writes it ("fiscal 2024", "Q4 FY26").
    if !cx.b.html.is_empty() {
        let text = html::visible_text(&cx.b.html);
        for (at, r) in unique_refs(cx) {
            if !r.starts_with("pd_") {
                continue;
            }
            if let Some(label) = lookup(cx, r).and_then(|p| str_of(p, "label"))
                && !text.contains(&html::collapse(label))
            {
                out.push(Finding::block(
                    "C7",
                    at,
                    format!("period `{r}` should show as {label:?}, which is not on the page"),
                ));
            }
        }
    }
    // A series is one measure: every point has the series' unit, currency and scale.
    let points = &cx.b.data["points"];
    for (id, s) in cx.b.data_map("series").into_iter().flatten() {
        for p in s["points"].as_array().into_iter().flatten() {
            let Some(dp) = str_of(p, "point").and_then(|d| points.get(d).map(|v| (d, v))) else {
                continue;
            };
            for k in ["unit", "currency", "scale"] {
                if dp.1[k] != s[k] {
                    out.push(Finding::block(
                        "C7",
                        format!("data.json:series.{id}"),
                        format!(
                            "`{}` has {k} {} but the series says {}",
                            dp.0, dp.1[k], s[k]
                        ),
                    ));
                }
            }
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn digits_drop_sign_point_and_fraction_zeros() {
        assert_eq!(digits("391035"), "391035");
        assert_eq!(digits("-0.3"), "03");
        assert_eq!(digits("2.0"), "2");
        assert_eq!(digits("10.0"), "10");
        assert_eq!(digits("100"), "100");
    }
}
