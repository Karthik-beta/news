//! `news service run`: the host loop (ADR 0008 §9). The SCM entry point is `news service run` without
//! `--console` (see [`crate::install`]); registration is `news service install`.

use crate::State;
use crate::handlers::{Bureau, Core};
use crate::log::LogRing;
use crate::update::UpdateGate;
use chrono::Utc;
use news_chief::board::ServiceState;
use news_chief::driver::Handlers;
use news_chief::{AiClaim, BoxFut, Chief, RunStopper, StartPolicy};
use news_core::NewsError;
use news_core::clock::{Clock, SystemClock};
use news_dispatch::worker::{self, ActiveJobs, Resolver, WorkOptions};
use news_hotline::Bus;
use news_rota::{Rota, RotaPolicy, UsageSource};
use news_store::{DbConfig, map_err};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use uuid::Uuid;

/// The advisory lock that makes one service per database schema: `(44, hashtext(current_schema()))`.
pub const SINGLETON_LOCK: i32 = 44;

/// Stops a run's work: cancel its worker tasks in every pool and await each (no timeout). Its
/// processes die with them; per-run Job Objects arrive with Ghost's spawner (#18).
pub struct ServiceStopper {
    pub active: ActiveJobs,
}

impl RunStopper for ServiceStopper {
    fn stop<'a>(&'a self, run: Uuid) -> BoxFut<'a, ()> {
        Box::pin(async move {
            self.active.cancel(Some(run));
            self.active.wait_idle(Some(run)).await;
        })
    }

    fn stopping(&self, run: Uuid) -> (Vec<i64>, Option<chrono::DateTime<Utc>>) {
        self.active.of_run(run)
    }
}

pub struct Options {
    pub db: DbConfig,
    pub pipe: String,
    pub resolve: Resolver,
    pub handlers: Handlers,
    pub usage: Arc<dyn UsageSource>,
    pub clock: Arc<dyn Clock>,
    pub worker_id: String,
    /// `service.update`: the caller keeps a clone and reads `restart_requested()` after `run` returns.
    pub update: Arc<UpdateGate>,
}

impl Options {
    pub fn new(db: DbConfig, resolve: Resolver) -> Self {
        Self {
            db,
            pipe: news_hotline::PIPE_NAME.to_owned(),
            resolve,
            handlers: Handlers::default(),
            usage: Arc::new(news_rota::Unknown),
            clock: Arc::new(SystemClock),
            worker_id: format!("bureau-{}", std::process::id()),
            update: Arc::new(UpdateGate::default()),
        }
    }
}

/// What a connected service holds while it is up.
struct Up {
    lock: sqlx::PgConnection,
    chief: Chief,
    rota: Rota,
    worker: tokio::task::JoinHandle<()>,
    stop_worker: watch::Sender<bool>,
}

/// Run the service until `shutdown` resolves.
pub async fn run(opts: Options, shutdown: impl Future<Output = ()>) -> Result<(), NewsError> {
    let state = Arc::new(State::default());
    let bus = Arc::new(Bus::default());
    let log = Arc::new(LogRing::default());
    let bureau = Arc::new(
        Bureau::new(state.clone(), bus.clone(), log.clone()).with_update(opts.update.clone()),
    );
    let say = |level: &str, msg: &str| {
        let line = log.push(level, "service", msg, None);
        bus.emit("service", line);
    };
    say(
        "info",
        &format!(
            "Bureau service {} starting (pid {})",
            crate::handlers::build(),
            std::process::id()
        ),
    );
    // Without the marker job every peer would read as outside it, so the H-tier check would pass a
    // spawned process: refuse to start instead (security review on #10, finding 6).
    #[cfg(windows)]
    let marker = Arc::new(
        news_win::Job::root()
            .map_err(|e| NewsError::internal(format!("cannot create the root marker job: {e}")))?,
    );
    #[cfg(windows)]
    hotline::listen(&opts.pipe, bureau.clone(), marker.clone()).map_err(|e| {
        NewsError::precondition(format!("cannot open the Hotline {}: {e}", opts.pipe))
            .with_hint("another Bureau service may be running")
    })?;
    tokio::pin!(shutdown);
    let active = ActiveJobs::default();
    loop {
        state.set(ServiceState::WaitingForDatabase);
        let mut up = tokio::select! {
            () = &mut shutdown => return Ok(()),
            r = connect(&opts, &bureau, &active, &say) => r?,
        };
        state.set(ServiceState::Up);
        say("info", "Bureau service up");
        let lost = tokio::select! {
            () = &mut shutdown => false,
            () = opts.update.wait_restart() => {
                say("info", "update: stopping for the restart");
                false
            }
            () = serve(&mut up) => true,
        };
        bureau.set_core(None);
        if lost {
            // The lock's session ended: fence before trying again (ADR 0008 §9 step 4).
            say(
                "warn",
                "lost the singleton lock; fencing before reconnecting",
            );
            fence(up, &active).await;
            #[cfg(windows)]
            let _ = marker.terminate();
            continue;
        }
        state.set(ServiceState::Stopping);
        say("info", "Bureau service stopping");
        let _ = up.stop_worker.send(true);
        if tokio::time::timeout(Duration::from_secs(15), up.worker)
            .await
            .is_err()
        {
            active.cancel(None);
        }
        up.chief.stop_drivers().await;
        return Ok(());
    }
}

