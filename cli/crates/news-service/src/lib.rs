//! The Bureau service host, `news service run` (ADR 0008 §9), and the Hotline's command handlers.
//!
//! Start order: the log ring and event bus, the root marker job, the Hotline listener (`starting`:
//! only `service.ping` answers), then wait for PostgreSQL, take the singleton lock, migrate, recover
//! (the takeover fence, then pending pauses), and start the pools, Chief's tick and drivers and Rota's
//! tick (`up`). If the lock's connection drops, the service fences itself (stops claiming, stops the
//! drivers, terminates every process in the root job, cancels and awaits every worker task) before it
//! tries again.

pub mod docker;
pub mod handlers;
pub mod host;
pub mod install;
pub mod log;
pub mod reads;
pub mod update;

use news_chief::board::ServiceState;
use std::sync::atomic::{AtomicU8, Ordering};

/// The service's state, shared by the host and the Hotline handlers.
#[derive(Debug, Default)]
pub struct State(AtomicU8);

impl State {
    pub fn get(&self) -> ServiceState {
        match self.0.load(Ordering::SeqCst) {
            0 => ServiceState::Starting,
            1 => ServiceState::WaitingForDatabase,
            2 => ServiceState::Recovering,
            3 => ServiceState::Up,
            _ => ServiceState::Stopping,
        }
    }

    pub fn set(&self, s: ServiceState) {
        let n = match s {
            ServiceState::Starting => 0,
            ServiceState::WaitingForDatabase => 1,
            ServiceState::Recovering => 2,
            ServiceState::Up => 3,
            ServiceState::Stopping | ServiceState::Down => 4,
        };
        self.0.store(n, Ordering::SeqCst);
    }
}

/// `news run status` in a terminal: the service's state from a `service.ping` with a 300 ms timeout,
/// `down` when there is no answer, or when the pipe's server is not this `news.exe`.
pub async fn ping_state() -> ServiceState {
    #[cfg(windows)]
    {
        let attempt = async {
            let mut client = news_win::connect_client(news_hotline::PIPE_NAME).ok()?;
            let me = std::env::current_exe().ok()?;
            let server = news_win::server_image(&client).ok()?;
            if !news_win::same_image(&server, &me.to_string_lossy()) {
                return None;
            }
            let env = news_hotline::call(
                &mut client,
                "cli-ping",
                "service.ping",
                serde_json::json!({}),
            )
            .await
            .ok()?;
            let state = env["data"]["state"].as_str()?.to_owned();
            Some(match state.as_str() {
                "starting" => ServiceState::Starting,
                "waiting_for_database" => ServiceState::WaitingForDatabase,
                "recovering" => ServiceState::Recovering,
                "up" => ServiceState::Up,
                _ => ServiceState::Stopping,
            })
        };
        tokio::time::timeout(std::time::Duration::from_millis(300), attempt)
            .await
            .ok()
            .flatten()
            .unwrap_or(ServiceState::Down)
    }
    #[cfg(not(windows))]
    {
        ServiceState::Down
    }
}
