//! Binds each job kind's owner (from `news_dispatch::KINDS`) to its subsystem crate's handler.

use news_core::NewsResult;
use news_core::job::Handler;
use std::path::PathBuf;

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
    let env = std::env::var_os("NEWS_ROOT");
    let recorded = news_service::install::recorded_root();
    match audit_root(env, recorded, news_critic::golden::find_root) {
        Ok(root) => {
            let ctx = crate::steps::Ctx {
                root: root.clone(),
                preview_base: std::env::var("NEWS_PREVIEW_URL")
                    .unwrap_or_else(|_| news_home::preview::DEFAULT_URL.to_owned()),
                home: std::sync::Arc::new(crate::steps::HttpHome),
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
