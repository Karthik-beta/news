//! Permission tiers (Bureau Design §6.3).
//!
//! R read-only · W writes run-scoped state · G gated (runs only as the authorised step of a live run;
//! checked by Chief, #10) · H human only: Karthik's actions, accepted only from Board over the Hotline
//! (ADR 0007).
//!
//! No H-tier command has a CLI form, and nothing here looks at the calling shell's environment: the
//! boundary is the service's Hotline (client check, spawn marker) and the G-tier rule with approvals
//! bound to `subject_sha` (§5.5, §9.3).

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Tier {
    R,
    W,
    G,
    H,
}
