//! `news ai input|accept|fallback`: Ghost's tools for one AI job (Bureau §3.9, §6.4). Built here for the page steps
//! (`ai.write` A1, `ai.repair` A2); the other steps' inputs, validators and fallbacks come with Ghost (#18).
//! A page job names its story in `args.story` (or `item_ref`).

use super::render::brief_if_any;
use super::{Reply, database};
use crate::cli::AiCmd;
use news_core::{NewsError, NewsResult};
use news_proof::plate::Plate;
use news_proof::validate::{RuleError, validate};
use news_proof::work::{self, TEMPLATED, Work, check_story, pretty, write_atomic};
use serde_json::{Value, json};

const PAGE_STEPS: [&str; 2] = ["ai.write", "ai.repair"];

struct PageJob {
    id: i64,
    kind: String,
    story: String,
    /// `ai.write` in update mode (Rewrite's direction flips): the figures that changed.
    update_diff: Option<Value>,
}

async fn page_job(job: i64, verb: &str) -> NewsResult<PageJob> {
    let (db, _) = database(2).await?;
    let j = news_dispatch::queue::get(&db, job).await?;
    if !PAGE_STEPS.contains(&j.kind.as_str()) {
        return Err(NewsError::not_built(
            &format!("ai {verb} for {}", j.kind),
            news_ghost::ISSUE,
        ));
    }
    let story = match &j.args["story"] {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => j.item_ref.clone(),
    }
    .ok_or_else(|| {
        NewsError::validation(format!("job {job} names no story (args.story or item_ref)"))
    })?;
    check_story(&story)?;
    let update_diff = (j.args["mode"] == "update").then(|| j.args["diff"].clone());
    Ok(PageJob {
        id: job,
        kind: j.kind,
        story,
        update_diff,
    })
}

fn brief(work: &Work, story: &str) -> NewsResult<news_core::brief::Brief> {
    brief_if_any(work, story)?.ok_or_else(|| {
        NewsError::not_found(format!("no brief for story {story}"))
            .with_hint(format!("Fixer writes {}", work.brief(story).display()))
    })
}

pub async fn run(cmd: AiCmd) -> NewsResult<Reply> {
    let work = Work::from_env();
    match cmd {
        AiCmd::Input { job } => input(&work, page_job(job, "input").await?),
        AiCmd::Accept { job, file } => accept(&work, page_job(job, "accept").await?, &file).await,
        AiCmd::Fallback { job } => fallback(&work, page_job(job, "fallback").await?),
        // Routed to `schema::ai` before this module.
        AiCmd::Schema { .. } => Err(NewsError::internal("ai schema is not a job command")),
    }
}

/// A1 reads the brief; A2 reads the brief, the refused Copy and its errors.
fn input(work: &Work, job: PageJob) -> NewsResult<Reply> {
    let brief_path = work.brief(&job.story);
    brief(work, &job.story)?;
    let path = if let Some(diff) = &job.update_diff {
        // A1 in update mode (skills/news-reporter): the stored Copy is the previous one.
        let composed = news_rewrite::update::input(work, &job.story, diff.clone())?;
        let path = work.ai_input(job.id);
        write_atomic(&path, &pretty(&composed))?;
        path
    } else if job.kind == "ai.write" {
        brief_path
    } else {
        let rejected = work::read_json(&work.rejected(&job.story)).map_err(|e| {
            e.with_hint("A2 repairs a Copy that news ai accept refused; there is none")
        })?;
        let composed = json!({
            "brief": work::read_json(&brief_path)?,
            "previous": rejected["copy"],
            "errors": rejected["errors"],
        });
        let path = work.ai_input(job.id);
        write_atomic(&path, &pretty(&composed))?;
        path
    };
    Ok(Reply::new(json!({ "path": path.display().to_string() })))
}

async fn accept(work: &Work, job: PageJob, file: &str) -> NewsResult<Reply> {
    let brief = brief(work, &job.story)?;
    let errors = match work::read_json(std::path::Path::new(file)) {
        Err(e) if e.code == news_core::ErrorCode::NotFound => return Err(e),
        Err(e) => vec![RuleError::new("schema", "/", e.message)],
        Ok(copy) => {
            let mut errors = news_proof::validate::schema_errors(&copy);
            if errors.is_empty() {
                let oui = copy["oui"].as_str().unwrap_or_default();
                let plate = Plate::connect().await?.check(oui).await?;
                errors = validate(&copy, Some(&brief), Some(&plate));
            }
            if errors.is_empty() {
                let stored = work::save(
                    work,
                    &job.story,
                    copy,
                    vec![],
                    "ai",
                    Some(job.id),
                    chrono::Utc::now(),
                )?;
                return Ok(Reply::new(json!({
                    "valid": true, "errors": [], "stored": true, "draft_id": stored.draft_id,
                })));
            }
            let rejected = json!({ "job": job.id, "copy": copy, "errors": errors });
            write_atomic(&work.rejected(&job.story), &pretty(&rejected))?;
            errors
        }
    };
    Ok(Reply::new(
        json!({ "valid": false, "errors": errors, "stored": false }),
    ))
}

/// D10: a Tier 2 page built by code, stored flagged `templated`.
fn fallback(work: &Work, job: PageJob) -> NewsResult<Reply> {
    let brief = brief(work, &job.story)?;
    let copy = news_proof::fallback::templated(&brief)?;
    let value = serde_json::to_value(&copy).map_err(|e| NewsError::internal(e.to_string()))?;
    let stored = work::save(
        work,
        &job.story,
        value,
        vec![TEMPLATED.to_string()],
        "fallback",
        Some(job.id),
        chrono::Utc::now(),
    )?;
    Ok(Reply::new(json!({
        "applied": true,
        "result": {"draft_id": stored.draft_id, "story": job.story, "flags": stored.flags},
    })))
}
