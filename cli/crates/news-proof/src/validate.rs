//! The Copy (`page-draft.v1`, Bureau Design §3.9) and its validators, run by `news ai accept` and `news save`:
//! JSON schema → OpenUI Lang parses → every ref exists in the brief → numeric-literal lint (C4) → banned words →
//! props against the component specs → lengths. Parsing and props need the component library, so they run in
//! Plate and arrive here as a [`Checked`]: its errors, and the literals the parser read, which the lints check.

use crate::lint;
use news_core::brief::Brief;
use news_core::refs::is_ref;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::OnceLock;

const SCHEMA: &str = include_str!("../../../../contracts/cli/ai/page-draft.v1.schema.json");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Copy {
    pub headline: String,
    pub fly_title: String,
    pub summary: String,
    pub oui: String,
    pub refs_used: Vec<String>,
    pub components_used: Vec<String>,
    pub angle: String,
}

/// One validator failure, as `news ai accept` returns it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleError {
    pub rule: String,
    #[serde(rename = "where")]
    pub at: String,
    pub message: String,
}

impl RuleError {
    pub fn new(rule: &str, at: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            rule: rule.to_string(),
            at: at.into(),
            message: message.into(),
        }
    }
}

/// Brand lengths (news-brand skill, "Every page has"), measured with each reference replaced by its brief text.
pub const HEADLINE_MAX: usize = 80;
pub const SUMMARY_MAX: usize = 200;
pub const FLY_TITLE_WORDS: std::ops::RangeInclusive<usize> = 2..=4;

fn validator() -> Option<&'static jsonschema::Validator> {
    static V: OnceLock<Option<jsonschema::Validator>> = OnceLock::new();
    V.get_or_init(|| {
        let schema: Value = serde_json::from_str(SCHEMA).ok()?;
        jsonschema::draft202012::new(&schema).ok()
    })
    .as_ref()
}

/// Schema errors of a Copy (empty when it is a valid page-draft.v1).
pub fn schema_errors(copy: &Value) -> Vec<RuleError> {
    let Some(validator) = validator() else {
        return vec![RuleError::new(
            "schema",
            "/",
            "page-draft.v1 does not compile",
        )];
    };
    validator
        .iter_errors(copy)
        .map(|e| {
            let at = e.instance_path().to_string();
            RuleError::new(
                "schema",
                if at.is_empty() { "/".into() } else { at },
                e.to_string(),
            )
        })
        .collect()
}

/// A literal of the page's OpenUI Lang as Plate's parser read it: a string's text, or a typed number.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Literal {
    #[serde(rename = "where")]
    pub at: String,
    pub text: String,
    pub number: bool,
}

/// Plate's `/check`: the parse and props errors, and every literal its parser read.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Checked {
    pub errors: Vec<RuleError>,
    pub literals: Vec<Literal>,
}

impl From<OuiToken> for Literal {
    fn from(t: OuiToken) -> Self {
        let (line, text, number) = match t {
            OuiToken::Text { line, text } => (line, text, false),
            OuiToken::Number { line, text } => (line, text, true),
        };
        Literal {
            at: format!("oui:{line}"),
            text,
            number,
        }
    }
}

/// A piece of OpenUI Lang source: a string literal's text, or a number typed outside any string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OuiToken {
    Text { line: usize, text: String },
    Number { line: usize, text: String },
}

