//! Classification by rules first (Bureau §3.4): page type, categories and region from a rules table
//! keyed on source and the connector's rule key (form or series family). Rules cover the structured
//! sources completely; only free text goes to A3.
//!
//! The table is code, reviewed like code. TODO(#15): move it to a `classify_rules` table if Taster
//! needs to edit rules at runtime (the 0100 migration block is reserved for it).

/// One rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    pub source_id: &'static str,
    pub rule_key: &'static str,
    pub page_type: &'static str,
    /// Taxonomy slugs, parent first.
    pub categories: &'static [&'static str],
    pub region: &'static str,
}

const EARNINGS: &[&str] = &["finance", "earnings"];
const INFLATION: &[&str] = &["economy", "inflation"];
const JOBS: &[&str] = &["economy", "jobs"];

const fn r(
    source_id: &'static str,
    rule_key: &'static str,
    page_type: &'static str,
    categories: &'static [&'static str],
) -> Rule {
    Rule {
        source_id,
        rule_key,
        page_type,
        categories,
        region: "us",
    }
}

pub const RULES: &[Rule] = &[
    r("sec-edgar", "form:10-K", "earnings", EARNINGS),
    r("sec-edgar", "form:10-Q", "earnings", EARNINGS),
    r("sec-edgar", "form:8-K/2.02", "earnings", EARNINGS),
    r("fred", "family:cpi", "macro", INFLATION),
    r("fred", "family:payrolls", "macro", JOBS),
    r("fred", "family:unemployment", "macro", JOBS),
    r("bls", "family:cpi", "macro", INFLATION),
    r("bls", "family:payrolls", "macro", JOBS),
    r("bls", "family:unemployment", "macro", JOBS),
];

pub fn classify(source_id: &str, rule_key: &str) -> Option<&'static Rule> {
    RULES
        .iter()
        .find(|r| r.source_id == source_id && r.rule_key == rule_key)
}

/// The taxonomy seed the rules use: `(parent slug, slug, name)`.
pub const TAXONOMY: &[(Option<&str>, &str, &str)] = &[
    (None, "economy", "Economy"),
    (Some("economy"), "inflation", "Inflation"),
    (Some("economy"), "jobs", "Jobs"),
    (None, "finance", "Finance"),
    (Some("finance"), "earnings", "Earnings"),
];

/// Insert the taxonomy seed. Idempotent (`unique nulls not distinct (parent_id, slug)`).
pub async fn seed_taxonomy(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    for (i, (parent, slug, name)) in TAXONOMY.iter().enumerate() {
        sqlx::query(
            "insert into taxonomy (parent_id, slug, name, sort_order)
             values ((select tax_id from taxonomy where parent_id is null and slug = $1), $2, $3, $4)
             on conflict (parent_id, slug) do nothing",
        )
        .bind(parent)
        .bind(slug)
        .bind(name)
        .bind(i32::try_from(i).unwrap_or(0))
        .execute(pool)
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_use_the_seeded_taxonomy() {
        for r in RULES {
            assert!(!r.categories.is_empty());
            for c in r.categories {
                assert!(TAXONOMY.iter().any(|(_, s, _)| s == c), "{c}");
            }
            assert!(matches!(r.page_type, "earnings" | "macro"));
        }
        // Children come after their parents, so the seed inserts in one pass.
        for (i, (parent, ..)) in TAXONOMY.iter().enumerate() {
            if let Some(p) = parent {
                assert!(
                    TAXONOMY[..i]
                        .iter()
                        .any(|(pp, s, _)| pp.is_none() && s == p)
                );
            }
        }
    }

    #[test]
    fn every_connector_rule_key_is_covered() {
        for s in news_connectors::fred::SERIES {
            assert!(classify("fred", &format!("family:{}", s.family)).is_some());
        }
        for s in news_connectors::bls::SERIES {
            assert!(classify("bls", &format!("family:{}", s.family)).is_some());
        }
        assert_eq!(
            classify("sec-edgar", "form:10-K").map(|r| r.page_type),
            Some("earnings")
        );
        assert!(classify("sec-edgar", "form:S-1").is_none());
    }
}
