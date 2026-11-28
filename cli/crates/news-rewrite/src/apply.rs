//! Applying a by-code revision to a story's page folder (`work/pages/<story>/`, `news_proof::work`):
//! `data.json`, the stored Copy and `page.json` (version and first publication), ready for
//! `news render` (Plate) and `news check`. Idempotent: running it again leaves the folder as it is.

use crate::find::PageChange;
use crate::patch::{add_restated_note, patch_data, rename_in};
use crate::plan::PagePlan;
use chrono::{DateTime, Utc};
use news_core::{NewsError, NewsResult};
use news_proof::work::{self, Work, check_story, pretty, read_json, timestamp, write_atomic};
use news_store::{PgPool, map_err};
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Applied {
    pub page_id: String,
    pub story: String,
    /// The version the re-render publishes.
    pub version: i32,
    pub kind: &'static str,
    /// The "restated" note was added by this run.
    pub restated_note: bool,
    /// A restatement that could not get its note (the page already has two notes).
    pub note_skipped: bool,
}

/// Store the derived points the revision re-keys to (`derived_points`, kept once, as Fixer's `save`
/// does), so the page's new `dv_` refs resolve. Run before [`apply`]. Only values the Ledger's guards
/// produced are stored; a refused or incompatible one makes the page a direction flip, which never
/// reaches here. Returns the number of rows written.
pub async fn store_derived(db: &PgPool, change: &PageChange) -> NewsResult<u64> {
    let mut written = 0;
    for d in &change.derived {
        let Some(n) = d.new_derived.as_ref().filter(|_| !d.incompatible) else {
            continue;
        };
        written += sqlx::query(
            "insert into derived_points (dp_id, formula, inputs, value, unit, rounding, guard)
             values ($1, $2, $3, $4::numeric, $5, $6, $7)
             on conflict (dp_id) do nothing",
        )
        .bind(&n.dp_id)
        .bind(n.formula)
        .bind(&n.inputs)
        .bind(&n.value)
        .bind(&n.unit)
        .bind(i16::try_from(n.rounding).unwrap_or(i16::MAX))
        .bind(&n.guard)
        .execute(db)
        .await
        .map_err(map_err)?
        .rows_affected();
    }
    Ok(written)
}

