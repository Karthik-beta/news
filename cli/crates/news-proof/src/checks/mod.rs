//! The checks C1-C14 (Bureau Design §3.10) and the runner that produces the Markup.

mod net;
mod page;
mod prose;
mod refs;

use serde::Serialize;
use std::collections::BTreeMap;

use crate::axe::AxeRunner;
use crate::bundle::Bundle;
use crate::copy::{self, Text};
use crate::lexicon::Lexicon;
use crate::links::LinkChecker;

/// The checks in order, with the short name the Markup shows.
pub const CHECKS: [(&str, &str); 14] = [
    ("C1", "Parse and render"),
    ("C2", "References resolve"),
    ("C3", "Values match source"),
    ("C4", "Numeric-literal lint"),
    ("C5", "Direction words"),
    ("C6", "Quotes verbatim"),
    ("C7", "Period and unit labels"),
    ("C8", "Licence"),
    ("C9", "Required fields"),
    ("C10", "Links and media"),
    ("C11", "Banned words and tone"),
    ("C12", "Size and structure"),
    ("C13", "Accessibility"),
    ("C14", "Duplicate page"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Block,
    Warn,
}

/// One problem a check found.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub check: &'static str,
    pub severity: Severity,
    pub at: String,
    pub message: String,
}

impl Finding {
    pub fn block(check: &'static str, at: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            check,
            severity: Severity::Block,
            at: at.into(),
            message: message.into(),
        }
    }
    pub fn warn(check: &'static str, at: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warn,
            ..Self::block(check, at, message)
        }
    }
}

/// `failures[]` and `warnings[]` entries of the Markup (`contracts/cli/commands/check.schema.json`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Failure {
    pub check: String,
    #[serde(rename = "where")]
    pub at: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CheckStatus {
    pub check: String,
    pub name: String,
    /// `pass`, `fail`, `warn` or `skipped`.
    pub status: String,
    /// Why a check was skipped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The Markup: the check report for one page (§3.10, `news check`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Markup {
    pub page_id: String,
    pub pass: bool,
    pub failures: Vec<Failure>,
    pub warnings: Vec<Failure>,
    pub checks: Vec<CheckStatus>,
}

/// An already published (or ready) page that clashes with this one (C14).
#[derive(Debug, Clone, PartialEq)]
pub struct Duplicate {
    pub page_id: String,
    pub slug: String,
    /// `slug` or `event key` (the story already has a page).
    pub reason: String,
}

/// When axe runs (C13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxeMode {
    /// Every Tier 1 page and one Tier 2 page in `tier2_one_in`.
    Auto,
    Off,
}

/// Everything a check may need from outside the bundle.
pub struct Env<'a> {
    pub lexicon: &'a Lexicon,
    /// Stored text blocks by (document, block) for the quotes' locators (C6).
    pub text_blocks: &'a BTreeMap<(i64, i64), String>,
    /// Pages that clash with this one, or `None` when the page index could not be read (C14 is skipped).
    pub duplicates: Option<&'a [Duplicate]>,
    /// `None` skips C10.
    pub links: Option<&'a dyn LinkChecker>,
    pub axe: &'a dyn AxeRunner,
    pub axe_mode: AxeMode,
}

/// What every check reads.
pub(crate) struct Ctx<'a> {
    pub b: &'a Bundle,
    pub lex: &'a Lexicon,
    pub env: &'a Env<'a>,
    /// The page writer's text: every string literal of `page.oui`, then the head text of `meta.json`.
    pub copy: Vec<Text>,
    /// Every reference the Copy uses, with where.
    pub refs: Vec<(String, String)>,
}

/// A check that may not have run: `Err(reason)` is "skipped".
pub(crate) type Run = Result<Vec<Finding>, String>;

pub fn run(b: &Bundle, env: &Env) -> Markup {
    let mut copy = copy::string_literals(&b.oui).unwrap_or_default();
    copy::mark_media(&b.oui, &mut copy);
    copy.extend(
        b.head_text()
            .into_iter()
            .map(|(k, text)| Text::new(format!("meta.json:{k}"), text)),
    );
    let refs = copy::refs_in(&copy);
    let cx = Ctx {
        b,
        lex: env.lexicon,
        env,
        copy,
        refs,
    };
    let runs: [Run; 14] = [
        Ok(refs::c1(&cx)),
        Ok(refs::c2(&cx)),
        Ok(refs::c3(&cx)),
        Ok(prose::c4(&cx)),
        Ok(prose::c5(&cx)),
        Ok(page::c6(&cx)),
        Ok(refs::c7(&cx)),
        Ok(page::c8(&cx)),
        Ok(page::c9(&cx)),
        net::c10(&cx),
        Ok(prose::c11(&cx)),
        Ok(page::c12(&cx)),
        net::c13(&cx),
        page::c14(&cx),
    ];
    let mut failures = Vec::new();
    let mut warnings = Vec::new();
    let mut checks = Vec::new();
    for ((id, name), run) in CHECKS.iter().zip(runs) {
        let (status, note) = match run {
            Err(why) => ("skipped", Some(why)),
            Ok(found) => {
                let blocked = found.iter().any(|f| f.severity == Severity::Block);
                let status = if blocked {
                    "fail"
                } else if found.is_empty() {
                    "pass"
                } else {
                    "warn"
                };
                for f in found {
                    let entry = Failure {
                        check: f.check.to_owned(),
                        at: f.at,
                        message: f.message,
                    };
                    match f.severity {
                        Severity::Block => failures.push(entry),
                        Severity::Warn => warnings.push(entry),
                    }
                }
                (status, None)
            }
        };
        checks.push(CheckStatus {
            check: (*id).to_owned(),
            name: (*name).to_owned(),
            status: status.to_owned(),
            note,
        });
    }
    Markup {
        page_id: b.page_id(),
        pass: failures.is_empty(),
        failures,
        warnings,
        checks,
    }
}
