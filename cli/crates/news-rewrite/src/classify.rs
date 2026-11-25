//! Classifying a revision (Bureau §3.11, step 3). Pure: exact decimals, no I/O, no AI.
//!
//! * **Direction flip**: a referenced value, or a derived change, crosses zero (negative, zero and
//!   positive are three different directions), or a derived change can no longer be computed (a guard
//!   refuses it), so a sentence about it may no longer be true.
//! * **Restatement**: a value that was an earnings fact (it carries a fiscal period) now has a later
//!   vintage with a different value.
//! * **Value only**: everything else; the prose holds no numbers, so only the data changes.
//!
//! shortcut: "a context line stops being true" and "C5 would now fail" are not checked here: a
//! page's `cx_` refs do not say which series they describe. Proof's C5 still blocks a re-rendered page
//! whose direction words no longer match (the revision then waits as `check_failed`); upgrade to
//! re-running Fixer's context rules when that shows up in Followup's Wraps.

use news_ledger::decimal::{Dec, parse};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    ValueOnly,
    Restatement,
    DirectionFlip,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::ValueOnly => "value_only",
            Kind::Restatement => "restatement",
            Kind::DirectionFlip => "direction_flip",
        }
    }
}

/// The exact decimal of a value as published, or `None` for a missing marker or text that is not a number.
pub fn dec(text: &str) -> Option<Dec> {
    let (numeric, _) = parse(text).ok()?;
    Dec::from_numeric(&numeric)
}

/// -1, 0 or 1.
pub fn sign(text: &str) -> Option<i8> {
    dec(text).map(|d| d.m.signum() as i8)
}

/// Whether the direction differs between two values. Unreadable values never count as a flip (they
/// are `value_only` problems for Proof's data checks).
pub fn crosses_zero(old: &str, new: &str) -> bool {
    matches!((sign(old), sign(new)), (Some(a), Some(b)) if a != b)
}

/// The derived value for `formula` over `a` and `b`, exactly as the Ledger's guard computes it
/// (`(a - b) / b × 100`, `a - b`, `a / total × 100`), rounded half away from zero to `rounding` places.
/// `None` when an input is not a number or the arithmetic is refused (division by zero).
pub fn recompute(formula: &str, rounding: u32, a: &str, b: &str) -> Option<String> {
    let (a, b) = (dec(a)?, dec(b)?);
    let v = match formula {
        "pct_change(a,b)" => a.checked_sub(b)?.ratio_pct(b, rounding)?,
        "pp_change(a,b)" | "diff(a,b)" => a.checked_sub(b)?.round(rounding)?,
        "share(a,total)" => a.ratio_pct(b, rounding)?,
        _ => return None,
    };
    Some(v.to_numeric())
}

/// What one page's revisions add up to.
pub fn classify(page: &crate::find::PageChange) -> Kind {
    let flip = page
        .points
        .iter()
        .any(|p| crosses_zero(&p.old_text, &p.new_text))
        || page.derived.iter().any(|d| match &d.new_value {
            Some(new) => crosses_zero(&d.old_value, new),
            None => true,
        });
    if flip {
        Kind::DirectionFlip
    } else if page.points.iter().any(|p| p.restated) {
        Kind::Restatement
    } else {
        Kind::ValueOnly
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_its_own_direction() {
        for (old, new, flip) in [
            ("2.9", "2.7", false),
            ("-0.3", "-0.4", false),
            ("-0.1", "0.2", true),
            ("0.2", "-0.1", true),
            ("0", "0.1", true),
            ("0.0", "0", false),
            ("1,234.5", "1,300.0", false),
            ("n/a", "1", false),
        ] {
            assert_eq!(crosses_zero(old, new), flip, "{old} -> {new}");
        }
    }

    #[test]
    fn derived_values_are_exact_and_rounded_half_away_from_zero() {
        assert_eq!(
            recompute("pp_change(a,b)", 1, "2.4", "2.7").as_deref(),
            Some("-0.3")
        );
        assert_eq!(
            recompute("pct_change(a,b)", 2, "110", "100").as_deref(),
            Some("10.00")
        );
        assert_eq!(
            recompute("share(a,total)", 1, "1", "3").as_deref(),
            Some("33.3")
        );
        assert_eq!(recompute("pct_change(a,b)", 1, "1", "0"), None);
        assert_eq!(recompute("diff(a,b)", 1, "x", "1"), None);
        assert_eq!(recompute("nope", 1, "1", "1"), None);
    }
}
