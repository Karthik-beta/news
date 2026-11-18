//! The terminal's read-only view of Chief (Bureau §6.4, ADR 0008 §11): `news run presets | status
//! [<run>] | lineup | list [--active]` and `news standing show [<key>] | history [<key>]`, run in this
//! process against PostgreSQL through the same functions the Hotline handlers call, so they work with
//! the service down and give identical JSON. Starts, gate answers, pause, resume, cancel and settings
//! changes have no CLI form: Board sends them over the Hotline.

use super::{Reply, database};
use crate::cli::{RunCmd, ServiceCmd, StandingCmd};
use news_core::shape::MAX_LIMIT;
use news_core::{NewsError, NewsResult};
use news_service::reads::{self, ReadCtx};
use serde_json::{Value, json};

async fn read(cmd: &str, args: Value, with_state: bool) -> NewsResult<Reply> {
    if cmd == "run.presets" {
        return Ok(Reply::new(news_chief::presets::catalogue_json()));
    }
    let (db, _) = database(2).await?;
    let state = if with_state {
        news_service::ping_state().await
    } else {
        news_chief::board::ServiceState::Down
    };
    let cx = ReadCtx {
        state,
        policy: None,
        stopper: None,
        now: chrono::Utc::now(),
    };
    let data = reads::read(&db, cmd, &args, &cx)
        .await
        .unwrap_or_else(|| Err(NewsError::internal(format!("{cmd} is not a read"))))?;
    Ok(Reply::new(data).default_limit(MAX_LIMIT))
}

pub async fn run(cmd: RunCmd) -> NewsResult<Reply> {
    match cmd {
        RunCmd::Presets => read("run.presets", json!({}), false).await,
        RunCmd::Status { run: None } => read("status", json!({}), true).await,
        RunCmd::Status { run: Some(r) } => read("run.show", json!({"run": r}), false).await,
        RunCmd::Lineup => read("lineup.list", json!({}), false).await,
        RunCmd::List { active } => {
            read(
                "run.list",
                json!({"active": active, "limit": MAX_LIMIT}),
                false,
            )
            .await
        }
    }
}

pub async fn standing(cmd: StandingCmd) -> NewsResult<Reply> {
    match cmd {
        StandingCmd::Show { key } => read("standing.get", json!({"prefix": key}), false).await,
        StandingCmd::History { key } => {
            read(
                "standing.history",
                json!({"key": key, "limit": MAX_LIMIT}),
                false,
            )
            .await
        }
    }
}

/// `news service run`: the Bureau service in this console until Ctrl-C.
pub async fn service(cmd: ServiceCmd) -> NewsResult<Reply> {
    let ServiceCmd::Run { console: _ } = cmd;
    let cfg = news_store::DbConfig::from_env()?;
    let mut opts = news_service::host::Options::new(cfg, news::registry::resolve);
    opts.handlers = news::registry::step_handlers();
    news_service::host::run(opts, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    Ok(Reply::new(json!({"stopped": true})))
}

/// The CLI forms of G-tier commands: always `gated` (ADR 0008 §6.3). Chief runs G-tier work in
/// process, as the authorised step of a live run; no shell, agent or `claude -p` can run one.
pub fn gated(command: &str) -> NewsResult<Reply> {
    Err(NewsError::gated(format!(
        "`news {command}` runs only as the authorised step of a live run"
    ))
    .with_hint("start the preset from Board; Chief runs this step itself (Bureau §5.5)"))
}
