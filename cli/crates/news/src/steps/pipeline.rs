//! The phases the page-producing presets share (Edition steps 4 to 11, Flash, Catchup's chunks, Salvage,
//! Reprint, Followup): brief, write, render and check with one repair, spot check, and publish through
//! the outbox. Every phase works from PostgreSQL and the work folder, so a step re-entered after a
//! restart (or after waiting for its AI jobs) finds its own earlier work and goes on from there.
//!
//! A job of this run that was cancelled (Chief cancels `ai` jobs at the token cap) or discarded has no
//! result: its item is dead and the phase moves on, so a run never waits for it for ever.

use super::tools::Check;
use super::{Ctx, db_err, inputs, step_err};
use news_chief::driver::{StepCx, StepError};
use news_chief::gates::{self, GRequest};
use news_chief::presets::GCommand;
use news_core::job::ErrorClass;
use news_dispatch::queue::NewJob;
use news_press::register::{Registered, register};
use news_proof::work::{self, Work};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const PAGE_SCHEMA: &str = "contracts/cli/ai/page-draft.v1.schema.json";
pub const SPOT_SCHEMA: &str = "contracts/cli/ai/a7.spotcheck.v1.schema.json";

/// A story the run plans a page for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub story_id: i64,
    pub tier: i16,
    pub action: String,
}

/// The run's Budget as rows, best first.
pub async fn run_items(cx: &StepCx) -> Result<Vec<Item>, StepError> {
    let rows: Vec<(i64, i16, String)> = sqlx::query_as(
        "select story_id, tier, action from run_plans where run_id = $1 order by rank, story_id",
    )
    .bind(cx.run_id)
    .fetch_all(&cx.db)
    .await
    .map_err(db_err)?;
    Ok(rows
        .into_iter()
        .map(|(story_id, tier, action)| Item {
            story_id,
            tier,
            action,
        })
        .collect())
}

pub fn work(ctx: &Ctx) -> Work {
    Work::new(ctx.root.join("work"))
}

pub fn fail(class: ErrorClass, message: impl Into<String>) -> StepError {
    StepError::Fail {
        class,
        message: message.into(),
    }
}

/// A phase that may have to wait for AI jobs it queued.
pub enum Ph<T> {
    Wait,
    Done(T),
}

/// What a job of this run came to.
#[derive(Debug, Clone, PartialEq)]
pub enum Job {
    Live,
    Completed(Value),
    /// Discarded or cancelled: no result.
    Dead(String),
}

/// The state of the run's job with this key, or `None` when it was never queued.
pub async fn job(cx: &StepCx, key: &str) -> Result<Option<Job>, StepError> {
    let row: Option<(String, Option<String>, Option<Value>)> = sqlx::query_as(
        "select state, error_class, result from jobs where run_id = $1 and unique_key = $2
          order by job_id desc limit 1",
    )
    .bind(cx.run_id)
    .bind(key)
    .fetch_optional(&cx.db)
    .await
    .map_err(db_err)?;
    Ok(row.map(|(state, class, result)| match state.as_str() {
        "completed" => Job::Completed(result.unwrap_or(Value::Null)),
        "discarded" | "cancelled" => Job::Dead(format!(
            "{state}{}",
            class.map(|c| format!(" ({c})")).unwrap_or_default()
        )),
        _ => Job::Live,
    }))
}

/// Queue `new` unless the run already has a job with its key, in any state: a dead job is the dead
/// list's, not queued again here.
pub async fn queue_once(cx: &StepCx, new: NewJob) -> Result<Job, StepError> {
    let key = new.unique_key.clone().unwrap_or_default();
    if let Some(j) = job(cx, &key).await? {
        return Ok(j);
    }
    cx.enqueue(new).await.map_err(step_err)?;
    Ok(Job::Live)
}

/// Brief and page inputs for each item. A story whose own data cannot be briefed (no page type yet,
/// no points) is dead, not the run; a busy database retries the step.
pub async fn brief(
    cx: &StepCx,
    ctx: &Ctx,
    stories: &[i64],
) -> Result<(Vec<inputs::Written>, Vec<Value>), StepError> {
    let work = work(ctx);
    let (mut ok, mut dead) = (Vec::new(), Vec::new());
    for &s in stories {
        match inputs::write(&cx.db, &work, &ctx.root, s).await {
            Ok(w) => ok.push(w),
            Err(e) if inputs::is_item_error(&e) => {
                dead.push(json!({"story": s, "error": e.message}));
            }
            Err(e) => return Err(step_err(e)),
        }
    }
    Ok((ok, dead))
}

