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

/// A gate's `subject_sha`: SHA-256 of the canonical JSON (keys sorted) of exactly what Board shows,
/// `{"preview": …, "summary": …}`. Chief stores the subject's own hash and items inside the summary
/// when it opens the gate, so this binds the subject too, and Board recomputes it from the gate it
/// displays instead of echoing the service's value (security review on #10, finding 4). A JSON
/// `null` preview is the same as none.
pub fn gate_sha(summary: &serde_json::Value, preview: Option<&serde_json::Value>) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let preview = preview.filter(|p| !p.is_null());
    let shown = serde_json::json!({"preview": preview, "summary": summary});
    Sha256::digest(serde_json::to_vec(&shown).unwrap_or_default()).into()
}

/// Lower-case hex.
pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_gate_hash_covers_summary_and_preview_in_any_key_order() {
        let s = json!({"text": "Plan", "subject_items": ["a", "b"]});
        let back: serde_json::Value =
            serde_json::from_str(r#"{"subject_items":["a","b"],"text":"Plan"}"#)
                .unwrap_or_default();
        assert_eq!(gate_sha(&s, None), gate_sha(&back, Some(&json!(null))));
        assert_ne!(gate_sha(&s, None), gate_sha(&s, Some(&json!({"draft": 1}))));
        assert_ne!(gate_sha(&s, None), gate_sha(&json!({"text": "Plan"}), None));
        assert_eq!(hex(&gate_sha(&s, None)).len(), 64);
    }
}
