//! Ghost, the AI job runner (Bureau Design §3.9, §7; ADR 0007, ADR 0008). The Bureau service hosts it:
//! Dispatch's worker claims `ai` jobs, renews their leases (the heartbeats) and calls [`handle`], which
//! runs one lean `claude -p` per job:
//!
//! - the §3.9 flags ([`claude::argv`]) and `pipeline/claude/page-call.settings.json`;
//! - an environment built from an allow-list ([`claude::child_env`]): no `ANTHROPIC_API_KEY`, no AWS,
//!   Cloudflare, database or write-API secret; `OTEL_RESOURCE_ATTRIBUTES` names the run, job and step;
//! - on Windows, created suspended with a Low-integrity token, put in the service's AI Job Object and
//!   only then resumed (the spawn marker), so the call and everything it starts can never send a
//!   control command, and can write only its Low-labelled work folder;
//! - its output goes to `news ai accept` through [`AcceptSink`]; an A1 Copy that fails validation gets
//!   one A2 repair, and a second failure is `ai_invalid_output`;
//! - a usage-limit reply is `ai_usage_limit` with the reset as `retry_after`: Dispatch pauses the `ai`
//!   pool until then and Chief pauses the runs and resumes them on its own (ADR 0008 §7.4);
//! - a run at its token ceiling (`quota.ai_run_tokens`) starts no new call; every call is an
//!   `ai_calls` row ([`store`]).
//!
//! Job arguments (`args` of every `ai.*` job), paths relative to the repository root:
//! `{"input": "work/briefs/81234.json", "schema": "contracts/cli/ai/page-draft.v1.schema.json",
//!   "prefix": "work/prefix/macro-release.3f9a1c.md", "step": "A1"}`; `prefix` and `step` are
//! optional (the step defaults from the kind). Enqueue a batch through [`order_by_prefix`] so calls
//! that share a prefix run back to back and the prompt cache stays warm.

pub mod claude;
pub mod store;

use claude::{CallSpec, ClaudeOutcome, Usage};
use news_core::job::{ErrorClass, Handler, JobFailure, JobFuture, JobInput};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use store::{AiCall, BoxFut, Store};

/// The issue that builds this subsystem.
pub const ISSUE: &str = "#18";

/// Job kinds this subsystem handles (see `news_dispatch::KINDS`).
pub const KINDS: &[&str] = &[
    "ai.write",
    "ai.repair",
    "ai.classify",
    "ai.storymatch",
    "ai.condense",
    "ai.spotcheck",
];

/// A call that has not finished by now is killed and retried (`transient`): well inside the AI lease
/// renewals, so a hung `claude` never holds an `ai` slot for ever.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Minimum pause after a usage limit, so a reset time already past never makes a tight loop.
const MIN_LIMIT_PAUSE: Duration = Duration::from_secs(60);

/// The longest reset time believed: a plan's longest window is a week. Anything past it (a reset given
/// in milliseconds, a garbled number) is dropped, so Dispatch's default pause (one hour) applies.
const MAX_LIMIT_PAUSE: Duration = Duration::from_secs(8 * 24 * 3600);

/// How many usage-limit pauses one job gets before it fails, with no successful call between.
const MAX_LIMIT_SNOOZES: u8 = 3;

/// The pause to ask Dispatch for after a usage limit: the wait until `resets_at`, at least a minute,
/// or `None` (Dispatch's default) when the reset is unknown or past [`MAX_LIMIT_PAUSE`].
fn limit_pause(
    resets_at: Option<chrono::DateTime<chrono::Utc>>,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<Duration> {
    let wait = (resets_at? - now).to_std().unwrap_or_default();
    (wait <= MAX_LIMIT_PAUSE).then(|| wait.max(MIN_LIMIT_PAUSE))
}

/// A job-argument path, checked: relative, no `..`, and once symlinks are resolved inside
/// `root/<under>`. Returns the resolved path.
fn confine(root: &Path, rel: &Path, under: &str) -> Result<PathBuf, JobFailure> {
    use std::path::Component;
    if rel.is_absolute()
        || !rel
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(bug(format!(
            "job path {} must be relative, with no `..`",
            rel.display()
        )));
    }
    let base = root
        .join(under)
        .canonicalize()
        .map_err(|e| bug(format!("cannot resolve {under}/ under the root: {e}")))?;
    let full = root
        .join(rel)
        .canonicalize()
        .map_err(|e| bug(format!("cannot resolve job path {}: {e}", rel.display())))?;
    if full.starts_with(&base) {
        Ok(full)
    } else {
        Err(bug(format!(
            "job path {} is outside {under}/",
            rel.display()
        )))
    }
}

