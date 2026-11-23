//! The Hotline's command handlers (ADR 0008 §8.4). The server loop (`news_hotline::serve_conn`) has
//! already admitted the peer and validated the args; these run the command.

use crate::State;
use crate::docker;
use crate::log::LogRing;
use crate::reads::{self, ReadCtx};
use crate::update::{self, UpdateGate};
use chrono::{DateTime, Utc};
use news_chief::gates::{Answer, Decision};
use news_chief::presets;
use news_chief::state::Status;
use news_chief::{Chief, IfBusy, RunStopper, StartPolicy, StartReq, Started};
use news_core::NewsError;
use news_hotline::{BoxFut, Bus, Peer, Refusal, Service};
use news_store::{PgPool, map_err};
use serde_json::{Map, Value, json};
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// What the handlers use once the database is up.
pub struct Core {
    pub db: PgPool,
    pub chief: Chief,
    pub policy: Arc<dyn StartPolicy>,
    pub stopper: Arc<dyn RunStopper>,
}

/// The Bureau service as the Hotline sees it.
pub struct Bureau {
    pub state: Arc<State>,
    pub reason: RwLock<Option<String>>,
    pub bus: Arc<Bus>,
    pub log: Arc<LogRing>,
    pub started_at: DateTime<Utc>,
    pub core: RwLock<Option<Arc<Core>>>,
    /// `service.update`: stops `ai` and `publish` claims, then asks the host to restart.
    pub update: Arc<UpdateGate>,
    /// `docker.*`: the Docker command line (a fake in tests).
    pub docker: Arc<dyn docker::Runner>,
}

impl Bureau {
    pub fn new(state: Arc<State>, bus: Arc<Bus>, log: Arc<LogRing>) -> Self {
        Self {
            state,
            reason: RwLock::new(None),
            bus,
            log,
            started_at: Utc::now(),
            core: RwLock::new(None),
            update: Arc::new(UpdateGate::default()),
            docker: docker::default_runner(),
        }
    }

    pub fn with_update(mut self, gate: Arc<UpdateGate>) -> Self {
        self.update = gate;
        self
    }

    pub fn with_docker(mut self, runner: Arc<dyn docker::Runner>) -> Self {
        self.docker = runner;
        self
    }

    /// `docker.status`, `docker.start`, `docker.stop`: the blocking command line runs off the reactor.
    async fn docker_cmd(&self, cmd: &str, args: &Value) -> Result<Value, NewsError> {
        let runner = self.docker.clone();
        let (cmd, name) = (
            cmd.to_owned(),
            args["container"].as_str().unwrap_or_default().to_owned(),
        );
        tokio::task::spawn_blocking(move || match cmd.as_str() {
            "docker.status" => Ok(docker::status(runner.as_ref())),
            other => docker::control(runner.as_ref(), &name, other == "docker.start"),
        })
        .await
        .map_err(|e| NewsError::internal(format!("docker task failed: {e}")))?
    }

    /// `service.update`: see [`crate::update`].
    async fn start_update(&self) -> Result<Value, NewsError> {
        let core = self.core()?;
        let paths = news_win::install_paths()
            .map_err(|e| NewsError::internal(format!("cannot find the install folder: {e}")))?;
        let running = std::env::current_exe()
            .map_err(|e| NewsError::internal(format!("cannot find this binary: {e}")))?;
        if !news_win::same_image(&running.to_string_lossy(), &paths.exe.to_string_lossy()) {
            return Err(NewsError::precondition(format!(
                "this service runs {}, not the installed {}: only the installed service updates itself",
                running.display(),
                paths.exe.display()
            ))
            .with_hint("run `news service install` (asks for administrator)"));
        }
        let (exe, staged) = (paths.exe, paths.staged);
        // Refused when the staged build has no recorded hash or does not match it.
        update::verify_staged(&staged)?;
        let waiting = update::in_flight(&core.db).await?;
        if self.update.begin() {
            let (gate, log, bus, db) = (
                self.update.clone(),
                self.log.clone(),
                self.bus.clone(),
                core.db.clone(),
            );
            tokio::spawn(async move {
                let say = |level: &str, msg: &str| {
                    bus.emit("service", log.push(level, "update", msg, None))
                };
                say("info", "update requested: waiting for a safe point");
                match update::finish(&db, &gate, &exe, &staged, Duration::from_secs(1)).await {
                    Ok(()) => say("info", "binary swapped: restarting the service"),
                    Err(e) => say("error", &format!("update failed: {}", e.message)),
                }
            });
        }
        let state = if waiting.is_empty() {
            "restarting"
        } else {
            "waiting_for_safe_point"
        };
        let text = update::text(&waiting);
        Ok(json!({"state": state, "waiting_on": waiting, "text": text}))
    }

    pub fn set_core(&self, core: Option<Arc<Core>>) {
        *self.core.write().unwrap_or_else(|p| p.into_inner()) = core;
    }

    pub fn set_reason(&self, r: Option<String>) {
        *self.reason.write().unwrap_or_else(|p| p.into_inner()) = r;
    }

