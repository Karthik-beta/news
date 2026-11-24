//! C4 (numeric-literal lint), C5 (direction words), C11 (banned words and tone): the checks on the prose
//! the page writer wrote.

use std::collections::BTreeSet;

use super::{Ctx, Finding};
use crate::copy;
use crate::lexicon::{C4, Lexicon};
use news_ledger::decimal::Dec;

/// Lower-case alphabetic words; apostrophes and hyphens separate words ("twenty-one" is two).
fn words(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphabetic())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Replaces whole-word occurrences of a lower-case phrase with spaces.
fn blank_phrase(hay: &str, phrase: &str) -> String {
    let mut out = String::with_capacity(hay.len());
    let mut rest = hay;
    while let Some(i) = rest.find(phrase) {
        let before = rest[..i]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric());
        let after = rest[i + phrase.len()..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric());
        out.push_str(&rest[..i]);
        if before && after {
            out.push(' ');
        } else {
            out.push_str(phrase);
        }
        rest = &rest[i + phrase.len()..];
    }
    out.push_str(rest);
    out
}

fn snippet(s: &str, at: usize) -> String {
    let start = s[..at].char_indices().rev().nth(12).map_or(0, |(i, _)| i);
    let end = s[at..]
        .char_indices()
        .nth(14)
        .map_or(s.len(), |(i, _)| at + i);
    s[start..end].trim().to_owned()
}

/// What is wrong with one piece of prose, by the C4 rules. `names` are entity names that carry digits and
/// that Roster resolved for this page.
pub fn numeric_lint(prose: &str, c4: &C4, names: &[String]) -> Vec<String> {
    let mut s = prose.to_owned();
    // Longest first, so "G77" is not half-eaten by "G7".
    let mut skip: Vec<&String> = names.iter().chain(&c4.allow_literals).collect();
    skip.sort_by_key(|n| std::cmp::Reverse(n.len()));
    for n in skip {
        s = s.replace(n.as_str(), " ");
    }
    let mut found = Vec::new();
    if let Some((i, _)) = s.char_indices().find(|(_, c)| c.is_numeric()) {
        found.push(format!(
            "a digit in prose near {:?}: write a reference, not a number",
            snippet(&s, i)
        ));
    }
    let mut lower = s.to_lowercase();
    for p in &c4.allow_phrases {
        lower = blank_phrase(&lower, p);
    }
    let ws = words(&lower);
    let is_in = |list: &[String], w: &str| list.iter().any(|x| x == w);
    for w in &ws {
        if is_in(&c4.number_words, w) {
            found.push(format!(
                "the written-out quantity {w:?}: write a reference, not a number"
            ));
        } else if is_in(&c4.fraction_words, w) {
            found.push(format!(
                "the fraction {w:?} used as an amount: write a reference"
            ));
        } else if is_in(&c4.multiplier_words, w) {
            found.push(format!(
                "the multiplier {w:?} states an amount: write a reference"
            ));
        }
    }
    // "several per cent", "a few percentage points": a unit phrase after any quantity word.
    let mut phrases: Vec<&String> = c4.unit_phrases.iter().collect();
    phrases.sort_by_key(|p| std::cmp::Reverse(p.len()));
    for phrase in phrases {
        let mut from = 0;
        while let Some(i) = lower[from..].find(phrase.as_str()).map(|i| i + from) {
            from = i + phrase.len();
            let whole = lower[from..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric())
                && lower[..i]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !c.is_alphanumeric());
            if whole
                && words(&lower[..i])
                    .pop()
                    .is_some_and(|p| is_in(&c4.quantity_words, &p))
            {
                found.push(format!(
                    "{phrase:?} after a quantity word states an amount: write a reference"
                ));
            }
            // Blank the phrase so a shorter one ("percent") does not match inside it ("percentage point").
            lower = format!(
                "{}{}{}",
                &lower[..i],
                " ".repeat(phrase.len()),
                &lower[from..]
            );
        }
    }
    found.dedup();
    found
}