/// One problem `news ai accept` found (`contracts/cli/commands/ai.accept.schema.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Problem {
    pub rule: String,
    #[serde(rename = "where")]
    pub at: String,
    pub message: String,
}

/// `news ai accept`'s reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verdict {
    pub valid: bool,
    pub errors: Vec<Problem>,
    pub stored: bool,
}

/// The `news ai accept` path: validate and store one output file for a job. The service binds it to
/// the accept command's library function (built in parallel); tests use a double.
pub trait AcceptSink: Send + Sync {
    fn accept<'a>(&'a self, job_id: i64, file: &'a Path)
    -> BoxFut<'a, Result<Verdict, JobFailure>>;
}

/// Everything a call needs, installed once by the Bureau service.
pub struct Ghost {
    /// The `claude` executable (the native `claude.exe`, not an npm `.cmd` shim).
    pub claude: PathBuf,
    /// The repository root: job argument paths are relative to it.
    pub root: PathBuf,
    /// `--settings` for every call.
    pub settings: PathBuf,
    /// Per-job working folders: the child's current directory (outside the repository, so no project
    /// `CLAUDE.md` or settings load), its outputs and repair inputs.
    pub work: PathBuf,
    /// Where the allow-listed variables are taken from (the service's own environment).
    pub parent_env: Vec<(String, String)>,
    /// The service's AI Job Object. Required on Windows: without it no call starts.
    pub job: Option<Arc<news_win::Job>>,
    pub accept: Arc<dyn AcceptSink>,
    pub store: Arc<dyn Store>,
    /// Usage-limit pauses per job since its last successful call (see [`MAX_LIMIT_SNOOZES`]). In
    /// memory: a service restart gives each job its three again.
    limit_snoozes: Mutex<HashMap<i64, u8>>,
}

impl Ghost {
    pub fn new(
        claude: PathBuf,
        root: PathBuf,
        accept: Arc<dyn AcceptSink>,
        store: Arc<dyn Store>,
    ) -> Self {
        Self {
            claude,
            settings: root.join("pipeline/claude/page-call.settings.json"),
            root,
            work: std::env::temp_dir().join("news-ghost"),
            parent_env: std::env::vars().collect(),
            job: None,
            accept,
            store,
            limit_snoozes: Mutex::default(),
        }
    }
}

static GHOST: OnceLock<Arc<Ghost>> = OnceLock::new();

/// Install Ghost for [`handle`]; `false` if one is already installed.
///
/// Do not wire this into the Bureau service yet. Calls now run at Low integrity in a Low-labelled work
/// folder (ADR 0008 §8.2), but two things remain: the service has no [`AcceptSink`] bound to
/// `news ai accept`, and #18 must prove the subscription login survives a token refresh at Low
/// integrity (the OAuth pre-refresh, ADR 0008 Consequences), since such a call cannot write
/// `%USERPROFILE%\.claude`.
pub fn install(g: Ghost) -> bool {
    GHOST.set(Arc::new(g)).is_ok()
}

/// Run one job with the installed Ghost.
pub fn handle(input: JobInput) -> JobFuture {
    Box::pin(async move {
        match GHOST.get() {
            Some(g) => g.run(input).await,
            None => Err(JobFailure::new(
                ErrorClass::Bug,
                format!(
                    "handler for {} needs Ghost installed by the Bureau service ({ISSUE})",
                    input.kind
                ),
            )),
        }
    })
}

pub const HANDLER: Handler = handle;

/// Sort a batch so jobs sharing a prefix file (named by its hash) are enqueued, and so claimed, back
/// to back. Stable: the order within one prefix is kept.
pub fn order_by_prefix(batch: &mut [news_dispatch::NewJob]) {
    batch.sort_by(|a, b| a.args["prefix"].as_str().cmp(&b.args["prefix"].as_str()));
}

