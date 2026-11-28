//! Edition's optional plan approval (Bureau §8.3 step 5). Only the stop is built here: the other
//! Edition steps are `NotBuilt` until their subsystems have step bodies.
//!
//! With `review_plan` off the step is skipped, and the Start (or the schedule that fired) covers the
//! publish step (`Cover::GateOrStart("plan")`). With it on, the run stops at `plan`; the approval then
//! covers exactly the items in the Budget the plan step set, because the gate's subject carries them.

use news_chief::BoxFut;
use news_chief::driver::{Handlers, StepCx, StepError, StepHandler, StepOutcome};
use news_chief::gates::GateOpen;
use serde_json::{Value, json};
use std::sync::Arc;

pub fn register(h: Handlers) -> Handlers {
    h.with("edition.review", Arc::new(Review))
}

struct Review;

/// The gate Board shows for a plan: the Budget's items are what the approval covers.
pub fn plan_gate(budget: &Value, plan: &Value) -> GateOpen {
    let items = budget
        .get("items")
        .cloned()
        .filter(Value::is_array)
        .unwrap_or_else(|| json!([]));
    let n = items.as_array().map_or(0, Vec::len);
    GateOpen {
        gate: "plan".into(),
        subject: json!({"items": items, "plan": plan}),
        summary: json!({"text": format!("Plan: {n} items to write and publish"), "items": n}),
        preview: None,
    }
}

impl StepHandler for Review {
    fn run<'a>(&'a self, cx: &'a mut StepCx) -> BoxFut<'a, Result<StepOutcome, StepError>> {
        Box::pin(async move {
            if cx.args["review_plan"] != json!(true) {
                return Ok(StepOutcome::Skip(
                    "review_plan is off: the start authorises publishing".into(),
                ));
            }
            // The plan step's output is the plan; `cx.budget` is the Budget it set on the run.
            let plan = cx.outputs.get("plan").cloned().unwrap_or(Value::Null);
            Ok(StepOutcome::OpenGates(vec![plan_gate(&cx.budget, &plan)]))
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_plan_gate_carries_the_budget_items_the_approval_covers() {
        let g = plan_gate(
            &json!({"items": ["a-page", 7]}),
            &json!({"counts": {"t1": 1}}),
        );
        assert_eq!(g.gate, "plan");
        assert_eq!(g.subject["items"], json!(["a-page", 7]));
        assert_eq!(g.summary["items"], 2);
        // No Budget yet: an empty plan, still a gate that covers nothing.
        let none = plan_gate(&json!({}), &Value::Null);
        assert_eq!(none.subject["items"], json!([]));
    }
}
