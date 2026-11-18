//! `service.update` (ADR 0007 decision 12): wait for a safe point (no AI call or publish in flight),
//! swap the binary, restart the service; live runs resume from PostgreSQL.
//!
//! The new build is staged next to the installed one as `news.exe.new` (the build step copies it
//! there; the service never downloads anything). Once an update is requested the service stops
//! claiming `ai` and `publish` jobs, so the in-flight ones finish and the safe point arrives. Then
//! the executable is renamed aside and the staged one renamed into place (Windows lets a running
//! image be renamed), and the service stops with [`RESTART_EXIT_CODE`], which the SCM's failure
//! actions turn into a start of the new build. A run keeps its steps in PostgreSQL, so Chief's
//! recovery continues it at the next unfinished step.

use news_core::NewsError;
use news_store::{PgPool, map_err};
use serde_json::{Value, json};
use sqlx::Row;
use std::io;
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

/// Wait for the safe point (polling every `poll`), swap the binary, and ask the host to restart. On a
/// failed swap the gate is cancelled so claims resume, and the error is returned for the log.
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
    if let Err(e) = swap(exe, staged) {
        gate.cancel();
        return Err(NewsError::internal(format!(
            "could not swap the binary: {e}"
        )));
    }
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
