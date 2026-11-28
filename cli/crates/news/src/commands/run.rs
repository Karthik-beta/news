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

/// Run the Bureau service until `shutdown` resolves; `true` when it stopped for an update restart.
async fn host(shutdown: impl std::future::Future<Output = ()>) -> NewsResult<bool> {
    let cfg = news_store::DbConfig::from_env()?;
    let mut opts = news_service::host::Options::new(cfg, news::registry::resolve);
    opts.handlers = news::registry::step_handlers();
    let update = opts.update.clone();
    news_service::host::run(opts, shutdown).await?;
    Ok(update.restart_requested())
}

/// The body the SCM runs: a runtime of its own, until Stop. A non-zero exit code makes the failure
/// actions restart the service (after an update, or after an error).
fn scm_body(mut stop: tokio::sync::watch::Receiver<bool>) -> u32 {
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(_) => return 1,
    };
    let _guard = news_telemetry::init_tracing();
    match rt.block_on(host(async move {
        let _ = stop.wait_for(|s| *s).await;
    })) {
        Ok(true) => news_service::update::RESTART_EXIT_CODE,
        Ok(false) => 0,
        Err(_) => 1,
    }
}

/// `news service ...`: run (the SCM entry, or a console with `--console`), and the Windows
/// administration of the service (`install`, `uninstall`, `start`, `stop`, `status`).
pub async fn service(cmd: ServiceCmd) -> NewsResult<Reply> {
    match cmd {
        ServiceCmd::Run { console } => {
            if !console {
                // Started by the SCM this blocks until Stop; from a terminal it fails with 1063 and
                // the service runs in this console instead.
                let started = tokio::task::spawn_blocking(|| {
                    news_win::scm::dispatch(news_service::install::SERVICE_NAME, scm_body)
                })
                .await
                .map_err(|e| NewsError::internal(format!("service task failed: {e}")))?;
                match started {
                    Ok(()) => return Ok(Reply::new(json!({"stopped": true}))),
                    Err(e)
                        if e.raw_os_error() == Some(news_win::scm::NOT_STARTED_BY_SCM)
                            || e.kind() == std::io::ErrorKind::Unsupported => {}
                    Err(e) => {
                        return Err(NewsError::precondition(format!(
                            "cannot hand the process to the Service Control Manager: {e}"
                        )));
                    }
                }
            }
            let restart = host(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await?;
            Ok(Reply::new(json!({"stopped": true, "updated": restart})))
        }
        ServiceCmd::Install {
            dry_run,
            account,
            root,
            elevated,
            sid,
        } => Ok(Reply::new(news_service::install::install(
            &news_service::install::InstallArgs {
                dry_run,
                elevated,
                account,
                root: root.or_else(|| news_critic::golden::find_root().ok()),
                sid,
            },
        )?)),
        ServiceCmd::Uninstall { elevated } => {
            Ok(Reply::new(news_service::install::uninstall(elevated)?))
        }
        ServiceCmd::Stage { path, elevated } => {
            Ok(Reply::new(news_service::install::stage(&path, elevated)?))
        }
        ServiceCmd::Start => Ok(Reply::new(news_service::install::start()?)),
        ServiceCmd::Stop => Ok(Reply::new(news_service::install::stop()?)),
        ServiceCmd::Status => Ok(Reply::new(news_service::install::status()?)),
    }
}

/// The CLI forms of G-tier commands: always `gated` (ADR 0008 §6.3). Chief runs G-tier work in
/// process, as the authorised step of a live run; no shell, agent or `claude -p` can run one.
pub fn gated(command: &str) -> NewsResult<Reply> {
    Err(NewsError::gated(format!(
        "`news {command}` runs only as the authorised step of a live run"
    ))
    .with_hint("start the preset from Board; Chief runs this step itself (Bureau §5.5)"))
}
