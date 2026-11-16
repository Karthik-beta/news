//! The job contract between Dispatch and subsystem handlers (Bureau Design §5.4).
//!
//! A handler takes a claimed job and returns its result JSON, or a [`JobFailure`] whose
//! [`ErrorClass`] decides what Dispatch does next: retry with backoff, snooze, pause the pool or discard.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// Error classes and their retry policy (§5.4, "Retry policy by error class").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorClass {
    Transient,
    RateLimited,
    AiUsageLimit,
    AiInvalidOutput,
    CheckFailed,
    UpstreamChanged,
    InvalidData,
    LicenceBlocked,
    Bug,
}

/// What Dispatch does with a failed job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// `retryable` after full-jitter backoff; discarded when attempts run out.
    Retry,
    /// `snoozed` until the given time; the attempt is given back.
    Snooze,
    /// Pause the job's pool until the given time, and snooze the job to then.
    PausePool,
    /// `discarded`: the dead list. Only Salvage (or Karthik) re-enqueues it.
    Discard,
}

impl ErrorClass {
    pub const ALL: [ErrorClass; 9] = [
        ErrorClass::Transient,
        ErrorClass::RateLimited,
        ErrorClass::AiUsageLimit,
        ErrorClass::AiInvalidOutput,
        ErrorClass::CheckFailed,
        ErrorClass::UpstreamChanged,
        ErrorClass::InvalidData,
        ErrorClass::LicenceBlocked,
        ErrorClass::Bug,
    ];

    pub const fn policy(self) -> Policy {
        match self {
            ErrorClass::Transient => Policy::Retry,
            ErrorClass::RateLimited => Policy::Snooze,
            ErrorClass::AiUsageLimit => Policy::PausePool,
            ErrorClass::AiInvalidOutput
            | ErrorClass::CheckFailed
            | ErrorClass::UpstreamChanged
            | ErrorClass::InvalidData
            | ErrorClass::LicenceBlocked
            | ErrorClass::Bug => Policy::Discard,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorClass::Transient => "transient",
            ErrorClass::RateLimited => "rate_limited",
            ErrorClass::AiUsageLimit => "ai_usage_limit",
            ErrorClass::AiInvalidOutput => "ai_invalid_output",
            ErrorClass::CheckFailed => "check_failed",
            ErrorClass::UpstreamChanged => "upstream_changed",
            ErrorClass::InvalidData => "invalid_data",
            ErrorClass::LicenceBlocked => "licence_blocked",
            ErrorClass::Bug => "bug",
        }
    }
}

impl FromStr for ErrorClass {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ErrorClass::ALL
            .into_iter()
            .find(|c| c.as_str() == s)
            .ok_or_else(|| {
                let names: Vec<&str> = ErrorClass::ALL.iter().map(|c| c.as_str()).collect();
                format!("unknown error class {s:?}; one of {}", names.join(", "))
            })
    }
}

impl std::fmt::Display for ErrorClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A claimed job as a handler sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobInput {
    pub job_id: i64,
    pub kind: String,
    pub pool: String,
    pub attempt: i16,
    pub run_id: Option<Uuid>,
    pub item_ref: Option<String>,
    pub args: Value,
    /// The claim this run holds. Call [`JobLease::ensure_lease`] before every external side effect.
    #[serde(skip)]
    pub lease: JobLease,
}

/// Checks that a claim still holds its lease. Implemented by Dispatch (one indexed select); this
/// crate does no I/O.
pub trait LeaseCheck: Send + Sync {
    fn ensure(&self) -> Pin<Box<dyn Future<Output = Result<(), JobFailure>> + Send + '_>>;
}

/// The lease a handler runs under: the job and the claim's `lease_id`, plus the check behind
/// [`ensure_lease`](Self::ensure_lease).
///
/// Queue state is exactly-once per lease, but a handler whose lease was lost keeps running until the
/// worker notices and cancels it, while the job may already run under a new lease elsewhere. So a
/// handler's side effects must be idempotent (keyed as in Bureau Design §5.8) and each external write
/// must be preceded by `ensure_lease().await?`: a stale holder then stops before writing, and an
/// overlap can at worst repeat an idempotent write already in flight.
#[derive(Clone, Default)]
pub struct JobLease {
    pub job_id: i64,
    pub lease_id: Option<Uuid>,
    check: Option<Arc<dyn LeaseCheck>>,
}

