//! Binds each job kind's owner (from `news_dispatch::KINDS`) to its subsystem crate's handler.

use news_core::job::Handler;

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
        "board" => news_board::HANDLER,
        _ => return None,
    })
}

/// The handler for a job kind, or `None` for an unknown kind.
pub fn resolve(kind: &str) -> Option<Handler> {
    news_dispatch::kinds::spec(kind).and_then(|s| handler_for_owner(s.owner))
}

#[cfg(test)]
mod tests {
    use super::*;

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
            ("board", news_board::KINDS),
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