    fn core(&self) -> Result<Arc<Core>, NewsError> {
        self.core
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
            .ok_or_else(|| NewsError::upstream("the Bureau service is not connected to PostgreSQL"))
    }

    async fn ping(&self) -> Value {
        let state = self.state.get();
        let (version, tz) = match self.core() {
            Ok(c) => match news_standing::current(&c.db).await {
                Ok(s) => (
                    Some(s.version),
                    news_standing::time_zone(&s).name().to_owned(),
                ),
                Err(_) => (None, "UTC".to_owned()),
            },
            Err(_) => (None, "UTC".to_owned()),
        };
        let reason = self
            .reason
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        json!({"service": "bureau", "state": state.as_str(), "reason": reason,
               "version": news_core::VERSION, "build": build(), "protocol": news_hotline::PROTOCOL,
               "pid": std::process::id(), "started_at": self.started_at,
               "settings_version": version, "time_zone": tz,
               "text": format!("Bureau service {}{}", state.as_str(),
                               reason.map(|r| format!(": {r}")).unwrap_or_default())})
    }

    async fn dispatch(&self, cmd: &str, args: Value) -> Result<Value, NewsError> {
        match cmd {
            "service.ping" => return Ok(self.ping().await),
            "service.log" => {
                return Ok(self.log.read(
                    args["after_seq"].as_u64().unwrap_or(0),
                    args["limit"].as_u64().unwrap_or(200) as usize,
                    args["level"].as_str(),
                ));
            }
            "docker.status" | "docker.start" | "docker.stop" => {
                return self.docker_cmd(cmd, &args).await;
            }
            "service.update" => return self.start_update().await,
            _ => {}
        }
        let core = self.core()?;
        let now = core.chief.now();
        let cx = ReadCtx {
            state: self.state.get(),
            policy: Some(core.policy.as_ref()),
            stopper: Some(core.stopper.as_ref()),
            now,
        };
        if let Some(r) = reads::read(&core.db, cmd, &args, &cx).await {
            return r;
        }
        h_command(&core, cmd, &args, &self.bus).await
    }
}

/// `version+build` of this binary; Board compares it with its own after an update.
pub fn build() -> String {
    format!(
        "{}+{}",
        news_core::VERSION,
        option_env!("NEWS_BUILD_COMMIT").unwrap_or("dev")
    )
}

