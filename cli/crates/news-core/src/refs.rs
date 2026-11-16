//! Reference tokens (Bureau Design §3.8): the six kinds of ID Reporter writes instead of a number,
//! series, period, quote or context line. Pattern: `^(dp|dv|sr|pd|q|cx)_[a-z0-9]+(_[a-z0-9]+)*$`.

/// The pattern used in `page-draft.v1` and `brief.v1`.
pub const REF_PATTERN: &str = "^(dp|dv|sr|pd|q|cx)_[a-z0-9]+(_[a-z0-9]+)*$";

/// The six prefixes.
pub const REF_PREFIXES: [&str; 6] = ["dp", "dv", "sr", "pd", "q", "cx"];

/// Whether `s` is a well-formed reference token (existence in a brief is checked elsewhere, C2).
pub fn is_ref(s: &str) -> bool {
    let Some((prefix, body)) = s.split_once('_') else {
        return false;
    };
    REF_PREFIXES.contains(&prefix)
        && body.split('_').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_six_kinds() {
        for r in [
            "dp_7kq2",
            "dv_3ab8",
            "sr_cpi_yoy_5y",
            "pd_2026_10",
            "q_1",
            "cx_1",
        ] {
            assert!(is_ref(r), "{r}");
        }
    }

    #[test]
    fn rejects_everything_else() {
        for r in [
            "", "dp", "dp_", "dp__x", "dp_x_", "DP_x", "dp_X", "xx_1", "dp-1", "dp_1 ", "_dp_1",
            "dp_1.5", "dp_é",
        ] {
            assert!(!is_ref(r), "{r:?}");
        }
    }
}