/// A1 jobs for the stories briefed. Waits while any is live; the result is the stories whose Copy
/// was stored and those that have none.
pub async fn write(cx: &StepCx, stories: &[i64]) -> Result<Ph<(Vec<i64>, Vec<Value>)>, StepError> {
    let mut live = false;
    let (mut ok, mut dead) = (Vec::new(), Vec::new());
    for &s in stories {
        let story = s.to_string();
        let new = NewJob {
            item_ref: Some(story.clone()),
            ..NewJob::new("ai.write")
                .unique_key(format!("write:{story}"))
                .args(json!({
                    "step": "A1", "story": story,
                    "input": format!("work/briefs/{story}.json"), "schema": PAGE_SCHEMA,
                }))
        };
        match queue_once(cx, new).await? {
            Job::Live => live = true,
            Job::Completed(_) => ok.push(s),
            Job::Dead(why) => {
                dead.push(json!({"story": s, "error": format!("the writer call was {why}")}))
            }
        }
    }
    Ok(if live { Ph::Wait } else { Ph::Done((ok, dead)) })
}

/// The page id and version a story's page folder will publish, from its `page.json`.
fn expected(work: &Work, story: &str) -> Option<(String, i32)> {
    let p = work::read_json(&work.page(story).join("page.json")).ok()?;
    Some((
        p["page_id"].as_str()?.to_owned(),
        i32::try_from(p["version"].as_i64()?).ok()?,
    ))
}

/// The page this folder would publish, when it is already registered as a draft (or further): its
/// render and check passed in an earlier entry of this step.
async fn registered(
    cx: &StepCx,
    work: &Work,
    story: &str,
) -> Result<Option<Registered>, StepError> {
    let Some((page_id, version)) = expected(work, story) else {
        return Ok(None);
    };
    let sha: Option<Vec<u8>> = sqlx::query_scalar(
        "select bundle_sha from pages where page_id = $1 and version = $2 and bundle_sha is not null
            and status in ('draft', 'ready', 'published')
         union all
         select bundle_sha from page_drafts where page_id = $1 and version = $2 limit 1",
    )
    .bind(&page_id)
    .bind(version)
    .fetch_optional(&cx.db)
    .await
    .map_err(db_err)?;
    Ok(sha
        .and_then(|s| <[u8; 32]>::try_from(s).ok())
        .map(|bundle_sha| Registered {
            page_id,
            version,
            bundle_sha,
            changed: false,
        }))
}

pub enum Rendered {
    Registered(Registered),
    /// Proof or the validators blocked the page: the writer may repair it once.
    Failed(Vec<Value>),
    /// The page cannot be built for a reason of its own (no inputs, a version conflict): dead, no repair.
    Broken(String),
}

fn broken(e: news_core::NewsError) -> Result<Rendered, StepError> {
    if e.retryable {
        Err(step_err(e))
    } else {
        Ok(Rendered::Broken(e.message))
    }
}

/// Render the story's page folder, check it, and register it as a draft when it passes. A Copy the
/// validators refuse at render is a failed page like one Proof blocks.
pub async fn render_one(cx: &StepCx, ctx: &Ctx, story_id: i64) -> Result<Rendered, StepError> {
    let work = work(ctx);
    let story = story_id.to_string();
    match ctx.tools.render(&work, &story).await {
        Ok(_) => {}
        Err(e) if e.code == news_core::ErrorCode::ValidationFailed => {
            return Ok(Rendered::Failed(vec![
                json!({"check": "copy", "where": "/", "message": e.message}),
            ]));
        }
        Err(e) => return broken(e),
    }
    check_one(cx, ctx, story_id).await
}

/// Check the story's rendered bundle and register it as a draft when it passes.
pub async fn check_one(cx: &StepCx, ctx: &Ctx, story_id: i64) -> Result<Rendered, StepError> {
    let work = work(ctx);
    let story = story_id.to_string();
    match ctx.tools.check(&cx.db, &work, &story).await {
        Ok(Check::Pass) => {}
        Ok(Check::Fail(f)) => return Ok(Rendered::Failed(f)),
        Err(e) => return broken(e),
    }
    match register(&cx.db, &ctx.root.join("work"), &story, story_id, cx.now).await {
        Ok(r) => Ok(Rendered::Registered(r)),
        Err(e) => broken(e),
    }
}

