//! The templated page (D10, Bureau Design §3.9 "Fallback"): when Reporter fails twice on a Tier 2 item, code builds
//! a Copy with no prose: key figures, the main chart, a data table and (added by the renderer) the sources, under a
//! headline from a page-type pattern with references. It is stored flagged `templated`, so it is never indexed and a
//! later run upgrades it.

use crate::validate::Copy;
use news_core::brief::{Brief, Fact};
use news_core::{NewsError, NewsResult};

/// The noun a page type's pattern headline uses.
fn noun(page_type: &str) -> &'static str {
    match page_type {
        "earnings" => "results",
        "policy" | "policy-decision" => "decision",
        "report" | "major-report" => "report",
        "news" | "news-story" => "update",
        "science" => "findings",
        _ => "figures",
    }
}

/// An OpenUI Lang string literal.
fn lit(s: &str) -> String {
    let mut out = String::from('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A source label as copy: the comma-separated parts the C4 lint would flag (Fixer adds periods such as
/// "change from 2024" or "2021 to 2025", and units such as "thousands") are left out, so code never types a number.
fn plain(label: &str, names: &[&str], otherwise: &str) -> String {
    let kept: Vec<&str> = label
        .split(", ")
        .filter(|part| crate::lint::numeric_literals(part, names).is_empty())
        .collect();
    if kept.is_empty() {
        otherwise.to_string()
    } else {
        kept.join(", ")
    }
}

/// The `pd_…` reference of a fact's period, when the brief has a period with that label.
fn period_of<'a>(brief: &'a Brief, fact: &Fact) -> Option<&'a str> {
    let label = fact.period.as_deref()?;
    brief
        .periods
        .iter()
        .find(|p| p.label == label)
        .map(|p| p.reference.as_str())
}

/// `{ref}`, followed by ` in {pd_…}` when the fact's period is a period in the brief.
fn cite(brief: &Brief, fact: &Fact) -> String {
    match period_of(brief, fact) {
        Some(pd) => format!("{{{}}} in {{{pd}}}", fact.reference),
        None => format!("{{{}}}", fact.reference),
    }
}

