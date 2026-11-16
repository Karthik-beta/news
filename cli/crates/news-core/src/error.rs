//! Stable error codes and the error shape (Bureau Design §6.2).
//!
//! The codes are a public contract: agents, Ghost and Chief branch on them. Add codes; never rename one.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

/// The stable error codes. Serialised in `snake_case`, exactly as listed in §6.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Usage,
    NotFound,
    Conflict,
    Precondition,
    Gated,
    ValidationFailed,
    RateLimited,
    UpstreamUnavailable,
    LicenceBlocked,
    BudgetExceeded,
    Internal,
}

impl ErrorCode {
    /// Every code, in the order of §6.2.
    pub const ALL: [ErrorCode; 11] = [
        ErrorCode::Usage,
        ErrorCode::NotFound,
        ErrorCode::Conflict,
        ErrorCode::Precondition,
        ErrorCode::Gated,
        ErrorCode::ValidationFailed,
        ErrorCode::RateLimited,
        ErrorCode::UpstreamUnavailable,
        ErrorCode::LicenceBlocked,
        ErrorCode::BudgetExceeded,
        ErrorCode::Internal,
    ];

    /// Process exit code: 2 usage · 3 not found · 4 conflict or precondition failed · 5 retryable
    /// (network, lock, busy) · 6 validation failed · 7 gated · 10 internal bug. Success is 0.
    ///
    /// `licence_blocked` and `budget_exceeded` are preconditions that waiting will not fix, so they
    /// share 4; `rate_limited` and `upstream_unavailable` are the retryable ones (5).
    pub const fn exit_code(self) -> i32 {
        match self {
            ErrorCode::Usage => 2,
            ErrorCode::NotFound => 3,
            ErrorCode::Conflict
            | ErrorCode::Precondition
            | ErrorCode::LicenceBlocked
            | ErrorCode::BudgetExceeded => 4,
            ErrorCode::RateLimited | ErrorCode::UpstreamUnavailable => 5,
            ErrorCode::ValidationFailed => 6,
            ErrorCode::Gated => 7,
            ErrorCode::Internal => 10,
        }
    }

    /// Whether repeating the same call later can succeed without any other change.
    pub const fn retryable(self) -> bool {
        matches!(
            self,
            ErrorCode::RateLimited | ErrorCode::UpstreamUnavailable
        )
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Usage => "usage",
            ErrorCode::NotFound => "not_found",
            ErrorCode::Conflict => "conflict",
            ErrorCode::Precondition => "precondition",
            ErrorCode::Gated => "gated",
            ErrorCode::ValidationFailed => "validation_failed",
            ErrorCode::RateLimited => "rate_limited",
            ErrorCode::UpstreamUnavailable => "upstream_unavailable",
            ErrorCode::LicenceBlocked => "licence_blocked",
            ErrorCode::BudgetExceeded => "budget_exceeded",
            ErrorCode::Internal => "internal",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The `error` object of a failed envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[error("{code}: {message}")]
pub struct NewsError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Structured context, for example the run that blocks a start.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
    /// Code-rendered text for Karthik, printed verbatim by the `/bureau` session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl NewsError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: code.retryable(),
            hint: None,
            details: None,
            text: None,
        }
    }

    pub fn usage(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Usage, message)
    }
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Conflict, message)
    }
    pub fn precondition(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Precondition, message)
    }
    pub fn gated(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Gated, message)
    }
    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::ValidationFailed, message)
    }
    pub fn upstream(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::UpstreamUnavailable, message)
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }

    /// The error every not-yet-built command returns: `precondition`, exit 4.
    pub fn not_built(command: &str, issue: &str) -> Self {
        Self::precondition(format!("`news {command}` is not built yet"))
            .with_hint(format!("planned in {issue}; see cli/README.md, \"Stubs\""))
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub const fn exit_code(&self) -> i32 {
        self.code.exit_code()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_follow_the_contract() {
        let expected = [
            ("usage", 2),
            ("not_found", 3),
            ("conflict", 4),
            ("precondition", 4),
            ("gated", 7),
            ("validation_failed", 6),
            ("rate_limited", 5),
            ("upstream_unavailable", 5),
            ("licence_blocked", 4),
            ("budget_exceeded", 4),
            ("internal", 10),
        ];
        assert_eq!(expected.len(), ErrorCode::ALL.len());
        for (code, (name, exit)) in ErrorCode::ALL.iter().zip(expected) {
            assert_eq!(code.as_str(), name);
            assert_eq!(code.exit_code(), exit, "{name}");
            assert_eq!(serde_json::to_value(code).unwrap(), Value::from(name));
        }
    }

    #[test]
    fn no_error_exits_zero_or_one() {
        for code in ErrorCode::ALL {
            assert!(code.exit_code() >= 2, "{code}");
        }
    }

    #[test]
    fn only_rate_limits_and_outages_are_retryable() {
        let retryable: Vec<_> = ErrorCode::ALL
            .into_iter()
            .filter(|c| c.retryable())
            .collect();
        assert_eq!(
            retryable,
            [ErrorCode::RateLimited, ErrorCode::UpstreamUnavailable]
        );
        for code in ErrorCode::ALL {
            assert_eq!(code.retryable(), code.exit_code() == 5, "{code}");
        }
    }

    #[test]
    fn error_shape_omits_empty_optionals() {
        let json = serde_json::to_value(NewsError::not_found("story 81234 not found")).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"code":"not_found","message":"story 81234 not found","retryable":false})
        );
    }

    #[test]
    fn not_built_is_a_precondition() {
        let e = NewsError::not_built("discover", "#9");
        assert_eq!(e.code, ErrorCode::Precondition);
        assert_eq!(e.exit_code(), 4);
        assert!(e.hint.unwrap().contains("#9"));
    }
}
