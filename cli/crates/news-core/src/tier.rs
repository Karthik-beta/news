//! Permission tiers (Bureau Design §6.3).
//!
//! R read-only · W writes run-scoped state · G gated (runs only as the authorised step of a live run;
//! checked by Chief, #10) · H human only (Karthik in a terminal, or the prompt hook).
//!
//! The H check here is a guardrail, not the security boundary (§6.1): it refuses when the process runs
//! inside a Claude Code tool shell, which sets `CLAUDECODE=1`. The boundary is the deny rules in
//! `.claude/settings.json`, start tickets and approvals (§5.5).

use crate::error::NewsError;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Tier {
    R,
    W,
    G,
    H,
}

/// Environment variable Claude Code sets in its tool shells.
pub const AGENT_SHELL_VAR: &str = "CLAUDECODE";

pub fn in_agent_shell_with(lookup: impl Fn(&str) -> Option<String>) -> bool {
    lookup(AGENT_SHELL_VAR).is_some_and(|v| !v.is_empty() && v != "0")
}

pub fn in_agent_shell() -> bool {
    in_agent_shell_with(|n| std::env::var(n).ok())
}

/// Refuse an H-tier command in an agent's tool shell: `gated`, exit 7.
pub fn require_human(command: &str) -> Result<(), NewsError> {
    require_human_with(command, |n| std::env::var(n).ok())
}

pub fn require_human_with(
    command: &str,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<(), NewsError> {
    if in_agent_shell_with(lookup) {
        Err(
            NewsError::gated(format!("`news {command}` is for Karthik in a terminal"))
                .with_hint("run it yourself in a terminal outside Claude Code"),
        )
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn human_commands_refuse_in_agent_shells() {
        let agent = |n: &str| (n == AGENT_SHELL_VAR).then(|| "1".to_owned());
        let err = require_human_with("jobs pause", agent).unwrap_err();
        assert_eq!(err.exit_code(), 7);
        assert!(require_human_with("jobs pause", |_| None).is_ok());
        assert!(require_human_with("jobs pause", |_| Some("0".into())).is_ok());
    }
}
