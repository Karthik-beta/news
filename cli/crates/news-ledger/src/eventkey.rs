//! Event keys: story identity for structured items (Bureau §3.4, "Event keys").

/// `earnings:<cik>:<fiscal_year><fiscal_period>`, the CIK zero-padded to 10 digits, e.g.
/// `earnings:0000320193:2026Q4`. `fp` is the company's own fiscal period as filed (`Q1`…`Q4`, `FY`).
pub fn earnings(cik: &str, fiscal_year: i16, fiscal_period: &str) -> Option<String> {
    let cik = cik10(cik)?;
    matches!(
        fiscal_period,
        "Q1" | "Q2" | "Q3" | "Q4" | "FY" | "H1" | "H2"
    )
    .then(|| format!("earnings:{cik}:{fiscal_year}{fiscal_period}"))
}

/// `macro:<country>.<series_family>:<reference_period>`, e.g. `macro:us.cpi:2026-10`.
pub fn macro_release(country: &str, family: &str, reference_period: &str) -> Option<String> {
    let ok = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    };
    (ok(country) && ok(family) && valid_period(reference_period))
        .then(|| format!("macro:{country}.{family}:{reference_period}"))
}

/// `policy:<central_bank>:<decision_date>`, e.g. `policy:fed:2026-12-09`. The decision date is the day
/// the bank announces, as its own statement dates it.
pub fn policy(central_bank: &str, decision_date: chrono::NaiveDate) -> Option<String> {
    let ok = !central_bank.is_empty()
        && central_bank
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    ok.then(|| format!("policy:{central_bank}:{decision_date}"))
}

/// `paper:doi:<doi>` (lower case, as DOIs compare), or `paper:<scheme>:<id>` for a repository id
/// (`paper:arxiv:2112.14514`, `paper:openalex:W123`).
pub fn paper(scheme: &str, id: &str) -> Option<String> {
    let ok = matches!(scheme, "doi" | "arxiv" | "openalex") && !id.trim().is_empty();
    ok.then(|| format!("paper:{scheme}:{}", id.trim().to_lowercase()))
}

/// `YYYY`, `YYYY-MM`, `YYYY-Qn` or `YYYY-MM-DD`.
fn valid_period(p: &str) -> bool {
    let b = p.as_bytes();
    let digits =
        |r: std::ops::Range<usize>| b.get(r).is_some_and(|s| s.iter().all(u8::is_ascii_digit));
    match b.len() {
        4 => digits(0..4),
        7 if b[4] == b'-' && b[5] == b'Q' => digits(0..4) && (b'1'..=b'4').contains(&b[6]),
        7 => digits(0..4) && b[4] == b'-' && digits(5..7),
        10 => digits(0..4) && b[4] == b'-' && digits(5..7) && b[7] == b'-' && digits(8..10),
        _ => false,
    }
}

/// A CIK as SEC writes it in URLs and keys: 10 digits, zero-padded. `None` if not 1–10 digits.
pub fn cik10(cik: &str) -> Option<String> {
    let t = cik.trim_start_matches("CIK");
    (!t.is_empty() && t.len() <= 10 && t.bytes().all(|b| b.is_ascii_digit()))
        .then(|| format!("{t:0>10}"))
}

/// True when `key` has a known kind and three `:`-separated parts.
pub fn is_valid(key: &str) -> bool {
    let parts: Vec<&str> = key.splitn(3, ':').collect();
    parts.len() == 3
        && matches!(
            parts[0],
            "earnings" | "macro" | "policy" | "report" | "paper"
        )
        && parts.iter().all(|p| !p.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_follow_the_patterns() {
        assert_eq!(
            earnings("320193", 2026, "Q4").as_deref(),
            Some("earnings:0000320193:2026Q4")
        );
        assert_eq!(earnings("320193", 2026, "Q5"), None);
        assert_eq!(
            macro_release("us", "cpi", "2026-10").as_deref(),
            Some("macro:us.cpi:2026-10")
        );
        assert_eq!(macro_release("US", "cpi", "2026-10"), None);
        assert_eq!(macro_release("us", "cpi", "Oct 2026"), None);
        assert!(is_valid("macro:us.cpi:2026-10"));
        assert!(!is_valid("story:123"));
        assert_eq!(cik10("CIK0000320193").as_deref(), Some("0000320193"));
        assert_eq!(cik10("12345678901"), None);
    }
}