/// The step a kind runs when its arguments name none (Bureau §7.1).
pub fn default_step(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "ai.write" => "A1",
        "ai.repair" => "A2",
        "ai.classify" => "A3",
        "ai.storymatch" => "A4",
        "ai.condense" => "A5",
        "ai.spotcheck" => "A7",
        _ => return None,
    })
}

#[derive(Debug, Deserialize)]
struct Args {
    step: Option<String>,
    input: PathBuf,
    schema: PathBuf,
    prefix: Option<PathBuf>,
}

fn bug(msg: impl Into<String>) -> JobFailure {
    JobFailure::new(ErrorClass::Bug, msg)
}

/// A finished process.
struct Finished {
    code: Option<i32>,
    stdout: String,
    stderr: String,
    ms: u64,
}

impl Ghost {
    /// Run one `ai` job: at most two calls (the A2 repair only for an A1 Copy).
    pub async fn run(&self, input: JobInput) -> Result<Value, JobFailure> {
        let args: Args = serde_json::from_value(input.args.clone())
            .map_err(|e| bug(format!("job {} has bad Ghost arguments: {e}", input.job_id)))?;
        let step = match &args.step {
            Some(s) => s.clone(),
            None => default_step(&input.kind)
                .ok_or_else(|| bug(format!("{} is not a Ghost kind", input.kind)))?
                .to_owned(),
        };
        let numbered =
            |n: &str| (1..=2).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_digit());
        if !step.strip_prefix('A').is_some_and(numbered) {
            return Err(bug(format!("bad step {step:?}")));
        }
        let root = self
            .root
            .canonicalize()
            .map_err(|e| bug(format!("cannot resolve the root: {e}")))?;
        let schema_path = confine(&root, &args.schema, "contracts")?;
        let brief_path = confine(&root, &args.input, "work")?;
        let prefix = match &args.prefix {
            // Only a built prefix may become the system prompt, never a brief with its untrusted text.
            Some(p) => Some(confine(&root, p, "work/prefix")?),
            None => None,
        };
        let schema: Value = std::fs::read_to_string(&schema_path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .ok_or_else(|| bug(format!("cannot read the schema {}", schema_path.display())))?;
        let schema = schema.to_string();
        let prompt_version = args
            .prefix
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let dir = self.work.join(input.job_id.to_string());
        std::fs::create_dir_all(&dir)
            .map_err(|e| JobFailure::new(ErrorClass::Transient, format!("work folder: {e}")))?;
        let mut stdin_path = brief_path.clone();
        let repairs: u8 = u8::from(step == "A1");
        let mut spent_here = 0u64;
        let mut round: u8 = 0;
        loop {
            let call_step = if round == 0 { step.as_str() } else { "A2" };
            let limits = self.store.limits(input.run_id, call_step).await?;
            if round == 0
                && let Some(ceiling) = limits.ceiling
                && limits.spent >= ceiling
            {
                // The run's AI budget is used up: no new call; the run finishes its code work.
                return Ok(json!({"step": step, "skipped": "token_ceiling",
                                 "spent": limits.spent, "ceiling": ceiling}));
            }
            input.lease.ensure_lease().await?;
            let spec = CallSpec {
                model: &limits.model,
                settings: &self.settings,
                prefix: prefix.as_deref(),
                schema: &schema,
            };
            let done = self
                .spawn(&input, call_step, &spec, &stdin_path, &dir)
                .await?;
            let reply = done
                .as_ref()
                .ok()
                .and_then(|f| claude::last_json(&f.stdout));
            let usage = reply.as_ref().map(Usage::from_reply).unwrap_or_default();
            spent_here += usage.tokens();
            let mut call = AiCall {
                ts: chrono::Utc::now(),
                run_id: input.run_id,
                job_id: input.job_id,
                step: call_step.to_owned(),
                model: limits.model.clone(),
                prompt_version: prompt_version.clone(),
                tokens_in: usage.tokens_in,
                tokens_out: usage.tokens_out,
                cache_read: usage.cache_read,
                cache_write_5m: usage.cache_write_5m,
                cache_write_1h: usage.cache_write_1h,
                turns: usage.turns,
                duration_ms: done
                    .as_ref()
                    .map_or(CALL_TIMEOUT.as_millis() as u64, |f| f.ms),
                subtype: reply
                    .as_ref()
                    .and_then(|r| r["subtype"].as_str())
                    .unwrap_or_default()
                    .to_owned(),
                validation: String::new(),
                repair_round: round,
                est_cost_usd: usage.cost_usd,
                tokens: usage.tokens(),
                input_file: stdin_path.to_string_lossy().into_owned(),
                output_file: None,
            };
            let Ok(done) = done else {
                call.subtype = "timeout".into();
                self.store.record(&call).await;
                return Err(JobFailure::new(
                    ErrorClass::Transient,
                    format!("claude -p ran over {} s; killed", CALL_TIMEOUT.as_secs()),
                ));
            };
            match claude::classify(done.code, &done.stdout) {
                ClaudeOutcome::UsageLimit { resets_at } => {
                    call.subtype = "usage_limit".into();
                    self.store.record(&call).await;
                    let n = {
                        let mut m = self.limit_snoozes.lock().unwrap_or_else(|e| e.into_inner());
                        let n = m.entry(input.job_id).or_default();
                        *n += 1;
                        *n
                    };
                    if n > MAX_LIMIT_SNOOZES {
                        self.limit_snoozes
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .remove(&input.job_id);
                        return Err(bug(format!(
                            "Claude usage limit hit {n} times with no call finishing in between; giving up"
                        )));
                    }
                    let mut f =
                        JobFailure::new(ErrorClass::AiUsageLimit, "Claude usage limit reached");
                    if let Some(d) = limit_pause(resets_at, chrono::Utc::now()) {
                        f = f.with_retry_after(d);
                    }
                    return Err(f);
                }
                ClaudeOutcome::Ok => {
                    self.limit_snoozes
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .remove(&input.job_id);
                }
                ClaudeOutcome::Failed if call.subtype != "error_max_structured_output_retries" => {
                    self.store.record(&call).await;
                    let class = if call.subtype.starts_with("error_max") {
                        ErrorClass::AiInvalidOutput // the same input would overrun again
                    } else {
                        ErrorClass::Transient
                    };
                    let tail: String = done.stderr.chars().rev().take(300).collect();
                    return Err(JobFailure::new(
                        class,
                        format!(
                            "claude -p failed (exit {:?}, subtype {:?}): {}",
                            done.code,
                            call.subtype,
                            tail.chars().rev().collect::<String>().trim()
                        ),
                    ));
                }
                // `error_max_structured_output_retries` goes on as an invalid output: it is repaired.
                ClaudeOutcome::Failed => {}
            }
            let output = reply
                .as_ref()
                .map(|r| r["structured_output"].clone())
                .filter(|o| o.is_object());
            // The call is recorded whatever happens next, so the run ceiling counts its tokens.
            let verdict = match &output {
                Some(out) => {
                    let file = dir.join(format!("{}.{round}.out.json", input.attempt));
                    call.output_file = Some(file.to_string_lossy().into_owned());
                    async {
                        std::fs::write(&file, out.to_string()).map_err(|e| {
                            JobFailure::new(ErrorClass::Transient, format!("output file: {e}"))
                        })?;
                        input.lease.ensure_lease().await?;
                        self.accept.accept(input.job_id, &file).await
                    }
                    .await
                }
                None => Ok(Verdict {
                    valid: false,
                    errors: vec![Problem {
                        rule: "schema".into(),
                        at: String::new(),
                        message: "no structured output matching the schema".into(),
                    }],
                    stored: false,
                }),
            };
            call.validation = match &verdict {
                Ok(v) if v.valid => "valid".into(),
                Ok(v) => v
                    .errors
                    .first()
                    .map_or("invalid".into(), |p| format!("invalid:{}", p.rule)),
                Err(_) => String::new(),
            };
            self.store.record(&call).await;
            let verdict = verdict?;
            if verdict.valid {
                return Ok(
                    json!({"step": step, "valid": true, "stored": verdict.stored,
                                 "repair_round": round, "output": call.output_file,
                                 "tokens": spent_here}),
                );
            }
            if round >= repairs {
                let rules: Vec<&str> = verdict.errors.iter().map(|p| p.rule.as_str()).collect();
                return Err(JobFailure::new(
                    ErrorClass::AiInvalidOutput,
                    format!(
                        "{step} output failed validation after {round} repair(s): {}",
                        rules.join(", ")
                    ),
                ));
            }
            // One A2 repair: the error list, the previous Copy and the brief (Bureau §3.9).
            let brief = std::fs::read_to_string(&brief_path).unwrap_or_default();
            let brief = serde_json::from_str::<Value>(&brief).unwrap_or(Value::String(brief));
            let repair = json!({"step": "A2", "errors": verdict.errors,
                                "previous": output.unwrap_or(Value::Null), "brief": brief});
            round += 1;
            stdin_path = dir.join(format!("{}.{round}.repair.json", input.attempt));
            std::fs::write(&stdin_path, repair.to_string()).map_err(|e| {
                JobFailure::new(ErrorClass::Transient, format!("repair input: {e}"))
            })?;
        }
    }

