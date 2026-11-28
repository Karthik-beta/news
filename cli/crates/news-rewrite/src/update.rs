//! The two things Rewrite hands to others: the A1 update job for a direction flip, and what
//! `news update <page>` shows.

use crate::plan::PagePlan;
use news_core::NewsResult;
use news_dispatch::queue::NewJob;
use news_proof::work::Work;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// The diff A1 reads in update mode (`skills/news-reporter`): one entry per changed figure, with the
/// new refs and the published text of the old and the new value. Verbatim text; nothing is converted.
pub fn diff(plan: &PagePlan) -> Vec<Value> {
    let points = plan
        .change
        .points
        .iter()
        .map(|p| json!({"ref": p.new_id, "old": p.old_text, "new": p.new_text}));
    let derived = plan
        .change
        .derived
        .iter()
        .map(|d| json!({"ref": d.new_id, "old": d.old_value, "new": d.new_value}));
    points.chain(derived).collect()
}

/// A1's update-mode input (`skills/news-reporter`): `{"mode": "update", "brief", "previous_copy",
/// "diff"}`, from the story's brief and its stored Copy. `news ai input` and the presets' update jobs
/// both compose it here.
pub fn input(work: &Work, story: &str, diff: Value) -> NewsResult<Value> {
    let previous = news_proof::work::load(work, story)?;
    Ok(json!({
        "mode": "update",
        "brief": news_proof::work::read_json(&work.brief(story))?,
        "previous_copy": previous.copy,
        "diff": diff,
    }))
}

/// The `ai.write` job (A1, update mode) for a direction flip, without Ghost's arguments (`input`,
/// `schema`, `prefix`): the caller adds them once it has written [`input`] (`steps::pipeline`). Its key
/// names the page, the version it would publish and the diff, so queuing the same flip twice is one
/// job; a later revision of the same page is a different diff and a different job.
pub fn update_job(plan: &PagePlan, story: &str) -> NewJob {
    let diff = diff(plan);
    let sha: String = Sha256::digest(Value::Array(diff.clone()).to_string().as_bytes())
        .iter()
        .take(6)
        .map(|b| format!("{b:02x}"))
        .collect();
    NewJob {
        item_ref: Some(story.to_owned()),
        ..NewJob::new("ai.write")
            .unique_key(format!(
                "update:{}:{}:{sha}",
                plan.change.page_id,
                plan.change.version + 1
            ))
            .args(json!({
                "mode": "update",
                "story": story,
                "page_id": plan.change.page_id,
                "from_version": plan.change.version,
                "diff": diff,
            }))
    }
}

/// `news update <page>`'s output (`contracts/cli/commands/update.schema.json`): the page with its
/// classification and what changed, and the refs that changed (old ids, as the page holds them).
pub fn show(plan: Option<&PagePlan>, page_id: &str, version: i32) -> Value {
    match plan {
        None => json!({
            "page": {"page_id": page_id, "version": version, "revision": "current", "changes": []},
            "changed_refs": [],
        }),
        Some(p) => json!({
            "page": {
                "page_id": page_id,
                "version": version,
                "revision": p.kind.as_str(),
                "by_code": p.by_code(),
                "changes": diff(p),
            },
            "changed_refs": p.change.changed_refs(),
        }),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::find::{PageChange, PointMove};
    use crate::plan::plan;

    fn flip() -> PagePlan {
        plan(vec![PageChange {
            page_id: "us-gdp".into(),
            version: 3,
            story_id: Some(9),
            points: vec![PointMove {
                old_id: "dp_old".into(),
                new_id: "dp_new".into(),
                old_text: "-0.2".into(),
                new_text: "0.1".into(),
                new_numeric: Some("0.1".into()),
                new_locator: json!({"kind": "json"}),
                restated: false,
                ..Default::default()
            }],
            derived: vec![],
        }])
        .remove(0)
    }

    #[test]
    fn a_flip_becomes_one_keyed_update_job_with_the_diff() {
        let p = flip();
        assert!(!p.by_code());
        let job = update_job(&p, "9");
        assert_eq!(job.kind, "ai.write");
        assert!(
            job.unique_key
                .as_deref()
                .unwrap()
                .starts_with("update:us-gdp:4:")
        );
        assert_eq!(job.args["mode"], "update");
        assert_eq!(
            job.args["diff"],
            json!([{"ref": "dp_new", "old": "-0.2", "new": "0.1"}])
        );
        assert_eq!(job.item_ref.as_deref(), Some("9"));
        assert_eq!(job.unique_key, update_job(&p, "9").unique_key);
    }

    #[test]
    fn show_names_the_revision_and_the_changed_refs() {
        let p = flip();
        let v = show(Some(&p), "us-gdp", 3);
        assert_eq!(v["page"]["revision"], "direction_flip");
        assert_eq!(v["changed_refs"], json!(["dp_old"]));
        let none = show(None, "us-gdp", 3);
        assert_eq!(none["page"]["revision"], "current");
        assert_eq!(none["changed_refs"], json!([]));
    }
}
