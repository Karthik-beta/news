//! The revision plan: what Edition step 4 budgets and Followup works through (Bureau §3.11, §8.3, §8.9).

use crate::classify::{Kind, classify};
use crate::find::PageChange;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// Pages one run may revise without a `bulk` approval (Bureau §8.3 step 4, §8.9).
pub const BULK: usize = 100;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PagePlan {
    pub change: PageChange,
    pub kind: Kind,
}

impl PagePlan {
    /// Re-rendered by code, no AI: value only, or a restatement (which also gets the "restated" note).
    pub fn by_code(&self) -> bool {
        self.kind != Kind::DirectionFlip
    }
}

/// Classify every change. Pages come out in page order.
pub fn plan(changes: Vec<PageChange>) -> Vec<PagePlan> {
    let mut pages: Vec<PagePlan> = changes
        .into_iter()
        .map(|change| PagePlan {
            kind: classify(&change),
            change,
        })
        .collect();
    pages.sort_by(|a, b| a.change.page_id.cmp(&b.change.page_id));
    pages
}

/// The plan's counts, the part of a Wrap and of Board's plan summary that comes from Rewrite.
pub fn counts(pages: &[PagePlan]) -> Value {
    let n = |k: Kind| pages.iter().filter(|p| p.kind == k).count();
    json!({
        "pages": pages.len(),
        "value_only": n(Kind::ValueOnly),
        "restatement": n(Kind::Restatement),
        "direction_flip": n(Kind::DirectionFlip),
        "needs_ai": n(Kind::DirectionFlip),
        "bulk": pages.len() > BULK,
    })
}

/// Edition step 4: the first `BULK` pages are planned (`revise` for code, `update` for flips); the
/// rest are deferred and the Wrap says so. Returns `(planned, deferred)`.
pub fn budget(pages: Vec<PagePlan>) -> (Vec<PagePlan>, usize) {
    let deferred = pages.len().saturating_sub(BULK);
    (pages.into_iter().take(BULK).collect(), deferred)
}

/// Edition step 10: of what Rewrite finds now, only the code revisions planned at step 4 are published, so
/// every revision published is inside the Budget the start (or the `plan` approval) covers. Returns
/// `(to publish, deferred)`; a page found since step 4, or planned for the AI, is not published here.
pub fn due_at_publish(found: Vec<PagePlan>, planned: &BTreeSet<String>) -> (Vec<PagePlan>, usize) {
    let (now, later): (Vec<_>, Vec<_>) = found
        .into_iter()
        .partition(|p| p.by_code() && planned.contains(&p.change.page_id));
    (now, later.len())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::find::{DvMove, PointMove};

    pub(crate) fn point(old: &str, new: &str, restated: bool) -> PointMove {
        PointMove {
            old_id: format!("dp_{old}"),
            new_id: format!("dp_{new}"),
            old_text: old.into(),
            new_text: new.into(),
            new_numeric: Some(new.into()),
            new_locator: json!({"kind": "json"}),
            restated,
        }
    }

    pub(crate) fn page(id: &str, points: Vec<PointMove>, derived: Vec<DvMove>) -> PageChange {
        PageChange {
            page_id: id.into(),
            version: 1,
            story_id: Some(7),
            points,
            derived,
        }
    }

    fn dv(old: &str, new: Option<&str>) -> DvMove {
        DvMove {
            old_id: "dv_x".into(),
            new_id: "dv_y".into(),
            formula: "pp_change(a,b)".into(),
            old_value: old.into(),
            new_value: new.map(str::to_owned),
            new_inputs: vec![],
        }
    }

    #[test]
    fn classification_table() {
        let value_only = page(
            "p1",
            vec![point("2.9", "2.8", false)],
            vec![dv("-0.3", Some("-0.2"))],
        );
        let restated = page("p2", vec![point("10.0", "9.5", true)], vec![]);
        let flip_dv = page(
            "p3",
            vec![point("2.4", "2.6", false)],
            vec![dv("-0.3", Some("0.1"))],
        );
        let flip_value = page("p4", vec![point("-0.2", "0.1", false)], vec![]);
        let refused = page(
            "p5",
            vec![point("2.4", "2.6", false)],
            vec![dv("-0.3", None)],
        );
        // A flip beats a restatement.
        let both = page("p6", vec![point("-1.0", "0.5", true)], vec![]);
        let plans = plan(vec![
            both, refused, flip_value, flip_dv, restated, value_only,
        ]);
        let kinds: Vec<_> = plans
            .iter()
            .map(|p| (p.change.page_id.as_str(), p.kind))
            .collect();
        assert_eq!(
            kinds,
            [
                ("p1", Kind::ValueOnly),
                ("p2", Kind::Restatement),
                ("p3", Kind::DirectionFlip),
                ("p4", Kind::DirectionFlip),
                ("p5", Kind::DirectionFlip),
                ("p6", Kind::DirectionFlip),
            ]
        );
        let c = counts(&plans);
        assert_eq!(c["value_only"], 1);
        assert_eq!(c["needs_ai"], 4);
        assert_eq!(c["bulk"], false);
    }

    #[test]
    fn a_new_vintage_with_the_same_text_is_not_a_change() {
        assert!(
            page(
                "p",
                vec![point("2.9", "2.9", false)],
                vec![dv("-0.3", Some("-0.3"))]
            )
            .is_noop()
        );
        assert!(!page("p", vec![point("2.9", "2.8", false)], vec![]).is_noop());
    }

    #[test]
    fn the_budget_plans_a_hundred_pages_and_defers_the_rest() {
        let many: Vec<_> = (0..120)
            .map(|i| {
                page(
                    &format!("p{i:03}"),
                    vec![point("2.9", "2.8", false)],
                    vec![],
                )
            })
            .collect();
        let pages = plan(many);
        assert_eq!(counts(&pages)["bulk"], true);
        let (planned, deferred) = budget(pages);
        assert_eq!((planned.len(), deferred), (100, 20));
    }

    #[test]
    fn step_10_publishes_only_what_step_4_planned() {
        let at_step_4 = plan(vec![
            page("a", vec![point("2.9", "2.8", false)], vec![]),
            page("b", vec![point("2.9", "2.8", false)], vec![]),
        ]);
        let (planned, _) = budget(at_step_4);
        let ids: BTreeSet<String> = planned.iter().map(|p| p.change.page_id.clone()).collect();
        // By step 10 a new revision for page c has been found, and b has become a flip.
        let found = plan(vec![
            page("a", vec![point("2.9", "2.7", false)], vec![]),
            page("b", vec![point("-0.1", "0.1", false)], vec![]),
            page("c", vec![point("2.9", "2.8", false)], vec![]),
        ]);
        let (publish, deferred) = due_at_publish(found, &ids);
        assert_eq!(
            publish
                .iter()
                .map(|p| p.change.page_id.as_str())
                .collect::<Vec<_>>(),
            ["a"]
        );
        assert_eq!(deferred, 2);
    }
}
