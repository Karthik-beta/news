//! The deterministic resolver (Bureau §3.5, steps 1 to 5), over an in-memory [`Index`].
//!
//! 1. **Identifier**: `scheme:value` (`cik:320193`, `iso3166:US`) names an entity exactly.
//! 2. **Alias**: the normalised text equals exactly one entity's alias (within the expected kind).
//! 3. **Gazetteer**: the longest alias spans inside a longer text; one entity, or no answer.
//! 4. **Fuzzy**: pg_trgm-style similarity of at least 0.6, one best entity, a margin of at least 0.1
//!    over the runner-up.
//! 5. **Unresolved**: everything else, with the reason and up to five candidates. The resolver never
//!    creates an entity and never picks between equals.
//!
//! The PostgreSQL side ([`crate::store`]) fetches only the aliases that could matter for one text into
//! a small [`Index`] and calls the same function, so tests, the CLI and the labelled set share one
//! implementation.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::norm::{normalize, similarity, tokens};

pub const FUZZY_MIN: f32 = 0.6;
pub const FUZZY_MARGIN: f32 = 0.1;
/// Below this a fuzzy candidate is not even worth listing as `below_threshold`.
const CANDIDATE_MIN: f32 = 0.4;
/// Longest alias, in words, the gazetteer looks for.
const MAX_SPAN: usize = 6;
/// A single-word alias this short is only trusted as the whole text, never inside a longer one.
const MIN_SOLO_LEN: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Entity {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub prominence: i16,
}

#[derive(Debug, Default, Clone)]
pub struct Index {
    pub entities: BTreeMap<i64, Entity>,
    aliases: BTreeMap<String, BTreeSet<i64>>,
    ids: BTreeMap<(String, String), i64>,
}

impl Index {
    pub fn add_entity(&mut self, e: Entity) {
        self.entities.insert(e.id, e);
    }

    /// Add an alias given as written; it is stored normalised.
    pub fn add_alias(&mut self, id: i64, alias: &str) {
        self.add_alias_norm(id, normalize(alias));
    }

    /// Add an alias already in `alias_norm` form (rows read back from `entity_aliases`).
    pub fn add_alias_norm(&mut self, id: i64, norm: String) {
        if !norm.is_empty() {
            self.aliases.entry(norm).or_default().insert(id);
        }
    }

    pub fn add_id(&mut self, id: i64, scheme: &str, value: &str) {
        self.ids
            .insert((scheme.to_owned(), canonical_id(scheme, value)), id);
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }
}

