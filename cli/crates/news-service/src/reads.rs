//! The R-tier reads, shared by the Hotline handlers and the terminal's `news run …` and
//! `news standing …` (ADR 0008 §11), so both give identical JSON.

use chrono::{DateTime, Utc};
use news_chief::board::{self, ServiceState};
use news_chief::{RunStopper, StartPolicy};
use news_core::NewsError;
use news_store::PgPool;
use serde_json::{Value, json};

/// What a read may need besides the database.
pub struct ReadCtx<'a> {
    pub state: ServiceState,
    pub policy: Option<&'a dyn StartPolicy>,
    pub stopper: Option<&'a dyn RunStopper>,
    pub now: DateTime<Utc>,
}

/// Run one R-tier command. `None` when `cmd` is not one of these reads.
pub async fn read(
    db: &PgPool,
    cmd: &str,
    args: &Value,
    cx: &ReadCtx<'_>,
) -> Option<Result<Value, NewsError>> {
    let s = |k: &str| args.get(k).and_then(Value::as_str).map(str::to_owned);
    let n = |k: &str, d: i64| args.get(k).and_then(Value::as_i64).unwrap_or(d);
    Some(match cmd {
        "status" => board::status(db, cx.state, cx.policy, cx.stopper, cx.now).await,
        "run.show" => match s("run") {
            Some(r) => board::run_show(db, &r, cx.now, cx.stopper).await,
            None => Err(NewsError::usage("run.show needs a run")),
        },
        "run.list" => {
            board::run_list(
                db,
                args["active"].as_bool().unwrap_or(false),
                s("preset").as_deref(),
                n("limit", 20),
                s("cursor").as_deref(),
                cx.now,
            )
            .await
        }
        "run.presets" => Ok(news_chief::presets::catalogue_json()),
        "lineup.list" => board::lineup(db, cx.policy, cx.now).await,
        "standing.get" => news_standing::current(db)
            .await
            .map(|st| st.items(s("prefix").as_deref())),
        "standing.history" => {
            standing_history(
                db,
                s("key").as_deref(),
                n("limit", 20),
                s("cursor").as_deref(),
            )
            .await
        }
        _ => return None,
    })
}

async fn standing_history(
    db: &PgPool,
    key: Option<&str>,
    limit: i64,
    cursor: Option<&str>,
) -> Result<Value, NewsError> {
    let before = match cursor {
        None => None,
        Some(c) => Some(
            c.parse::<i32>()
                .map_err(|_| NewsError::usage("cursor is not one this command returned"))?,
        ),
    };
    let mut rows = news_standing::history(db, key, limit + 1, before).await?;
    let more = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    let mut out = json!({"items": rows});
    if more && let Some(last) = rows.last() {
        out["next_cursor"] = json!(last.version.to_string());
    }
    Ok(out)
}