/// The string literals and number literals of `.oui` source. Identifiers (statement names, `true`, `null`) are
/// skipped; a number outside a string is always typed by the writer.
pub fn oui_tokens(oui: &str) -> Vec<OuiToken> {
    let mut out = Vec::new();
    let mut chars = oui.chars().peekable();
    let mut line = 1;
    while let Some(c) = chars.next() {
        match c {
            '\n' => line += 1,
            '"' | '\'' => {
                let start = line;
                let mut text = String::new();
                while let Some(d) = chars.next() {
                    match d {
                        '\\' => {
                            if let Some(e) = chars.next() {
                                text.push(match e {
                                    'n' => '\n',
                                    't' => '\t',
                                    other => other,
                                });
                            }
                        }
                        d if d == c => break,
                        d => {
                            if d == '\n' {
                                line += 1;
                            }
                            text.push(d);
                        }
                    }
                }
                out.push(OuiToken::Text { line: start, text });
            }
            c if c.is_alphabetic() || c == '_' => {
                while chars
                    .next_if(|d| d.is_alphanumeric() || *d == '_')
                    .is_some()
                {}
            }
            c if c.is_numeric() => {
                let mut text = c.to_string();
                while let Some(d) = chars.next_if(|d| d.is_alphanumeric() || *d == '.') {
                    text.push(d);
                }
                out.push(OuiToken::Number { line, text });
            }
            _ => {}
        }
    }
    out
}

/// Every reference a text cites: `{ref}` tokens and a whole string that is a reference (a component prop).
pub fn refs_in(text: &str) -> Vec<String> {
    if is_ref(text) {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        rest = &rest[open + 1..];
        if let Some(close) = rest.find('}') {
            let inner = rest[..close].trim();
            if is_ref(inner) {
                out.push(inner.to_string());
            }
        }
    }
    out
}

/// The texts of a Copy the lints read, with where each one is.
fn texts(copy: &Copy, literals: &[Literal]) -> Vec<(String, String)> {
    let mut out = vec![
        ("headline".to_string(), copy.headline.clone()),
        ("fly_title".to_string(), copy.fly_title.clone()),
        ("summary".to_string(), copy.summary.clone()),
    ];
    for l in literals.iter().filter(|l| !l.number) {
        out.push((l.at.clone(), l.text.clone()));
    }
    out
}

/// Each reference the brief issues, with the text it stands for (a fact's value, a label, a line or a quote).
fn ref_texts(brief: &Brief) -> BTreeMap<&str, &str> {
    let facts = brief.facts.iter().map(|f| (&f.reference, &f.value));
    let series = brief.series.iter().map(|s| (&s.reference, &s.label));
    let periods = brief.periods.iter().map(|p| (&p.reference, &p.label));
    let context = brief.context.iter().map(|c| (&c.id, &c.text));
    let quotes = brief.quotes.iter().map(|q| (&q.id, &q.text));
    facts
        .chain(series)
        .chain(periods)
        .chain(context)
        .chain(quotes)
        .map(|(r, t)| (r.as_str(), t.as_str()))
        .collect()
}

/// `text` with each `{ref}` replaced by the brief's text for it: about what the reader sees.
fn rendered_estimate(text: &str, brief: Option<&Brief>) -> String {
    let refs = brief.map(ref_texts).unwrap_or_default();
    let mut out = text.to_string();
    for r in refs_in(text) {
        let shown = refs.get(r.as_str()).copied().unwrap_or(r.as_str());
        out = out.replacen(&format!("{{{r}}}"), shown, 1);
    }
    out
}

