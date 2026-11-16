//! A data point exactly as published, with its provenance (the `data_points` row, Bureau §3.4).

use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::decimal::{self, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeriodType {
    Instant,
    Duration,
}

impl PeriodType {
    pub fn as_str(self) -> &'static str {
        match self {
            PeriodType::Instant => "instant",
            PeriodType::Duration => "duration",
        }
    }
}

/// One published value. Field meanings follow the `data_points` table; nothing is converted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataPoint {
    pub dp_id: String,
    /// Canonical identity context (ADR 0018): sorted `key=value` pairs joined by `;`, `""` when none.
    pub context_key: String,
    pub series_key: String,
    /// Identifier the Ledger resolves to `entity_id` (Roster step 1): `("cik", "0000320193")`.
    pub entity: Option<(String, String)>,
    pub concept: String,
    pub label: String,
    pub value_text: String,
    /// Canonical numeric string of `value_text` ([`decimal::parse`]); `None` only when the source
    /// published a non-numeric value that is still a value (never for missing markers).
    pub value: Option<String>,
    /// How `value_text` was written; `style.format(value) == value_text`.
    pub style: Style,
    pub unit: String,
    pub currency: Option<String>,
    pub scale: Option<String>,
    pub decimals: Option<i16>,
    pub period_label: String,
    pub period_start: Option<NaiveDate>,
    pub period_end: Option<NaiveDate>,
    pub period_type: PeriodType,
    pub fiscal_year: Option<i16>,
    pub fiscal_period: Option<String>,
    pub seasonal_adj: Option<String>,
    pub vintage: DateTime<Utc>,
    /// Set by the Ledger store from earlier vintages; parsers leave it `None`.
    pub supersedes: Option<String>,
    /// SHA-256 of the Morgue snapshot the value was read from (`data_points.snapshot_sha`).
    pub snapshot_sha: [u8; 32],
    pub locator: Value,
    pub parser: String,
}

/// A value the source lists but did not publish (`"."`, `"-"`), kept so pages can say "not published".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Missing {
    pub series_key: String,
    pub period_label: String,
    pub value_text: String,
    pub locator: Value,
}

/// Everything needed to build a [`DataPoint`] except the derived fields (`dp_id`, `value`, `style`).
#[derive(Debug, Clone)]
pub struct Draft {
    pub context: Vec<(String, String)>,
    pub series_key: String,
    pub entity: Option<(String, String)>,
    pub concept: String,
    pub label: String,
    pub value_text: String,
    pub unit: String,
    pub currency: Option<String>,
    pub scale: Option<String>,
    pub decimals: Option<i16>,
    pub period_label: String,
    pub period_start: Option<NaiveDate>,
    pub period_end: Option<NaiveDate>,
    pub period_type: PeriodType,
    pub fiscal_year: Option<i16>,
    pub fiscal_period: Option<String>,
    pub seasonal_adj: Option<String>,
    pub vintage: DateTime<Utc>,
    pub snapshot_sha: [u8; 32],
    pub locator: Value,
    pub parser: String,
}

impl Draft {
    /// Finish the point: parse `value_text` exactly and derive `dp_id`. A text that is not a number
    /// is an error for the caller to turn into a typed parse error with this draft's locator.
    pub fn build(self) -> Result<DataPoint, decimal::DecimalError> {
        let (numeric, style) = decimal::parse(&self.value_text)?;
        let context_key = context_key(&self.context);
        let dp_id = dp_id(
            &self.series_key,
            &context_key,
            &period_identity(
                &self.period_label,
                self.period_start,
                self.period_end,
                self.period_type,
            ),
            self.vintage,
        );
        Ok(DataPoint {
            dp_id,
            context_key,
            series_key: self.series_key,
            entity: self.entity,
            concept: self.concept,
            label: self.label,
            value_text: self.value_text,
            value: Some(numeric),
            style,
            unit: self.unit,
            currency: self.currency,
            scale: self.scale,
            decimals: self.decimals,
            period_label: self.period_label,
            period_start: self.period_start,
            period_end: self.period_end,
            period_type: self.period_type,
            fiscal_year: self.fiscal_year,
            fiscal_period: self.fiscal_period,
            seasonal_adj: self.seasonal_adj,
            vintage: self.vintage,
            supersedes: None,
            snapshot_sha: self.snapshot_sha,
            locator: self.locator,
            parser: self.parser,
        })
    }
}

