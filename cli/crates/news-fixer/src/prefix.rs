//! Page types, their skills and the cached prompt prefix (Bureau §3.9, decision D4).
//!
//! The prefix is instructions + `news-brand` + one `page-*` skill + the component index + the specs
//! of the components that page type's typical layout uses, in that order, written to one file named
//! by its hash (`<page-type>.<hash12>.md`). The same inputs give the same bytes, so every call of a
//! page type shares one prefix and prompt caching stays warm. Skills and instructions are compiled
//! in, like the scorer's seeds; line endings are normalised so a Windows checkout hashes the same.

use crate::components::Component;
use news_core::{NewsError, NewsResult};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const INSTRUCTIONS: &str = include_str!("../../../../pipeline/prefix/reporter-instructions.md");
const BRAND: &str = include_str!("../../../../skills/news-brand/SKILL.md");

/// Every page type with a `page-*` skill, and the skill.
const SKILLS: [(&str, &str); 7] = [
    (
        "earnings",
        include_str!("../../../../skills/page-earnings/SKILL.md"),
    ),
    (
        "explainer",
        include_str!("../../../../skills/page-explainer/SKILL.md"),
    ),
    (
        "macro-release",
        include_str!("../../../../skills/page-macro-release/SKILL.md"),
    ),
    (
        "major-report",
        include_str!("../../../../skills/page-major-report/SKILL.md"),
    ),
    (
        "news-story",
        include_str!("../../../../skills/page-news-story/SKILL.md"),
    ),
    (
        "policy-decision",
        include_str!("../../../../skills/page-policy-decision/SKILL.md"),
    ),
    (
        "science",
        include_str!("../../../../skills/page-science/SKILL.md"),
    ),
];

/// The page type (skill name without `page-`) for a candidate's page type (Clerk's rules, A3).
pub fn skill_page_type(candidate: &str) -> Option<&'static str> {
    Some(match candidate {
        "macro" => "macro-release",
        "policy" => "policy-decision",
        "report" => "major-report",
        "news" => "news-story",
        "paper" => "science",
        other => SKILLS.iter().find(|(t, _)| *t == other)?.0,
    })
}

fn skill(page_type: &str) -> NewsResult<String> {
    SKILLS
        .iter()
        .find(|(t, _)| *t == page_type)
        .map(|(_, s)| normalise(s))
        .ok_or_else(|| NewsError::validation(format!("no page skill for page type {page_type:?}")))
}

fn normalise(s: &str) -> String {
    s.replace("\r\n", "\n")
}

/// A skill without its YAML front matter (the description is for skill loaders, not the page call).
fn body(skill: &str) -> &str {
    skill
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map_or(skill, |(_, b)| b)
        .trim()
}

/// Words in a layout line → components (`news components` names). `Step line` is matched apart.
const ALIASES: &[(&str, &str)] = &[
    ("Bars", "BarChart"),
    ("BarChart", "BarChart"),
    ("ChoroplethMap", "ChoroplethMap"),
    ("DataTable", "DataTable"),
    ("GlossaryTerm", "GlossaryTerm"),
    ("ImageFigure", "ImageFigure"),
    ("LineChart", "LineChart"),
    ("StepLineChart", "StepLineChart"),
    ("VideoEmbed", "VideoEmbed"),
    ("Finding", "Finding"),
    ("Glossary", "GlossaryTerm"),
    ("Image", "ImageFigure"),
    ("KeyFigures", "KeyFigures"),
    ("Line", "LineChart"),
    ("Map", "ChoroplethMap"),
    ("MethodSummary", "MethodSummary"),
    ("Note", "Note"),
    ("PolicyDecision", "PolicyDecision"),
    ("Prose", "Prose"),
    ("Quote", "Quote"),
    ("Sankey", "Sankey"),
    ("Table", "DataTable"),
    ("Timeline", "Timeline"),
    ("Video", "VideoEmbed"),
    ("Waterfall", "Waterfall"),
];

/// The skill's "Typical layout" paragraph for a tier: the `Tier 2:` paragraph when there is one and
/// the tier is 2, else the first.
fn layout(skill: &str, tier: u8) -> String {
    let Some(section) = skill
        .split("\n## ")
        .find(|s| s.starts_with("Typical layout"))
    else {
        return String::new();
    };
    let paragraphs: Vec<&str> = section
        .split_once('\n')
        .map_or("", |(_, b)| b)
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if tier == 2
        && let Some(p) = paragraphs.iter().find(|p| p.starts_with("Tier 2:"))
    {
        return (*p).to_owned();
    }
    let first = paragraphs.first().copied().unwrap_or_default();
    // Tiers can also be `Tier 1:` / `Tier 2:` list lines inside the first paragraph.
    let tier_line = |n: u8| {
        first.lines().find(|l| {
            l.trim_start_matches(['-', '*', ' '])
                .starts_with(&format!("Tier {n}:"))
        })
    };
    tier_line(tier.clamp(1, 2)).unwrap_or(first).to_owned()
}