/// Render and check each story; a failing page gets one A2 repair (`repair:<story>`), then a second
/// check. Returns the registered pages and the dead ones with their failures. Waits while a repair
/// is live.
pub async fn render(
    cx: &StepCx,
    ctx: &Ctx,
    stories: &[i64],
) -> Result<Ph<(Vec<Registered>, Vec<Value>)>, StepError> {
    let w = work(ctx);
    let (mut ok, mut dead) = (Vec::new(), Vec::new());
    let mut waiting = false;
    for &s in stories {
        let story = s.to_string();
        let repair_key = format!("repair:{story}");
        let repair = job(cx, &repair_key).await?;
        if matches!(repair, Some(Job::Live)) {
            waiting = true;
            continue;
        }
        if let Some(r) = registered(cx, &w, &story).await? {
            ok.push(r);
            continue;
        }
        match render_one(cx, ctx, s).await? {
            Rendered::Registered(r) => ok.push(r),
            Rendered::Broken(why) => dead.push(json!({"story": s, "error": why})),
            Rendered::Failed(failures) => {
                match repair {
                    None => {
                        // The Copy Proof blocked, with the failures as the errors A2 repairs.
                        let copy = work::load(&w, &story).map_err(step_err)?;
                        let errors: Vec<Value> = failures
                        .iter()
                        .map(|f| json!({"rule": f["check"], "where": f["where"], "message": f["message"]}))
                        .collect();
                        let rejected = json!({"job": null, "copy": copy.copy, "errors": errors});
                        work::write_atomic(&w.rejected(&story), &work::pretty(&rejected))
                            .map_err(step_err)?;
                        let brief = work::read_json(&w.brief(&story)).map_err(step_err)?;
                        let input = json!({"brief": brief, "previous": rejected["copy"], "errors": rejected["errors"]});
                        let rel = format!("work/ai/repair-{story}.json");
                        work::write_atomic(&ctx.root.join(&rel), &input.to_string())
                            .map_err(step_err)?;
                        let new = NewJob {
                        item_ref: Some(story.clone()),
                        ..NewJob::new("ai.repair").unique_key(&repair_key).args(json!({
                            "step": "A2", "story": story, "input": rel, "schema": PAGE_SCHEMA,
                        }))
                    };
                        queue_once(cx, new).await?;
                        waiting = true;
                    }
                    // Repaired once and still blocked, or the repair call has no result: dead.
                    Some(_) => dead.push(json!({"story": s, "failures": failures})),
                }
            }
        }
    }
    Ok(if waiting {
        Ph::Wait
    } else {
        Ph::Done((ok, dead))
    })
}

/// A deterministic share of pages by id, so a re-entered step samples the same ones.
fn sampled(page_id: &str, rate: f64) -> bool {
    let h = Sha256::digest(page_id.as_bytes());
    let n = u64::from_be_bytes([h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]]);
    (n as f64 / u64::MAX as f64) < rate
}

/// A7 on a sample of the run's draft pages (`proof.sample_rate`), and on every Tier 1 page while the site
/// is under a month old. A high-severity flag blocks an unpublished page (`spot_blocked`); a call with no
/// result leaves the page unchecked and not blocked. Returns `(checked, blocked, unchecked)`.
pub async fn spotcheck(
    cx: &StepCx,
    ctx: &Ctx,
    stories: &[i64],
) -> Result<Ph<(usize, usize, usize)>, StepError> {
    let rate = news_standing::current(&cx.db)
        .await
        .map_err(step_err)?
        .get("proof.sample_rate")
        .as_f64()
        .unwrap_or(0.05);
    let young: bool = sqlx::query_scalar(
        "select coalesce(min(published_at) > $1 - interval '30 days', true) from pages where published_at is not null",
    )
    .bind(cx.now)
    .fetch_one(&cx.db)
    .await
    .map_err(db_err)?;
    let drafts: Vec<(i64, String, i16)> = sqlx::query_as(
        "select story_id, page_id, coalesce(tier, 2::smallint) from pages
          where story_id = any($1) and status = 'draft'
         union all
         select p.story_id, d.page_id, coalesce(d.tier, p.tier, 2::smallint) from page_drafts d
           join pages p on p.page_id = d.page_id where p.story_id = any($1) and d.state = 'draft'
         order by 2",
    )
    .bind(stories)
    .fetch_all(&cx.db)
    .await
    .map_err(db_err)?;
    let w = work(ctx);
    let (mut live, mut checked, mut blocked, mut unchecked) = (false, 0, 0, 0);
    for (story_id, page_id, tier) in drafts {
        if !(sampled(&page_id, rate) || (young && tier == 1)) {
            continue;
        }
        let story = story_id.to_string();
        let md = std::fs::read_to_string(w.page(&story).join("index.md")).unwrap_or_default();
        let brief = work::read_json(&w.brief(&story)).unwrap_or(Value::Null);
        let rel = format!("work/ai/spot-{story}.json");
        work::write_atomic(
            &ctx.root.join(&rel),
            &json!({"brief": brief, "page": md}).to_string(),
        )
        .map_err(step_err)?;
        let new = NewJob {
            item_ref: Some(story.clone()),
            ..NewJob::new("ai.spotcheck")
                .unique_key(format!("spot:{story}"))
                .args(json!({
                    "step": "A7", "story": story, "input": rel, "schema": SPOT_SCHEMA,
                }))
        };
        match queue_once(cx, new).await? {
            Job::Live => live = true,
            Job::Dead(_) => unchecked += 1,
            Job::Completed(r) => {
                checked += 1;
                let flagged = r["output"]
                    .as_str()
                    .and_then(|p| std::fs::read_to_string(p).ok())
                    .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                    .is_some_and(|o| {
                        o["issues"]
                            .as_array()
                            .is_some_and(|i| i.iter().any(|x| x["severity"] == "high"))
                    });
                if flagged {
                    blocked += 1;
                    sqlx::query(
                        "update pages set flags = array_append(flags, 'spot_blocked')
                          where page_id = $1 and status = 'draft' and not 'spot_blocked' = any(flags)",
                    )
                    .bind(&page_id)
                    .execute(&cx.db)
                    .await
                    .map_err(db_err)?;
                    sqlx::query(
                        "update page_drafts set flags = array_append(flags, 'spot_blocked')
                          where page_id = $1 and state = 'draft' and not 'spot_blocked' = any(flags)",
                    )
                    .bind(&page_id)
                    .execute(&cx.db)
                    .await
                    .map_err(db_err)?;
                }
            }
        }
    }
    Ok(if live {
        Ph::Wait
    } else {
        Ph::Done((checked, blocked, unchecked))
    })
}

