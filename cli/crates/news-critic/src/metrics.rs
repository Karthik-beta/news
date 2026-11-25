//! Critic's deterministic metrics (Bureau §9.1): each set's stored outputs scored by code against its
//! labels. No model is called here; the same functions score Audit's fresh outputs.
//!
//! A case with no output, or an output of the wrong shape, counts as failed: metrics are over every
//! case of the set, so a silent model never looks better than a wrong one.

use crate::golden::{Output, Set, render};
use chrono::{DateTime, Utc};
use news_home::check;
use news_home::draft::Draft;
use news_home::facts::PageFacts;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// One set's result.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Score {
    pub metrics: BTreeMap<String, f64>,
    pub passed: u32,
    pub failed: u32,
    /// (case id, why it failed).
    pub failures: Vec<(String, String)>,
}

impl Score {
    fn case(&mut self, id: &str, why: Option<String>) {
        match why {
            None => self.passed += 1,
            Some(w) => {
                self.failed += 1;
                self.failures.push((id.to_owned(), w));
            }
        }
    }

    fn put(&mut self, name: &str, v: f64) {
        self.metrics.insert(name.to_owned(), v);
    }
}

fn ratio(n: usize, d: usize) -> f64 {
    if d == 0 { 0.0 } else { n as f64 / d as f64 }
}

/// Score a set. `outputs` is `None` for a check of the set alone: every valid case passes, every case
/// with a problem fails, and only `cases` is reported.
pub fn score(set: &Set, outputs: Option<&[Output]>) -> Score {
    let mut s = Score::default();
    for (id, p) in &set.problems {
        s.case(id, Some(format!("invalid case: {p}")));
    }
    s.put("cases", set.cases.len() as f64);
    let Some(outputs) = outputs else {
        s.passed += set.cases.len() as u32;
        return s;
    };
    let by_case: BTreeMap<&str, &Output> = outputs.iter().map(|o| (o.case.as_str(), o)).collect();
    let cases: Vec<(&str, &Value, Option<&Output>)> = set
        .cases
        .iter()
        .map(|c| {
            let id = c["id"].as_str().unwrap_or_default();
            (id, c, by_case.get(id).copied())
        })
        .collect();
    match set.manifest.suite.as_str() {
        "writer" => writer(&mut s, &cases),
        "classify" => classify(&mut s, &cases),
        "curator" => curator(&mut s, &cases),
        "spotcheck" => spotcheck(&mut s, &cases, &set.briefs),
        _ => {}
    }
    s
}

type Cases<'a> = [(&'a str, &'a Value, Option<&'a Output>)];

fn is_c4_c5(validation: &str) -> bool {
    ["invalid:C4", "invalid:C5"]
        .iter()
        .any(|p| validation.starts_with(p))
}

/// G1: valid on first try and after repair, C4/C5 violations left after repair, tokens per page.
fn writer(s: &mut Score, cases: &Cases) {
    let (mut first, mut after, mut c45, mut tokens) = (0, 0, 0, 0u64);
    for (id, _, out) in cases {
        let calls = out.map(|o| o.calls.as_slice()).unwrap_or_default();
        let (Some(a1), Some(last)) = (calls.first(), calls.last()) else {
            s.case(id, Some("no calls recorded".into()));
            continue;
        };
        tokens += calls.iter().map(|c| c.tokens).sum::<u64>();
        first += usize::from(a1.validation == "valid");
        let ok = last.validation == "valid";
        after += usize::from(ok);
        c45 += usize::from(is_c4_c5(&last.validation));
        s.case(
            id,
            (!ok).then(|| format!("after repair: {}", last.validation)),
        );
    }
    let n = cases.len();
    s.put("valid_first_try", ratio(first, n));
    s.put("valid_after_repair", ratio(after, n));
    s.put("c4_c5_after_repair", c45 as f64);
    s.put(
        "tokens_per_page",
        if n == 0 {
            0.0
        } else {
            tokens as f64 / n as f64
        },
    );
}

