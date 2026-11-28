//! Reprint (Bureau §8.13, Technical Design "Renderer versions"): publish pages again after the renderer
//! improved. Select the published pages on an older renderer, re-render each from its stored Copy and
//! data (no tokens), check, show Karthik a sample of before and after, then publish the new versions at
//! the paced rate.
//!
//! A page's inputs are the story folder's `data.json`, `sources.json`, `page.json` and `copy.json` and its
//! brief, which `news prune` leaves in place. The new bundle is version n + 1 of the page; a page whose
//! bundle comes out byte for byte the same is left alone.

use super::pipeline::{self, fail, work};
use super::{Ctx, Unset, db_err, step_err};
use news_chief::BoxFut;
use news_chief::driver::{Handlers, StepCx, StepError, StepHandler, StepOutcome};
use news_chief::gates::GateOpen;
use news_core::job::ErrorClass;
use news_proof::work::{pretty, read_json, write_atomic};
use serde_json::{Value, json};
use std::sync::Arc;

/// How many before and after previews the approval shows.
pub const SAMPLE: usize = 10;

#[derive(Clone, Copy)]
enum Step {
    Select,
    Rerender,
    Check,
    Review,
    Publish,
}

const STEPS: [(&str, Step); 5] = [
    ("reprint.select", Step::Select),
    ("reprint.rerender", Step::Rerender),
    ("reprint.check", Step::Check),
    ("reprint.review", Step::Review),
    ("reprint.publish", Step::Publish),
];

struct Reprint(Step, Option<Arc<Ctx>>);

pub fn register(h: Handlers, ctx: Arc<Ctx>) -> Handlers {
    STEPS.into_iter().fold(h, |h, (name, step)| {
        h.with(name, Arc::new(Reprint(step, Some(ctx.clone()))))
    })
}

pub(super) fn register_unset(h: Handlers, unset: Arc<Unset>) -> Handlers {
    STEPS.into_iter().fold(h, |h, (name, step)| {
        if matches!(step, Step::Review) {
            h.with(name, Arc::new(Reprint(step, None)))
        } else {
            h.with(name, unset.clone())
        }
    })
}

/// The numbers in a renderer version, `plate-1.12.0` -> `[1, 12, 0]`; versions with no number compare
/// as text.
fn parts(v: &str) -> Vec<u64> {
    v.split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect()
}

/// Whether `have` is an older renderer than `want`.
pub fn older(have: &str, want: &str) -> bool {
    let (a, b) = (parts(have), parts(want));
    if a.is_empty() || b.is_empty() {
        return have < want;
    }
    a < b
}

impl StepHandler for Reprint {
    fn run<'a>(&'a self, cx: &'a mut StepCx) -> BoxFut<'a, Result<StepOutcome, StepError>> {
        Box::pin(async move { run(self.1.as_deref(), self.0, cx).await })
    }
}

fn pages(cx: &StepCx, step: &str) -> Vec<Value> {
    cx.outputs
        .get(step)
        .and_then(|o| o["pages"].as_array().cloned())
        .unwrap_or_default()
}

