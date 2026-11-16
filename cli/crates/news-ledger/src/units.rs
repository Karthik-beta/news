//! The unit vocabulary. Units are stored verbatim; this only says what kind of unit a string is, so
//! pages can format it and guards can refuse unlike comparisons. An unknown unit is never guessed: the
//! caller marks the item `needs_human` (Bureau §3.4, "Failure modes").

/// ISO 4217 codes the renderer can format. Any other three-letter unit is unknown, not money.
pub const CURRENCIES: &[&str] = &[
    "AUD", "BRL", "CAD", "CHF", "CNY", "DKK", "EUR", "GBP", "HKD", "IDR", "ILS", "INR", "JPY", "KRW",
    "MXN", "NOK", "NZD", "PLN", "RUB", "SAR", "SEK", "SGD", "THB", "TRY", "TWD", "USD", "ZAR",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitClass {
    /// An amount of money in `currency`.
    Money,
    /// Money per share (`USD/shares`).
    MoneyPerShare,
    Shares,
    /// XBRL `pure`: a ratio with no unit.
    Pure,
    Percent,
    /// An index level ("Index 1982-1984=100").
    Index,
    /// A count of people, jobs or things, possibly in thousands (the scale is in the unit text).
    Count,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitInfo {
    pub class: UnitClass,
    pub currency: Option<String>,
}

/// Verbatim unit strings known from the sources built so far (SEC XBRL unit keys, FRED `units`,
/// the BLS series catalogue in `news-connectors`).
const COUNTS: &[&str] = &[
    "Thousands of Persons",
    "Persons",
    "Number",
    "All employees, thousands",
];
const PERCENTS: &[&str] = &["Percent", "Percent Change", "Percent Change from Year Ago", "percent"];

/// Classify a verbatim unit. `None` = unknown: store it verbatim and flag `needs_human`.
pub fn classify(unit: &str) -> Option<UnitInfo> {
    let info = |class| {
        Some(UnitInfo {
            class,
            currency: None,
        })
    };
    if CURRENCIES.contains(&unit) {
        return Some(UnitInfo {
            class: UnitClass::Money,
            currency: Some(unit.to_owned()),
        });
    }
    if let Some(ccy) = unit.strip_suffix("/shares")
        && CURRENCIES.contains(&ccy)
    {
        return Some(UnitInfo {
            class: UnitClass::MoneyPerShare,
            currency: Some(ccy.to_owned()),
        });
    }
    match unit {
        "shares" => return info(UnitClass::Shares),
        "pure" => return info(UnitClass::Pure),
        _ => {}
    }
    if PERCENTS.contains(&unit) {
        return info(UnitClass::Percent);
    }
    if unit.starts_with("Index ") || unit == "Index" {
        return info(UnitClass::Index);
    }
    if COUNTS.contains(&unit) {
        return info(UnitClass::Count);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_carries_its_currency_and_nothing_converts() {
        let usd = classify("USD").map(|u| (u.class, u.currency));
        assert_eq!(usd, Some((UnitClass::Money, Some("USD".to_owned()))));
        let eps = classify("USD/shares").map(|u| (u.class, u.currency));
        assert_eq!(eps, Some((UnitClass::MoneyPerShare, Some("USD".to_owned()))));
        assert_eq!(classify("Index 1982-1984=100").map(|u| u.class), Some(UnitClass::Index));
    }

    #[test]
    fn unknown_units_are_not_guessed() {
        for u in ["XYZ", "USD/Contract", "Barrels", "usd", ""] {
            assert!(classify(u).is_none(), "{u}");
        }
    }
}
