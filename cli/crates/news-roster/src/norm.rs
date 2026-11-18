//! Alias normalisation (Bureau §3.5, step 2): Unicode NFKC, case-fold, punctuation to spaces, then
//! legal suffixes (`Inc.`, `plc`, `Ltd`, `AG`, `S.A.` …) and a leading `the` stripped. Both sides of
//! every comparison go through the same function, so what is stored is what is matched.

use std::collections::BTreeSet;

use unicode_normalization::UnicodeNormalization;

/// Legal-form words dropped from the end of a name. Multi-word forms are listed as the words appear
/// after punctuation is removed (`S.A.` → `s a`, `N.V.` → `n v`).
const SUFFIXES: &[&[&str]] = &[
    &["inc"],
    &["incorporated"],
    &["corp"],
    &["corporation"],
    &["co"],
    &["company"],
    &["ltd"],
    &["limited"],
    &["plc"],
    &["llc"],
    &["lp"],
    &["llp"],
    &["ag"],
    &["se"],
    &["sa"],
    &["s", "a"],
    &["nv"],
    &["n", "v"],
    &["bv"],
    &["b", "v"],
    &["gmbh"],
    &["spa"],
    &["s", "p", "a"],
    &["ab"],
    &["asa"],
    &["oyj"],
    &["kk"],
    &["k", "k"],
    &["pty"],
    &["pte"],
    &["sarl"],
    &["srl"],
];

/// Words that are noise at the start of a name.
const PREFIXES: &[&str] = &["the"];

pub fn tokens(text: &str) -> Vec<String> {
    let folded: String = text
        .nfkc()
        .flat_map(char::to_lowercase)
        .map(|c| {
            // `&` reads as "and"; every other non-alphanumeric character separates words.
            if c == '&' {
                " and ".to_owned()
            } else if c == '\'' || c == '\u{2019}' {
                // "People's" and "Peoples" are the same word; apostrophes vanish, not split.
                String::new()
            } else if c.is_alphanumeric() {
                c.to_string()
            } else {
                " ".to_owned()
            }
        })
        .collect();
    folded.split_whitespace().map(str::to_owned).collect()
}

/// The normalised form stored in `entity_aliases.alias_norm`.
pub fn normalize(text: &str) -> String {
    let mut t = tokens(text);
    while t.len() > 1 && PREFIXES.contains(&t[0].as_str()) {
        t.remove(0);
    }
    // Strip legal forms from the end, repeatedly ("Foo Co. Ltd"), but never the whole name.
    loop {
        let before = t.len();
        for suf in SUFFIXES {
            if t.len() > suf.len()
                && t[t.len() - suf.len()..]
                    .iter()
                    .map(String::as_str)
                    .eq(suf.iter().copied())
            {
                t.truncate(t.len() - suf.len());
                break;
            }
        }
        if t.len() == before {
            break;
        }
    }
    t.join(" ")
}

/// pg_trgm trigrams of a normalised string: each word padded with two spaces before and one after.
pub fn trigrams(s: &str) -> BTreeSet<[char; 3]> {
    let mut out = BTreeSet::new();
    for w in s.split_whitespace() {
        let padded: Vec<char> = "  ".chars().chain(w.chars()).chain(" ".chars()).collect();
        for g in padded.windows(3) {
            out.insert([g[0], g[1], g[2]]);
        }
    }
    out
}

/// pg_trgm `similarity`: shared trigrams over all distinct trigrams.
pub fn similarity(a: &str, b: &str) -> f32 {
    let (ta, tb) = (trigrams(a), trigrams(b));
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let shared = ta.intersection(&tb).count();
    let total = ta.len() + tb.len() - shared;
    // Counts are tiny: the conversions are exact.
    #[allow(clippy::cast_precision_loss)]
    {
        shared as f32 / total as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legal_suffixes_punctuation_and_case_go() {
        for (a, b) in [
            ("Apple Inc.", "apple"),
            ("APPLE INC", "apple"),
            ("Siemens AG", "siemens"),
            ("Banco Santander, S.A.", "banco santander"),
            ("Unilever plc", "unilever"),
            ("The Coca-Cola Company", "coca cola"),
            ("Toyota Motor Corporation", "toyota motor"),
            ("Foo Co., Ltd.", "foo"),
            ("ING Groep N.V.", "ing groep"),
            ("Procter & Gamble Co", "procter and gamble"),
            ("People\u{2019}s Bank of China", "peoples bank of china"),
            ("Nestlé S.A.", "nestlé"),
            ("\u{FF21}pple", "apple"),
        ] {
            assert_eq!(normalize(a), b, "{a}");
        }
    }

    #[test]
    fn a_name_is_never_stripped_to_nothing() {
        assert_eq!(normalize("Inc"), "inc");
        assert_eq!(normalize("The Co"), "co");
        assert_eq!(normalize("  "), "");
    }

    #[test]
    fn similarity_matches_pg_trgm() {
        // pg_trgm: similarity('word', 'word') = 1; 'cat' vs 'cats': 3 shared of 6 distinct.
        assert!((similarity("word", "word") - 1.0).abs() < 1e-6);
        assert!((similarity("cat", "cats") - 0.5).abs() < 1e-6);
        assert!(similarity("bank of england", "bank of canada") < 0.6);
        assert!(similarity("apple", "banana") < 0.1);
    }
}
