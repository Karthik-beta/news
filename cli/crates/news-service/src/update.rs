//! `service.update` (ADR 0007 decision 12): wait for a safe point (no AI call or publish in flight),
//! swap the binary, restart the service; live runs resume from PostgreSQL.
//!
//! The installed binary is `%ProgramFiles%\News\news.exe`, in a folder only administrators can write.
//! A new build is staged there by `news service stage <path>`, an elevated step that copies it to
//! `news.exe.new` and writes its SHA-256 to `news.exe.new.sha256` beside it (the service never
//! downloads anything). The service refuses an update when the hash file is missing or does not match
//! the staged file, both when the update is requested and again just before the swap. Once an update
//! is requested the service stops claiming `ai` and `publish` jobs, so the in-flight ones finish and
//! the safe point arrives. Then the executable is renamed aside and the staged one renamed into place
//! (Windows lets a running image be renamed), and the service stops with [`RESTART_EXIT_CODE`], which
//! the SCM's failure actions turn into a start of the new build. A run keeps its steps in PostgreSQL,
//! so Chief's recovery continues it at the next unfinished step.

use news_core::NewsError;
use news_store::{PgPool, map_err};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::io;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

/// The service-specific exit code that means "restart me, I was updated".
pub const RESTART_EXIT_CODE: u32 = 75;

/// Shared by the Hotline handler (which starts an update) and the host (which stops claims and
/// restarts).
#[derive(Debug, Default)]
pub struct UpdateGate {
    updating: AtomicBool,
    restart: AtomicBool,
    notify: Notify,
}

impl UpdateGate {
    /// Begin an update; `false` when one is already under way.
    pub fn begin(&self) -> bool {
        !self.updating.swap(true, Ordering::SeqCst)
    }

    /// Give up (the swap failed): claims resume.
    pub fn cancel(&self) {
        self.updating.store(false, Ordering::SeqCst);
    }

    pub fn updating(&self) -> bool {
        self.updating.load(Ordering::SeqCst)
    }

    pub fn request_restart(&self) {
        self.restart.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub fn restart_requested(&self) -> bool {
        self.restart.load(Ordering::SeqCst)
    }

    /// Resolves once a restart has been requested.
    pub async fn wait_restart(&self) {
        loop {
            let n = self.notify.notified();
            tokio::pin!(n);
            n.as_mut().enable();
            if self.restart_requested() {
                return;
            }
            n.await;
        }
    }

    /// The `pre_claim` rule: while updating, no `ai` or `publish` job is claimed.
    pub fn allows_claim(&self, pool: &str) -> bool {
        !(self.updating() && matches!(pool, "ai" | "publish"))
    }
}

/// Where the new build waits: `news.exe` becomes `news.exe.new`.
pub fn staged_path(exe: &Path) -> PathBuf {
    let mut name = exe.file_name().unwrap_or_default().to_os_string();
    name.push(".new");
    exe.with_file_name(name)
}

/// Where the staged build's SHA-256 is kept: `news.exe.new` has `news.exe.new.sha256`.
pub fn hash_path(staged: &Path) -> PathBuf {
    let mut name = staged.file_name().unwrap_or_default().to_os_string();
    name.push(".sha256");
    staged.with_file_name(name)
}

/// A file's SHA-256 as lower-case hex.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>())
}

/// Hash the staged file and record the hash beside it. `news service stage` does this, elevated, after
/// it copies the build into the admin-only folder, so the hash describes the copy the service will run.
pub fn record_staged_hash(staged: &Path) -> io::Result<String> {
    let hash = sha256_file(staged)?;
    std::fs::write(hash_path(staged), format!("{hash}\n"))?;
    Ok(hash)
}

/// Check the staged build against its recorded hash: refused when the build or the hash file is
/// missing, or when they differ. Returns the hash.
pub fn verify_staged(staged: &Path) -> Result<String, NewsError> {
    let stage_hint =
        "stage it with `news service stage <path to news.exe>` (asks for administrator)";
    if std::fs::metadata(staged).map_or(0, |m| m.len()) == 0 {
        return Err(
            NewsError::precondition(format!("no staged build at {}", staged.display()))
                .with_hint(stage_hint),
        );
    }
    let recorded = match std::fs::read_to_string(hash_path(staged)) {
        Ok(s) => s.trim().to_ascii_lowercase(),
        Err(e) => {
            return Err(NewsError::precondition(format!(
                "the staged build has no recorded hash ({e}); it was not staged by `news service stage`"
            ))
            .with_hint(stage_hint));
        }
    };
    let actual = sha256_file(staged)
        .map_err(|e| NewsError::internal(format!("cannot hash the staged build: {e}")))?;
    if recorded.len() != 64 || recorded != actual {
        return Err(NewsError::precondition(
            "the staged build does not match its recorded SHA-256: refusing to install it",
        )
        .with_hint(stage_hint));
    }
    Ok(actual)
}

/// Rename the installed binary aside (`news.exe.old`, replacing an older one) and the staged one into
/// place; put the old one back if the second rename fails.
pub fn swap(exe: &Path, staged: &Path) -> io::Result<()> {
    let mut name = exe.file_name().unwrap_or_default().to_os_string();
    name.push(".old");
    let old = exe.with_file_name(name);
    let _ = std::fs::remove_file(&old);
    std::fs::rename(exe, &old)?;
    if let Err(e) = std::fs::rename(staged, exe) {
        let _ = std::fs::rename(&old, exe);
        return Err(e);
    }
    Ok(())
}

