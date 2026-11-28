//! Binds each job kind's owner (from `news_dispatch::KINDS`) to its subsystem crate's handler.

use news_core::NewsResult;
use news_core::job::Handler;
use news_store::PgPool;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub fn handler_for_owner(owner: &str) -> Option<Handler> {
    Some(match owner {
        "wire" => news_wire::HANDLER,
        "clerk" => news_clerk::HANDLER,
        "diary" => news_diary::HANDLER,
        "taster" => news_taster::HANDLER,
        "nose" => news_nose::HANDLER,
        "fixer" => news_fixer::HANDLER,
        "ghost" => news_ghost::HANDLER,
        "proof" => news_proof::HANDLER,
        "rewrite" => news_rewrite::HANDLER,
        "press" => news_press::HANDLER,
        "telemetry" => news_telemetry::HANDLER,
        _ => return None,
    })
}

/// What Audit says when no repository root can be found.
const ROOT_UNSET: &str = "the repository root is not set: set NEWS_ROOT to the News repository, \
    or run `news service install` from inside it (Audit reads pipeline/eval/)";

/// The repository root Audit reads, in this order: `NEWS_ROOT`, the root `news service install`
/// recorded, then `pipeline/eval/` at or above the working directory (`here`). A service starts in
/// `C:\Windows\System32`, so only the first two can work there. A setting that names a folder
/// without `pipeline/eval/` is an error, never skipped.
pub fn audit_root(
    env: Option<std::ffi::OsString>,
    recorded: Option<PathBuf>,
    here: impl FnOnce() -> NewsResult<PathBuf>,
) -> Result<PathBuf, String> {
    let set = env
        .filter(|v| !v.is_empty())
        .map(|v| ("NEWS_ROOT", PathBuf::from(v)))
        .or_else(|| recorded.map(|p| ("the root `news service install` recorded", p)));
    match set {
        Some((_, p)) if p.join("pipeline/eval").is_dir() => Ok(p),
        Some((what, p)) => Err(format!(
            "{what} is {}, which has no pipeline/eval/",
            p.display()
        )),
        None => here().map_err(|_| ROOT_UNSET.to_owned()),
    }
}

/// Chief's step handlers: Audit's and Huddle's (#19) and the preset steps built so far (`steps`, #20).
/// Every other preset step is `NotBuilt`, so such a run ends `failed` at that step. With no repository
/// root the steps that need one are bound too, and fail saying so.
pub fn step_handlers() -> news_chief::driver::Handlers {
    let h = news_critic::huddle::register(news_chief::driver::Handlers::default());
    match service_root() {
        Ok(root) => {
            let ctx = crate::steps::Ctx {
                root: root.clone(),
                preview_base: std::env::var("NEWS_PREVIEW_URL")
                    .unwrap_or_else(|_| news_home::preview::DEFAULT_URL.to_owned()),
                home: std::sync::Arc::new(crate::steps::HttpHome),
                tools: std::sync::Arc::new(crate::steps::tools::LocalTools {
                    site: std::env::var_os("NEWS_SITE_DIR")
                        .filter(|v| !v.is_empty())
                        .map_or_else(|| root.join("site"), PathBuf::from),
                }),
                press: std::sync::Arc::new(crate::steps::tools::HttpPress),
            };
            crate::steps::register(news_critic::audit::register(h, root), ctx)
        }
        Err(message) => {
            tracing::warn!("Audit and Frontpage cannot run: {message}");
            let h = news_critic::audit::register_unset(h, message.clone());
            crate::steps::register_unset(h, message)
        }
    }
}

/// The repository root the service works in ([`audit_root`] from this process's setting).
fn service_root() -> Result<PathBuf, String> {
    let env = std::env::var_os("NEWS_ROOT");
    let recorded = news_service::install::recorded_root();
    audit_root(env, recorded, news_critic::golden::find_root)
}

