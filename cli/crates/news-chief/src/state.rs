//! The run state machine (ADR 0008 §3). [`transition`] performs every non-terminal change and refuses a
//! pair the table does not allow; terminal statuses are written only by `wrap::finish`.

use chrono::{DateTime, Utc};
use news_core::NewsError;
use news_store::map_err;
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Status {
    Queued,
    Running,
    WaitingApproval,
    WaitingAi,
    Pausing,
    Paused,
    Succeeded,
    Failed,
    Cancelled,
    Skipped,
}

use Status::*;

impl Status {
    pub const ALL: [Status; 10] = [
        Queued,
        Running,
        WaitingApproval,
        WaitingAi,
        Pausing,
        Paused,
        Succeeded,
        Failed,
        Cancelled,
        Skipped,
    ];
    pub const LIVE: [Status; 6] = [Queued, Running, WaitingAi, WaitingApproval, Pausing, Paused];

    pub fn as_str(self) -> &'static str {
        match self {
            Queued => "queued",
            Running => "running",
            WaitingApproval => "waiting_approval",
            WaitingAi => "waiting_ai",
            Pausing => "pausing",
            Paused => "paused",
            Succeeded => "succeeded",
            Failed => "failed",
            Cancelled => "cancelled",
            Skipped => "skipped",
        }
    }

    pub fn parse(s: &str) -> Option<Status> {
        Status::ALL.into_iter().find(|x| x.as_str() == s)
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Succeeded | Failed | Cancelled | Skipped)
    }

    /// Holds the heavy slot (when the run is heavy): the `runs_one_heavy` index.
    pub fn holds_slot(self) -> bool {
        matches!(self, Running | WaitingAi | Pausing)
    }
}