/// The AI calls and publishes in flight, as the contract's `waiting_on`.
pub async fn in_flight(db: &PgPool) -> Result<Vec<Value>, NewsError> {
    let rows = sqlx::query(
        "select j.job_id, j.pool, r.run_id, r.short_id from jobs j join runs r on r.run_id = j.run_id \
         where j.state = 'running' and j.pool in ('ai', 'publish') order by j.job_id",
    )
    .fetch_all(db)
    .await
    .map_err(map_err)?;
    rows.iter()
        .map(|r| {
            let pool: String = r.try_get("pool").map_err(map_err)?;
            let run_id: uuid::Uuid = r.try_get("run_id").map_err(map_err)?;
            let short_id: String = r.try_get("short_id").map_err(map_err)?;
            let job_id: i64 = r.try_get("job_id").map_err(map_err)?;
            Ok(
                json!({"kind": if pool == "ai" { "ai_call" } else { "publish" },
                      "run_id": run_id, "short_id": short_id, "job_id": job_id}),
            )
        })
        .collect()
}

/// Wait for the safe point (polling every `poll`), check the staged build against its hash again,
/// swap the binary, and ask the host to restart. On a refused or failed swap the gate is cancelled so
/// claims resume, and the error is returned for the log.
pub async fn finish(
    db: &PgPool,
    gate: &UpdateGate,
    exe: &Path,
    staged: &Path,
    poll: std::time::Duration,
) -> Result<(), NewsError> {
    loop {
        match in_flight(db).await {
            Ok(w) if w.is_empty() => break,
            Ok(_) => {}
            Err(e) => {
                gate.cancel();
                return Err(e);
            }
        }
        tokio::time::sleep(poll).await;
    }
    if let Err(e) = verify_staged(staged) {
        gate.cancel();
        return Err(e);
    }
    if let Err(e) = swap(exe, staged) {
        gate.cancel();
        return Err(NewsError::internal(format!(
            "could not swap the binary: {e}"
        )));
    }
    // The hash belonged to the build now in place; the next update needs a new stage.
    let _ = std::fs::remove_file(hash_path(staged));
    gate.request_restart();
    Ok(())
}

/// The reply text for `service.update`.
pub fn text(waiting: &[Value]) -> String {
    if waiting.is_empty() {
        "No AI call or publish is in flight: swapping the binary and restarting the service now. \
         Restart Board when the service is back."
            .to_owned()
    } else {
        format!(
            "Waiting for {} AI call(s) and publish(es) to finish before swapping the binary; no new \
             ones start meanwhile.",
            waiting.len()
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn swap_replaces_the_binary_and_keeps_the_old_one() {
        let dir = std::env::temp_dir().join(format!("news-update-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("news.exe");
        std::fs::write(&exe, "old build").unwrap();
        std::fs::write(dir.join("news.exe.old"), "older").unwrap();
        let staged = staged_path(&exe);
        assert_eq!(staged, dir.join("news.exe.new"));
        std::fs::write(&staged, "new build").unwrap();
        swap(&exe, &staged).unwrap();
        assert_eq!(std::fs::read_to_string(&exe).unwrap(), "new build");
        assert_eq!(
            std::fs::read_to_string(dir.join("news.exe.old")).unwrap(),
            "old build"
        );
        assert!(!staged.exists());
        // A missing staged file leaves the installed one in place.
        assert!(swap(&exe, &staged).is_err());
        assert_eq!(std::fs::read_to_string(&exe).unwrap(), "new build");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_staged_build_is_refused_without_its_hash_or_when_it_differs() {
        let dir = std::env::temp_dir().join(format!("news-stage-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let staged = dir.join("news.exe.new");
        assert_eq!(hash_path(&staged), dir.join("news.exe.new.sha256"));
        // Nothing staged.
        assert!(verify_staged(&staged).is_err());
        // Copied there by hand, never through `news service stage`: no hash, refused.
        std::fs::write(&staged, "new build").unwrap();
        let e = verify_staged(&staged).unwrap_err();
        assert!(e.message.contains("no recorded hash"), "{}", e.message);
        // Staged properly.
        let h = record_staged_hash(&staged).unwrap();
        assert_eq!(h, sha256_file(&staged).unwrap());
        assert_eq!(verify_staged(&staged).unwrap(), h);
        // The build changed after it was staged: refused.
        std::fs::write(&staged, "tampered build").unwrap();
        let e = verify_staged(&staged).unwrap_err();
        assert!(e.message.contains("does not match"), "{}", e.message);
        // A hash file that is not a SHA-256: refused.
        std::fs::write(hash_path(&staged), "").unwrap();
        assert!(verify_staged(&staged).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_hash_is_the_files_sha256() {
        let dir = std::env::temp_dir().join(format!("news-sha-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("abc");
        std::fs::write(&f, "abc").unwrap();
        assert_eq!(
            sha256_file(&f).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn the_gate_stops_ai_and_publish_claims_until_cancelled_and_wakes_the_host() {
        let g = std::sync::Arc::new(UpdateGate::default());
        assert!(g.allows_claim("ai"));
        assert!(g.begin());
        assert!(!g.begin(), "one update at a time");
        assert!(!g.allows_claim("ai") && !g.allows_claim("publish"));
        assert!(g.allows_claim("net") && g.allows_claim("cpu"));
        g.cancel();
        assert!(g.allows_claim("ai"));
        let waiter = tokio::spawn({
            let g = g.clone();
            async move { g.wait_restart().await }
        });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        g.request_restart();
        tokio::time::timeout(std::time::Duration::from_secs(2), waiter)
            .await
            .unwrap()
            .unwrap();
    }

    #[test]
    fn the_text_says_what_the_service_is_doing() {
        assert!(text(&[]).contains("swapping the binary"));
        assert!(text(&[json!({})]).contains("Waiting for 1"));
    }
}
