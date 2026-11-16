//! Exact decimals parsed from published text, and the style that writes them back unchanged.
//!
//! A published figure is split into a canonical numeric string (what goes into `data_points.value`,
//! a PostgreSQL `numeric`) and a [`Style`] (sign character, thousands separator, bare leading or
//! trailing dot, exponent form). `style.format(&numeric) == value_text` for every accepted text, so the
//! stored decimal never drifts from what the source printed. Nothing here touches a float.

use serde::{Deserialize, Serialize};

/// Why a published string is not a number. Missing-value markers (".", "-", "(NA)") land here too;
/// connectors check [`is_missing_marker`] first and record them as missing observations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("not a published number: {text:?} ({reason})")]
pub struct DecimalError {
    pub text: String,
    pub reason: &'static str,
}

/// How the source wrote the number, apart from its digits.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Style {
    /// The sign character as written: `-`, `+` or U+2212 `−`. `None` when unsigned.
    pub sign: Option<char>,
    /// Thousands separator in the integer part (only `,` is accepted, in groups of three).
    pub group: Option<char>,
    /// ".5" rather than "0.5".
    pub leading_dot: bool,
    /// "5." rather than "5".
    pub trailing_dot: bool,
    /// Exponent marker (`e` or `E`), whether its sign was an explicit `+`, and its digit count.
    pub exp: Option<ExpStyle>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpStyle {
    pub marker: char,
    pub plus: bool,
    pub width: usize,
}

/// Markers sources use for "no value published". Never parsed as numbers.
pub fn is_missing_marker(text: &str) -> bool {
    matches!(
        text.trim(),
        "" | "."
            | "-"
            | "–"
            | "—"
            | "(NA)"
            | "NA"
            | "N/A"
            | "n/a"
            | "(X)"
            | "(D)"
            | "(S)"
            | "nan"
            | "NaN"
    )
}

/// Parse published text into `(numeric, style)`. `numeric` is canonical: optional `-`, the integer
/// digits exactly as written (leading zeros kept), optional `.` and fraction digits, optional `E` and a
/// signed exponent. PostgreSQL's `numeric` input accepts it as is.
pub fn parse(text: &str) -> Result<(String, Style), DecimalError> {
    let err = |reason| DecimalError {
        text: text.to_owned(),
        reason,
    };
    let mut style = Style::default();
    let mut rest = text;
    let mut negative = false;
    if let Some(c) = rest.chars().next()
        && matches!(c, '-' | '+' | '\u{2212}')
    {
        style.sign = Some(c);
        negative = c != '+';
        rest = &rest[c.len_utf8()..];
    }
    let (mantissa, exp) = match rest.find(['e', 'E']) {
        Some(i) => (&rest[..i], Some((&rest[i..i + 1], &rest[i + 1..]))),
        None => (rest, None),
    };
    let (int_raw, frac) = match mantissa.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (mantissa, None),
    };
    // Integer part: plain digits, or digits grouped by ',' in threes.
    let int: String = if int_raw.contains(',') {
        let groups: Vec<&str> = int_raw.split(',').collect();
        let first_ok = groups
            .first()
            .is_some_and(|g| (1..=3).contains(&g.len()) && g.bytes().all(|b| b.is_ascii_digit()));
        let rest_ok = groups[1..]
            .iter()
            .all(|g| g.len() == 3 && g.bytes().all(|b| b.is_ascii_digit()));
        if !(first_ok && rest_ok) {
            return Err(err("misplaced thousands separator"));
        }
        style.group = Some(',');
        groups.concat()
    } else {
        if !int_raw.bytes().all(|b| b.is_ascii_digit()) {
            return Err(err("unexpected character"));
        }
        int_raw.to_owned()
    };
    let frac = match frac {
        Some(f) if !f.bytes().all(|b| b.is_ascii_digit()) => {
            return Err(err("unexpected character in the fraction"));
        }
        Some("") => {
            if int.is_empty() {
                return Err(err("no digits"));
            }
            style.trailing_dot = true;
            None
        }
        Some(f) => Some(f.to_owned()),
        None => None,
    };
    if int.is_empty() {
        if frac.is_none() {
            return Err(err("no digits"));
        }
        style.leading_dot = true;
    }
    let mut numeric = String::with_capacity(text.len() + 1);
    if negative {
        numeric.push('-');
    }
    numeric.push_str(if int.is_empty() { "0" } else { &int });
    if let Some(f) = &frac {
        numeric.push('.');
        numeric.push_str(f);
    }
    if let Some((marker, e)) = exp {
        let (plus, neg, digits) = match e.as_bytes().first() {
            Some(b'+') => (true, false, &e[1..]),
            Some(b'-') => (false, true, &e[1..]),
            _ => (false, false, e),
        };
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(err("bad exponent"));
        }
        let marker = marker.chars().next().unwrap_or('E');
        style.exp = Some(ExpStyle {
            marker,
            plus,
            width: digits.len(),
        });
        numeric.push('E');
        if neg {
            numeric.push('-');
        }
        numeric.push_str(digits);
    }
    Ok((numeric, style))
}

