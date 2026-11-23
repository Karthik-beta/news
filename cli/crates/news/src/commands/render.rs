//! `news save` and `news render`: store a Copy and render its page bundle through Plate (Bureau §3.9, §3.10, §6.4).
//! Folders and files: `news_proof::work`.

use super::{Reply, read_json};
use news_core::brief::Brief;
use news_core::{NewsError, NewsResult};
use news_proof::plate::Plate;
use news_proof::render as bundle;
use news_proof::validate::{RuleError, validate};
use news_proof::work::{self, Work, check_story};
use serde_json::json;

/// The brief for `story`, or `None` when Fixer has not written one.
pub fn brief_if_any(work: &Work, story: &str) -> NewsResult<Option<Brief>> {
    let path = work.brief(story);
    if !path.is_file() {
        return Ok(None);
    }
    serde_json::from_value(work::read_json(&path)?)
        .map(Some)
        .map_err(|e| NewsError::validation(format!("the brief for {story} is not brief.v1: {e}")))
}

pub fn invalid(errors: &[RuleError]) -> NewsError {
    let first = errors
        .first()
        .map(|e| format!(": {} at {}: {}", e.rule, e.at, e.message))
        .unwrap_or_default();
    NewsError::validation(format!(
        "the Copy fails {} validator check(s){first}",
        errors.len()
    ))
    .with_details(json!({ "errors": errors }))
}

/// `news save <story> <file>`: the code validators (schema, refs against the brief when there is one, C4, banned
/// words, lengths), then store. OpenUI parse and props run in `news render` (C1) and `news ai accept`.
pub fn save(story: &str, file: &str) -> NewsResult<Reply> {
    check_story(story)?;
    let copy = read_json(file)?;
    let work = Work::from_env();
    let brief = brief_if_any(&work, story)?;
    let errors = validate(&copy, brief.as_ref(), None);
    if !errors.is_empty() {
        return Err(invalid(&errors));
    }
    let stored = work::save(&work, story, copy, vec![], "save", None, chrono::Utc::now())?;
    let mut reply = Reply::new(json!({ "draft_id": stored.draft_id }));
    if brief.is_none() {
        reply.warnings.push(format!(
            "no brief at {}: references were not checked against it",
            work.brief(story).display()
        ));
    }
    Ok(reply)
}

/// `news render <story>`: the bundle, written into the story's page folder, where `news check` reads it.
pub async fn render(story: &str) -> NewsResult<Reply> {
    check_story(story)?;
    let work = Work::from_env();
    let inputs = bundle::load_inputs(&work, story)?;
    let plate = Plate::connect().await?;
    let rendered = plate.render(&inputs.render_request()).await?;
    let built = bundle::assemble(&inputs, &rendered)?;
    let dir = work.page(story);
    bundle::write(&built, &dir)?;
    let at = |f: &str| dir.join(f).display().to_string();
    let mut reply = Reply::new(json!({
        "paths": {"html": at("index.html"), "md": at("index.md"), "json": at("data.json"), "bundle": dir.display().to_string()},
        "bundle_sha256": built.sha256,
        "renderer_version": rendered.renderer_version,
        "flags": built.meta.get("flags").cloned().unwrap_or_else(|| json!([])),
        "problems": rendered.problems,
    }));
    if !rendered.problems.is_empty() {
        reply.warnings.push(format!(
            "{} data problem(s): the page shows missing states, and news check blocks it",
            rendered.problems.len()
        ));
    }
    Ok(reply)
}
