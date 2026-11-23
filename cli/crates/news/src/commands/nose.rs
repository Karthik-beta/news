//! `news plan --run` and `news scores explain|weights`: Nose and Desk (Bureau §3.7, §4; ADR 0012).
//! Calibration (`scores propose|label`) is Huddle's, built in #19.

use chrono::{DurationRound, TimeDelta, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Reply, database, stub};
use crate::cli::ScoresCmd;
use news_core::{NewsError, NewsResult};
use news_nose::desk::{self, Candidate};
use news_nose::score::{self, StoryInput};
use news_nose::{config, store};
use news_telemetry::{ClickHouse, ScoreRow};

/// A run plans at most this many pages; the Desk limits are integer sums of it.
const MAX_TARGET: i64 = 1000;

fn db_err(e: sqlx::Error) -> NewsError {
    news_store::map_err(e)
}

fn scorer() -> NewsResult<config::ScorerConfig> {
    config::v1().map_err(NewsError::internal)
}

/// Score every open story with inputs at the run's start hour, plan the run, and store both.
pub async fn plan(run: &str) -> NewsResult<Reply> {
    let run_id = Uuid::parse_str(run)
        .map_err(|_| NewsError::usage(format!("--run {run:?} is not a run id (a UUID)")))?;
    let cfg = scorer()?;
    let (db, _) = database(2).await?;
    let info = store::run_info(&db, run_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| NewsError::not_found(format!("no run {run_id}")))?;
    let target = match info.args.get("target").and_then(Value::as_i64) {
        Some(t) => t,
        None => news_standing::at(&db, info.settings_version)
            .await?
            .int("run.target_pages"),
    };
    if target > MAX_TARGET {
        return Err(NewsError::usage(format!(
            "target pages {target} is over the limit of {MAX_TARGET}"
        )));
    }
    let target = u32::try_from(target)
        .ok()
        .filter(|t| *t > 0)
        .ok_or_else(|| NewsError::validation(format!("target pages {target} is not positive")))?;
    // The score's idempotency key is story + scorer version + hour (§5.8), so everything is computed
    // at the run's start hour: re-running the plan gives the same rows.
    let as_of = info
        .started
        .duration_trunc(TimeDelta::hours(1))
        .map_err(|e| NewsError::internal(e.to_string()))?;
    let coverage = store::coverage(&db, as_of).await.map_err(db_err)?;
    let inputs = store::open_inputs(&db).await.map_err(db_err)?;

    let mut warnings = Vec::new();
    let mut scored = Vec::new();
    for (story_id, v) in inputs {
        let result = serde_json::from_value::<StoryInput>(v)
            .map_err(|e| e.to_string())
            .and_then(|input| {
                score::score(&input, as_of, &coverage, &cfg).map(|s| (story_id, input, s))
            });
        match result {
            Ok(x) => scored.push(x),
            Err(e) => warnings.push(format!("story {story_id} not scored: {e}")),
        }
    }
    if scored.is_empty() {
        return Err(
            NewsError::precondition("nothing scored: no open story has scoring inputs")
                .with_hint("Taster fills stories.score_input when it groups candidates (#15)"),
        );
    }

    let cands: Vec<Candidate> = scored
        .iter()
        .map(|(id, input, s)| Candidate::new(*id, input, s))
        .collect();
    let plan = desk::select(&cands, target);
    let rows: Vec<(i64, &score::Score)> = scored.iter().map(|(id, _, s)| (*id, s)).collect();
    let fresh = store::save(&db, run_id, as_of, &rows, &plan)
        .await
        .map_err(|e| match e {
            store::SaveError::Locked(m) => NewsError::precondition(m)
                .with_hint("a run's Budget is fixed once it is past planning or approved"),
            store::SaveError::Db(e) => db_err(e),
        })?;
    // Log every score once (Bureau §4.7): only when this plan inserted new score rows, so a re-run
    // adds nothing. Telemetry never fails the command.
    if fresh && let Some(ch) = ClickHouse::from_env() {
        let now = Utc::now();
        let ch_rows: Vec<ScoreRow> = scored
            .iter()
            .map(|(id, _, s)| {
                let pick = plan.picks.iter().find(|p| p.story_id == *id);
                ScoreRow {
                    ts: now,
                    run_id: run_id.to_string(),
                    story_id: u64::try_from(*id).unwrap_or_default(),
                    scorer_version: s.scorer_version.clone(),
                    components: serde_json::to_string(&s.components).unwrap_or_default(),
                    total: s.total,
                    selected: u8::from(pick.is_some()),
                    tier: pick.map_or(0, |p| p.tier),
                }
            })
            .collect();
        let sent = tokio::task::spawn_blocking(move || ch.insert_rows("news.scores", &ch_rows))
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
        if let Err(e) = sent {
            tracing::debug!(error = %e, "scores not written to ClickHouse");
        }
    }
    let mut reply = Reply::new(desk::summary(&run_id.to_string(), &plan));
    reply.warnings = warnings;
    Ok(reply)
}

pub async fn scores(cmd: ScoresCmd, path: &str) -> NewsResult<Reply> {
    match cmd {
        ScoresCmd::Weights => {
            let cfg = scorer()?;
            Ok(Reply::new(json!({
                "version": cfg.version,
                "weights": cfg.weights,
                "half_life_hours": cfg.half_life_hours,
                "event_weights": cfg.event_weights,
                "coverage_targets": cfg.coverage_targets,
            })))
        }
        ScoresCmd::Explain { story } => {
            let story_id: i64 = story
                .parse()
                .map_err(|_| NewsError::usage(format!("{story:?} is not a story number")))?;
            let (db, _) = database(2).await?;
            let row = store::latest_score(&db, story_id)
                .await
                .map_err(db_err)?
                .ok_or_else(|| {
                    NewsError::not_found(format!("story {story_id} has no score"))
                        .with_hint("`news plan --run <id>` scores open stories")
                })?;
            Ok(Reply::new(json!({
                "story_id": row.story_id,
                "computed_at": row.computed_at,
                "scorer_version": row.scorer_version,
                "total": row.total,
                "components": row.components,
                "explain": row.explain,
            })))
        }
        // Calibration is Huddle's (#19).
        ScoresCmd::Propose { .. } | ScoresCmd::Label { .. } => stub(path, "#19"),
    }
}