/// Sorted `key=value` pairs joined by `;` (ADR 0018). Keys and values are trimmed of nothing: they are
/// the source's own identifiers.
pub fn context_key(pairs: &[(String, String)]) -> String {
    let mut v: Vec<String> = pairs.iter().map(|(k, val)| format!("{k}={val}")).collect();
    v.sort();
    v.join(";")
}

/// The period part of the dp_id input: `type|start|end|label`, dates as ISO 8601, empty when absent.
pub fn period_identity(
    label: &str,
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
    t: PeriodType,
) -> String {
    let d = |x: Option<NaiveDate>| x.map(|d| d.to_string()).unwrap_or_default();
    format!("{}|{}|{}|{label}", t.as_str(), d(start), d(end))
}

/// `dp_` + the first 16 characters of lower-case RFC 4648 base32 of
/// `sha256(series_key|context_key|period|vintage)`, vintage as RFC 3339 UTC to the second.
pub fn dp_id(series_key: &str, context_key: &str, period: &str, vintage: DateTime<Utc>) -> String {
    let input = format!(
        "{series_key}|{context_key}|{period}|{}",
        vintage.to_rfc3339_opts(SecondsFormat::Secs, true)
    );
    format!("dp_{}", &base32(&Sha256::digest(input.as_bytes()))[..16])
}

/// `dv_` id for a derived point: hash of the formula and its ordered inputs.
pub fn dv_id(formula: &str, inputs: &[&str]) -> String {
    let input = format!("{formula}|{}", inputs.join("|"));
    format!("dv_{}", &base32(&Sha256::digest(input.as_bytes()))[..16])
}

/// Lower-case RFC 4648 base32 without padding.
pub fn base32(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut out = String::with_capacity(bytes.len() * 8 / 5 + 1);
    let mut buf: u32 = 0;
    let mut bits = 0;
    for &b in bytes {
        buf = (buf << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(char::from(ALPHABET[((buf >> bits) & 31) as usize]));
        }
    }
    if bits > 0 {
        out.push(char::from(ALPHABET[((buf << (5 - bits)) & 31) as usize]));
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn base32_matches_rfc4648_vectors() {
        assert_eq!(base32(b"foobar"), "mzxw6ytboi");
        assert_eq!(base32(b"f"), "my");
    }

    #[test]
    fn dp_id_is_stable_and_shaped_for_the_table_check() {
        let v = Utc.with_ymd_and_hms(2026, 11, 13, 0, 0, 0).unwrap();
        let a = dp_id(
            "fred:CPIAUCSL",
            "",
            "duration|2026-10-01|2026-10-31|2026-10-01",
            v,
        );
        let b = dp_id(
            "fred:CPIAUCSL",
            "",
            "duration|2026-10-01|2026-10-31|2026-10-01",
            v,
        );
        assert_eq!(a, b);
        assert_eq!(a.len(), 19);
        assert!(
            a.starts_with("dp_")
                && a[3..]
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        );
        // A different vintage or context is a different point.
        assert_ne!(
            a,
            dp_id(
                "fred:CPIAUCSL",
                "",
                "duration|2026-10-01|2026-10-31|2026-10-01",
                v + chrono::Duration::days(1)
            )
        );
        assert_ne!(
            a,
            dp_id(
                "fred:CPIAUCSL",
                "accn=1",
                "duration|2026-10-01|2026-10-31|2026-10-01",
                v
            )
        );
    }

    #[test]
    fn context_key_is_sorted() {
        let k = context_key(&[
            ("unit".into(), "USD".into()),
            ("accn".into(), "0001".into()),
        ]);
        assert_eq!(k, "accn=0001;unit=USD");
        assert_eq!(context_key(&[]), "");
    }
}