async fn connect(
    opts: &Options,
    bureau: &Arc<Bureau>,
    active: &ActiveJobs,
    say: &impl Fn(&str, &str),
) -> Result<Up, NewsError> {
    let pool = loop {
        match news_store::connect(&opts.db, 16).await {
            Ok(p) => break p,
            Err(e) => {
                bureau.set_reason(Some(e.message.clone()));
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    };
    bureau.set_reason(None);
    let mut lock = pool.acquire().await.map_err(map_err)?.detach();
    let got: bool =
        sqlx::query_scalar("select pg_try_advisory_lock($1, hashtext(current_schema()))")
            .bind(SINGLETON_LOCK)
            .fetch_one(&mut lock)
            .await
            .map_err(map_err)?;
    if !got {
        return Err(NewsError::conflict(
            "another Bureau service holds the lock for this database",
        ));
    }
    bureau.state.set(ServiceState::Recovering);
    news_store::migrate(&pool, &opts.db.schema).await?;
    let policy = RotaPolicy::default();
    let bus = bureau.bus.clone();
    let emit: news_chief::Emit = Arc::new(move |topic, data| bus.emit(topic, data));
    let stopper: Arc<dyn RunStopper> = Arc::new(ServiceStopper {
        active: active.clone(),
    });
    let chief = Chief::builder(pool.clone(), opts.clock.clone())
        .policy(Arc::new(policy.clone()))
        .stopper(stopper.clone())
        .handlers(opts.handlers.clone())
        .emit(emit.clone())
        .build();
    let recovered = chief.recover().await?;
    say("info", &format!("recovered: {recovered}"));
    let rota = Rota::new(
        pool.clone(),
        chief.clone(),
        opts.clock.clone(),
        policy.clone(),
        opts.usage.clone(),
        emit,
    );
    if let Err(e) = rota.tick().await {
        say("warn", &format!("first Rota tick failed: {e}"));
    }
    let (stop_worker, mut rx) = watch::channel(false);
    let mut wo = WorkOptions::new(
        news_dispatch::POOLS
            .iter()
            .map(|p| (*p).to_owned())
            .collect(),
        opts.worker_id.clone(),
    );
    wo.active = active.clone();
    let hook = policy.clone();
    let gate = opts.update.clone();
    wo.pre_claim = Some(Arc::new(move |pool: &str| {
        gate.allows_claim(pool) && (pool != "ai" || hook.ai_claim(Utc::now()) == AiClaim::Allow)
    }));
    let resolve = opts.resolve;
    let wdb = pool.clone();
    let worker = tokio::spawn(async move {
        let stop = async move {
            let _ = rx.wait_for(|s| *s).await;
        };
        if let Err(e) = worker::run(wdb, wo, resolve, stop).await {
            tracing::warn!(error = %e, "worker loop ended");
        }
    });
    bureau.set_core(Some(Arc::new(Core {
        db: pool.clone(),
        chief: chief.clone(),
        policy: Arc::new(policy),
        stopper,
    })));
    Ok(Up {
        lock,
        chief,
        rota,
        worker,
        stop_worker,
    })
}

/// Tick Chief every second and Rota every 30; return when the lock connection is lost.
async fn serve(up: &mut Up) {
    let mut n: u64 = 0;
    loop {
        if let Err(e) = up.chief.tick().await {
            tracing::warn!(error = %e, "Chief tick failed");
        }
        if let Err(e) = up.chief.ensure_drivers().await {
            tracing::warn!(error = %e, "starting drivers failed");
        }
        if n % 30 == 29
            && let Err(e) = up.rota.tick().await
        {
            tracing::warn!(error = %e, "Rota tick failed");
        }
        n += 1;
        tokio::time::sleep(Duration::from_secs(1)).await;
        if n.is_multiple_of(5) && !lock_alive(&mut up.lock).await {
            return;
        }
    }
}

/// A query on the lock's own connection: the session, and so the lock, still exist.
async fn lock_alive(lock: &mut sqlx::PgConnection) -> bool {
    let probe = sqlx::query("select 1").execute(&mut *lock);
    matches!(
        tokio::time::timeout(Duration::from_secs(5), probe).await,
        Ok(Ok(_))
    )
}

/// Fence a service that lost its lock: stop claiming, stop the drivers, cancel and await every worker
/// task. Recovery after re-acquiring revokes every running job's lease.
async fn fence(up: Up, active: &ActiveJobs) {
    let _ = up.stop_worker.send(true);
    active.cancel(None);
    active.wait_idle(None).await;
    let _ = up.worker.await;
    up.chief.stop_drivers().await;
    drop(up.lock);
}

#[cfg(windows)]
mod hotline {
    use super::*;
    use news_hotline::{Integrity, Peer, Service};

    fn integrity(rid: u32) -> Integrity {
        match rid {
            r if r < 0x1000 => Integrity::Untrusted,
            r if r < 0x2000 => Integrity::Low,
            r if r < 0x3000 => Integrity::Medium,
            r if r < 0x4000 => Integrity::High,
            _ => Integrity::System,
        }
    }

    /// The Hotline peer from the kernel's facts.
    pub fn peer(f: &news_win::PeerFacts, owner_sid: &str, own_image: &str) -> Peer {
        Peer {
            pid: f.pid,
            same_user: f.sid.as_deref() == Some(owner_sid),
            integrity: integrity(f.integrity_rid),
            image_is_ours: news_win::same_image(&f.image, own_image),
            argv1_is_board: f.argv.get(1).map(String::as_str) == Some("board"),
            in_marker_job: f.in_marker_job,
            image: Some(f.image.clone()),
        }
    }

    /// Open the pipe (first instance: a second service fails here) and serve it in the background.
    pub fn listen(pipe: &str, svc: Arc<Bureau>, marker: Arc<news_win::Job>) -> std::io::Result<()> {
        let sid = news_win::current_user_sid()?;
        let sddl = news_win::sddl_for(&sid);
        // A client must be the installed, admin-only binary (ADR 0007 decision 12), never a build
        // under the user's own folders, even when this service was started from one.
        let me = news_win::install_paths()?
            .exe
            .to_string_lossy()
            .into_owned();
        let first = news_win::create_pipe(pipe, &sddl, true)?;
        let pipe = pipe.to_owned();
        let accept = |server: tokio::net::windows::named_pipe::NamedPipeServer| -> news_hotline::BoxFut<'static, _> {
            Box::pin(async move {
                if let Err(e) = server.connect().await {
                    tracing::warn!(error = %e, "Hotline connect failed");
                }
                server
            })
        };
        let serve = move |conn: tokio::net::windows::named_pipe::NamedPipeServer| {
            let (svc, marker, sid, me) = (svc.clone(), marker.clone(), sid.clone(), me.clone());
            tokio::spawn(async move {
                match news_win::peer_facts(&conn, Some(marker.as_ref())) {
                    Ok((facts, process)) => {
                        let svc: Arc<dyn Service> = svc;
                        news_hotline::serve_conn(conn, peer(&facts, &sid, &me), svc).await;
                        drop(process);
                    }
                    Err(e) => tracing::warn!(error = %e, "cannot identify a Hotline client"),
                }
            });
        };
        // The client is served before the next instance is opened; while all instances are busy
        // (ERROR_PIPE_BUSY) the loop backs off and retries.
        let create = move || news_win::create_pipe(&pipe, &sddl, false);
        tokio::spawn(news_hotline::accept_loop(first, accept, create, serve));
        Ok(())
    }
}