/// Stage every draft page of these stories (not blocked by a spot check) in the outbox, each under its
/// own G-tier check, then send the run's outbox. Returns `(staged, sent, dead)`.
pub async fn publish(
    cx: &StepCx,
    ctx: &Ctx,
    stories: &[i64],
) -> Result<(usize, u32, u32), StepError> {
    let drafts: Vec<(String, i32, Vec<u8>)> = sqlx::query_as(
        "select page_id, version, bundle_sha from pages
          where story_id = any($1) and status = 'draft' and bundle_sha is not null
            and not 'spot_blocked' = any(flags)
         union all
         select d.page_id, d.version, d.bundle_sha from page_drafts d join pages p on p.page_id = d.page_id
          where p.story_id = any($1) and d.state = 'draft' and not 'spot_blocked' = any(d.flags)
         order by 1",
    )
    .bind(stories)
    .fetch_all(&cx.db)
    .await
    .map_err(db_err)?;
    let mut staged = 0;
    for (page_id, version, sha) in &drafts {
        let sha: [u8; 32] = sha.as_slice().try_into().map_err(|_| {
            fail(
                ErrorClass::Bug,
                format!("page {page_id} has a bundle hash of the wrong length"),
            )
        })?;
        let mut tx = cx.db.begin().await.map_err(db_err)?;
        {
            let mut auth = gates::authorize(
                &mut tx,
                GRequest {
                    run_id: cx.run_id,
                    step_seq: cx.seq,
                    command: GCommand::Publish,
                    item: Some(page_id),
                },
            )
            .await
            .map_err(step_err)?;
            news_press::store::stage_put(&mut auth, page_id, *version, &sha, cx.now)
                .await
                .map_err(step_err)?;
        }
        tx.commit().await.map_err(db_err)?;
        staged += 1;
    }
    let d = send(cx, ctx).await?;
    Ok((staged, d.0, d.1))
}

/// Send the run's outbox rows. No answer from the API is a retry of the step (the same keys go again);
/// a refused key stops the run, because every row would meet it.
pub async fn send(cx: &StepCx, ctx: &Ctx) -> Result<(u32, u32), StepError> {
    let root = ctx.root.join("work");
    let d = ctx
        .press
        .drain(&cx.db, &root, cx.run_id)
        .await
        .map_err(step_err)?;
    if d.refused > 0 {
        return Err(fail(
            ErrorClass::Bug,
            "the write API refused the key (401 or 403); fix it in Settings and Resume",
        ));
    }
    if d.unknown > 0 {
        return Err(StepError::Retry(
            "the write API gave no answer; the same keys go again".into(),
        ));
    }
    Ok((d.sent, d.dead))
}

/// Story ids from a list a step recorded in its output.
pub fn ids(v: &Value) -> Vec<i64> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
        .collect()
}

pub fn set(ids: &[i64]) -> BTreeSet<i64> {
    ids.iter().copied().collect()
}

/// The page ids the plan step (or `classify`) put in the Budget under `key` (`items`, `revise`, `update`).
pub fn budget_ids(cx: &StepCx, key: &str) -> Vec<String> {
    cx.budget[key]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}

/// The stories of these pages. A run that publishes revisions stages their pending drafts through the
/// stories of the pages its Budget names, so a re-entered step loses none of them.
pub async fn stories_of(cx: &StepCx, page_ids: &[String]) -> Result<Vec<i64>, StepError> {
    sqlx::query_scalar(
        "select distinct story_id from pages where page_id = any($1) and story_id is not null order by 1",
    )
    .bind(page_ids)
    .fetch_all(&cx.db)
    .await
    .map_err(db_err)
}