/// Build the templated Copy for a brief. Tier 1 pages have no templated fallback.
pub fn templated(brief: &Brief) -> NewsResult<Copy> {
    if brief.item.tier != 2 {
        return Err(NewsError::precondition(format!(
            "story {} is Tier {}: only Tier 2 items get a templated page (D10)",
            brief.item.story_id, brief.item.tier
        )));
    }
    let facts: Vec<&Fact> = brief.facts.iter().take(4).collect();
    let series: Vec<_> = brief
        .series
        .iter()
        .filter(|s| s.points >= 1)
        .take(4)
        .collect();
    if facts.is_empty() && series.is_empty() {
        return Err(NewsError::precondition(format!(
            "the brief for story {} has no facts or series to build a templated page from",
            brief.item.story_id
        )));
    }

    let mut lines = Vec::new();
    let mut children = Vec::new();
    let mut components = Vec::new();
    let mut refs: Vec<String> = Vec::new();
    let names: Vec<&str> = brief.entities.iter().map(|e| e.name.as_str()).collect();
    let figure = |i: usize, f: &Fact| {
        format!(
            "f{i} = KeyFigure({}, {})",
            lit(&plain(&f.label, &names, "Figure")),
            lit(&f.reference)
        )
    };
    match facts.as_slice() {
        [] => {}
        [one] => {
            children.push("f1".to_string());
            lines.push(figure(1, one));
            components.push("KeyFigure");
        }
        many => {
            let names: Vec<String> = (1..=many.len()).map(|i| format!("f{i}")).collect();
            children.push("figures".to_string());
            lines.push(format!("figures = KeyFigures([{}])", names.join(", ")));
            lines.extend(many.iter().enumerate().map(|(i, f)| figure(i + 1, f)));
            components.extend(["KeyFigures", "KeyFigure"]);
        }
    }
    refs.extend(facts.iter().map(|f| f.reference.clone()));
    if let Some(main) = series.iter().find(|s| s.points >= 2) {
        children.push("chart".to_string());
        let label = lit(&plain(&main.label, &names, "The series"));
        lines.push(format!(
            "chart = LineChart({label}, {label}, [{}])",
            lit(&main.reference)
        ));
        components.push("LineChart");
    }
    if let Some(first) = series.first() {
        let list: Vec<String> = series.iter().map(|s| lit(&s.reference)).collect();
        children.push("table".to_string());
        let label = lit(&plain(&first.label, &names, "The series"));
        lines.push(format!(
            "table = DataTable({label}, {label}, [{}])",
            list.join(", ")
        ));
        components.push("DataTable");
        refs.extend(series.iter().map(|s| s.reference.clone()));
    }
    let oui = format!(
        "root = Page([{}])\n{}\n",
        children.join(", "),
        lines.join("\n")
    );

    let noun = noun(&brief.item.page_type);
    let subject = brief
        .entities
        .first()
        .map(|e| e.name.as_str())
        .filter(|n| n.chars().count() <= 50)
        .unwrap_or("Latest");
    let mut headline = match facts.first() {
        Some(f) => format!("{subject} {noun}: {}", cite(brief, f)),
        None => format!("{subject} {noun} from the source data"),
    };
    if headline.chars().count() < 20 {
        headline.push_str(", as published");
    }
    let fly_title = if subject.chars().count() + noun.len() < 40 {
        format!("{subject} {noun}")
    } else {
        format!("Latest {noun}")
    };
    let summary = if facts.is_empty() {
        "Figures from the source data, shown without commentary, with the full series in the chart and table."
            .to_string()
    } else {
        let cited: Vec<String> = facts.iter().take(3).map(|f| cite(brief, f)).collect();
        format!(
            "Figures from the source data, shown without commentary: {}.",
            cited.join("; ")
        )
    };
    for f in &facts {
        if let Some(pd) = period_of(brief, f) {
            refs.push(pd.to_string());
        }
    }
    refs.sort();
    refs.dedup();
    Ok(Copy {
        headline,
        fly_title,
        summary,
        oui,
        refs_used: refs,
        components_used: components.into_iter().map(str::to_string).collect(),
        angle: "latest-figure".to_string(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::validate::{tests::brief, validate};

    #[test]
    fn builds_a_valid_copy_with_no_prose_from_a_tier_2_brief() {
        let copy = templated(&brief()).unwrap();
        assert_eq!(
            copy.headline,
            "United States figures: {dp_7kq2} in {pd_2026_10}"
        );
        assert_eq!(copy.fly_title, "United States figures");
        assert!(
            copy.oui
                .starts_with("root = Page([figures, chart, table])\n")
        );
        assert!(copy.oui.contains("f1 = KeyFigure(\"CPI-U\", \"dp_7kq2\")"));
        // The series label has a digit in its only part, so code names it plainly instead.
        assert!(
            copy.oui
                .contains("chart = LineChart(\"The series\", \"The series\"")
        );
        let value = serde_json::to_value(&copy).unwrap();
        assert_eq!(validate(&value, Some(&brief()), None), []);
    }

    #[test]
    fn labels_lose_the_parts_that_would_type_a_number() {
        assert_eq!(
            plain("GDP (current US$), change from 2024", &[], "x"),
            "GDP (current US$)"
        );
        assert_eq!(
            plain(
                "All employees, thousands, total nonfarm, seasonally adjusted",
                &[],
                "x"
            ),
            "All employees, total nonfarm, seasonally adjusted"
        );
        assert_eq!(plain("G7 output, 2025", &["G7"], "x"), "G7 output");
        assert_eq!(plain("2021 to 2025", &[], "Figure"), "Figure");
    }

    #[test]
    fn refuses_tier_1_and_empty_briefs() {
        let mut b = brief();
        b.item.tier = 1;
        assert_eq!(
            templated(&b).unwrap_err().code,
            news_core::ErrorCode::Precondition
        );
        let mut b = brief();
        b.facts.clear();
        b.series.clear();
        assert!(templated(&b).is_err());
    }

    #[test]
    fn a_single_fact_stands_alone_and_labels_are_escaped() {
        let mut b = brief();
        b.facts.truncate(1);
        b.facts[0].label = "Say \"hi\"".into();
        b.series.clear();
        b.entities.clear();
        let copy = templated(&b).unwrap();
        assert_eq!(
            copy.oui,
            "root = Page([f1])\nf1 = KeyFigure(\"Say \\\"hi\\\"\", \"dp_7kq2\")\n"
        );
        assert_eq!(copy.headline, "Latest figures: {dp_7kq2} in {pd_2026_10}");
    }
}
