//! Proof's word lists and limits (`data/seeds/proof-v1.toml`), compiled in so a check result always names
//! the lists that made it.

use serde::Deserialize;
use std::collections::BTreeMap;

const SEED: &str = include_str!("../../../../data/seeds/proof-v1.toml");
const BANNED: &str = include_str!("../../../../data/seeds/banned-words.toml");

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C4 {
    pub allow_literals: Vec<String>,
    pub allow_phrases: Vec<String>,
    pub number_words: Vec<String>,
    pub fraction_words: Vec<String>,
    pub multiplier_words: Vec<String>,
    pub quantity_words: Vec<String>,
    pub unit_phrases: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C5 {
    pub up: Vec<String>,
    pub down: Vec<String>,
    pub flat: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Disclaimer {
    pub categories: Vec<String>,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C9 {
    pub disclaimer: Vec<Disclaimer>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C10 {
    pub image_hosts: Vec<String>,
    pub cache_hours: i64,
}

/// `data/seeds/banned-words.toml`: every group is a list of words or phrases.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct BannedFile {
    #[allow(dead_code)]
    version: String,
    hype: Vec<String>,
    verdicts: Vec<String>,
    forecasts: Vec<String>,
    science: Vec<String>,
    advice: Vec<String>,
    voice: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C12 {
    pub bundle_bytes_max: u64,
    pub components: BTreeMap<String, [usize; 2]>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C13 {
    pub tier2_one_in: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lexicon {
    pub version: String,
    pub c4: C4,
    pub c5: C5,
    pub c9: C9,
    pub c10: C10,
    pub c12: C12,
    pub c13: C13,
    /// Banned words and phrases (C11), lower case, from `banned-words.toml`.
    #[serde(skip)]
    pub banned: Vec<String>,
}

impl Lexicon {
    /// The compiled-in `proof-v1` lists.
    pub fn v1() -> Result<Self, String> {
        let mut lex: Lexicon =
            toml::from_str(SEED).map_err(|e| format!("data/seeds/proof-v1.toml: {e}"))?;
        let b: BannedFile =
            toml::from_str(BANNED).map_err(|e| format!("data/seeds/banned-words.toml: {e}"))?;
        lex.banned = [
            b.hype,
            b.verdicts,
            b.forecasts,
            b.science,
            b.advice,
            b.voice,
        ]
        .concat()
        .iter()
        .map(|w| w.to_lowercase())
        .collect();
        Ok(lex)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_seed_parses_and_has_about_sixty_direction_words() {
        let lex = Lexicon::v1().unwrap();
        assert_eq!(lex.version, "proof-v1");
        let n = lex.c5.up.len() + lex.c5.down.len() + lex.c5.flat.len();
        assert!((55..=70).contains(&n), "{n} direction words");
        for w in lex
            .c5
            .up
            .iter()
            .chain(&lex.c5.down)
            .chain(&lex.c5.flat)
            .chain(&lex.c4.number_words)
        {
            assert_eq!(w, &w.to_lowercase(), "{w}");
        }
        // A word cannot point both ways.
        for w in &lex.c5.up {
            assert!(!lex.c5.down.contains(w) && !lex.c5.flat.contains(w), "{w}");
        }
    }
}