impl Style {
    /// Write a canonical numeric string (as returned by [`parse`]) the way the source wrote it.
    pub fn format(&self, numeric: &str) -> String {
        let (negative, body) = match numeric.strip_prefix('-') {
            Some(b) => (true, b),
            None => (false, numeric),
        };
        let (mantissa, exp) = match body.split_once('E') {
            Some((m, e)) => (m, Some(e)),
            None => (body, None),
        };
        let (int, frac) = match mantissa.split_once('.') {
            Some((i, f)) => (i, Some(f)),
            None => (mantissa, None),
        };
        let mut out = String::with_capacity(numeric.len() + 8);
        match self.sign {
            Some(c) => out.push(c),
            None if negative => out.push('-'),
            None => {}
        }
        if !(self.leading_dot && int == "0") {
            match self.group {
                Some(g) => {
                    let first = match int.len() % 3 {
                        0 => 3.min(int.len()),
                        n => n,
                    };
                    out.push_str(&int[..first]);
                    let mut i = first;
                    while i < int.len() {
                        out.push(g);
                        out.push_str(&int[i..i + 3]);
                        i += 3;
                    }
                }
                None => out.push_str(int),
            }
        }
        if let Some(f) = frac {
            out.push('.');
            out.push_str(f);
        } else if self.trailing_dot {
            out.push('.');
        }
        if let (Some(e), Some(es)) = (exp, &self.exp) {
            out.push(es.marker);
            match e.strip_prefix('-') {
                Some(d) => {
                    out.push('-');
                    out.push_str(d);
                }
                None => {
                    if es.plus {
                        out.push('+');
                    }
                    out.push_str(e);
                }
            }
        }
        out
    }
}

/// An exact decimal as an integer mantissa and a power-of-ten scale: `m × 10^-scale`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dec {
    pub m: i128,
    pub scale: u32,
}

/// Largest scale kept in arithmetic; inputs beyond it are refused rather than rounded.
const MAX_SCALE: u32 = 18;

impl Dec {
    /// Parse a canonical numeric string from [`parse`]. `None` if it does not fit in i128 at scale ≤ 18.
    pub fn from_numeric(numeric: &str) -> Option<Dec> {
        let (negative, body) = match numeric.strip_prefix('-') {
            Some(b) => (true, b),
            None => (false, numeric),
        };
        let (mantissa, exp) = match body.split_once('E') {
            Some((m, e)) => (m, e.parse::<i32>().ok()?),
            None => (body, 0),
        };
        let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        let mut m: i128 = 0;
        for b in int.bytes().chain(frac.bytes()) {
            m = m
                .checked_mul(10)?
                .checked_add(i128::from(b.checked_sub(b'0')?))?;
        }
        let mut scale = i64::try_from(frac.len()).ok()? - i64::from(exp);
        while scale < 0 {
            m = m.checked_mul(10)?;
            scale += 1;
        }
        let scale = u32::try_from(scale).ok()?;
        if scale > MAX_SCALE {
            return None;
        }
        Some(Dec {
            m: if negative { -m } else { m },
            scale,
        })
    }

    fn rescale(self, scale: u32) -> Option<Dec> {
        let factor = 10i128.checked_pow(scale.checked_sub(self.scale)?)?;
        Some(Dec {
            m: self.m.checked_mul(factor)?,
            scale,
        })
    }

    /// `a - b`, exactly.
    pub fn checked_sub(self, other: Dec) -> Option<Dec> {
        let s = self.scale.max(other.scale);
        let (a, b) = (self.rescale(s)?, other.rescale(s)?);
        Some(Dec {
            m: a.m.checked_sub(b.m)?,
            scale: s,
        })
    }

    /// `self × 100 / den`, rounded half away from zero to `places` decimals. `None` when `den` is 0
    /// or on overflow.
    pub fn ratio_pct(self, den: Dec, places: u32) -> Option<Dec> {
        if den.m == 0 {
            return None;
        }
        let s = self.scale.max(den.scale);
        let (a, b) = (self.rescale(s)?, den.rescale(s)?);
        let num =
            a.m.checked_mul(100)?
                .checked_mul(10i128.checked_pow(places)?)?;
        Some(Dec {
            m: div_round(num, b.m)?,
            scale: places,
        })
    }

    /// Round half away from zero to `places` decimals (only ever reduces the scale).
    pub fn round(self, places: u32) -> Option<Dec> {
        if places >= self.scale {
            return self.rescale(places);
        }
        let d = 10i128.checked_pow(self.scale - places)?;
        Some(Dec {
            m: div_round(self.m, d)?,
            scale: places,
        })
    }