/// Run the validators on a Copy. `plate` is Plate's check of its OpenUI Lang; `None` skips parse and props.
/// Without a brief, references are not checked against it.
pub fn validate(copy: &Value, brief: Option<&Brief>, plate: Option<&Checked>) -> Vec<RuleError> {
    let mut errors = schema_errors(copy);
    if !errors.is_empty() {
        return errors;
    }
    let Ok(copy) = serde_json::from_value::<Copy>(copy.clone()) else {
        return vec![RuleError::new("schema", "/", "not a page-draft.v1 Copy")];
    };
    let (parse, props): (Vec<RuleError>, Vec<RuleError>) = plate
        .map(|p| p.errors.clone())
        .unwrap_or_default()
        .into_iter()
        .partition(|e| e.rule != "props");
    errors.extend(parse);

    // shortcut: without Plate (`news save`) a local lexer stands in for the parser; `news ai accept` and
    // `news render` lint exactly what Plate parsed.
    let literals: Vec<Literal> = match plate {
        Some(p) => p.literals.clone(),
        None => oui_tokens(&copy.oui)
            .into_iter()
            .map(Literal::from)
            .collect(),
    };
    let texts = texts(&copy, &literals);
    if let Some(brief) = brief {
        let known = brief.refs();
        let cited = texts
            .iter()
            .flat_map(|(at, text)| refs_in(text).into_iter().map(move |r| (at.clone(), r)))
            .chain(
                copy.refs_used
                    .iter()
                    .enumerate()
                    .map(|(i, r)| (format!("refs_used[{i}]"), r.clone())),
            );
        for (at, r) in cited {
            if !known.contains(r.as_str()) {
                errors.push(RuleError::new(
                    "refs",
                    at,
                    format!("{r} is not in the brief"),
                ));
            }
        }
    }

    let names: Vec<&str> = brief
        .map(|b| b.entities.iter().map(|e| e.name.as_str()).collect())
        .unwrap_or_default();
    for (at, text) in &texts {
        let found = lint::numeric_literals(text, &names);
        if !found.is_empty() {
            errors.push(RuleError::new(
                "numeric_literal",
                at.clone(),
                format!(
                    "typed number {:?}: write values, periods and dates as {{ref}} tokens",
                    found.join(", ")
                ),
            ));
        }
    }
    for l in literals.iter().filter(|l| l.number) {
        errors.push(RuleError::new(
            "numeric_literal",
            l.at.clone(),
            format!(
                "typed number {:?}: component props take references, not numbers",
                l.text
            ),
        ));
    }

    for (at, text) in &texts {
        let found = lint::banned_words(text);
        if !found.is_empty() {
            errors.push(RuleError::new(
                "banned_word",
                at.clone(),
                format!(
                    "banned word {:?} (brand rules: no hype words)",
                    found.join(", ")
                ),
            ));
        }
    }

    errors.extend(props);

    let headline = rendered_estimate(&copy.headline, brief).chars().count();
    if headline > HEADLINE_MAX {
        errors.push(RuleError::new(
            "length",
            "headline",
            format!("about {headline} characters as shown; at most {HEADLINE_MAX}"),
        ));
    }
    let summary = rendered_estimate(&copy.summary, brief).chars().count();
    if summary > SUMMARY_MAX {
        errors.push(RuleError::new(
            "length",
            "summary",
            format!("about {summary} characters as shown; at most {SUMMARY_MAX}"),
        ));
    }
    let words = copy.fly_title.split_whitespace().count();
    if !FLY_TITLE_WORDS.contains(&words) {
        errors.push(RuleError::new(
            "length",
            "fly_title",
            format!("{words} words; a fly title has 2 to 4"),
        ));
    }
    errors
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    pub(crate) fn brief() -> Brief {
        serde_json::from_value(json!({
            "brief_version": 1,
            "item": {"story_id": 1, "event_key": null, "page_type": "macro", "tier": 2, "action": "new", "slug_hint": "us-cpi-october-2026"},
            "entities": [{"id": "ent_us", "name": "United States", "kind": "country"}],
            "facts": [{"ref": "dp_7kq2", "label": "CPI-U", "value": "2.4", "unit": "Percent", "period": "October 2026"},
                      {"ref": "dv_3ab8", "label": "Change", "value": "-0.3", "unit": "pp"}],
            "series": [{"ref": "sr_cpi", "label": "CPI-U 12-month change", "points": 60}],
            "periods": [{"ref": "pd_2026_10", "label": "October 2026"}],
            "context": [], "quotes": [], "source_text": {"untrusted": true, "points": []},
            "components": {"suggested": []}, "rules": {"disclaimer": null, "fiscal_note": null}
        }))
        .unwrap()
    }

    pub(crate) fn clean() -> Value {
        json!({
            "headline": "Annual inflation in America fell to {dp_7kq2} in {pd_2026_10}",
            "fly_title": "America's inflation",
            "summary": "Consumer prices rose by {dp_7kq2} on a year earlier, a change of {dv_3ab8} on the month before.",
            "oui": "root = Page([figs])\nfigs = KeyFigures([a, b])\na = KeyFigure(\"Inflation\", \"dp_7kq2\")\nb = KeyFigure(\"Change in {pd_2026_10}\", \"dv_3ab8\")",
            "refs_used": ["dp_7kq2", "dv_3ab8", "pd_2026_10"],
            "components_used": ["KeyFigures", "KeyFigure"],
            "angle": "change"
        })
    }

    fn with(field: &str, value: Value) -> Value {
        let mut c = clean();
        c[field] = value;
        c
    }

    fn rules(errors: &[RuleError]) -> Vec<&str> {
        errors.iter().map(|e| e.rule.as_str()).collect()
    }

    #[test]
    fn the_contract_compiles() {
        assert!(validator().is_some());
    }

    #[test]
    fn accepts_a_clean_copy() {
        assert_eq!(
            validate(&clean(), Some(&brief()), Some(&Checked::default())),
            []
        );
        // Without a brief the brief-based checks are skipped, and the rest still pass.
        assert_eq!(validate(&clean(), None, None), []);
    }

    #[test]
    fn rejects_a_copy_that_breaks_the_schema() {
        let mut c = clean();
        c.as_object_mut().unwrap().remove("angle");
        assert_eq!(rules(&validate(&c, Some(&brief()), None)), ["schema"]);
        assert_eq!(
            rules(&validate(&with("headline", json!("short")), None, None)),
            ["schema"]
        );
        assert_eq!(
            rules(&validate(&with("refs_used", json!(["7kq2"])), None, None)),
            ["schema"]
        );
    }

    #[test]
    fn rejects_digits_and_written_quantities_in_prose() {
        let e = validate(
            &with(
                "headline",
                json!("Annual inflation in America fell to 2.4% in October"),
            ),
            Some(&brief()),
            None,
        );
        assert_eq!(rules(&e), ["numeric_literal"]);
        assert_eq!(e[0].at, "headline");
        let e = validate(
            &with(
                "summary",
                json!(
                    "Consumer prices rose by three per cent on a year earlier, and food prices doubled."
                ),
            ),
            Some(&brief()),
            None,
        );
        assert_eq!(rules(&e), ["numeric_literal"]);
        assert!(e[0].message.contains("three") && e[0].message.contains("doubled"));
        // In .oui: a string prop and a number typed as an argument.
        let oui = "root = Page([c])\nc = LineChart(\"Prices rose in 2024\", \"CPI, %\", [\"sr_cpi\"], null, null, 3)";
        let e = validate(&with("oui", json!(oui)), Some(&brief()), None);
        assert_eq!(rules(&e), ["numeric_literal", "numeric_literal"]);
        assert_eq!((e[0].at.as_str(), e[1].at.as_str()), ("oui:2", "oui:2"));
        // Entity names with digits that the brief resolved are not numbers.
        let mut b = brief();
        b.entities.push(news_core::brief::Entity {
            id: "c".into(),
            name: "3M".into(),
            kind: "company".into(),
        });
        let h = json!("Sales at 3M rose to {dp_7kq2} in {pd_2026_10} and more");
        assert_eq!(validate(&with("headline", h.clone()), Some(&b), None), []);
        assert_eq!(
            rules(&validate(&with("headline", h), Some(&brief()), None)),
            ["numeric_literal"]
        );
    }

    #[test]
    fn rejects_unknown_refs() {
        let e = validate(
            &with(
                "headline",
                json!("Annual inflation in America fell to {dp_nope} in {pd_2026_10}"),
            ),
            Some(&brief()),
            None,
        );
        assert_eq!(rules(&e), ["refs"]);
        assert_eq!(e[0].at, "headline");
        let e = validate(
            &with("refs_used", json!(["dp_7kq2", "sr_gone"])),
            Some(&brief()),
            None,
        );
        assert_eq!(
            (e[0].rule.as_str(), e[0].at.as_str()),
            ("refs", "refs_used[1]")
        );
        let oui = "root = Page([a])\na = KeyFigure(\"Inflation\", \"dp_other\")";
        let e = validate(&with("oui", json!(oui)), Some(&brief()), None);
        assert_eq!((e[0].rule.as_str(), e[0].at.as_str()), ("refs", "oui:2"));
    }

    #[test]
    fn rejects_banned_words() {
        let e = validate(
            &with(
                "headline",
                json!("Annual inflation in America plunged to {dp_7kq2} in {pd_2026_10}"),
            ),
            Some(&brief()),
            None,
        );
        assert_eq!(rules(&e), ["banned_word"]);
    }

    #[test]
    fn passes_plate_parse_and_props_errors_through_in_order() {
        let plate = Checked {
            errors: vec![
                RuleError::new("props", "oui:a.value", "Invalid string"),
                RuleError::new("openui_parse", "oui", "x is not defined"),
            ],
            literals: vec![],
        };
        let e = validate(
            &with(
                "headline",
                json!("Annual inflation soared to {dp_7kq2} in {pd_2026_10}"),
            ),
            Some(&brief()),
            Some(&plate),
        );
        assert_eq!(rules(&e), ["openui_parse", "banned_word", "props"]);
    }

    #[test]
    fn lints_the_literals_plate_parsed_not_a_second_lexer() {
        let oui = "root = Page([c])
c = LineChart(\"Prices rose in 2024\", \"CPI, %\", [\"sr_cpi\"], null, null, 3)";
        let copy = with("oui", json!(oui));
        // Plate read no literal (say, the lexer saw a string the parser does not keep): nothing to lint.
        assert_eq!(
            validate(&copy, Some(&brief()), Some(&Checked::default())),
            []
        );
        let lit = |text: &str, number| Literal {
            at: "oui:c".into(),
            text: text.into(),
            number,
        };
        let parsed = Checked {
            errors: vec![],
            literals: vec![lit("Prices rose in 2024", false), lit("3", true)],
        };
        let e = validate(&copy, Some(&brief()), Some(&parsed));
        assert_eq!(rules(&e), ["numeric_literal", "numeric_literal"]);
        assert_eq!((e[0].at.as_str(), e[1].at.as_str()), ("oui:c", "oui:c"));
    }

    #[test]
    fn checks_lengths_as_shown() {
        let long = format!("Inflation {} {{dp_7kq2}}", "word ".repeat(15));
        assert_eq!(
            rules(&validate(
                &with("headline", json!(long)),
                Some(&brief()),
                None
            )),
            ["length"]
        );
        assert_eq!(
            rules(&validate(
                &with("fly_title", json!("Inflation")),
                Some(&brief()),
                None
            )),
            ["length"]
        );
    }

    #[test]
    fn lexes_oui_strings_and_numbers() {
        let t = oui_tokens("a = X(\"say \\\"hi\\\"\", 'it''s', 12.5, b2, null)\nb2 = Y(\"x\")");
        assert_eq!(
            t,
            [
                OuiToken::Text {
                    line: 1,
                    text: "say \"hi\"".into()
                },
                OuiToken::Text {
                    line: 1,
                    text: "it".into()
                },
                OuiToken::Text {
                    line: 1,
                    text: "s".into()
                },
                OuiToken::Number {
                    line: 1,
                    text: "12.5".into()
                },
                OuiToken::Text {
                    line: 2,
                    text: "x".into()
                },
            ]
        );
        assert_eq!(refs_in("dp_a"), ["dp_a"]);
        assert_eq!(refs_in("to {dp_a} in { pd_b } {nope} {"), ["dp_a", "pd_b"]);
    }
}