    /// Start `claude -p` and wait for it. `Err(())` inside `Ok` is a timeout (the child is killed).
    async fn spawn(
        &self,
        input: &JobInput,
        step: &str,
        spec: &CallSpec<'_>,
        stdin: &Path,
        cwd: &Path,
    ) -> Result<Result<Finished, ()>, JobFailure> {
        let stdin = std::fs::File::open(stdin).map_err(|e| {
            bug(format!(
                "cannot open the call input {}: {e}",
                stdin.display()
            ))
        })?;
        let args = claude::argv(spec);
        let env = claude::child_env(&self.parent_env, input.run_id, input.job_id, step);
        let started = Instant::now();
        #[cfg(windows)]
        let waited = self
            .spawn_low(stdin, &args, env, cwd, &format!("{}.{step}", input.attempt))
            .await?;
        #[cfg(not(windows))]
        let waited = {
            use std::process::Stdio;
            let child = tokio::process::Command::new(&self.claude)
                .args(args)
                .env_clear()
                .envs(env)
                .current_dir(cwd)
                .stdin(Stdio::from(stdin))
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .map_err(|e| self.start_failed(e))?;
            tokio::time::timeout(CALL_TIMEOUT, child.wait_with_output())
                .await
                .map(|r| r.map(|o| (o.status.code(), o.stdout, o.stderr)))
        };
        match waited {
            Err(_) => Ok(Err(())),
            Ok(Err(e)) => Err(JobFailure::new(
                ErrorClass::Transient,
                format!("waiting for claude: {e}"),
            )),
            Ok(Ok((code, stdout, stderr))) => Ok(Ok(Finished {
                code,
                stdout: String::from_utf8_lossy(&stdout).into_owned(),
                stderr: String::from_utf8_lossy(&stderr).into_owned(),
                ms: started.elapsed().as_millis() as u64,
            })),
        }
    }