fn obj(v: &Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

fn steps_of(preset: &str) -> Vec<Value> {
    presets::preset(preset).map_or_else(Vec::new, |p| {
        p.steps
            .iter()
            .enumerate()
            .map(|(i, s)| json!({"seq": i + 1, "id": s.id, "title": s.title, "kind": s.kind.as_str()}))
            .collect()
    })
}

/// The H-tier commands (only Board reaches them).
pub async fn h_command(
    core: &Core,
    cmd: &str,
    args: &Value,
    bus: &Bus,
) -> Result<Value, NewsError> {
    let chief = &core.chief;
    let run = args["run"].as_str().unwrap_or_default();
    match cmd {
        "run.start" => {
            let preset = args["preset"].as_str().unwrap_or_default();
            let if_busy = if args["if_busy"] == "queue" {
                IfBusy::Queue
            } else {
                IfBusy::Offer
            };
            let out = chief
                .start(StartReq::board(preset, obj(&args["args"]), if_busy))
                .await?;
            let (status, position) = match &out {
                Started::Running(_) => ("running", Value::Null),
                Started::Queued { position, .. } => ("queued", json!(position)),
                Started::Skipped { .. } => {
                    return Err(NewsError::internal("a Board start was skipped"));
                }
            };
            let r = out.run();
            let name = presets::preset(preset).map_or(preset, |p| p.name);
            Ok(
                json!({"run_id": r.run_id, "short_id": r.short_id, "preset": r.preset, "status": status,
                "position": position, "steps": steps_of(preset),
                "text": match status {
                    "running" => format!("{name} {} started.", r.short_id),
                    _ => format!("{name} {} queued, position {position}.", r.short_id),
                }}),
            )
        }
        "run.start_after_pause" => {
            let preset = args["preset"].as_str().unwrap_or_default();
            let (held, new, pos) = chief
                .start_after_pause(
                    preset,
                    obj(&args["args"]),
                    args["pause_run"].as_str().unwrap_or_default(),
                )
                .await?;
            Ok(
                json!({"paused": {"run_id": held.run_id, "short_id": held.short_id, "status": "pausing"},
                      "started": {"run_id": new.run_id, "short_id": new.short_id, "preset": new.preset,
                                  "status": "queued", "position": pos},
                      "text": format!("{} is pausing after its work in flight; {} {} starts when it has stopped.",
                                      held.short_id, new.preset, new.short_id)}),
            )
        }
        "gate.approve" | "gate.changes" | "gate.reject" => {
            let decision = match cmd {
                "gate.approve" => Decision::Approved,
                "gate.changes" => Decision::Changes,
                _ => Decision::Rejected,
            };
            let a = Answer {
                run,
                gate: args["gate"].as_str().unwrap_or_default(),
                round: args["round"].as_i64().unwrap_or(0) as i16,
                subject_sha: args["subject_sha"].as_str().unwrap_or_default(),
                decision,
                note: args["note"].as_str(),
            };
            let (r, status) = chief.answer(a.clone()).await?;
            Ok(
                json!({"decision": decision.as_str(), "run_id": r.run_id, "short_id": r.short_id,
                      "gate": a.gate, "round": a.round, "subject_sha": a.subject_sha,
                      "run_status": status.as_str(),
                      "text": format!("Gate {} of {} {}; the run is {}.", a.gate, r.short_id, decision.as_str(), status.as_str())}),
            )
        }
        "run.pause" => {
            let (r, mode, stalled) = chief.pause(run).await?;
            Ok(
                json!({"run_id": r.run_id, "short_id": r.short_id, "status": "pausing",
                "stop_mode": mode.as_str(), "stalled": stalled,
                "text": if stalled {
                    format!("{} has stalled: it is being stopped now.", r.short_id)
                } else {
                    format!("{} pauses once its work in flight has finished.", r.short_id)
                }}),
            )
        }
        "run.resume" => {
            let (r, to, pos) = chief.resume(run).await?;
            Ok(
                json!({"run_id": r.run_id, "short_id": r.short_id, "status": to.as_str(), "position": pos,
                "text": match to {
                    Status::Queued => format!("{} queued for the heavy slot, position {}.", r.short_id, pos.unwrap_or(1)),
                    _ => format!("{} resumed.", r.short_id),
                }}),
            )
        }
        "run.cancel" => {
            let (r, to, wrap) = chief.cancel(run).await?;
            Ok(
                json!({"run_id": r.run_id, "short_id": r.short_id, "status": to.as_str(), "wrap": wrap,
                "text": match to {
                    Status::Cancelled => format!("{} cancelled.", r.short_id),
                    _ => format!("{} is stopping; it ends cancelled when its work has stopped.", r.short_id),
                }}),
            )
        }
        "lineup.cancel" => {
            let (r, wrap) = chief.lineup_cancel(run).await?;
            Ok(
                json!({"run_id": r.run_id, "short_id": r.short_id, "status": "cancelled", "wrap": wrap,
                      "text": format!("{} removed from the Lineup.", r.short_id)}),
            )
        }
        "huddle.answer" => {
            let labels = news_nose::huddle::parse_labels(args)?;
            let stored = news_nose::huddle::store_labels(&core.db, run, &labels).await?;
            Ok(json!({"stored": stored, "text": format!("{stored} answers stored.")}))
        }
        "standing.set" => {
            let base = args["base_version"].as_i64().unwrap_or(0) as i32;
            let changes: Vec<(String, Value)> = args["changes"]
                .as_array()
                .map(|l| {
                    l.iter()
                        .map(|c| {
                            (
                                c["key"].as_str().unwrap_or_default().to_owned(),
                                c["value"].clone(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            let check = |s: &news_standing::Schedule| -> Result<(), NewsError> {
                let p = presets::preset(&s.preset).ok_or_else(|| {
                    NewsError::validation(format!("unknown preset {:?}", s.preset))
                })?;
                p.resolve_args(&s.args).map(|_| ())
            };
            let r =
                news_standing::set(&core.db, base, &changes, args["note"].as_str(), &check).await?;
            bus.emit(
                "standing",
                json!({"version": r.version, "who": "board", "changes": r.changes}),
            );
            Ok(
                json!({"version": r.version, "created_at": r.created_at, "changes": r.changes,
                      "text": news_standing::set_text(&r)}),
            )
        }
        _ => Err(NewsError::usage(format!("unknown command {cmd:?}"))),
    }
}

impl Service for Bureau {
    fn handle<'a>(
        &'a self,
        cmd: &'a str,
        args: Value,
        _peer: &'a Peer,
    ) -> BoxFut<'a, Result<Value, NewsError>> {
        Box::pin(self.dispatch(cmd, args))
    }

    fn ready(&self) -> bool {
        self.state.get() == news_chief::board::ServiceState::Up
    }

    fn refused<'a>(&'a self, cmd: &'a str, peer: &'a Peer, refusal: Refusal) -> BoxFut<'a, ()> {
        Box::pin(async move {
            let line = self.log.push(
                "warn",
                "hotline",
                &format!("refused {cmd} from pid {} ({})", peer.pid, refusal.as_str()),
                None,
            );
            self.bus.emit("service", line);
            if let Ok(core) = self.core() {
                let r = sqlx::query(
                    "insert into hotline_refusals (cmd, refusal, pid, image) values ($1, $2, $3, $4)",
                )
                .bind(cmd)
                .bind(refusal.as_str())
                .bind(peer.pid as i32)
                .bind(&peer.image)
                .execute(&core.db)
                .await
                .map_err(map_err);
                if let Err(e) = r {
                    tracing::warn!(error = %e, "could not record a Hotline refusal");
                }
            }
        })
    }

    fn bus(&self) -> &Bus {
        &self.bus
    }
}