impl JobLease {
    pub fn new(job_id: i64, lease_id: Option<Uuid>, check: Arc<dyn LeaseCheck>) -> Self {
        Self {
            job_id,
            lease_id,
            check: Some(check),
        }
    }

    /// `Ok` while this claim still holds the job; a `transient` failure once it does not, and a `bug`
    /// failure for input that no Dispatch worker handed out.
    pub async fn ensure_lease(&self) -> Result<(), JobFailure> {
        match &self.check {
            Some(c) => c.ensure().await,
            None => Err(JobFailure::new(
                ErrorClass::Bug,
                format!("job {} has no lease to check", self.job_id),
            )),
        }
    }
}

impl std::fmt::Debug for JobLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobLease")
            .field("job_id", &self.job_id)
            .field("lease_id", &self.lease_id)
            .finish_non_exhaustive()
    }
}

impl PartialEq for JobLease {
    fn eq(&self, other: &Self) -> bool {
        (self.job_id, self.lease_id) == (other.job_id, other.lease_id)
    }
}

/// Why a job failed, and when to try again for classes that wait.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[error("{class}: {message}")]
pub struct JobFailure {
    pub class: ErrorClass,
    pub message: String,
    /// For `rate_limited` and `ai_usage_limit`: how long to wait (from `Retry-After`).
    #[serde(skip_serializing_if = "Option::is_none", with = "opt_secs", default)]
    pub retry_after: Option<Duration>,
}

impl JobFailure {
    pub fn new(class: ErrorClass, message: impl Into<String>) -> Self {
        Self {
            class,
            message: message.into(),
            retry_after: None,
        }
    }

    pub fn with_retry_after(mut self, d: Duration) -> Self {
        self.retry_after = Some(d);
        self
    }

    /// A handler that exists in the registry but whose subsystem is not built yet.
    pub fn not_built(kind: &str, issue: &str) -> Self {
        Self::new(
            ErrorClass::Bug,
            format!("handler for {kind} is not built yet ({issue})"),
        )
    }
}

mod opt_secs {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(v: &Option<Duration>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(d) => s.serialize_u64(d.as_secs()),
            None => s.serialize_none(),
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Duration>, D::Error> {
        Ok(Option::<u64>::deserialize(d)?.map(Duration::from_secs))
    }
}

pub type JobFuture = Pin<Box<dyn Future<Output = Result<Value, JobFailure>> + Send>>;

/// A subsystem's job handler.
pub type Handler = fn(JobInput) -> JobFuture;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn policies_follow_the_table() {
        assert_eq!(ErrorClass::Transient.policy(), Policy::Retry);
        assert_eq!(ErrorClass::RateLimited.policy(), Policy::Snooze);
        assert_eq!(ErrorClass::AiUsageLimit.policy(), Policy::PausePool);
        for c in [
            ErrorClass::AiInvalidOutput,
            ErrorClass::CheckFailed,
            ErrorClass::UpstreamChanged,
            ErrorClass::InvalidData,
            ErrorClass::LicenceBlocked,
            ErrorClass::Bug,
        ] {
            assert_eq!(c.policy(), Policy::Discard, "{c}");
        }
    }

    #[test]
    fn classes_round_trip_through_their_names() {
        for c in ErrorClass::ALL {
            assert_eq!(c.as_str().parse::<ErrorClass>().unwrap(), c);
            assert_eq!(serde_json::to_value(c).unwrap(), c.as_str());
        }
        assert!("nope".parse::<ErrorClass>().is_err());
    }

    #[test]
    fn failure_serialises_retry_after_in_seconds() {
        let f = JobFailure::new(ErrorClass::RateLimited, "429")
            .with_retry_after(Duration::from_secs(30));
        let v = serde_json::to_value(&f).unwrap();
        assert_eq!(v["retry_after"], 30);
        let back: JobFailure = serde_json::from_value(v).unwrap();
        assert_eq!(back, f);
    }
}