pub(super) fn c4(cx: &Ctx) -> Vec<Finding> {
    // A templated page (D10) is built by code from source labels, which can carry digits ("Table 1").
    if cx.b.meta["flags"]
        .as_array()
        .is_some_and(|f| f.iter().any(|v| v == "templated"))
    {
        return Vec::new();
    }
    let names: Vec<String> = cx.b.meta["entities"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| e["name"].as_str())
        .filter(|n| n.chars().any(char::is_numeric))
        .map(str::to_owned)
        .collect();
    let mut out = Vec::new();
    for t in cx.copy.iter().filter(|t| t.is_prose()) {
        for m in numeric_lint(&copy::split(&t.text).prose, &cx.lex.c4, &names) {
            out.push(Finding::block("C4", &t.at, m));
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dir {
    Up,
    Down,
    Flat,
}

fn sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let b = text.as_bytes();
    for (i, c) in text.char_indices() {
        if matches!(c, '.' | '!' | '?') && b.get(i + 1).is_none_or(u8::is_ascii_whitespace) {
            out.push(&text[start..=i]);
            start = i + 1;
        }
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

fn sign(v: &str) -> Option<Dir> {
    let d = Dec::from_numeric(v)?;
    Some(match d.m.cmp(&0) {
        std::cmp::Ordering::Greater => Dir::Up,
        std::cmp::Ordering::Less => Dir::Down,
        std::cmp::Ordering::Equal => Dir::Flat,
    })
}

fn direction_of(lex: &Lexicon, w: &str) -> Option<Dir> {
    let has = |l: &[String]| l.iter().any(|x| x == w);
    if has(&lex.c5.up) {
        Some(Dir::Up)
    } else if has(&lex.c5.down) {
        Some(Dir::Down)
    } else if has(&lex.c5.flat) {
        Some(Dir::Flat)
    } else {
        None
    }
}

pub(super) fn c5(cx: &Ctx) -> Vec<Finding> {
    let mut out = Vec::new();
    for t in cx.copy.iter().filter(|t| t.is_prose()) {
        for sentence in sentences(&t.text) {
            let sp = copy::split(sentence);
            // The signs of the changes this sentence cites (`dv_…` with a change formula).
            let signs: Vec<(&String, Dir)> = sp
                .tokens
                .iter()
                .filter(|r| r.starts_with("dv_"))
                .filter_map(|r| {
                    let d = cx.b.data["derived"].get(r.as_str())?;
                    matches!(
                        d["formula"].as_str(),
                        Some("pct_change" | "pp_change" | "diff")
                    )
                    .then(|| sign(d["value"].as_str()?).map(|s| (r, s)))
                    .flatten()
                })
                .collect();
            if signs.is_empty() {
                continue;
            }
            let mut seen = BTreeSet::new();
            for w in words(&sp.prose) {
                let Some(dir) = direction_of(cx.lex, &w) else {
                    continue;
                };
                if signs.iter().any(|(_, s)| *s == dir) || !seen.insert(w.clone()) {
                    continue;
                }
                let (r, s) = signs[0];
                out.push(Finding::block(
                    "C5",
                    &t.at,
                    format!(
                        "{w:?} does not agree with `{r}`, which is {}",
                        match s {
                            Dir::Up => "positive",
                            Dir::Down => "negative",
                            Dir::Flat => "zero",
                        }
                    ),
                ));
            }
        }
    }
    out
}

pub(super) fn c11(cx: &Ctx) -> Vec<Finding> {
    let mut out = Vec::new();
    for t in cx.copy.iter().filter(|t| t.is_prose()) {
        let prose = copy::split(&t.text).prose;
        let lower = prose.to_lowercase();
        for p in &cx.lex.banned {
            if blank_phrase(&lower, p) != lower {
                out.push(Finding::block(
                    "C11",
                    &t.at,
                    format!("the banned word or phrase {p:?} (data/seeds/banned-words.toml)"),
                ));
            }
        }
        if prose.contains('!') || prose.contains('?') {
            out.push(Finding::block(
                "C11",
                &t.at,
                "no exclamation marks or questions (brand voice)",
            ));
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn lint(s: &str) -> Vec<String> {
        numeric_lint(s, &Lexicon::v1().unwrap().c4, &[])
    }

    #[test]
    fn blocks_digits_and_written_out_quantities() {
        for bad in [
            "Revenue was 2024 USD",
            "Inflation was three per cent",
            "Sales doubled in the year",
            "Costs fell by a half",
            "Rates are up several percentage points",
            "It cost twenty-one million",
            "A rise of \u{bd}",
        ] {
            assert!(!lint(bad).is_empty(), "{bad}");
        }
    }

    #[test]
    fn allows_names_and_fixed_expressions() {
        for ok in [
            "The G7 and G20 met after COVID-19 and the 5G auction",
            "One of the largest rises",
            "A one-off charge",
            "Sales rose in  and fell in the Americas",
        ] {
            assert!(lint(ok).is_empty(), "{ok}: {:?}", lint(ok));
        }
        // An entity name Roster resolved, with a digit in it.
        let names = vec!["Formula 1".to_owned()];
        assert!(
            numeric_lint(
                "Viewing of Formula 1 fell",
                &Lexicon::v1().unwrap().c4,
                &names
            )
            .is_empty()
        );
    }

    #[test]
    fn splits_sentences() {
        assert_eq!(
            sentences("It rose. It fell! Then?"),
            ["It rose.", " It fell!", " Then?"]
        );
    }
}