/// Write the revised inputs. `published_at` is the page's first publication, which stays; `now` is the
/// time of this revision (the Copy's `saved_at`, so the page shows "Updated on …").
pub fn apply(
    work: &Work,
    story: &str,
    plan: &PagePlan,
    published_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> NewsResult<Applied> {
    if !plan.by_code() {
        return Err(NewsError::precondition(format!(
            "page {} needs the AI (a direction flip): queue an update job instead",
            plan.change.page_id
        )));
    }
    apply_inner(work, story, plan, published_at, now, false)
}

/// A direction flip after the A1 update job stored the new Copy: patch `data.json` (the new points the
/// Copy's refs resolve to) and `page.json` (version and first publication); the Copy is the writer's and
/// stays as stored. Idempotent like [`apply`].
pub fn apply_flip(
    work: &Work,
    story: &str,
    plan: &PagePlan,
    published_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> NewsResult<Applied> {
    if plan.by_code() {
        return Err(NewsError::precondition(format!(
            "page {} is re-rendered by code, not updated by the AI",
            plan.change.page_id
        )));
    }
    apply_inner(work, story, plan, published_at, now, true)
}

fn apply_inner(
    work: &Work,
    story: &str,
    plan: &PagePlan,
    published_at: DateTime<Utc>,
    now: DateTime<Utc>,
    flip: bool,
) -> NewsResult<Applied> {
    check_story(story)?;
    let dir = work.page(story);
    let data_path = dir.join("data.json");
    let mut data = read_json(&data_path)?;
    patch_data(&mut data, &plan.change);

    let stored = work::load(work, story)?;
    let mut copy = stored.copy.clone();
    if !flip {
        rename_in(&mut copy, &plan.change.renames());
    }
    let (mut added, mut skipped) = (false, false);
    if plan.kind == crate::classify::Kind::Restatement {
        let oui = copy["oui"].as_str().unwrap_or_default().to_owned();
        match add_restated_note(&oui)
            .map_err(|e| NewsError::validation(format!("cannot add the restated note: {e}")))?
        {
            Some(with_note) => {
                copy["oui"] = json!(with_note);
                if let Some(used) = copy["components_used"].as_array_mut()
                    && !used.iter().any(|c| c == "Note")
                {
                    used.push(json!("Note"));
                }
                added = true;
            }
            // Already noted by an earlier run, or at the two-note limit.
            None => skipped = !oui.contains("restated = Note("),
        }
    }

    let page_path = dir.join("page.json");
    let mut page = read_json(&page_path)?;
    let version = plan.change.version + 1;
    page["version"] = json!(version);
    page["published_at"] = json!(timestamp(published_at));

    write_atomic(&data_path, &pretty(&data))?;
    if !flip {
        work::save(work, story, copy, stored.flags, "save", None, now)?;
    }
    write_atomic(&page_path, &pretty(&page))?;
    Ok(Applied {
        page_id: plan.change.page_id.clone(),
        story: story.to_owned(),
        version,
        kind: plan.kind.as_str(),
        restated_note: added,
        note_skipped: skipped,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::classify::Kind;
    use crate::find::PointMove;
    use crate::plan::plan;

    fn folder(tag: &str) -> (std::path::PathBuf, Work) {
        let dir =
            std::env::temp_dir().join(format!("news-rewrite-apply-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let work = Work::new(&dir);
        let page = work.page("7");
        std::fs::create_dir_all(&page).unwrap();
        std::fs::write(
            page.join("data.json"),
            json!({"version": 1, "points": {"dp_old": {"label": "L", "value": "10", "value_text": "10", "unit": "USD",
                   "period": "pd_1", "source": "src_a", "locator": {"kind": "xbrl"}}}})
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            page.join("page.json"),
            json!({"categories": ["economy"], "region": "us", "version": 1}).to_string(),
        )
        .unwrap();
        let t = DateTime::parse_from_rfc3339("2026-11-01T10:00:00Z")
            .unwrap()
            .to_utc();
        work::save(
            &work,
            "7",
            json!({"headline": "Sales {dp_old}", "oui": "root = Page([a])\na = KeyFigure(\"Sales\", \"dp_old\")",
                   "refs_used": ["dp_old"], "components_used": ["Page", "KeyFigure"]}),
            vec![],
            "save",
            None,
            t,
        )
        .unwrap();
        (dir, work)
    }

    fn pending(restated: bool, old: &str, new: &str) -> PagePlan {
        plan(vec![crate::find::PageChange {
            page_id: "acme-q3".into(),
            version: 1,
            story_id: Some(7),
            points: vec![PointMove {
                old_id: "dp_old".into(),
                new_id: "dp_new".into(),
                old_text: old.into(),
                new_text: new.into(),
                new_numeric: Some(new.into()),
                new_locator: json!({"kind": "xbrl", "tag": "Revenues"}),
                restated,
                ..Default::default()
            }],
            derived: vec![],
        }])
        .remove(0)
    }

    #[test]
    fn a_restatement_is_rewritten_by_code_with_the_note_and_is_idempotent() {
        let (dir, work) = folder("restated");
        let now = DateTime::parse_from_rfc3339("2026-11-20T10:00:00Z")
            .unwrap()
            .to_utc();
        let first = DateTime::parse_from_rfc3339("2026-11-01T10:00:00Z")
            .unwrap()
            .to_utc();
        let p = pending(true, "10", "9");
        assert_eq!(p.kind, Kind::Restatement);
        let a = apply(&work, "7", &p, first, now).unwrap();
        assert_eq!((a.version, a.restated_note), (2, true));
        let stored = work::load(&work, "7").unwrap();
        assert_eq!(stored.saved_at, "2026-11-20T10:00:00Z");
        assert!(
            stored.copy["headline"]
                .as_str()
                .unwrap()
                .contains("{dp_new}")
        );
        let oui = stored.copy["oui"].as_str().unwrap();
        assert!(
            oui.contains("dp_new") && oui.contains("restated = Note("),
            "{oui}"
        );
        assert!(
            stored.copy["components_used"]
                .as_array()
                .unwrap()
                .contains(&json!("Note"))
        );
        let page = read_json(&work.page("7").join("page.json")).unwrap();
        assert_eq!(
            (page["version"].clone(), page["published_at"].clone()),
            (json!(2), json!("2026-11-01T10:00:00Z"))
        );
        let data = read_json(&work.page("7").join("data.json")).unwrap();
        assert_eq!(data["points"]["dp_new"]["supersedes"], "dp_old");

        let again = apply(&work, "7", &p, first, now).unwrap();
        assert!(!again.restated_note && !again.note_skipped);
        assert_eq!(work::load(&work, "7").unwrap(), stored);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_value_only_revision_adds_no_note_and_a_flip_is_refused() {
        let (dir, work) = folder("value");
        let now = DateTime::parse_from_rfc3339("2026-11-20T10:00:00Z")
            .unwrap()
            .to_utc();
        let a = apply(&work, "7", &pending(false, "10", "11"), now, now).unwrap();
        assert!(!a.restated_note);
        assert!(
            !work::load(&work, "7").unwrap().copy["oui"]
                .as_str()
                .unwrap()
                .contains("Note(")
        );
        let err = apply(&work, "7", &pending(false, "-1", "1"), now, now).unwrap_err();
        assert_eq!(err.code, news_core::ErrorCode::Precondition);
        let _ = std::fs::remove_dir_all(dir);
    }
}