/// The components a page type's typical layout names, in layout order.
pub fn suggested(page_type: &str, tier: u8) -> NewsResult<Vec<&'static str>> {
    let text = layout(&skill(page_type)?, tier);
    let words: Vec<&str> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let mut out: Vec<&'static str> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let found = if words[i] == "Step" && words.get(i + 1) == Some(&"line") {
            i += 1;
            Some("StepLineChart")
        } else {
            ALIASES
                .iter()
                .find(|(w, _)| *w == words[i])
                .map(|(_, c)| *c)
        };
        if let Some(c) = found
            && !out.contains(&c)
        {
            out.push(c);
        }
        i += 1;
    }
    Ok(out)
}

/// The specs a page type's prefix carries: every tier's suggestions, first-seen order.
fn preselected(page_type: &str) -> NewsResult<Vec<&'static str>> {
    let mut out = suggested(page_type, 1)?;
    for c in suggested(page_type, 2)? {
        if !out.contains(&c) {
            out.push(c);
        }
    }
    Ok(out)
}

/// One built prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prefix {
    pub page_type: String,
    pub text: String,
    /// SHA-256 of `text`, lower-case hex.
    pub hash: String,
    /// Pre-selected components the index does not have (a repair round supplies their specs).
    pub missing: Vec<String>,
}

impl Prefix {
    /// `<page-type>.<first 12 hex of the hash>.md`.
    pub fn file_name(&self) -> String {
        format!("{}.{}.md", self.page_type, &self.hash[..12])
    }

    /// Estimated tokens ([`crate::build::tokens`]).
    pub fn tokens(&self) -> usize {
        crate::build::tokens(&self.text)
    }
}

fn spec(c: &Component) -> String {
    let mut s = format!("## {}\n{}\n", c.name, c.line);
    if let Some(d) = &c.description {
        s.push_str(&format!("{d}\n"));
    }
    if let Some(d) = &c.data_shape {
        s.push_str(&format!("Data: {d}\n"));
    }
    if let Some(p) = &c.props {
        s.push_str(&format!("Props: {p}\n"));
    }
    if let Some(e) = &c.example {
        s.push_str(&format!("Example: {e}\n"));
    }
    s
}

/// Build the prefix for `page_type` from the component index.
pub fn build(page_type: &str, index: &[Component]) -> NewsResult<Prefix> {
    let page = skill(page_type)?;
    let wanted = preselected(page_type)?;
    let lines: Vec<&str> = index.iter().map(|c| c.line.as_str()).collect();
    let mut specs = Vec::new();
    let mut missing = Vec::new();
    for name in wanted {
        match index.iter().find(|c| c.name == name) {
            Some(c) => specs.push(spec(c)),
            None => missing.push(name.to_owned()),
        }
    }
    let text = normalise(&format!(
        "{}\n\n---\n\n{}\n\n---\n\n{}\n\n---\n\n# Component index\n\n{}\n\n---\n\n# Component specs\n\n{}",
        normalise(INSTRUCTIONS).trim(),
        body(&normalise(BRAND)),
        body(&page),
        lines.join("\n"),
        specs.join("\n"),
    ));
    let hash: String = Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok(Prefix {
        page_type: page_type.to_owned(),
        text,
        hash,
        missing,
    })
}