    /// Plain decimal text: `-12.30`.
    pub fn to_numeric(self) -> String {
        let digits = self.m.unsigned_abs().to_string();
        let scale = self.scale as usize;
        let padded = if digits.len() <= scale {
            format!("{}{digits}", "0".repeat(scale + 1 - digits.len()))
        } else {
            digits
        };
        let (i, f) = padded.split_at(padded.len() - scale);
        let sign = if self.m < 0 { "-" } else { "" };
        if f.is_empty() {
            format!("{sign}{i}")
        } else {
            format!("{sign}{i}.{f}")
        }
    }
}

fn div_round(num: i128, den: i128) -> Option<i128> {
    let q = num.checked_div(den)?;
    let r = num.checked_rem(den)?;
    // |2r| >= |den| rounds away from zero.
    if r.checked_abs()?.checked_mul(2)? >= den.checked_abs()? {
        if (num < 0) == (den < 0) {
            q.checked_add(1)
        } else {
            q.checked_sub(1)
        }
    } else {
        Some(q)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn real_published_forms_round_trip() {
        for t in [
            "337.511",
            "-0.3",
            "1,234.5",
            "159293",
            "394328000000",
            "0.25",
            ".5",
            "5.",
            "+2.1",
            "\u{2212}0.4",
            "1.5E-3",
            "2e+05",
            "007",
            "-1,000,000",
        ] {
            let (n, s) = parse(t).unwrap();
            assert_eq!(s.format(&n), t, "{t} via {n}");
        }
        assert_eq!(parse("1,234.5").unwrap().0, "1234.5");
        assert_eq!(parse(".5").unwrap().0, "0.5");
        assert_eq!(parse("\u{2212}0.4").unwrap().0, "-0.4");
    }

    #[test]
    fn junk_and_markers_are_refused() {
        for t in [
            "",
            ".",
            "-",
            "1,23",
            "12,3456",
            "1.2.3",
            "abc",
            "1e",
            "1,234,56.0",
            "--1",
        ] {
            assert!(parse(t).is_err(), "{t}");
        }
        assert!(is_missing_marker("."));
        assert!(is_missing_marker("-"));
        assert!(!is_missing_marker("0"));
    }

    #[test]
    fn arithmetic_is_exact() {
        let a = Dec::from_numeric("337.511").unwrap();
        let b = Dec::from_numeric("324.800").unwrap();
        // (337.511 - 324.8) / 324.8 * 100 = 3.9134...
        assert_eq!(
            a.checked_sub(b)
                .unwrap()
                .ratio_pct(b, 1)
                .unwrap()
                .to_numeric(),
            "3.9"
        );
        assert_eq!(Dec::from_numeric("1.5E-3").unwrap().to_numeric(), "0.0015");
        assert_eq!(Dec::from_numeric("2E5").unwrap().to_numeric(), "200000");
        assert_eq!(
            Dec::from_numeric("-0.25")
                .unwrap()
                .round(1)
                .unwrap()
                .to_numeric(),
            "-0.3"
        );
        assert!(a.ratio_pct(Dec::from_numeric("0").unwrap(), 1).is_none());
    }

    fn published() -> impl Strategy<Value = String> {
        (
            prop::option::of(prop::sample::select(vec!['-', '+', '\u{2212}'])),
            any::<bool>(),
            0u64..10_000_000_000,
            prop::option::of("[0-9]{1,6}"),
            prop::option::of((prop::sample::select(vec!['e', 'E']), any::<bool>(), 0u8..20)),
        )
            .prop_map(|(sign, group, int, frac, exp)| {
                let mut s = String::new();
                if let Some(c) = sign {
                    s.push(c);
                }
                let digits = int.to_string();
                if group {
                    let first = match digits.len() % 3 {
                        0 => 3,
                        n => n,
                    };
                    s.push_str(&digits[..first]);
                    for c in digits.as_bytes()[first..].chunks(3) {
                        s.push(',');
                        s.push_str(std::str::from_utf8(c).unwrap());
                    }
                } else {
                    s.push_str(&digits);
                }
                if let Some(f) = frac {
                    s.push('.');
                    s.push_str(&f);
                }
                if let Some((m, neg, e)) = exp {
                    s.push(m);
                    if neg {
                        s.push('-');
                    }
                    s.push_str(&e.to_string());
                }
                s
            })
    }

    proptest! {
        /// Formatting `value` gives back `value_text` (Bureau §3.4 round-trip test).
        #[test]
        fn value_formats_back_to_value_text(t in published()) {
            let (numeric, style) = parse(&t).unwrap();
            prop_assert_eq!(style.format(&numeric), t);
        }

        /// The canonical string and the exact decimal agree.
        #[test]
        fn numeric_and_dec_agree(int in -1_000_000_000i64..1_000_000_000, places in 0u32..6) {
            let text = Dec { m: i128::from(int), scale: places }.to_numeric();
            let (numeric, _) = parse(&text).unwrap();
            prop_assert_eq!(Dec::from_numeric(&numeric).unwrap(), Dec { m: i128::from(int), scale: places });
        }
    }
}