    fn start_failed(&self, e: std::io::Error) -> JobFailure {
        let class = if e.kind() == std::io::ErrorKind::NotFound {
            ErrorClass::Bug
        } else {
            ErrorClass::Transient
        };
        JobFailure::new(
            class,
            format!("cannot start {}: {e}", self.claude.display()),
        )
    }

    /// The Windows call (ADR 0008 §8.2): `claude -p` is created suspended with a Low-integrity token,
    /// put in the AI Job Object and only then resumed. Its work folder is labelled Low, so it is the
    /// only place the call can write; its temp files go there too. Its stdout and stderr are files in
    /// that folder (`<tag>.stdout`, `<tag>.stderr`), read once it exits: no pipe, so a process that
    /// holds a copy of the handle (a grandchild, or anything started meanwhile) cannot stall the call.
    #[cfg(windows)]
    async fn spawn_low(
        &self,
        stdin: std::fs::File,
        args: &[String],
        mut env: Vec<(String, String)>,
        cwd: &Path,
        tag: &str,
    ) -> Result<Waited, JobFailure> {
        let Some(job) = &self.job else {
            return Err(bug(
                "no AI Job Object: Ghost never starts claude outside the spawn marker",
            ));
        };
        news_win::label_low(cwd)
            .map_err(|e| bug(format!("cannot label the work folder Low: {e}")))?;
        env.retain(|(k, _)| !["TEMP", "TMP"].contains(&k.to_ascii_uppercase().as_str()));
        let here = cwd.to_string_lossy().into_owned();
        env.extend(["TEMP", "TMP"].map(|k| (k.to_owned(), here.clone())));
        let (out_path, err_path) = (
            cwd.join(format!("{tag}.stdout")),
            cwd.join(format!("{tag}.stderr")),
        );
        let (out, err) = std::fs::File::create(&out_path)
            .and_then(|o| Ok((o, std::fs::File::create(&err_path)?)))
            .map_err(|e| JobFailure::new(ErrorClass::Transient, format!("call output: {e}")))?;
        let child =
            news_win::spawn_low_suspended(&self.claude, args, &env, cwd, &stdin, &out, &err)
                .map_err(|e| self.start_failed(e))?;
        drop((stdin, out, err));
        let child = Arc::new(child);
        /// Kills the child when the call ends, fails or its future is dropped (a no-op once it has
        /// exited), so the blocking wait below always returns.
        struct Kill(Arc<news_win::LowChild>);
        impl Drop for Kill {
            fn drop(&mut self) {
                let _ = self.0.kill();
            }
        }
        let _kill = Kill(child.clone());
        // On an error it is still suspended: it never ran an instruction outside the job.
        job.adopt_suspended(child.pid)
            .map_err(|e| bug(format!("cannot put claude in the AI Job Object: {e}")))?;
        let done = tokio::task::spawn_blocking(move || child.wait());
        Ok(tokio::time::timeout(CALL_TIMEOUT, done)
            .await
            .map(|joined| {
                let code = joined.map_err(std::io::Error::other)??;
                Ok((
                    Some(code as i32),
                    std::fs::read(&out_path)?,
                    std::fs::read(&err_path)?,
                ))
            }))
    }
}

