//! C10 (links and media) and C13 (accessibility): the checks that reach outside the bundle.

use std::collections::BTreeSet;

use super::{AxeMode, Ctx, Finding, Run};
use crate::html;

/// FNV-1a, so "one Tier 2 page in ten" is the same page every time it is checked.
fn stable_hash(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    })
}

/// `https://host/…` to `host`.
fn host(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("https://")?;
    rest.split(['/', '?', '#'])
        .next()
        .map(|h| h.split('@').next_back().unwrap_or(h))
        .filter(|h| !h.is_empty())
}

pub(super) fn c10(cx: &Ctx) -> Run {
    let Some(links) = cx.env.links else {
        return Err("link checks are off".into());
    };
    let mut out = Vec::new();
    let mut probed = BTreeSet::new();
    let mut probe = |url: &str, at: &str, block: bool, what: &str, out: &mut Vec<Finding>| {
        if !probed.insert(url.to_owned()) {
            return;
        }
        let p = links.check(url);
        if !p.ok {
            let msg = format!("{what} {url} does not answer ({})", p.note);
            out.push(if block {
                Finding::block("C10", at, msg)
            } else {
                Finding::warn("C10", at, msg)
            });
        }
    };
    for (id, s) in cx.b.sources["sources"].as_object().into_iter().flatten() {
        if let Some(url) = s["url"].as_str() {
            probe(
                url,
                &format!("sources.json:{id}"),
                true,
                "the source",
                &mut out,
            );
        }
    }
    for t in html::tags(&cx.b.html) {
        let Some(src) = t.attr("src").filter(|s| !s.is_empty()) else {
            continue;
        };
        match t.name.as_str() {
            "img" => {
                if src.starts_with("data:") || (src.starts_with('/') && !src.starts_with("//")) {
                    continue;
                }
                if host(src).is_none_or(|h| !cx.lex.c10.image_hosts.iter().any(|a| a == h)) {
                    out.push(Finding::block(
                        "C10",
                        "index.html",
                        format!("image {src} is not from an allow-listed host"),
                    ));
                } else {
                    probe(src, "index.html", false, "the image", &mut out);
                }
            }
            "iframe" | "video" | "source" | "embed" => {
                if src.starts_with("https://") {
                    probe(src, "index.html", true, "the embed", &mut out);
                } else if !src.starts_with('/') {
                    out.push(Finding::block(
                        "C10",
                        "index.html",
                        format!("embed {src} is not https"),
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

pub(super) fn c13(cx: &Ctx) -> Run {
    if cx.env.axe_mode == AxeMode::Off {
        return Err("axe is off".into());
    }
    let tier = cx.b.meta["tier"].as_i64().unwrap_or(2);
    let one_in = cx.lex.c13.tier2_one_in.max(1);
    if tier != 1 && !stable_hash(&cx.b.page_id()).is_multiple_of(one_in) {
        return Err(format!(
            "not sampled: axe runs on every Tier 1 page and one Tier 2 page in {one_in}"
        ));
    }
    // If axe cannot run, the page is not shown to be accessible: fail rather than pass unchecked.
    match cx.env.axe.run(&cx.b.dir) {
        Err(e) => Ok(vec![Finding::block(
            "C13",
            "index.html",
            format!("axe could not run: {e}"),
        )]),
        Ok(violations) => Ok(violations
            .iter()
            .filter(|v| v.blocks())
            .map(|v| {
                Finding::block(
                    "C13",
                    "index.html",
                    format!(
                        "{} ({}, {} elements): {}",
                        v.id,
                        v.impact.as_deref().unwrap_or("?"),
                        v.nodes,
                        v.help
                    ),
                )
            })
            .collect()),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn host_of_an_https_url() {
        assert_eq!(
            host("https://images-assets.nasa.gov/a/b.jpg"),
            Some("images-assets.nasa.gov")
        );
        assert_eq!(host("https://x.example?q=1"), Some("x.example"));
        assert_eq!(host("http://x.example/"), None);
    }

    #[test]
    fn about_one_page_in_ten_is_sampled() {
        let n = (0..1000)
            .filter(|i| stable_hash(&format!("2026/11/page-{i}")).is_multiple_of(10))
            .count();
        assert!((60..=140).contains(&n), "{n}");
    }
}