/// Identifier values as the schemes write them: CIKs are 10 digits, ISO codes upper case.
pub fn canonical_id(scheme: &str, value: &str) -> String {
    let v = value.trim();
    match scheme {
        "cik" => {
            let t = v.trim_start_matches("CIK");
            if t.bytes().all(|b| b.is_ascii_digit()) && !t.is_empty() && t.len() <= 10 {
                format!("{t:0>10}")
            } else {
                v.to_owned()
            }
        }
        "iso3166" | "iso4217" | "lei" => v.to_ascii_uppercase(),
        _ => v.to_owned(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Identifier,
    Alias,
    Gazetteer,
    Fuzzy,
    Unresolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    NoMatch,
    Ambiguous,
    BelowThreshold,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Reason::NoMatch => "no_match",
            Reason::Ambiguous => "ambiguous",
            Reason::BelowThreshold => "below_threshold",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Resolution {
    pub entity_id: Option<i64>,
    pub name: Option<String>,
    pub kind: Option<String>,
    pub step: Step,
    pub score: Option<f32>,
    /// Set when `step` is `Unresolved`.
    pub reason: Option<Reason>,
    /// Up to five `(entity id, score)` pairs for the Huddle shortlist (unresolved only).
    pub candidates: Vec<(i64, f32)>,
}

fn resolved(ix: &Index, id: i64, step: Step, score: f32) -> Resolution {
    let e = ix.entities.get(&id);
    Resolution {
        entity_id: Some(id),
        name: e.map(|e| e.name.clone()),
        kind: e.map(|e| e.kind.clone()),
        step,
        score: Some(score),
        reason: None,
        candidates: vec![],
    }
}

fn unresolved(reason: Reason, mut candidates: Vec<(i64, f32)>) -> Resolution {
    candidates.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    candidates.truncate(5);
    Resolution {
        entity_id: None,
        name: None,
        kind: None,
        step: Step::Unresolved,
        score: None,
        reason: Some(reason),
        candidates,
    }
}

fn kind_ok(ix: &Index, id: i64, kind: Option<&str>) -> bool {
    match (kind, ix.entities.get(&id)) {
        (Some(k), Some(e)) => e.kind == k,
        (None, Some(_)) => true,
        (_, None) => false,
    }
}

/// Identifier schemes Roster knows (Bureau §3.5). Anything else before a colon is just text
/// ("NASA: Artemis launch").
const SCHEMES: &[&str] = &[
    "cik", "lei", "iso3166", "iso4217", "ror", "wikidata", "figi", "cb",
];

/// `scheme:value` for a known scheme, or `ticker@<MIC>:<symbol>`.
fn as_identifier(text: &str) -> Option<(&str, &str)> {
    let (s, v) = text.split_once(':')?;
    let known = SCHEMES.contains(&s) || s.strip_prefix("ticker@").is_some_and(|m| !m.is_empty());
    (known && !v.trim().is_empty()).then_some((s, v.trim()))
}

/// Resolve `text` to one entity, or say why not. `kind` is the kind the caller expects.
pub fn resolve(ix: &Index, text: &str, kind: Option<&str>) -> Resolution {
    // 1. Identifier.
    if let Some((scheme, value)) = as_identifier(text.trim()) {
        if let Some(id) = ix
            .ids
            .get(&(scheme.to_owned(), canonical_id(scheme, value)))
            && kind_ok(ix, *id, kind)
        {
            return resolved(ix, *id, Step::Identifier, 1.0);
        }
        return unresolved(Reason::NoMatch, vec![]);
    }
    let norm = normalize(text);
    if norm.is_empty() {
        return unresolved(Reason::NoMatch, vec![]);
    }
    // 2. Exact alias.
    if let Some(set) = ix.aliases.get(&norm) {
        let hits: Vec<i64> = set
            .iter()
            .copied()
            .filter(|i| kind_ok(ix, *i, kind))
            .collect();
        match hits.as_slice() {
            [one] => return resolved(ix, *one, Step::Alias, 1.0),
            [] => {}
            many => {
                return unresolved(Reason::Ambiguous, many.iter().map(|i| (*i, 1.0)).collect());
            }
        }
    }
    // 3. Gazetteer scan: only inside a longer text.
    let toks = tokens(text);
    if toks.len() > 1 {
        let mut spans: Vec<(usize, usize, BTreeSet<i64>)> = Vec::new();
        // Short single words are never evidence on their own, but they can show that the text names
        // a second entity, in which case one trusted match is not the whole story.
        let mut weak: BTreeSet<i64> = BTreeSet::new();
        for start in 0..toks.len() {
            for len in 1..=MAX_SPAN.min(toks.len() - start) {
                let phrase = toks[start..start + len].join(" ");
                if len == 1 && phrase.chars().count() < MIN_SOLO_LEN {
                    if let Some(set) = ix.aliases.get(&phrase) {
                        weak.extend(set.iter().copied().filter(|i| kind_ok(ix, *i, kind)));
                    }
                    continue;
                }
                if let Some(set) = ix.aliases.get(&phrase) {
                    let hit: BTreeSet<i64> = set
                        .iter()
                        .copied()
                        .filter(|i| kind_ok(ix, *i, kind))
                        .collect();
                    if !hit.is_empty() {
                        spans.push((start, start + len, hit));
                    }
                }
            }
        }
        // Longest match wins: drop a span wholly inside a longer one.
        let keep: Vec<&(usize, usize, BTreeSet<i64>)> = spans
            .iter()
            .filter(|(s, e, _)| {
                !spans
                    .iter()
                    .any(|(s2, e2, _)| (*s2 <= *s && *e2 >= *e) && (e2 - s2) > (e - s))
            })
            .collect();
        let entities: BTreeSet<i64> = keep
            .iter()
            .flat_map(|(_, _, h)| h.iter().copied())
            .collect();
        match entities.len() {
            0 => {}
            1 => {
                if let Some(id) = entities.iter().next() {
                    if weak.iter().any(|w| w != id) {
                        let all: BTreeSet<i64> = entities.union(&weak).copied().collect();
                        return unresolved(
                            Reason::Ambiguous,
                            all.iter().map(|i| (*i, 1.0)).collect(),
                        );
                    }
                    return resolved(ix, *id, Step::Gazetteer, 1.0);
                }
            }
            _ => {
                return unresolved(
                    Reason::Ambiguous,
                    entities.iter().map(|i| (*i, 1.0)).collect(),
                );
            }
        }
    }
    // 4. Fuzzy.
    let mut best: BTreeMap<i64, f32> = BTreeMap::new();
    for (alias, set) in &ix.aliases {
        let s = similarity(&norm, alias);
        if s < CANDIDATE_MIN {
            continue;
        }
        for id in set.iter().filter(|i| kind_ok(ix, **i, kind)) {
            let e = best.entry(*id).or_insert(0.0);
            if s > *e {
                *e = s;
            }
        }
    }
    let mut ranked: Vec<(i64, f32)> = best.into_iter().collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    match ranked.as_slice() {
        [] => unresolved(Reason::NoMatch, vec![]),
        [(id, s)] if *s >= FUZZY_MIN => resolved(ix, *id, Step::Fuzzy, *s),
        [(id, s), (_, s2), ..] if *s >= FUZZY_MIN && *s - *s2 >= FUZZY_MARGIN => {
            resolved(ix, *id, Step::Fuzzy, *s)
        }
        [(_, s), ..] if *s >= FUZZY_MIN => unresolved(Reason::Ambiguous, ranked.clone()),
        _ => unresolved(Reason::BelowThreshold, ranked.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ix() -> Index {
        let mut ix = Index::default();
        for (id, kind, name) in [
            (1, "company", "Apple Inc."),
            (2, "company", "Apple Hospitality REIT"),
            (3, "central_bank", "Bank of England"),
            (4, "central_bank", "Bank of Canada"),
            (5, "country", "China"),
            (6, "company", "Delta Air Lines"),
            (7, "company", "Delta Apparel"),
        ] {
            ix.add_entity(Entity {
                id,
                kind: kind.into(),
                name: name.into(),
                prominence: 3,
            });
            ix.add_alias(id, name);
        }
        ix.add_alias(5, "Chinese");
        ix.add_alias(6, "Delta");
        ix.add_alias(7, "Delta");
        ix.add_id(1, "cik", "0000320193");
        ix.add_id(5, "iso3166", "CN");
        ix
    }

    #[test]
    fn each_step_resolves_what_it_should() {
        let ix = ix();
        let r = resolve(&ix, "cik:320193", None);
        assert_eq!((r.entity_id, r.step), (Some(1), Step::Identifier));
        let r = resolve(&ix, "iso3166:cn", Some("country"));
        assert_eq!(r.entity_id, Some(5));
        let r = resolve(&ix, "APPLE INC", None);
        assert_eq!((r.entity_id, r.step), (Some(1), Step::Alias));
        let r = resolve(&ix, "Chinese", Some("country"));
        assert_eq!(r.entity_id, Some(5));
        let r = resolve(
            &ix,
            "Bank of England raises rates again",
            Some("central_bank"),
        );
        assert_eq!((r.entity_id, r.step), (Some(3), Step::Gazetteer));
        let r = resolve(&ix, "Bank of Englnd", None);
        assert_eq!((r.entity_id, r.step), (Some(3), Step::Fuzzy));
    }

    #[test]
    fn ambiguity_and_collisions_stay_unresolved() {
        let ix = ix();
        let r = resolve(&ix, "Delta", None);
        assert_eq!(
            (r.step, r.reason),
            (Step::Unresolved, Some(Reason::Ambiguous))
        );
        assert_eq!(r.candidates.len(), 2);
        // The expected kind does not help here (both are companies): still unresolved.
        assert_eq!(
            resolve(&ix, "Delta", Some("company")).step,
            Step::Unresolved
        );
        // "Bank of …" collisions: a bank we do not know is not the nearest one.
        let r = resolve(&ix, "Bank of Chile", Some("central_bank"));
        assert_eq!(r.step, Step::Unresolved);
        // A text naming two entities is not one entity.
        let r = resolve(&ix, "Bank of England and Bank of Canada meet", None);
        assert_eq!(r.reason, Some(Reason::Ambiguous));
        // Unknown identifier: no match, never a guess.
        assert_eq!(resolve(&ix, "cik:999", None).step, Step::Unresolved);
        assert_eq!(resolve(&ix, "", None).reason, Some(Reason::NoMatch));
    }

    #[test]
    fn longest_span_wins_and_short_words_are_not_trusted_inside_text() {
        let mut ix = ix();
        ix.add_entity(Entity {
            id: 9,
            kind: "company".into(),
            name: "ING".into(),
            prominence: 3,
        });
        ix.add_alias(9, "ING");
        // "ing" alone resolves; inside a sentence a 3-letter word is not evidence.
        assert_eq!(resolve(&ix, "ING", None).entity_id, Some(9));
        assert_eq!(
            resolve(&ix, "the ING of the matter", None).step,
            Step::Unresolved
        );
        // "Apple Hospitality" beats the shorter "Apple" inside the same text.
        let r = resolve(&ix, "Apple Hospitality REIT reports results", None);
        assert_eq!(r.entity_id, Some(2));
    }
}
