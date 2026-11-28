//! Edition step 4 and Flash's capture: score the open stories, select the Budget, and set the run's
//! Budget (`runs.budget`), which is what the G-tier check covers. Scoring and selection are
//! `news plan --run`'s (Nose and Desk); the revisions due now come from Rewrite.

use super::{db_err, step_err};
use chrono::{DurationRound, TimeDelta};
use news_chief::driver::{StepCx, StepError};
use news_core::{NewsError, NewsResult};
use news_nose::desk::{self, Candidate};
use news_nose::score::{self, StoryInput};
use news_nose::{huddle, store};
use news_rewrite::{find, plan as rplan};
use serde_json::{Value, json};
use std::collections::BTreeSet;

const MAX_TARGET: i64 = 1000;

/// The page a story's page will have: the page it already has, else the slug of its event key (the
/// brief's `slug_hint`), else `story-<id>` (as `inputs::write` names it).
pub async fn page_ids(db: &news_store::PgPool, stories: &[i64]) -> NewsResult<Vec<String>> {
    let rows: Vec<(i64, Option<String>, Option<String>)> = sqlx::query_as(
        "select s.story_id, s.event_key,
                (select p.page_id from pages p where p.story_id = s.story_id order by p.version desc limit 1)
           from stories s where s.story_id = any($1) order by s.story_id",
    )
    .bind(stories)
    .fetch_all(db)
    .await
    .map_err(news_store::map_err)?;
    Ok(rows
        .into_iter()
        .map(|(id, key, page)| {
            page.or_else(|| key.as_deref().and_then(news_fixer::build::slug))
                .unwrap_or_else(|| format!("story-{id}"))
        })
        .collect())
}

/// Score the open stories (only `only`, when given) at the run's start hour and store the plan. Returns
/// the planned story ids and Nose's summary.
pub async fn plan_run(
    cx: &StepCx,
    only: Option<&[i64]>,
    target: Option<i64>,
) -> NewsResult<(Vec<i64>, Value)> {
    let db = &cx.db;
    let cfg = huddle::current_config(db).await?;
    let info = store::run_info(db, cx.run_id)
        .await
        .map_err(news_store::map_err)?
        .ok_or_else(|| NewsError::not_found(format!("no run {}", cx.run_id)))?;
    let target = match target {
        Some(t) => t,
        None => match info.args.get("target").and_then(Value::as_i64) {
            Some(t) => t,
            None => news_standing::at(db, info.settings_version)
                .await?
                .int("run.target_pages"),
        },
    };
    let target = u32::try_from(target.min(MAX_TARGET))
        .ok()
        .filter(|t| *t > 0)
        .ok_or_else(|| NewsError::validation("the run plans no pages"))?;
    let as_of = info
        .started
        .duration_trunc(TimeDelta::hours(1))
        .map_err(|e| NewsError::internal(e.to_string()))?;
    let coverage = store::coverage(db, as_of)
        .await
        .map_err(news_store::map_err)?;
    let inputs = store::open_inputs(db).await.map_err(news_store::map_err)?;
    let mut scored = Vec::new();
    let mut warnings = Vec::new();
    for (story_id, v) in inputs {
        if only.is_some_and(|o| !o.contains(&story_id)) {
            continue;
        }
        match serde_json::from_value::<StoryInput>(v)
            .map_err(|e| e.to_string())
            .and_then(|input| {
                score::score(&input, as_of, &coverage, &cfg).map(|s| (story_id, input, s))
            }) {
            Ok(x) => scored.push(x),
            Err(e) => warnings.push(format!("story {story_id} not scored: {e}")),
        }
    }
    let cands: Vec<Candidate> = scored
        .iter()
        .map(|(id, input, s)| Candidate::new(*id, input, s))
        .collect();
    let plan = desk::select(&cands, target);
    let rows: Vec<(i64, &score::Score)> = scored.iter().map(|(id, _, s)| (*id, s)).collect();
    store::save(db, cx.run_id, as_of, &rows, &plan)
        .await
        .map_err(|e| match e {
            store::SaveError::Locked(m) => NewsError::precondition(m),
            store::SaveError::Db(e) => news_store::map_err(e),
        })?;
    let mut summary = desk::summary(&cx.run_id.to_string(), &plan);
    summary["warnings"] = json!(warnings);
    Ok((plan.picks.iter().map(|p| p.story_id).collect(), summary))
}

/// Set the run's Budget: the pages this run may publish (new pages, and the revisions planned).
pub async fn set_budget(cx: &StepCx, items: &[String], extra: Value) -> Result<(), StepError> {
    let mut budget = json!({"items": items});
    if let (Some(b), Some(e)) = (budget.as_object_mut(), extra.as_object()) {
        b.extend(e.clone());
    }
    sqlx::query("update runs set budget = $2 where run_id = $1")
        .bind(cx.run_id)
        .bind(budget)
        .execute(&cx.db)
        .await
        .map_err(db_err)?;
    Ok(())
}

/// Edition step 4: the plan, the revisions due now (up to the `bulk` threshold), and the Budget.
pub async fn edition(cx: &StepCx) -> Result<Value, StepError> {
    let (stories, summary) = plan_run(cx, None, None).await.map_err(step_err)?;
    revisions_into_budget(cx, stories, summary).await
}

/// Add the revisions due now to a plan and set the Budget.
pub async fn revisions_into_budget(
    cx: &StepCx,
    stories: Vec<i64>,
    summary: Value,
) -> Result<Value, StepError> {
    let found = find::find(&cx.db, None).await.map_err(step_err)?;
    let (planned, deferred) = rplan::budget(rplan::plan(found));
    let mut items = page_ids(&cx.db, &stories).await.map_err(step_err)?;
    let revise: Vec<String> = planned
        .iter()
        .filter(|p| p.by_code())
        .map(|p| p.change.page_id.clone())
        .collect();
    let update: Vec<String> = planned
        .iter()
        .filter(|p| !p.by_code())
        .map(|p| p.change.page_id.clone())
        .collect();
    let all: BTreeSet<String> = items
        .drain(..)
        .chain(revise.iter().cloned())
        .chain(update.iter().cloned())
        .collect();
    let items: Vec<String> = all.into_iter().collect();
    set_budget(cx, &items, json!({"revise": revise, "update": update})).await?;
    Ok(json!({
        "plan": summary, "stories": stories, "revise": revise, "update": update, "deferred_revisions": deferred,
        "wrap": {"planned": stories.len(), "revisions_planned": revise.len() + update.len(),
                 "revisions_deferred": deferred},
    }))
}