/// Write the prefix to `dir` under its hash name, unless that file is already there (content
/// addressed, so it is the same bytes). Returns the path.
pub fn write(dir: &Path, prefix: &Prefix) -> std::io::Result<PathBuf> {
    let path = dir.join(prefix.file_name());
    if !path.is_file() {
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join(format!("{}.tmp", prefix.file_name()));
        std::fs::write(&tmp, prefix.text.as_bytes())?;
        std::fs::rename(&tmp, &path)?;
    }
    Ok(path)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::components::parse;
    use serde_json::json;

    fn index() -> Vec<Component> {
        let names = [
            "BarChart",
            "DataTable",
            "KeyFigures",
            "LineChart",
            "Prose",
            "Quote",
            "PolicyDecision",
        ];
        let comps: Vec<_> = names
            .iter()
            .map(|n| {
                json!({"name": n, "line": format!("{n} — x — y"), "tokens": 3,
                            "description": "One sentence.", "dataShape": "dp_ refs",
                            "props": {"type": "object"}, "example": format!("<{n} />")})
            })
            .collect();
        parse(
            &json!({"version": 1, "budget": {"tokens": 4000, "perEntry": 24}, "tokens": 1,
                     "components": comps})
            .to_string(),
        )
        .unwrap()
    }

    #[test]
    fn candidate_types_map_to_skills() {
        assert_eq!(skill_page_type("macro"), Some("macro-release"));
        assert_eq!(skill_page_type("paper"), Some("science"));
        assert_eq!(skill_page_type("science"), Some("science"));
        assert_eq!(skill_page_type("explainer"), Some("explainer"));
        assert_eq!(skill_page_type("horoscope"), None);
        for (t, _) in SKILLS {
            assert!(!suggested(t, 1).unwrap().is_empty(), "{t}");
        }
    }

    #[test]
    fn suggestions_follow_each_layout() {
        assert_eq!(
            suggested("macro-release", 1).unwrap(),
            ["KeyFigures", "LineChart", "BarChart", "Prose", "DataTable"]
        );
        assert_eq!(
            suggested("policy-decision", 1).unwrap(),
            [
                "PolicyDecision",
                "Quote",
                "StepLineChart",
                "Prose",
                "Timeline",
                "VideoEmbed"
            ]
        );
        assert_eq!(
            suggested("earnings", 2).unwrap(),
            ["KeyFigures", "Prose", "DataTable"]
        );
        assert!(suggested("earnings", 1).unwrap().contains(&"Sankey"));
    }

    #[test]
    fn prefix_is_ordered_hashed_and_stable() {
        let a = build("macro-release", &index()).unwrap();
        let b = build("macro-release", &index()).unwrap();
        assert_eq!(a, b, "same inputs, same bytes");
        assert_eq!(a.hash.len(), 64);
        assert!(a.file_name().starts_with("macro-release.") && a.file_name().ends_with(".md"));
        let at = |s: &str| a.text.find(s).unwrap_or_else(|| panic!("{s} missing"));
        assert!(at("# Instructions") < at("# News brand rules"));
        assert!(at("# News brand rules") < at("# Macro release pages"));
        assert!(at("# Macro release pages") < at("# Component index"));
        assert!(at("# Component index") < at("## KeyFigures"));
        assert!(!a.text.contains("\r\n") && !a.text.contains("description: \""));
        // The whole index is listed; specs only for the page type's components.
        assert!(a.text.contains("PolicyDecision — x — y") && !a.text.contains("## PolicyDecision"));
        assert!(a.missing.is_empty());
        // Another page type or index is another prefix.
        let p = build("policy-decision", &index()).unwrap();
        assert_ne!(p.hash, a.hash);
        assert_eq!(p.missing, ["StepLineChart", "Timeline", "VideoEmbed"]);
        assert_ne!(build("macro-release", &index()[1..]).unwrap().hash, a.hash);
        assert!(build("horoscope", &index()).is_err());
    }

    /// A fresh process names every page type's prefix file the same (no per-process state, such as
    /// hash-map seeds or the clock, reaches the hash).
    #[test]
    fn prefix_hash_is_stable_across_runs() {
        let names = || {
            SKILLS
                .iter()
                .map(|(t, _)| build(t, &index()).unwrap().file_name())
                .collect::<Vec<_>>()
                .join(" ")
        };
        if std::env::var_os("NEWS_PREFIX_CHILD").is_some() {
            println!("NAMES<{}>", names());
            return;
        }
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "prefix::tests::prefix_hash_is_stable_across_runs",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("NEWS_PREFIX_CHILD", "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        let child = stdout
            .split_once("NAMES<")
            .and_then(|(_, r)| r.split_once('>'))
            .map(|(n, _)| n)
            .unwrap_or_else(|| panic!("the child run printed no names: {stdout}"));
        assert_eq!(child, names());
        assert_eq!(child.split(' ').count(), SKILLS.len());
    }

    #[test]
    fn write_is_content_addressed() {
        let dir = std::env::temp_dir().join(format!("news-prefix-{}", std::process::id()));
        let p = build("science", &index()).unwrap();
        let path = write(&dir, &p).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), p.text);
        assert_eq!(write(&dir, &p).unwrap(), path);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
