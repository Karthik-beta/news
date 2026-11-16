//! Derived points and their guards (decision D5, Bureau §3.4 "Derived figures").
//!
//! A change or share is computed only between points with the same unit, currency, scale and seasonal
//! adjustment, and comparable periods. If a guard fails there is no derived point, so Reporter cannot
//! mention the change. Arithmetic is exact ([`Dec`]); nothing converts.

use chrono::NaiveDate;
use serde::Serialize;
use serde_json::{Value, json};

use crate::decimal::Dec;
use crate::point::{DataPoint, PeriodType, dv_id};
use crate::units::{UnitClass, classify};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Formula {
    /// `(a - b) / b × 100`, in percent.
    PctChange,
    /// `a - b` where both are percentages, in percentage points.
    PpChange,
    /// `a - b`, in the inputs' unit.
    Diff,
    /// `a / total × 100`, in percent; same period required.
    Share,
}

impl Formula {
    pub fn as_str(self) -> &'static str {
        match self {
            Formula::PctChange => "pct_change(a,b)",
            Formula::PpChange => "pp_change(a,b)",
            Formula::Diff => "diff(a,b)",
            Formula::Share => "share(a,total)",
        }
    }
}

/// Which periods count as comparable for this change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Comparison {
    /// The same fiscal (or calendar) period a year apart.
    YearOnYear,
    /// Adjacent periods of the same length (month on month, quarter on quarter).
    Sequential,
    /// The very same period (shares).
    SamePeriod,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Refusal {
    #[error("units differ: {0:?} vs {1:?}")]
    Unit(String, String),
    #[error("currencies differ: {0:?} vs {1:?}")]
    Currency(Option<String>, Option<String>),
    #[error("scales differ: {0:?} vs {1:?}")]
    Scale(Option<String>, Option<String>),
    #[error("seasonal adjustment differs: {0:?} vs {1:?}")]
    Seasonal(Option<String>, Option<String>),
    #[error("periods are not comparable: {0}")]
    Period(String),
    #[error("unit {0:?} is unknown (needs_human)")]
    UnknownUnit(String),
    #[error("{0} is not defined for unit {1:?}")]
    WrongUnitKind(&'static str, String),
    #[error("an input has no numeric value")]
    NoValue,
    #[error("arithmetic refused (division by zero or overflow)")]
    Arithmetic,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Derived {
    pub dp_id: String,
    pub formula: &'static str,
    pub inputs: Vec<String>,
    /// Exact decimal text, rounded half away from zero to `rounding` places.
    pub value: String,
    pub unit: String,
    pub rounding: u32,
    pub guard: Value,
}

/// Compute `formula` over `a` (the newer or the part) and `b` (the base or the total), or refuse.
pub fn derive(
    formula: Formula,
    a: &DataPoint,
    b: &DataPoint,
    cmp: Comparison,
    rounding: u32,
) -> Result<Derived, Refusal> {
    if a.unit != b.unit {
        return Err(Refusal::Unit(a.unit.clone(), b.unit.clone()));
    }
    if a.currency != b.currency {
        return Err(Refusal::Currency(a.currency.clone(), b.currency.clone()));
    }
    if a.scale != b.scale {
        return Err(Refusal::Scale(a.scale.clone(), b.scale.clone()));
    }
    if a.seasonal_adj != b.seasonal_adj {
        return Err(Refusal::Seasonal(
            a.seasonal_adj.clone(),
            b.seasonal_adj.clone(),
        ));
    }
    let class = classify(&a.unit)
        .ok_or_else(|| Refusal::UnknownUnit(a.unit.clone()))?
        .class;
    let cmp = if formula == Formula::Share {
        Comparison::SamePeriod
    } else {
        cmp
    };
    comparable(a, b, cmp).map_err(Refusal::Period)?;
    let (va, vb) = match (&a.value, &b.value) {
        (Some(x), Some(y)) => (
            Dec::from_numeric(x).ok_or(Refusal::Arithmetic)?,
            Dec::from_numeric(y).ok_or(Refusal::Arithmetic)?,
        ),
        _ => return Err(Refusal::NoValue),
    };
    let (value, unit) = match formula {
        Formula::PctChange => {
            if class == UnitClass::Percent {
                return Err(Refusal::WrongUnitKind("pct_change", a.unit.clone()));
            }
            let d = va.checked_sub(vb).ok_or(Refusal::Arithmetic)?;
            (d.ratio_pct(vb, rounding), "Percent".to_owned())
        }
        Formula::PpChange => {
            if class != UnitClass::Percent {
                return Err(Refusal::WrongUnitKind("pp_change", a.unit.clone()));
            }
            let d = va.checked_sub(vb).ok_or(Refusal::Arithmetic)?;
            (d.round(rounding), "Percentage points".to_owned())
        }
        Formula::Diff => {
            let d = va.checked_sub(vb).ok_or(Refusal::Arithmetic)?;
            (d.round(rounding), a.unit.clone())
        }
        Formula::Share => (va.ratio_pct(vb, rounding), "Percent".to_owned()),
    };
    let value = value.ok_or(Refusal::Arithmetic)?.to_numeric();
    let inputs = vec![a.dp_id.clone(), b.dp_id.clone()];
    let guard = json!({
        "unit": a.unit, "currency": a.currency, "scale": a.scale, "seasonal_adj": a.seasonal_adj,
        "comparison": cmp,
        "periods": [a.period_label, b.period_label],
    });
    Ok(Derived {
        dp_id: dv_id(formula.as_str(), &[&a.dp_id, &b.dp_id]),
        formula: formula.as_str(),
        inputs,
        value,
        unit,
        rounding,
        guard,
    })
}

/// Length class of a duration in days, so a 13-week quarter and a calendar quarter compare.
fn length_class(days: i64) -> Option<&'static str> {
    Some(match days {
        28..=31 => "month",
        84..=98 => "quarter",
        180..=186 => "half",
        266..=280 => "nine_months",
        357..=373 => "year",
        _ => return None,
    })
}

fn length(p: &DataPoint) -> Result<Option<(&'static str, NaiveDate, NaiveDate)>, String> {
    match p.period_type {
        PeriodType::Instant => Ok(None),
        PeriodType::Duration => {
            let (Some(s), Some(e)) = (p.period_start, p.period_end) else {
                return Err(format!("{} has no period dates", p.dp_id));
            };
            let days = (e - s).num_days() + 1;
            let class = length_class(days)
                .ok_or_else(|| format!("{} spans {days} days, not a standard period", p.dp_id))?;
            Ok(Some((class, s, e)))
        }
    }
}

fn comparable(a: &DataPoint, b: &DataPoint, cmp: Comparison) -> Result<(), String> {
    if a.period_type != b.period_type {
        return Err("one is an instant, the other a duration".into());
    }
    let (la, lb) = (length(a)?, length(b)?);
    if let (Some((ca, ..)), Some((cb, ..))) = (la, lb)
        && ca != cb
    {
        return Err(format!("{ca} vs {cb}"));
    }
    let (Some(ea), Some(eb)) = (a.period_end, b.period_end) else {
        return Err("a period end is missing".into());
    };
    match cmp {
        Comparison::SamePeriod => (a.period_start == b.period_start && ea == eb)
            .then_some(())
            .ok_or_else(|| "not the same period".into()),
        Comparison::YearOnYear => {
            if let (Some(fa), Some(fb)) = (&a.fiscal_period, &b.fiscal_period)
                && fa != fb
            {
                return Err(format!("fiscal periods {fa} vs {fb}"));
            }
            // 52/53-week fiscal years end on a weekday, so a year apart is 357–373 days.
            let gap = (ea - eb).num_days();
            (357..=373)
                .contains(&gap)
                .then_some(())
                .ok_or_else(|| format!("ends {gap} days apart, not a year"))
        }
        Comparison::Sequential => match (la, lb) {
            (Some((_, sa, _)), Some((..))) => {
                let gap = (sa - eb).num_days();
                (1..=8)
                    .contains(&gap)
                    .then_some(())
                    .ok_or_else(|| format!("not adjacent ({gap} days between)"))
            }
            _ => (ea > eb)
                .then_some(())
                .ok_or_else(|| "instants out of order".into()),
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::point::Draft;
    use chrono::{TimeZone, Utc};
    use proptest::prelude::*;

    fn point(value: &str, start: (i32, u32, u32), end: (i32, u32, u32)) -> DataPoint {
        let d = |(y, m, dd): (i32, u32, u32)| NaiveDate::from_ymd_opt(y, m, dd).unwrap();
        Draft {
            context: vec![],
            series_key: "bls:CUUR0000SA0".into(),
            entity: None,
            concept: "CUUR0000SA0".into(),
            label: "CPI-U".into(),
            value_text: value.into(),
            unit: "Index 1982-1984=100".into(),
            currency: None,
            scale: None,
            decimals: None,
            period_label: format!("{end:?}"),
            period_start: Some(d(start)),
            period_end: Some(d(end)),
            period_type: PeriodType::Duration,
            fiscal_year: None,
            fiscal_period: None,
            seasonal_adj: Some("NSA".into()),
            vintage: Utc.with_ymd_and_hms(2026, 11, 13, 13, 30, 0).unwrap(),
            snapshot_sha: [0; 32],
            locator: json!({}),
            parser: "test@1".into(),
        }
        .build()
        .unwrap()
    }

    #[test]
    fn year_on_year_cpi_is_exact() {
        let a = point("337.511", (2026, 9, 1), (2026, 9, 30));
        let b = point("324.800", (2025, 9, 1), (2025, 9, 30));
        let d = derive(Formula::PctChange, &a, &b, Comparison::YearOnYear, 1).unwrap();
        assert_eq!(d.value, "3.9");
        assert_eq!(d.unit, "Percent");
        assert!(d.dp_id.starts_with("dv_"));
        assert_eq!(d.inputs, vec![a.dp_id.clone(), b.dp_id.clone()]);
    }

    #[test]
    fn month_on_month_needs_adjacent_months() {
        let a = point("337.511", (2026, 10, 1), (2026, 10, 31));
        let b = point("336.940", (2026, 9, 1), (2026, 9, 30));
        assert!(derive(Formula::PctChange, &a, &b, Comparison::Sequential, 1).is_ok());
        let c = point("334.980", (2026, 8, 1), (2026, 8, 31));
        assert!(matches!(
            derive(Formula::PctChange, &a, &c, Comparison::Sequential, 1),
            Err(Refusal::Period(_))
        ));
    }

    #[test]
    fn fifty_three_week_fiscal_years_still_compare() {
        // Apple: FY2023 ended 30 Sep 2023, FY2024 ended 28 Sep 2024 (52 weeks), FY2022 24 Sep 2022.
        let mut a = point("391035000000", (2023, 10, 1), (2024, 9, 28));
        let mut b = point("383285000000", (2022, 9, 25), (2023, 9, 30));
        for p in [&mut a, &mut b] {
            p.unit = "USD".into();
            p.currency = Some("USD".into());
            p.seasonal_adj = None;
            p.fiscal_period = Some("FY".into());
        }
        let d = derive(Formula::PctChange, &a, &b, Comparison::YearOnYear, 1).unwrap();
        assert_eq!(d.value, "2.0");
    }

    #[test]
    fn pp_change_only_on_percent_and_pct_change_never_on_percent() {
        let mut a = point("4.4", (2026, 10, 1), (2026, 10, 31));
        let mut b = point("4.3", (2026, 9, 1), (2026, 9, 30));
        assert!(matches!(
            derive(Formula::PpChange, &a, &b, Comparison::Sequential, 1),
            Err(Refusal::WrongUnitKind(..))
        ));
        a.unit = "Percent".into();
        b.unit = "Percent".into();
        assert_eq!(
            derive(Formula::PpChange, &a, &b, Comparison::Sequential, 1)
                .unwrap()
                .value,
            "0.1"
        );
        assert!(derive(Formula::PctChange, &a, &b, Comparison::Sequential, 1).is_err());
    }

    #[derive(Debug, Clone, Copy)]
    enum Break {
        Unit,
        Currency,
        Scale,
        Seasonal,
        Period,
        PeriodType,
    }

    proptest! {
        /// A guard never lets a change be computed across different units, currencies, scales,
        /// seasonal adjustments or non-comparable periods.
        #[test]
        fn guards_never_cross_unlike_points(
            brk in prop::sample::select(vec![Break::Unit, Break::Currency, Break::Scale, Break::Seasonal, Break::Period, Break::PeriodType]),
            formula in prop::sample::select(vec![Formula::PctChange, Formula::Diff, Formula::Share, Formula::PpChange]),
            cmp in prop::sample::select(vec![Comparison::YearOnYear, Comparison::Sequential, Comparison::SamePeriod]),
            va in 1u32..1_000_000, vb in 1u32..1_000_000,
            shift_days in 9i64..300,
        ) {
            let mut a = point(&va.to_string(), (2026, 9, 1), (2026, 9, 30));
            let mut b = point(&vb.to_string(), (2025, 9, 1), (2025, 9, 30));
            match brk {
                Break::Unit => b.unit = "Index 2015=100".into(),
                Break::Currency => { a.unit = "USD".into(); b.unit = "USD".into(); a.currency = Some("USD".into()); b.currency = Some("EUR".into()); }
                Break::Scale => b.scale = Some("thousands".into()),
                Break::Seasonal => b.seasonal_adj = Some("SA".into()),
                Break::Period => {
                    b.period_end = b.period_end.map(|d| d + chrono::Duration::days(shift_days));
                }
                Break::PeriodType => b.period_type = PeriodType::Instant,
            }
            prop_assert!(derive(formula, &a, &b, cmp, 1).is_err());
        }
    }
}
