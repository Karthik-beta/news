//! Secrets (ADR 0007 decision 11). Board writes them to Windows Credential Manager under Karthik's
//! account and the service reads them when a step needs one. A secret is typed into a field, goes to
//! `news_win::cred_write` and nowhere else: it is not sent over the Hotline (no command carries one),
//! not stored in PostgreSQL, not logged, and its `Debug` output is masked. Board shows only whether
//! one is set.

use news_core::secrets::{SECRETS, Secret, target};

/// A secret being typed. Cleared on save and on drop; never printed.
#[derive(Default)]
pub struct Entry(String);

impl std::fmt::Debug for Entry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Entry(<{} characters>)", self.0.chars().count())
    }
}

impl Entry {
    pub fn buffer(&mut self) -> &mut String {
        &mut self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }

    /// Take the typed text, leaving the field empty.
    fn take(&mut self) -> String {
        std::mem::take(&mut self.0).trim().to_owned()
    }
}

impl Drop for Entry {
    fn drop(&mut self) {
        let mut b = std::mem::take(&mut self.0).into_bytes();
        b.fill(0);
    }
}

/// What the writer needs from the platform, so tests can use a fake store.
pub trait Store {
    fn write(&self, target: &str, secret: &str) -> std::io::Result<()>;
    fn exists(&self, target: &str) -> bool;
    fn delete(&self, target: &str) -> std::io::Result<()>;
}

/// Windows Credential Manager.
pub struct Credman;

impl Store for Credman {
    fn write(&self, target: &str, secret: &str) -> std::io::Result<()> {
        news_win::cred_write(target, secret)
    }
    fn exists(&self, target: &str) -> bool {
        news_win::cred_exists(target)
    }
    fn delete(&self, target: &str) -> std::io::Result<()> {
        news_win::cred_delete(target)
    }
}

/// Save what was typed for `secret`; the field is emptied either way.
pub fn save(store: &dyn Store, secret: &Secret, entry: &mut Entry) -> Result<String, String> {
    let value = entry.take();
    if value.is_empty() {
        return Err(format!("Type the {} first.", secret.label));
    }
    store
        .write(&target(secret.name), &value)
        .map_err(|e| format!("Windows would not store {}: {e}", secret.label))?;
    Ok(format!(
        "{} saved to Windows Credential Manager.",
        secret.label
    ))
}

pub fn remove(store: &dyn Store, secret: &Secret) -> Result<String, String> {
    store
        .delete(&target(secret.name))
        .map_err(|e| format!("Could not remove {}: {e}", secret.label))?;
    Ok(format!("{} removed.", secret.label))
}

/// Which secrets are set, in registry order.
pub fn which_are_set(store: &dyn Store) -> Vec<(&'static Secret, bool)> {
    SECRETS
        .iter()
        .map(|s| (s, store.exists(&target(s.name))))
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Fake(RefCell<HashMap<String, String>>);

    impl Store for Fake {
        fn write(&self, t: &str, s: &str) -> std::io::Result<()> {
            self.0.borrow_mut().insert(t.to_owned(), s.to_owned());
            Ok(())
        }
        fn exists(&self, t: &str) -> bool {
            self.0.borrow().contains_key(t)
        }
        fn delete(&self, t: &str) -> std::io::Result<()> {
            self.0.borrow_mut().remove(t);
            Ok(())
        }
    }

    const SENTINEL: &str = "sentinel-secret-7f3a9c";

    #[test]
    fn saving_stores_under_the_news_target_clears_the_field_and_never_echoes_the_value() {
        let store = Fake::default();
        let secret = &SECRETS[0];
        let mut e = Entry::default();
        e.buffer().push_str(SENTINEL);
        assert!(format!("{e:?}").find(SENTINEL).is_none());
        let msg = save(&store, secret, &mut e).unwrap();
        assert!(!msg.contains(SENTINEL));
        assert!(e.is_empty(), "the field is emptied");
        assert_eq!(
            store.0.borrow().get("news/write-api-key").unwrap(),
            SENTINEL
        );
        assert!(
            which_are_set(&store)
                .iter()
                .any(|(s, set)| s.name == "write-api-key" && *set)
        );
        remove(&store, secret).unwrap();
        assert!(!store.exists("news/write-api-key"));
        assert!(
            save(&store, secret, &mut e).is_err(),
            "an empty field saves nothing"
        );
    }

    /// The sentinel test of the issue: a secret written through Board's code path is in the
    /// credential store and in no request Board could send, no message it shows, and no `Debug`.
    #[cfg(windows)]
    #[test]
    fn a_real_credential_round_trips_and_is_deleted() {
        let name = format!("news-test/board-{}", uuid_like());
        news_win::cred_write(&name, SENTINEL).unwrap();
        assert!(news_win::cred_exists(&name));
        assert_eq!(news_win::cred_read(&name).as_deref(), Some(SENTINEL));
        news_win::cred_delete(&name).unwrap();
        assert!(!news_win::cred_exists(&name));
        // Deleting what is not there is fine.
        news_win::cred_delete(&name).unwrap();
    }

    #[cfg(windows)]
    fn uuid_like() -> String {
        format!(
            "{}-{}",
            std::process::id(),
            std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos()
        )
    }
}