/// Install the handlers that need the service's pool: Rewrite (`page.revise`) and, given `claude`,
/// Ghost (`ai.*`, with [`NewsAccept`] as its accept sink and its calls in `job`, the root marker job).
/// Returns which were installed now (`false`: none given, or one already was).
///
/// [`NewsAccept`]: crate::steps::accept::NewsAccept
pub fn install_ai(
    db: &PgPool,
    root: &Path,
    claude: Option<PathBuf>,
    job: Option<Arc<news_win::Job>>,
) -> (bool, bool) {
    let rewrite = news_rewrite::install(news_rewrite::Context {
        db: db.clone(),
        work: news_proof::work::Work::new(root.join("work")),
    });
    // Plate (in NewsAccept) and ClickHouse (in PgStore) are reqwest clients, built with rustls and no
    // provider of its own: without this, building one panics in a process that has not installed it.
    news_wire::runner::install_tls_provider();
    let ghost = claude.is_some_and(|claude| {
        let accept = Arc::new(crate::steps::accept::NewsAccept {
            db: db.clone(),
            root: root.to_path_buf(),
        });
        let store = Arc::new(news_ghost::store::PgStore {
            db: db.clone(),
            clickhouse: news_telemetry::ClickHouse::from_env(),
        });
        let mut ghost = news_ghost::Ghost::new(claude, root.to_path_buf(), accept, store);
        ghost.job = job;
        news_ghost::install(ghost)
    });
    (ghost, rewrite)
}

/// [`install_ai`] for `news service run` (`host::Options::on_connect`), with `claude` at the path
/// `news service install` recorded (`news_win::tool_path`: the installed service refuses when none was;
/// a dev build falls back to PATH). Without a repository root or `claude` it leaves those handlers
/// out, so their jobs fail as `bug`, and returns the gap as warnings for the service log Board shows.
pub fn install_service_ai(db: &PgPool, job: Option<Arc<news_win::Job>>) -> Vec<String> {
    let (root, claude) = (service_root(), news_win::tool_path("claude"));
    let gaps = ai_gaps(&root, &claude);
    if let Ok(root) = root {
        install_ai(db, &root, claude.ok(), job);
    }
    gaps
}

/// What [`install_service_ai`] cannot install, as service-log lines.
fn ai_gaps(root: &Result<PathBuf, String>, claude: &std::io::Result<PathBuf>) -> Vec<String> {
    match (root, claude) {
        (Err(m), _) => vec![format!("Ghost and Rewrite are not installed: {m}")],
        (Ok(_), Err(e)) => vec![format!("Ghost is not installed: cannot find claude: {e}")],
        (Ok(_), Ok(_)) => vec![],
    }
}

/// The handler for a job kind, or `None` for an unknown kind.
pub fn resolve(kind: &str) -> Option<Handler> {
    news_dispatch::kinds::spec(kind).and_then(|s| handler_for_owner(s.owner))
}

#[cfg(test)]
mod tests {
    use super::*;
    use news_core::NewsError;