/// A call's exit code, stdout and stderr, or the timeout.
#[cfg(windows)]
type Waited = Result<std::io::Result<(Option<i32>, Vec<u8>, Vec<u8>)>, tokio::time::error::Elapsed>;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use news_dispatch::NewJob;

    #[test]
    fn the_limit_pause_is_clamped_to_a_plans_longest_window() {
        let now = chrono::Utc::now();
        let at = |s: i64| Some(now + chrono::Duration::seconds(s));
        assert_eq!(limit_pause(at(3600), now), Some(Duration::from_secs(3600)));
        assert_eq!(limit_pause(at(-5), now), Some(MIN_LIMIT_PAUSE));
        assert_eq!(limit_pause(at(8 * 24 * 3600), now), Some(MAX_LIMIT_PAUSE));
        assert_eq!(limit_pause(at(8 * 24 * 3600 + 1), now), None);
        assert_eq!(limit_pause(None, now), None);
        // A reset given in milliseconds reads as thousands of years away.
        let ms = chrono::DateTime::from_timestamp(now.timestamp() * 1000, 0);
        assert_eq!(limit_pause(ms, now), None);
    }

    #[test]
    fn every_kind_has_a_default_step() {
        for k in KINDS {
            assert!(default_step(k).is_some(), "{k}");
        }
        assert_eq!(default_step("doc.fetch"), None);
    }

    #[test]
    fn a_batch_is_grouped_by_prefix_keeping_its_order() {
        let job = |n: i64, p: &str| {
            NewJob::new("ai.write").args(json!({"prefix": p, "input": format!("b{n}.json")}))
        };
        let mut batch = vec![
            job(1, "work/prefix/macro.bbb.md"),
            job(2, "work/prefix/earnings.aaa.md"),
            job(3, "work/prefix/macro.bbb.md"),
            job(4, "work/prefix/earnings.aaa.md"),
        ];
        order_by_prefix(&mut batch);
        let order: Vec<&str> = batch
            .iter()
            .map(|j| j.args["input"].as_str().unwrap())
            .collect();
        assert_eq!(order, ["b2.json", "b4.json", "b1.json", "b3.json"]);
    }

    #[tokio::test]
    async fn without_the_service_the_handler_refuses_before_any_call() {
        let input = JobInput {
            job_id: 1,
            kind: "ai.write".into(),
            pool: "ai".into(),
            attempt: 1,
            run_id: None,
            item_ref: None,
            args: json!({}),
            lease: Default::default(),
        };
        let f = handle(input).await.unwrap_err();
        assert_eq!(f.class, ErrorClass::Bug);
        assert!(f.message.contains(ISSUE));
    }
}
