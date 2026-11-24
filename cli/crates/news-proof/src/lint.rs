//! The text lints shared by the Copy validators (`news ai accept`, `news save`) and the code checks:
//! the numeric-literal lint (C4) and the brand's banned words (C11's word list). Bureau Design §3.9, §3.10.
//!
//! Numbers on a page come from data by code, never typed by a model: prose may carry a value only as a
//! `{ref}` token, so after the tokens are removed it may hold no digit and no written-out quantity.

use news_core::refs::is_ref;

/// Terms with digits that are names, not amounts (C4's allow-list).
pub const ALLOWED_TERMS: &[&str] = &["G7", "G20", "COVID-19", "5G"];

/// Fixed expressions with a quantity word that are not amounts.
pub const FIXED_EXPRESSIONS: &[&str] = &["one of the", "one-off"];

/// Written-out quantities (C4): cardinals zero to ninety-nine (compounds split at the hyphen), the large
/// numbers, fractions used as amounts and multipliers.
pub const QUANTITY_WORDS: &[&str] = &[
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
    "twenty",
    "thirty",
    "forty",
    "fifty",
    "sixty",
    "seventy",
    "eighty",
    "ninety",
    "hundred",
    "thousand",
    "million",
    "billion",
    "trillion",
    "half",
    "halves",
    "third",
    "thirds",
    "quarter",
    "quarters",
    "tenth",
    "tenths",
    "twice",
    "double",
    "doubled",
    "doubles",
    "doubling",
    "triple",
    "tripled",
    "triples",
    "tripling",
    "halved",
    "halving",
];

/// Hype words the brand never uses (design/content-style.md, "Voice"), with their inflections.
pub const BANNED_WORDS: &[&str] = &[
    "soar",
    "soared",
    "soaring",
    "soars",
    "plunge",
    "plunged",
    "plunges",
    "plunging",
    "crashed",
    "skyrocket",
    "skyrocketed",
    "skyrocketing",
    "skyrockets",
    "stunning",
    "massive",
    "historic",
    "game-changing",
    "game-changer",
];

/// The text with every reference token removed: `{dp_…}` and the other five kinds, braced or bare.
pub fn strip_refs(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if !is_ref(word) {
            out.push_str(word);
        }
        word.clear();
    };
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(if c == '{' || c == '}' { ' ' } else { c });
        }
    }
    flush(&mut word, &mut out);
    out
}

/// `text` with every whole-word occurrence of `term` replaced by a space (case-sensitive unless `fold`).
fn remove_term(text: &str, term: &str, fold: bool) -> String {
    if term.is_empty() {
        return text.to_string();
    }
    let (hay, needle) = if fold {
        (text.to_lowercase(), term.to_lowercase())
    } else {
        (text.to_string(), term.to_string())
    };
    // Lower-casing can change byte lengths outside ASCII; only fold when it does not.
    if hay.len() != text.len() {
        return text.to_string();
    }
    let boundary = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric());
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while let Some(at) = hay[i..].find(&needle) {
        let start = i + at;
        let end = start + needle.len();
        if boundary(hay[..start].chars().next_back()) && boundary(hay[end..].chars().next()) {
            out.push_str(&text[i..start]);
            out.push(' ');
        } else {
            out.push_str(&text[i..end]);
        }
        i = end;
    }
    out.push_str(&text[i..]);
    out
}

/// C4: the digits and written-out quantities left in `text` once its reference tokens, the allow-listed
/// terms and `names` (entity names with digits that Roster resolved for this brief) are removed.
/// Empty when the text is clean.
pub fn numeric_literals(text: &str, names: &[&str]) -> Vec<String> {
    let mut s = strip_refs(text);
    for term in names.iter().chain(ALLOWED_TERMS) {
        s = remove_term(&s, term, false);
    }
    for expr in FIXED_EXPRESSIONS {
        s = remove_term(&s, expr, true);
    }
    let mut found = Vec::new();
    let mut run = String::new();
    for c in s.chars().chain(std::iter::once(' ')) {
        if c.is_numeric() {
            run.push(c);
        } else if !run.is_empty() {
            found.push(std::mem::take(&mut run));
        }
    }
    found.extend(
        words(&s)
            .filter(|w| QUANTITY_WORDS.contains(&w.as_str()))
            .map(|w| w.to_string()),
    );
    found
}

/// Lower-case words split at anything that is not a letter (so compounds split at the hyphen).
fn words(s: &str) -> impl Iterator<Item = String> + '_ {
    s.split(|c: char| !c.is_alphabetic())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
}

/// The banned words in `text`, as written.
pub fn banned_words(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphabetic() || c == '-'))
        .map(|w| w.trim_matches('-'))
        .filter(|w| BANNED_WORDS.contains(&w.to_lowercase().as_str()))
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_braced_and_bare_reference_tokens() {
        assert_eq!(
            strip_refs("rose to {dp_7kq2} in {pd_2026_10}").trim(),
            "rose to    in"
        );
        assert_eq!(strip_refs("dp_aapl_fy2024").trim(), "");
        assert!(
            numeric_literals("Sales were {dp_aapl_net_sales_fy2024} in {pd_fy2024}", &[])
                .is_empty()
        );
    }

    #[test]
    fn blocks_the_design_examples() {
        assert_eq!(numeric_literals("revenue was 2024 USD", &[]), ["2024"]);
        // Blocked whether or not 2024 is a period in the brief: periods are pd_ references.
        assert_eq!(
            numeric_literals("in {pd_fy2024} revenue was 2024 USD", &[]),
            ["2024"]
        );
        assert_eq!(
            numeric_literals("prices rose three per cent", &[]),
            ["three"]
        );
        assert_eq!(numeric_literals("Profit doubled", &[]), ["doubled"]);
        assert_eq!(
            numeric_literals("about a quarter of sales", &[]),
            ["quarter"]
        );
        assert_eq!(
            numeric_literals("up twenty-one points", &[]),
            ["twenty", "one"]
        );
        assert_eq!(numeric_literals("rose 2.4%", &[]), ["2", "4"]);
        assert_eq!(numeric_literals("a ½ share", &[]), ["½"]);
    }

    #[test]
    fn allows_names_and_fixed_expressions() {
        assert!(numeric_literals("Among the G7 and the G20", &[]).is_empty());
        assert!(numeric_literals("after COVID-19 and with 5G", &[]).is_empty());
        assert!(numeric_literals("It was one of the largest, a one-off", &[]).is_empty());
        assert!(numeric_literals("One of the largest", &[]).is_empty());
        // An allow-listed term inside a longer token is not exempt.
        assert_eq!(numeric_literals("G70", &[]), ["70"]);
        // Entity names that Roster resolved for the brief.
        assert!(numeric_literals("Shares of 3M rose", &["3M"]).is_empty());
        assert_eq!(numeric_literals("Shares of 3M rose", &[]), ["3"]);
        // Words that only contain a quantity word are fine.
        assert!(numeric_literals("headquarters, behalf, someone, often", &[]).is_empty());
    }

    #[test]
    fn finds_banned_words_in_any_case() {
        assert_eq!(
            banned_words("Sales Soared after a game-changing launch"),
            ["Soared", "game-changing"]
        );
        assert!(banned_words("Sales rose; costs fell").is_empty());
    }
}