    #[test]
    #[allow(clippy::unwrap_used)]
    fn audit_root_prefers_the_setting_and_says_so_when_there_is_none() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let elsewhere = || Ok(PathBuf::from("/elsewhere"));
        // NEWS_ROOT beats the recorded root and the working directory.
        let got = audit_root(
            Some(repo.clone().into()),
            Some("/recorded".into()),
            elsewhere,
        );
        assert_eq!(got, Ok(repo.clone()));
        // The recorded root beats the working directory.
        assert_eq!(audit_root(None, Some(repo.clone()), elsewhere), Ok(repo));
        // A setting that points at no repository is an error naming it, not a fall-through.
        let bad = audit_root(Some("/no/such/repo".into()), None, elsewhere).unwrap_err();
        assert!(
            bad.contains("NEWS_ROOT") && bad.contains("pipeline/eval"),
            "{bad}"
        );
        // Nothing set: the working directory's repository, or the clear error.
        assert_eq!(audit_root(None, None, elsewhere), Ok("/elsewhere".into()));
        let none = audit_root(None, None, || Err(NewsError::precondition("x"))).unwrap_err();
        assert!(none.contains("repository root is not set") && none.contains("NEWS_ROOT"));
    }

    #[test]
    fn a_missing_root_or_claude_is_a_line_for_the_service_log() {
        // `claude` is recorded by `news service install` with the other tools.
        assert!(news_win::TOOLS.contains(&"claude"));
        // What `tool_path` says when the installed service has no path for it.
        let unrecorded = || {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no path for claude was recorded; run `news service install` again",
            ))
        };
        let root = Ok(PathBuf::from("/repo"));
        assert_eq!(
            ai_gaps(&Err("the repository root is not set".into()), &unrecorded()),
            ["Ghost and Rewrite are not installed: the repository root is not set"]
        );
        assert_eq!(
            ai_gaps(&root, &unrecorded()),
            [
                "Ghost is not installed: cannot find claude: no path for claude was recorded; \
              run `news service install` again"
            ]
        );
        assert!(ai_gaps(&root, &Ok(PathBuf::from("/bin/claude"))).is_empty());
    }

    #[test]
    fn every_kind_has_a_handler() {
        for s in news_dispatch::kinds::all() {
            assert!(resolve(s.kind).is_some(), "{}", s.kind);
        }
        assert!(resolve("no.such.kind").is_none());
    }

    #[test]
    fn subsystem_kind_lists_agree_with_the_registry() {
        let owned: &[(&str, &[&str])] = &[
            ("wire", news_wire::KINDS),
            ("clerk", news_clerk::KINDS),
            ("diary", news_diary::KINDS),
            ("taster", news_taster::KINDS),
            ("nose", news_nose::KINDS),
            ("fixer", news_fixer::KINDS),
            ("ghost", news_ghost::KINDS),
            ("proof", news_proof::KINDS),
            ("rewrite", news_rewrite::KINDS),
            ("press", news_press::KINDS),
            ("telemetry", news_telemetry::KINDS),
        ];
        let mut total = 0;
        for (owner, kinds) in owned {
            for k in *kinds {
                let spec =
                    news_dispatch::kinds::spec(k).unwrap_or_else(|| panic!("{k} not registered"));
                assert_eq!(spec.owner, *owner, "{k}");
                total += 1;
            }
        }
        assert_eq!(total, news_dispatch::kinds::all().count());
        // Subsystems without job kinds still exist as crates.
        assert!(news_contacts::KINDS.is_empty() && news_roster::KINDS.is_empty());
        assert!(news_chief::KINDS.is_empty() && news_critic::KINDS.is_empty());
    }

    /// The job kinds the preset steps queue: every `NewJob::new("<kind>")` in their crates' sources.
    #[allow(clippy::unwrap_used)]
    fn queued_kinds() -> std::collections::BTreeSet<String> {
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut kinds = std::collections::BTreeSet::new();
        for dir in ["news/src/steps", "news-rewrite/src", "news-critic/src"] {
            for entry in std::fs::read_dir(crates.join(dir)).unwrap() {
                let text = std::fs::read_to_string(entry.unwrap().path()).unwrap_or_default();
                for rest in text.split("NewJob::new(\"").skip(1) {
                    kinds.insert(rest.split('"').next().unwrap().to_owned());
                }
            }
        }
        kinds
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn the_service_installs_ghost_and_rewrite_for_every_ai_kind_the_presets_queue() {
        let mut kinds = queued_kinds();
        for k in ["ai.write", "ai.repair", "ai.spotcheck"] {
            assert!(kinds.contains(k), "{k} not found in {kinds:?}");
        }
        // No preset queues `page.revise` today (Followup revises in its step), but the service runs it.
        kinds.insert("page.revise".into());
        // No connection is made: every handler below stops at its arguments.
        let db = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost/unused")
            .unwrap();
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let installed = install_ai(&db, &repo, Some(PathBuf::from("claude")), None);
        assert_eq!(installed, (true, true), "(ghost, rewrite)");
        for kind in &kinds {
            let handler = resolve(kind).unwrap_or_else(|| panic!("no handler for {kind}"));
            // Wire's, Diary's and Taster's kinds are still stubs (#9, #12): not the AI path.
            let owner = news_dispatch::kinds::spec(kind).unwrap().owner;
            if !matches!(owner, "ghost" | "rewrite") {
                continue;
            }
            let input = news_core::job::JobInput {
                job_id: 1,
                kind: kind.clone(),
                pool: news_dispatch::kinds::spec(kind).unwrap().pool.to_owned(),
                attempt: 1,
                run_id: None,
                item_ref: None,
                args: serde_json::json!({}),
                lease: Default::default(),
            };
            // An installed handler gets as far as the job's (empty) arguments; one the service
            // never installed fails saying it needs the Bureau service.
            if let Err(f) = handler(input).await {
                assert!(
                    !f.message.contains("Bureau service"),
                    "{kind}: {}",
                    f.message
                );
            }
        }
    }

    #[tokio::test]
    async fn stub_handlers_fail_as_not_built_bugs() {
        let input = news_core::job::JobInput {
            job_id: 1,
            kind: "doc.fetch".into(),
            pool: "net".into(),
            attempt: 1,
            run_id: None,
            item_ref: None,
            args: serde_json::json!({}),
            lease: Default::default(),
        };
        let err = resolve("doc.fetch")
            .map(|h| h(input))
            .unwrap_or_else(|| panic!("no handler"))
            .await;
        let failure = err.err().unwrap_or_else(|| panic!("stub succeeded"));
        assert_eq!(failure.class, news_core::job::ErrorClass::Bug);
        assert!(failure.message.contains("#9"));
    }
}