/// Every allowed change (ADR 0008 §3). `None` is a new run.
pub fn allowed(from: Option<Status>, to: Status) -> bool {
    match from {
        None => matches!(to, Running | Queued | Skipped),
        Some(f) => matches!(
            (f, to),
            (Queued, Running | Skipped | Cancelled)
                | (
                    Running,
                    WaitingAi | WaitingApproval | Pausing | Succeeded | Failed
                )
                | (
                    WaitingAi,
                    Running | WaitingApproval | Pausing | Succeeded | Failed
                )
                | (WaitingApproval, Running | Queued | Cancelled)
                | (Pausing, Pausing | Paused | Cancelled)
                | (Paused, Running | Queued | Cancelled)
        ),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopMode {
    Drain,
    Now,
    Cancel,
}

impl StopMode {
    pub fn as_str(self) -> &'static str {
        match self {
            StopMode::Drain => "drain",
            StopMode::Now => "now",
            StopMode::Cancel => "cancel",
        }
    }
    pub fn parse(s: &str) -> Option<StopMode> {
        [StopMode::Drain, StopMode::Now, StopMode::Cancel]
            .into_iter()
            .find(|m| m.as_str() == s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PausedReason {
    Karthik,
    UsageLimit,
    Quota,
    Update,
}

impl PausedReason {
    pub fn as_str(self) -> &'static str {
        match self {
            PausedReason::Karthik => "karthik",
            PausedReason::UsageLimit => "usage_limit",
            PausedReason::Quota => "quota",
            PausedReason::Update => "update",
        }
    }
}

/// A change to apply with [`transition`].
#[derive(Debug, Clone, Copy)]
pub struct Change {
    pub to: Status,
    pub stop_mode: Option<StopMode>,
    pub paused_reason: Option<PausedReason>,
    pub resume_at: Option<DateTime<Utc>>,
    /// Join the Lineup again at rank 1 (`queued_at` = now): a resume or approval with the slot busy.
    pub rejoin: bool,
}

impl Change {
    pub fn to(to: Status) -> Self {
        Self {
            to,
            stop_mode: None,
            paused_reason: None,
            resume_at: None,
            rejoin: false,
        }
    }
}

/// Change a run's status from one of `from` (the row is updated only while it is in one of them).
/// Starting (`running`, `waiting_ai`) clears `start_by` and sets `started_at` the first time; leaving
/// `pausing`/`paused` clears the pause fields. `precondition` when the run is in another status.
pub async fn transition(
    conn: &mut PgConnection,
    run_id: Uuid,
    from: &[Status],
    ch: Change,
    now: DateTime<Utc>,
) -> Result<(), NewsError> {
    if ch.to.is_terminal() {
        return Err(NewsError::internal(
            "terminal statuses are written only by wrap::finish",
        ));
    }
    if let Some(bad) = from.iter().find(|f| !allowed(Some(**f), ch.to)) {
        return Err(NewsError::internal(format!(
            "transition {} -> {} is not in the table",
            bad.as_str(),
            ch.to.as_str()
        )));
    }
    let from: Vec<&str> = from.iter().map(|s| s.as_str()).collect();
    let done = sqlx::query(
        "update runs set status = $2,
                stop_mode = case when $2 = 'pausing' then $3 else null end,
                paused_reason = case when $2 in ('pausing', 'paused') then coalesce($4, paused_reason) else null end,
                resume_at = case when $2 in ('pausing', 'paused') then coalesce($5, resume_at) else null end,
                lineup_rank = case when $6 then 1 else lineup_rank end,
                queued_at = case when $6 then $7 else queued_at end,
                start_by = case when $2 in ('running', 'waiting_ai') then null else start_by end,
                started_at = case when $2 in ('running', 'waiting_ai') then coalesce(started_at, $7) else started_at end,
                last_seen_at = case when $2 in ('running', 'waiting_ai') then $7 else last_seen_at end
          where run_id = $1 and status = any($8)",
    )
    .bind(run_id)
    .bind(ch.to.as_str())
    .bind(ch.stop_mode.map(StopMode::as_str))
    .bind(ch.paused_reason.map(PausedReason::as_str))
    .bind(ch.resume_at)
    .bind(ch.rejoin)
    .bind(now)
    .bind(&from)
    .execute(&mut *conn)
    .await
    .map_err(map_err)?;
    if done.rows_affected() == 1 {
        return Ok(());
    }
    let status: Option<String> = sqlx::query_scalar("select status from runs where run_id = $1")
        .bind(run_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_err)?;
    Err(match status {
        None => NewsError::not_found(format!("run {run_id} not found")),
        Some(s) => NewsError::precondition(format!(
            "the run is {s}; it must be {} to become {}",
            from.join(" or "),
            ch.to.as_str()
        )),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn the_transition_table_is_exactly_adr_0008() {
        let expected: &[(Option<Status>, Status)] = &[
            (None, Running),
            (None, Queued),
            (None, Skipped),
            (Some(Queued), Running),
            (Some(Queued), Skipped),
            (Some(Queued), Cancelled),
            (Some(Running), WaitingAi),
            (Some(WaitingAi), Running),
            (Some(Running), WaitingApproval),
            (Some(WaitingAi), WaitingApproval),
            (Some(WaitingApproval), Running),
            (Some(WaitingApproval), Queued),
            (Some(Running), Pausing),
            (Some(WaitingAi), Pausing),
            (Some(Pausing), Pausing),
            (Some(Pausing), Paused),
            (Some(Pausing), Cancelled),
            (Some(WaitingApproval), Cancelled),
            (Some(Paused), Cancelled),
            (Some(Paused), Running),
            (Some(Paused), Queued),
            (Some(Running), Succeeded),
            (Some(WaitingAi), Succeeded),
            (Some(Running), Failed),
            (Some(WaitingAi), Failed),
        ];
        let froms = std::iter::once(None).chain(Status::ALL.into_iter().map(Some));
        for from in froms {
            for to in Status::ALL {
                assert_eq!(
                    allowed(from, to),
                    expected.contains(&(from, to)),
                    "{:?} -> {to:?}",
                    from
                );
            }
        }
    }

    #[test]
    fn terminal_runs_never_move() {
        for t in Status::ALL.into_iter().filter(|s| s.is_terminal()) {
            assert!(Status::ALL.into_iter().all(|to| !allowed(Some(t), to)));
            assert!(!t.holds_slot());
            assert!(!Status::LIVE.contains(&t));
        }
        assert_eq!(Status::parse("waiting_ai"), Some(WaitingAi));
    }
}