async fn run(ctx: Option<&Ctx>, step: Step, cx: &StepCx) -> Result<StepOutcome, StepError> {
    if matches!(step, Step::Review) {
        return Ok(review(cx));
    }
    let ctx = ctx.ok_or_else(|| fail(ErrorClass::InvalidData, "the repository root is not set"))?;
    let w = work(ctx);
    match step {
        Step::Select => {
            let want = cx.args["renderer"].as_str().unwrap_or_default().to_owned();
            let types: Vec<String> = cx.args["page_types"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| t.as_str().map(str::to_owned))
                .collect();
            let limit = cx.args["limit"].as_i64().unwrap_or(200);
            let rows: Vec<(String, i64, Option<String>, i32)> = sqlx::query_as(
                "select page_id, story_id, renderer_version, version from pages
                  where status = 'published' and story_id is not null
                    and (cardinality($1::text[]) = 0 or page_type = any($1))
                  order by published_at, page_id",
            )
            .bind(&types)
            .fetch_all(&cx.db)
            .await
            .map_err(db_err)?;
            let picked: Vec<Value> = rows
                .into_iter()
                .filter(|r| r.2.as_deref().is_none_or(|have| older(have, &want)))
                .take(usize::try_from(limit).unwrap_or(0))
                .map(|(page_id, story_id, renderer, version)| {
                    json!({"page_id": page_id, "story_id": story_id, "renderer": renderer, "version": version})
                })
                .collect();
            let ids: Vec<String> = picked
                .iter()
                .filter_map(|p| p["page_id"].as_str().map(str::to_owned))
                .collect();
            super::plan::set_budget(cx, &ids, json!({})).await?;
            Ok(StepOutcome::Done(
                json!({"pages": picked, "wrap": {"selected": picked.len()}}),
            ))
        }
        Step::Rerender => {
            let want = cx.args["renderer"].as_str().unwrap_or_default().to_owned();
            let (mut ok, mut dead) = (Vec::new(), Vec::new());
            for p in pages(cx, "select") {
                let (page_id, story) = (
                    p["page_id"].as_str().unwrap_or_default(),
                    p["story_id"].to_string(),
                );
                // The next version, with the first publication kept.
                let page_path = w.page(&story).join("page.json");
                let bumped = read_json(&page_path).map(|mut page| {
                    page["version"] = json!(p["version"].as_i64().unwrap_or(1) + 1);
                    page
                });
                let rendered = match bumped {
                    Ok(page) => {
                        write_atomic(&page_path, &pretty(&page)).map_err(step_err)?;
                        ctx.tools.render(&w, &story).await
                    }
                    Err(e) => Err(e),
                };
                match rendered {
                    Ok(r) if older(&r.renderer_version, &want) => dead.push(json!({
                        "page_id": page_id,
                        "error": format!("the renderer is {}, not {want}", r.renderer_version),
                    })),
                    Ok(r) => ok.push(json!({"page_id": page_id, "story_id": p["story_id"], "renderer": r.renderer_version,
                                            "after": r.bundle_sha})),
                    Err(e) if e.retryable => return Err(step_err(e)),
                    Err(e) => dead.push(json!({"page_id": page_id, "error": e.message})),
                }
            }
            Ok(StepOutcome::Done(json!({
                "pages": ok, "dead": dead, "wrap": {"rerendered": ok.len(), "failed": dead.len()},
            })))
        }
        Step::Check => {
            let (mut ok, mut dead) = (Vec::new(), Vec::new());
            for p in pages(cx, "rerender") {
                let id = p["page_id"].as_str().unwrap_or_default();
                let story_id = p["story_id"].as_i64().unwrap_or_default();
                match pipeline::check_one(cx, ctx, story_id).await? {
                    pipeline::Rendered::Registered(r) => {
                        let before: Option<Vec<u8>> = sqlx::query_scalar(
                            "select bundle_sha from publish_outbox where page_id = $1 and op = 'put_page' and sent_at is not null
                              and version < $2 order by version desc limit 1",
                        )
                        .bind(id)
                        .bind(r.version)
                        .fetch_optional(&cx.db)
                        .await
                        .map_err(db_err)?;
                        let after = news_chief::gates::hex(&r.bundle_sha);
                        let before = before.map(|b| news_chief::gates::hex(&b));
                        ok.push(json!({"page_id": id, "story_id": story_id, "before": before, "after": after,
                                       "renderer": p["renderer"], "version": r.version}));
                    }
                    pipeline::Rendered::Failed(f) => {
                        dead.push(json!({"page_id": id, "failures": f}))
                    }
                    pipeline::Rendered::Broken(why) => {
                        dead.push(json!({"page_id": id, "error": why}))
                    }
                }
            }
            Ok(StepOutcome::Done(json!({
                "pages": ok, "dead": dead, "wrap": {"checked": ok.len(), "failed": dead.len()},
            })))
        }
        Step::Review => Ok(review(cx)),
        Step::Publish => {
            let rejected = cx
                .outputs
                .get("review")
                .and_then(|r| r["decisions"].as_array())
                .is_some_and(|d| d.iter().any(|x| x["decision"] == "rejected"));
            if rejected {
                // The live pages stay as they were: their unsent next versions are thrown away.
                let mut discarded = 0;
                for p in pages(cx, "check") {
                    let id = p["page_id"].as_str().unwrap_or_default();
                    if news_press::register::discard_draft(&cx.db, id)
                        .await
                        .map_err(step_err)?
                    {
                        discarded += 1;
                    }
                }
                return Ok(StepOutcome::Done(
                    json!({"discarded": discarded, "wrap": {"discarded": discarded}}),
                ));
            }
            let stories: Vec<i64> = pages(cx, "check")
                .iter()
                .filter_map(|p| p["story_id"].as_i64())
                .collect();
            if stories.is_empty() {
                return Ok(StepOutcome::Done(
                    json!({"staged": 0, "sent": 0, "wrap": {"published": 0}}),
                ));
            }
            let (staged, sent, dead) = pipeline::publish(cx, ctx, &stories).await?;
            Ok(StepOutcome::Done(
                json!({"staged": staged, "sent": sent, "wrap": {"published": sent, "failed": dead}}),
            ))
        }
    }
}

/// The sample approval: the pages checked, with before and after for the first few.
fn review(cx: &StepCx) -> StepOutcome {
    let all = pages(cx, "check");
    let ids: Vec<Value> = all.iter().map(|p| p["page_id"].clone()).collect();
    let sample: Vec<Value> = all.iter().take(SAMPLE).cloned().collect();
    let n = all.len();
    StepOutcome::OpenGates(vec![GateOpen {
        gate: "sample".into(),
        subject: json!({"items": ids, "pages": all}),
        summary: json!({"text": format!("Reprint {n} pages; before and after shown for {}", sample.len()),
                    "subject_items": ids, "count": n}),
        preview: Some(json!({"sample": sample})),
    }])
}

#[cfg(test)]
mod tests {
    use super::older;

    #[test]
    fn renderer_versions_compare_by_their_numbers() {
        assert!(older("plate-1.2.0", "plate-1.10.0"));
        assert!(!older("plate-1.10.0", "plate-1.2.0"));
        assert!(!older("plate-2.0.0", "plate-2.0.0"));
        assert!(older("a", "b"));
    }
}