fn strings(v: &Value) -> BTreeSet<&str> {
    v.as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// G2: page type and region accuracy, category F1 (micro, over all cases).
fn classify(s: &mut Score, cases: &Cases) {
    let (mut types, mut regions, mut tp, mut fp, mut fneg) = (0, 0, 0, 0, 0);
    for (id, case, out) in cases {
        let label = &case["label"];
        let want = strings(&label["categories"]);
        let got = out.and_then(|o| o.output.as_ref());
        let Some(got) = got.filter(|g| g["page_type"].is_string()) else {
            fneg += want.len();
            s.case(id, Some("no a3.classify output".into()));
            continue;
        };
        let cats = strings(&got["categories"]);
        tp += cats.intersection(&want).count();
        fp += cats.difference(&want).count();
        fneg += want.difference(&cats).count();
        regions += usize::from(got["region"] == label["region"]);
        let ok = got["page_type"] == label["page_type"];
        types += usize::from(ok);
        s.case(
            id,
            (!ok).then(|| format!("page type {} for {}", got["page_type"], label["page_type"])),
        );
    }
    let n = cases.len();
    s.put("page_type_accuracy", ratio(types, n));
    s.put("region_accuracy", ratio(regions, n));
    s.put("category_f1", ratio(2 * tp, 2 * tp + fp + fneg));
}

fn time(v: &Value) -> Option<DateTime<Utc>> {
    v.as_str()
        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
        .map(|t| t.with_timezone(&Utc))
}

fn page_facts(p: &Value) -> PageFacts {
    let text = |k: &str| p[k].as_str().map(str::to_owned);
    let list = |k: &str| strings(&p[k]).into_iter().map(str::to_owned).collect();
    PageFacts {
        page_id: text("page_id").unwrap_or_default(),
        status: text("status").unwrap_or_default(),
        title: text("title"),
        tier: p["tier"].as_i64().and_then(|t| i16::try_from(t).ok()),
        page_type: text("page_type"),
        published_at: time(&p["published_at"]),
        // Order matters: the primary category is first.
        categories: p["categories"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|c| c.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        region: text("region"),
        entities: list("entities"),
        figures: list("figures"),
        charts: Vec::new(),
        input: None,
    }
}

/// G5: `news home check` pass rate, pages placed that must not be, and breadth (top-level categories
/// and regions of the placed pages, as the check counts them).
fn curator(s: &mut Score, cases: &Cases) {
    let (mut pass, mut excluded, mut cats, mut regions) = (0, 0usize, 0usize, 0usize);
    for (id, case, out) in cases {
        let draft = out
            .and_then(|o| o.output.clone())
            .and_then(|d| serde_json::from_value::<Draft>(d).ok());
        let (Some(draft), Some(now)) = (draft, time(&case["now"])) else {
            s.case(id, Some("no valid home draft".into()));
            continue;
        };
        let pages: BTreeMap<String, PageFacts> = case["pages"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(page_facts)
                    .map(|p| (p.page_id.clone(), p))
                    .collect()
            })
            .unwrap_or_default();
        let report = check::run(&draft, &pages, now);
        let placed: Vec<&PageFacts> = draft
            .placed()
            .iter()
            .filter_map(|(slot, _)| pages.get(&slot.page_id))
            .collect();
        let bad: Vec<&str> = strings(&case["expect"]["excluded"])
            .into_iter()
            .filter(|e| placed.iter().any(|p| p.page_id == *e))
            .collect();
        excluded += bad.len();
        cats += placed
            .iter()
            .filter_map(|p| p.categories.first()?.split('/').next())
            .collect::<BTreeSet<_>>()
            .len();
        regions += placed
            .iter()
            .filter_map(|p| p.region.as_deref())
            .collect::<BTreeSet<_>>()
            .len();
        pass += usize::from(report.pass);
        let why = if !bad.is_empty() {
            Some(format!("placed excluded pages {bad:?}"))
        } else if !report.pass {
            Some(
                report
                    .failures
                    .iter()
                    .map(|f| f.code.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        } else {
            None
        };
        s.case(id, why);
    }
    let n = cases.len();
    s.put("check_pass_rate", ratio(pass, n));
    s.put("excluded_violations", excluded as f64);
    s.put("mean_categories", ratio(cats, n));
    s.put("mean_regions", ratio(regions, n));
}

/// An A7 issue points at the seeded fault when either text contains the other (A7 quotes up to a
/// hundred and twenty characters, enough to find the sentence).
fn overlaps(issue: &str, fault: &str) -> bool {
    let (i, f) = (issue.trim(), fault.trim());
    !i.is_empty() && (f.contains(i) || i.contains(f))
}

/// G6: recall of seeded faults (by location), share of A7 issues that point at no seeded fault, and
/// controls flagged.
fn spotcheck(s: &mut Score, cases: &Cases, briefs: &BTreeMap<String, Value>) {
    let (mut faults, mut found, mut kinds, mut issues, mut false_issues) = (0, 0, 0, 0, 0);
    let (mut controls, mut controls_flagged) = (0, 0);
    for (id, case, out) in cases {
        let brief = briefs
            .get(case["brief_case"].as_str().unwrap_or_default())
            .cloned()
            .unwrap_or_default();
        let fault = case["fault"].as_object().and_then(|f| {
            Some((
                f["kind"].as_str()?,
                render(f["where"].as_str()?, &brief).ok()?,
            ))
        });
        if fault.is_some() {
            faults += 1;
        } else {
            controls += 1;
        }
        let got = out.and_then(|o| o.output.as_ref());
        let Some(list) = got.and_then(|g| g["issues"].as_array()) else {
            s.case(id, Some("no a7.spotcheck output".into()));
            continue;
        };
        let hits: Vec<&Value> = match &fault {
            Some((_, w)) => list
                .iter()
                .filter(|i| overlaps(i["where"].as_str().unwrap_or_default(), w))
                .collect(),
            None => Vec::new(),
        };
        issues += list.len();
        false_issues += list.len() - hits.len();
        let why = match &fault {
            Some((kind, _)) => {
                let hit = !hits.is_empty();
                found += usize::from(hit);
                kinds += usize::from(hits.iter().any(|i| i["kind"] == *kind));
                (!hit).then(|| format!("missed the seeded {kind}"))
            }
            None => {
                controls_flagged += usize::from(!list.is_empty());
                (!list.is_empty()).then(|| "flagged a clean control".to_owned())
            }
        };
        s.case(id, why);
    }
    s.put("a7_recall", ratio(found, faults));
    s.put("a7_kind_match", ratio(kinds, found));
    s.put("false_flag_rate", ratio(false_issues, issues));
    s.put("control_flag_rate", ratio(controls_flagged, controls));
}

#[cfg(test)]
mod tests {
    //! Hand-made outputs, the validator-contract layer of §9.1: they test the scoring, not a model.
    use super::*;
    use crate::golden::{Call, Manifest};
    use serde_json::json;

    fn set(suite: &str, cases: Vec<Value>) -> Set {
        Set {
            manifest: Manifest {
                set: "gx".into(),
                name: "Test".into(),
                suite: suite.into(),
                version: 1,
                step: "A1".into(),
                target_cases: 10,
                targets: BTreeMap::new(),
                notes: None,
            },
            cases,
            problems: Vec::new(),
            briefs: BTreeMap::from([(
                "g1-b".to_owned(),
                json!({"facts": [{"ref": "dv_a", "value": "0.4", "unit": "percent"}]}),
            )]),
        }
    }

    fn out(case: &str, output: Value) -> Output {
        Output {
            case: case.into(),
            output: Some(output),
            calls: Vec::new(),
        }
    }

    fn call(validation: &str, round: u32) -> Call {
        Call {
            step: if round == 0 { "A1" } else { "A2" }.into(),
            validation: validation.into(),
            repair_round: round,
            tokens: 100,
        }
    }

    #[test]
    fn checking_a_set_alone_passes_valid_cases_and_fails_problems() {
        let mut s = set("writer", vec![json!({"id": "a"}), json!({"id": "b"})]);
        s.problems.push(("c".into(), "bad".into()));
        let r = score(&s, None);
        assert_eq!((r.passed, r.failed), (2, 1));
        assert_eq!(r.metrics["cases"], 2.0);
    }

    #[test]
    fn writer_counts_first_try_repairs_and_c4_c5() {
        let s = set(
            "writer",
            ["a", "b", "c", "d"].map(|i| json!({"id": i})).to_vec(),
        );
        let o = |c: &str, calls: Vec<Call>| Output {
            case: c.into(),
            output: None,
            calls,
        };
        let outs = vec![
            o("a", vec![call("valid", 0)]),
            o("b", vec![call("invalid:C2", 0), call("valid", 1)]),
            o("c", vec![call("invalid:C4", 0), call("invalid:C4", 1)]),
        ];
        let r = score(&s, Some(&outs));
        assert_eq!(r.metrics["valid_first_try"], 0.25);
        assert_eq!(r.metrics["valid_after_repair"], 0.5);
        assert_eq!(r.metrics["c4_c5_after_repair"], 1.0);
        assert_eq!(r.metrics["tokens_per_page"], 125.0);
        assert_eq!((r.passed, r.failed), (2, 2));
    }

    #[test]
    fn classify_scores_types_regions_and_category_f1() {
        let case = |id: &str| json!({"id": id, "label": {"page_type": "science", "categories": ["science", "health"], "region": "us"}});
        let s = set("classify", vec![case("a"), case("b")]);
        let outs = vec![
            out(
                "a",
                json!({"page_type": "science", "categories": ["science"], "region": "us", "confidence": "high"}),
            ),
            out(
                "b",
                json!({"page_type": "news-story", "categories": ["science", "economy"], "region": "eu", "confidence": "low"}),
            ),
        ];
        let r = score(&s, Some(&outs));
        assert_eq!(r.metrics["page_type_accuracy"], 0.5);
        assert_eq!(r.metrics["region_accuracy"], 0.5);
        // tp 2, fp 1, fn 2 → 4 / 7
        assert!((r.metrics["category_f1"] - 4.0 / 7.0).abs() < 1e-12);
        assert_eq!((r.passed, r.failed), (1, 1));
    }

    #[test]
    fn spotcheck_measures_recall_false_flags_and_controls() {
        let cases = vec![
            json!({"id": "f1", "brief_case": "g1-b", "fault": {"kind": "wrong_direction", "where": "fell {dv_a}", "note": ""}}),
            json!({"id": "f2", "brief_case": "g1-b", "fault": {"kind": "tone", "where": "Buy now.", "note": ""}}),
            json!({"id": "c1", "brief_case": "g1-b", "fault": null}),
        ];
        let s = set("spotcheck", cases);
        let outs = vec![
            out(
                "f1",
                json!({"verdict": "flag", "issues": [
                {"kind": "wrong_direction", "where": "Prices fell 0.4 percent", "evidence": [], "severity": "high"},
                {"kind": "tone", "where": "elsewhere", "evidence": [], "severity": "low"}]}),
            ),
            out("f2", json!({"verdict": "pass", "issues": []})),
            out(
                "c1",
                json!({"verdict": "flag", "issues": [
                {"kind": "other", "where": "x", "evidence": [], "severity": "low"}]}),
            ),
        ];
        let r = score(&s, Some(&outs));
        assert_eq!(r.metrics["a7_recall"], 0.5);
        assert_eq!(r.metrics["a7_kind_match"], 1.0);
        assert!((r.metrics["false_flag_rate"] - 2.0 / 3.0).abs() < 1e-12);
        assert_eq!(r.metrics["control_flag_rate"], 1.0);
        assert_eq!((r.passed, r.failed), (1, 2));
    }

    #[test]
    fn curator_runs_the_home_check_and_counts_excluded_pages() {
        let page = |id: &str, status: &str, cat: &str, region: &str, ent: &str| {
            json!({"page_id": id, "status": status, "title": "T", "tier": 2, "page_type": "macro",
                   "published_at": "2026-11-19T00:00:00Z", "categories": [cat], "region": region,
                   "entities": [ent], "figures": []})
        };
        let case = json!({"id": "g5-x", "now": "2026-11-20T00:00:00Z", "expect": {"excluded": ["w"]}, "pages": [
            page("a", "published", "economy", "uk", "e1"), page("b", "published", "science", "us", "e2"),
            page("c", "published", "finance", "europe", "e3"), page("w", "withdrawn", "economy", "uk", "e4")]});
        let s = set("curator", vec![case]);
        let slot = |id: &str| json!({"page_id": id});
        let draft = json!({"curator_model": "test", "slots": {"lead": slot("w"), "side": [slot("a")],
                           "sections": [{"category": "economy", "pages": [slot("b"), slot("c")]}]}});
        let r = score(&s, Some(&[out("g5-x", draft)]));
        assert_eq!(r.metrics["excluded_violations"], 1.0);
        assert_eq!(r.metrics["check_pass_rate"], 0.0);
        assert_eq!(r.metrics["mean_regions"], 3.0);
        assert_eq!((r.passed, r.failed), (0, 1));
        assert!(r.failures[0].1.contains("excluded"));
    }

    #[test]
    fn missing_outputs_fail_their_cases() {
        let s = set(
            "classify",
            vec![
                json!({"id": "a", "label": {"page_type": "science", "categories": ["science"], "region": "us"}}),
            ],
        );
        let r = score(&s, Some(&[]));
        assert_eq!((r.passed, r.failed), (0, 1));
        assert_eq!(r.metrics["page_type_accuracy"], 0.0);
    }
}
