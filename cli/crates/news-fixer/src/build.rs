//! Building a brief from a story's [`Inputs`] (Bureau §3.8), with its size budget.
//!
//! * **Facts** are the story's data points with their published text, plus derived changes (`dv_`)
//!   against the previous period and a year earlier, computed only where the Ledger's guards allow
//!   (same unit, currency, scale, seasonal adjustment, comparable periods).
//! * **Series** (`sr_`) are each story series' last five years; **periods** (`pd_`) are named from
//!   their dates; **context lines** (`cx_`) come from [`crate::context`].
//! * **Quotes** (`q_`) are verbatim spans of text blocks; all other source text goes under
//!   `source_text`, fenced as untrusted. Nothing else in the brief is taken from document text.
//! * **Budget**: above [`TARGET_TOKENS`], the lowest-priority facts and series go first (never the
//!   main series' latest point and its changes); if the source text still does not fit, it is replaced
//!   by a code extract and an A5 condensation is requested ([`Condense`]). Above [`HARD_CAP_TOKENS`]
//!   the build fails with `budget_exceeded`.
//!
//! Next to the brief, [`Refs`] says what each issued ref resolves to, so the renderer and the checks
//! can fill and verify it without trusting the brief.

use crate::context;
use crate::inputs::{Document, Inputs, Point};
use chrono::{Datelike, Months, NaiveDate};
use news_core::brief::{
    BRIEF_VERSION, Brief, Components, ContextLine, Fact, Item, Media, Period, Quote, Rules, Series,
    SourceText,
};
use news_core::{ErrorCode, NewsError, NewsResult};
use news_ledger::PeriodType;
use news_ledger::guard::{Comparison, Formula, derive};
use news_ledger::point::base32;
use news_ledger::units::{UnitClass, classify};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Size the brief aims for (Bureau §3.8).
pub const TARGET_TOKENS: usize = 3000;
/// Size above which no brief is written (`budget_exceeded`).
pub const HARD_CAP_TOKENS: usize = 4000;
/// Series cover this many months up to the latest point.
const SERIES_MONTHS: u32 = 60;
const MAX_QUOTES: usize = 3;
/// Bytes a quote or an extract point keeps, at most (cut at a sentence end, else a word).
const QUOTE_BYTES: usize = 300;
const EXTRACT_BYTES: usize = 400;
const MAX_RELATED: usize = 10;

/// Estimated tokens of `text`.
///
/// shortcut: ceil(UTF-8 bytes / 3), the same estimate the component index budget uses (ADR 0001); no
/// tokenizer crate is in the lockfile. Upgrade to a real tokenizer if telemetry shows briefs' real
/// input tokens differ from this by more than about 15%.
pub fn tokens(text: &str) -> usize {
    text.len().div_ceil(3)
}

/// What each ref in a brief resolves to. `dp_` refs are `data_points` rows and need no entry.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Refs {
    pub derived: BTreeMap<String, DerivedRef>,
    pub series: BTreeMap<String, SeriesRef>,
    pub periods: BTreeMap<String, PeriodRef>,
    pub quotes: BTreeMap<String, QuoteRef>,
    pub context: BTreeMap<String, ContextRef>,
}

/// A `derived_points` row (Ledger, §3.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedRef {
    pub formula: String,
    pub inputs: Vec<String>,
    pub value: String,
    pub unit: String,
    pub rounding: u32,
    pub guard: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeriesRef {
    pub series_key: String,
    pub unit: String,
    /// `[pd_, dp_]` pairs, oldest first.
    pub points: Vec<[String; 2]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeriodRef {
    pub label: String,
    pub start: Option<NaiveDate>,
    pub end: Option<NaiveDate>,
}

/// A quote is `text_blocks(doc, block).text[start..end]` (byte offsets).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteRef {
    pub doc: i64,
    pub block: i32,
    pub start: usize,
    pub end: usize,
}

/// The rule and the points a context line was computed from: `[latest dp_, earlier dp_]`, and the
/// series it reads is the latest point's (see [`crate::context::recheck`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextRef {
    pub rule: String,
    pub inputs: Vec<String>,
}

/// An A5 request: condense these blocks for this story (job kind `ai.condense`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Condense {
    pub story_id: i64,
    pub page_type: String,
    /// `[doc, block]` pairs, in brief order.
    pub blocks: Vec<(i64, i32)>,
}

/// A built brief and what goes with it.
#[derive(Debug, Clone, PartialEq)]
pub struct Built {
    pub brief: Brief,
    pub refs: Refs,
    /// Estimated tokens of the compact JSON brief.
    pub tokens: usize,
    /// Refs dropped to fit the budget, lowest priority first.
    pub trimmed: Vec<String>,
    /// Set when the source text was replaced by a code extract and A5 should condense it.
    pub condense: Option<Condense>,
}

/// One story series: the story's latest point in it and its history as of that point.
struct Group<'a> {
    anchor: &'a Point,
    /// One point per period, latest vintage no later than the anchor's, oldest first.
    history: Vec<&'a Point>,
}

/// A fact or a series, with its priority (lower is kept longer).
#[derive(Clone)]
enum Entry {
    Fact {
        fact: Fact,
        point: Option<usize>,
        group: usize,
    },
    Series {
        series: Series,
        group: usize,
        points: Vec<usize>,
    },
}

struct Ranked {
    key: (u8, usize, usize),
    entry: Entry,
    protected: bool,
}

fn group_key(p: &Point) -> (&str, &str) {
    (&p.series_key, &p.unit)
}

fn groups(inputs: &Inputs) -> Vec<Group<'_>> {
    let mut keys: Vec<(&str, &str)> = Vec::new();
    for p in &inputs.points {
        if !keys.contains(&group_key(p)) {
            keys.push(group_key(p));
        }
    }
    keys.into_iter()
        .map(|k| {
            let own: Vec<&Point> = inputs.points.iter().filter(|p| group_key(p) == k).collect();
            let anchor = own
                .iter()
                .copied()
                .max_by(|a, b| {
                    (a.period_end, a.vintage, &a.dp_id).cmp(&(b.period_end, b.vintage, &b.dp_id))
                })
                .unwrap_or(own[0]);
            // Latest vintage per period, no later than the anchor's, up to the anchor's period.
            let mut per: BTreeMap<(Option<NaiveDate>, Option<NaiveDate>), &Point> = BTreeMap::new();
            for p in inputs.history.iter().chain(own.iter().copied()) {
                if group_key(p) != k
                    || p.vintage > anchor.vintage
                    || p.period_end.is_none()
                    || p.period_end > anchor.period_end
                {
                    continue;
                }
                let slot = per.entry((p.period_end, p.period_start)).or_insert(p);
                if (p.vintage, &p.dp_id) > (slot.vintage, &slot.dp_id) {
                    *slot = p;
                }
            }
            let mut history: Vec<&Point> = per.into_values().collect();
            // The anchor stands for its own period.
            for h in &mut history {
                if h.period_end == anchor.period_end && h.period_start == anchor.period_start {
                    *h = anchor;
                }
            }
            Group { anchor, history }
        })
        .collect()
}

fn unit_text(p: &Point) -> String {
    match p.scale.as_deref() {
        Some(scale) if scale != "units" => format!("{}, {scale}", p.unit),
        _ => p.unit.clone(),
    }
}

fn point_fact(p: &Point) -> Fact {
    Fact {
        reference: p.dp_id.clone(),
        label: p.label.clone(),
        value: p.value_text.clone(),
        unit: unit_text(p),
        period: Some(p.period_label.clone()),
        derived: None,
    }
}

fn fraction_digits(text: &str) -> u32 {
    text.split_once('.')
        .map_or(0, |(_, f)| f.bytes().take_while(u8::is_ascii_digit).count())
        .try_into()
        .unwrap_or(0)
}

/// The change from `b` to `a`, where the Ledger's guards allow it.
fn change(a: &Point, b: &Point, cmp: Comparison) -> Option<(Fact, DerivedRef)> {
    let class = classify(&a.unit)?.class;
    let (formula, rounding) = if class == UnitClass::Percent {
        (
            Formula::PpChange,
            fraction_digits(&a.value_text).max(fraction_digits(&b.value_text)),
        )
    } else {
        (Formula::PctChange, 1)
    };
    let d = derive(formula, &a.ledger()?, &b.ledger()?, cmp, rounding).ok()?;
    let name = d.formula.split('(').next().unwrap_or(d.formula).to_owned();
    Some((
        Fact {
            reference: d.dp_id.clone(),
            label: format!("{}, change from {}", a.label, b.period_label),
            value: d.value.clone(),
            unit: d.unit.clone(),
            period: Some(a.period_label.clone()),
            derived: Some(name.clone()),
        },
        DerivedRef {
            formula: name,
            inputs: d.inputs,
            value: d.value,
            unit: d.unit,
            rounding: d.rounding,
            guard: d.guard,
        },
    ))
}

fn last_day_of_month(s: NaiveDate) -> Option<NaiveDate> {
    s.checked_add_months(Months::new(1))?.pred_opt()
}

/// A period's ref, from its dates: `pd_2026_10` for a calendar month, `pd_2025` for a calendar year,
/// `pd_2026_09_16` for a day, `pd_20231001_20240928` for any other span; a hash of the label when there
/// are no dates.
pub fn period_ref(p: &Point) -> String {
    match (p.period_start, p.period_end) {
        (_, Some(e)) if p.period_type == PeriodType::Instant || p.period_start == Some(e) => {
            format!("pd_{}", e.format("%Y_%m_%d"))
        }
        (Some(s), Some(e)) if s.day() == 1 && last_day_of_month(s) == Some(e) => {
            format!("pd_{}_{:02}", s.year(), s.month())
        }
        (Some(s), Some(e))
            if (s.month(), s.day(), e.month(), e.day()) == (1, 1, 12, 31)
                && s.year() == e.year() =>
        {
            format!("pd_{}", s.year())
        }
        (Some(s), Some(e)) => format!("pd_{}_{}", s.format("%Y%m%d"), e.format("%Y%m%d")),
        _ => format!(
            "pd_{}",
            &base32(&Sha256::digest(p.period_label.as_bytes()))[..10]
        ),
    }
}

fn series_ref(points: &[&Point]) -> String {
    let (first, last) = (points[0], points[points.len() - 1]);
    let input = format!(
        "{}|{}|{}|{}",
        first.series_key, first.unit, first.dp_id, last.dp_id
    );
    format!("sr_{}", &base32(&Sha256::digest(input.as_bytes()))[..12])
}

/// The longest prefix of `text` up to `max` bytes ending at a sentence end, else at a space.
fn cut(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let head = &text[..end];
    let sentence = ['.', '!', '?']
        .iter()
        .filter_map(|c| head.rfind(&format!("{c} ")).map(|i| i + 1))
        .max();
    match sentence.or_else(|| head.rfind(' ')) {
        Some(i) if i > 0 => head[..i].trim_end(),
        _ => head,
    }
}

fn is_heading(kind: Option<&str>) -> bool {
    kind == Some("heading")
}

/// Up to [`MAX_QUOTES`] spans: quote blocks first, then paragraphs with quotation marks, then each
/// document's first paragraph.
fn quotes(docs: &[Document]) -> Vec<(Quote, QuoteRef)> {
    let blocks: Vec<(&Document, &crate::inputs::Block)> = docs
        .iter()
        .flat_map(|d| d.blocks.iter().map(move |b| (d, b)))
        .filter(|(_, b)| !is_heading(b.kind.as_deref()) && !b.text.trim().is_empty())
        .collect();
    let first_of_doc = |d: &Document, b: &crate::inputs::Block| {
        d.blocks
            .iter()
            .find(|x| !is_heading(x.kind.as_deref()) && !x.text.trim().is_empty())
            .is_some_and(|x| x.block_no == b.block_no)
    };
    let rank = |(d, b): &(&Document, &crate::inputs::Block)| {
        if b.kind.as_deref() == Some("quote") {
            Some(0)
        } else if b.text.contains(['“', '"']) {
            Some(1)
        } else if first_of_doc(d, b) {
            Some(2)
        } else {
            None
        }
    };
    let mut picked: Vec<_> = blocks
        .iter()
        .enumerate()
        .filter_map(|(i, x)| Some((rank(x)?, i, x)))
        .collect();
    picked.sort_by_key(|(r, i, _)| (*r, *i));
    picked
        .into_iter()
        .take(MAX_QUOTES)
        .map(|(_, _, (d, b))| {
            let start = b.text.len() - b.text.trim_start().len();
            let span = cut(b.text[start..].trim_end(), QUOTE_BYTES);
            let end = start + span.len();
            (
                Quote {
                    id: format!("q_{}_{}", d.doc_id, b.block_no),
                    doc: d.doc_id,
                    block: b.block_no,
                    text: span.to_owned(),
                },
                QuoteRef {
                    doc: d.doc_id,
                    block: b.block_no,
                    start,
                    end,
                },
            )
        })
        .collect()
}

/// A5's code fallback (§7.1): headings, each document's first paragraph, and sentences naming one of
/// the story's entities, each cut to [`EXTRACT_BYTES`].
fn extract(inputs: &Inputs) -> Vec<String> {
    let names: Vec<String> = inputs
        .entities
        .iter()
        .map(|e| e.name.to_lowercase())
        .collect();
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: &str| {
        let s = cut(s.trim(), EXTRACT_BYTES).to_owned();
        if !s.is_empty() && !out.contains(&s) {
            out.push(s);
        }
    };
    for d in &inputs.documents {
        let mut first = true;
        for b in &d.blocks {
            if is_heading(b.kind.as_deref()) {
                push(&b.text);
            } else if first && !b.text.trim().is_empty() {
                first = false;
                push(&b.text);
            } else {
                for sentence in b.text.split_inclusive(". ") {
                    let lower = sentence.to_lowercase();
                    if names.iter().any(|n| lower.contains(n.as_str())) {
                        push(sentence);
                    }
                }
            }
        }
    }
    out
}

fn disclaimer(inputs: &Inputs) -> Option<String> {
    let has = |c: &[&str]| inputs.categories.iter().any(|x| c.contains(&x.as_str()));
    if has(&["health", "medicine"]) {
        Some("not-medical-advice".into())
    } else if has(&["economy", "finance"])
        || [
            "earnings",
            "macro-release",
            "policy-decision",
            "major-report",
        ]
        .contains(&inputs.page_type.as_str())
    {
        Some("not-investment-advice".into())
    } else {
        None
    }
}

/// A note when the facts are a company's fiscal periods: their labels stay the company's, and the
/// fiscal year's end is stated from its dates, never inferred.
fn fiscal_note(points: &[&Point]) -> Option<String> {
    let fiscal: Vec<&&Point> = points
        .iter()
        .filter(|p| p.fiscal_period.is_some())
        .collect();
    if fiscal.is_empty() {
        return None;
    }
    let year_end = fiscal
        .iter()
        .filter(|p| p.fiscal_period.as_deref() == Some("FY"))
        .filter_map(|p| p.period_end)
        .max()
        .filter(|e| (e.month(), e.day()) != (12, 31));
    Some(match year_end {
        Some(e) => format!(
            "Periods are the company's fiscal periods, labelled as it labels them; its fiscal year ends on {e}"
        ),
        None => "Periods are the company's fiscal periods, labelled as it labels them".to_owned(),
    })
}

/// An image as the brief shows it: where it is, its size and licence. Alt text and credit are the
/// source's words, so they stay out (the renderer adds them from the stored record).
fn image(v: &Value) -> Value {
    let keep = ["url", "licence", "width", "height", "rehost"];
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(k, _)| keep.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        ),
        _ => Value::Null,
    }
}

fn slug(event_key: &str) -> Option<String> {
    let s: String = event_key
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    (!s.is_empty()).then_some(s)
}

/// Everything ranked, before the budget.
struct Plan<'a> {
    inputs: &'a Inputs,
    groups: Vec<Group<'a>>,
    ranked: Vec<Ranked>,
    derived: BTreeMap<String, DerivedRef>,
    /// Context lines with the index of the group they belong to.
    context: Vec<(usize, context::Found<'a>)>,
    quotes: Vec<(Quote, QuoteRef)>,
    suggested: Vec<String>,
}

fn plan<'a>(inputs: &'a Inputs, suggested: &[&str]) -> Plan<'a> {
    let groups = groups(inputs);
    let mut ranked: Vec<Ranked> = Vec::new();
    let mut derived = BTreeMap::new();
    let mut context = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let policy = inputs.page_type == "policy-decision";
    // Index of a point among `inputs.points` then `inputs.history`, for period lookup.
    let all: Vec<&Point> = inputs.points.iter().chain(inputs.history.iter()).collect();
    let index_of = |p: &Point| all.iter().position(|q| q.dp_id == p.dp_id);
    let mut add = |ranked: &mut Vec<Ranked>, key, entry: Entry, protected| {
        let r = match &entry {
            Entry::Fact { fact, .. } => fact.reference.clone(),
            Entry::Series { series, .. } => series.reference.clone(),
        };
        if !seen.contains(&r) {
            seen.push(r);
            ranked.push(Ranked {
                key,
                entry,
                protected,
            });
        }
    };
    for (g, grp) in groups.iter().enumerate() {
        let a = grp.anchor;
        add(
            &mut ranked,
            (0, g, 0),
            Entry::Fact {
                fact: point_fact(a),
                point: index_of(a),
                group: g,
            },
            g == 0,
        );
        let pos = grp.history.iter().position(|h| h.dp_id == a.dp_id);
        let prev = pos.and_then(|i| i.checked_sub(1)).map(|i| grp.history[i]);
        let year_ago = grp.history.iter().copied().find(|q| {
            q.period_end
                .zip(a.period_end)
                .is_some_and(|(q, a)| (357..=373).contains(&(a - q).num_days()))
        });
        let mut n = 0;
        for (b, cmp) in [
            (prev, Comparison::Sequential),
            (year_ago, Comparison::YearOnYear),
        ] {
            let Some(b) = b else { continue };
            if let Some((fact, d)) = change(a, b, cmp)
                && !derived.contains_key(&fact.reference)
            {
                derived.insert(fact.reference.clone(), d);
                add(
                    &mut ranked,
                    (1, g, n),
                    Entry::Fact {
                        fact,
                        point: None,
                        group: g,
                    },
                    g == 0,
                );
            }
            add(
                &mut ranked,
                (2, g, n + 1),
                Entry::Fact {
                    fact: point_fact(b),
                    point: index_of(b),
                    group: g,
                },
                false,
            );
            n += 2;
        }
        // The last five years, when there are at least three points.
        let from = a
            .period_end
            .and_then(|e| e.checked_sub_months(Months::new(SERIES_MONTHS)));
        let window: Vec<&Point> = grp
            .history
            .iter()
            .copied()
            .filter(|p| from.is_some() && p.period_end > from)
            .collect();
        if window.len() >= 3 {
            let series = Series {
                reference: series_ref(&window),
                label: format!(
                    "{}, {} to {}",
                    a.label,
                    window[0].period_label,
                    window[window.len() - 1].period_label
                ),
                points: u32::try_from(window.len()).unwrap_or(u32::MAX),
            };
            let points = window.iter().filter_map(|p| index_of(p)).collect();
            add(
                &mut ranked,
                (2, g, 0),
                Entry::Series {
                    series,
                    group: g,
                    points,
                },
                false,
            );
        }
        for found in context::lines(&grp.history, policy) {
            context.push((g, found));
        }
    }
    for (i, p) in inputs.points.iter().enumerate() {
        let g = groups
            .iter()
            .position(|x| group_key(x.anchor) == group_key(p))
            .unwrap_or(0);
        add(
            &mut ranked,
            (3, g, i),
            Entry::Fact {
                fact: point_fact(p),
                point: index_of(p),
                group: g,
            },
            false,
        );
    }
    ranked.sort_by_key(|r| r.key);
    Plan {
        inputs,
        groups,
        ranked,
        derived,
        context,
        quotes: quotes(&inputs.documents),
        suggested: suggested.iter().map(|s| (*s).to_owned()).collect(),
    }
}

impl Plan<'_> {
    fn all(&self) -> Vec<&Point> {
        self.inputs
            .points
            .iter()
            .chain(self.inputs.history.iter())
            .collect()
    }

    /// The brief and its refs for the kept entries and these source-text points.
    fn assemble(&self, kept: &[bool], text: &[String]) -> (Brief, Refs) {
        let all = self.all();
        let mut refs = Refs::default();
        let mut periods: Vec<Period> = Vec::new();
        let mut period = |refs: &mut Refs, p: &Point| {
            let r = period_ref(p);
            if !refs.periods.contains_key(&r) {
                refs.periods.insert(
                    r.clone(),
                    PeriodRef {
                        label: p.period_label.clone(),
                        start: p.period_start,
                        end: p.period_end,
                    },
                );
                periods.push(Period {
                    reference: r.clone(),
                    label: p.period_label.clone(),
                });
            }
            r
        };
        let mut facts = Vec::new();
        let mut series = Vec::new();
        let mut kept_groups: Vec<usize> = Vec::new();
        let mut fact_points: Vec<&Point> = Vec::new();
        for (r, keep) in self.ranked.iter().zip(kept) {
            if !keep {
                continue;
            }
            match &r.entry {
                Entry::Fact { fact, point, group } => {
                    if let Some(p) = point.map(|i| all[i]) {
                        period(&mut refs, p);
                        fact_points.push(p);
                        if p.dp_id == self.groups[*group].anchor.dp_id {
                            kept_groups.push(*group);
                        }
                    } else if let Some(d) = self.derived.get(&fact.reference) {
                        refs.derived.insert(fact.reference.clone(), d.clone());
                    }
                    facts.push(fact.clone());
                }
                Entry::Series {
                    series: s,
                    group,
                    points,
                } => {
                    let pairs = points
                        .iter()
                        .map(|&i| [period(&mut refs, all[i]), all[i].dp_id.clone()])
                        .collect();
                    let a = self.groups[*group].anchor;
                    refs.series.insert(
                        s.reference.clone(),
                        SeriesRef {
                            series_key: a.series_key.clone(),
                            unit: a.unit.clone(),
                            points: pairs,
                        },
                    );
                    series.push(s.clone());
                }
            }
        }
        let mut lines = Vec::new();
        for (g, found) in &self.context {
            if !kept_groups.contains(g) {
                continue;
            }
            let id = format!("cx_{}", lines.len() + 1);
            let mut inputs = vec![self.groups[*g].anchor.dp_id.clone()];
            if let Some(p) = found.since {
                period(&mut refs, p);
                inputs.push(p.dp_id.clone());
            }
            refs.context.insert(
                id.clone(),
                ContextRef {
                    rule: found.rule.to_owned(),
                    inputs,
                },
            );
            lines.push(ContextLine {
                id,
                text: found.text.clone(),
                rule: found.rule.to_owned(),
            });
        }
        for (q, r) in &self.quotes {
            refs.quotes.insert(q.id.clone(), r.clone());
        }
        let inputs = self.inputs;
        let brief = Brief {
            brief_version: BRIEF_VERSION,
            item: Item {
                story_id: inputs.story_id,
                event_key: inputs.event_key.clone(),
                page_type: inputs.page_type.clone(),
                tier: inputs.tier,
                action: inputs.action,
                slug_hint: inputs.event_key.as_deref().and_then(slug),
                data_as_of: inputs.points.iter().map(|p| p.vintage.date_naive()).max(),
            },
            entities: inputs.entities.clone(),
            facts,
            series,
            periods,
            context: lines,
            quotes: self.quotes.iter().map(|(q, _)| q.clone()).collect(),
            source_text: SourceText {
                untrusted: true,
                points: text.to_vec(),
            },
            media: Media {
                images: inputs.images.iter().map(image).collect(),
                video: None,
            },
            related_pages: inputs.related.iter().take(MAX_RELATED).cloned().collect(),
            components: Components {
                suggested: self.suggested.clone(),
            },
            rules: Rules {
                disclaimer: disclaimer(inputs),
                fiscal_note: fiscal_note(&fact_points),
            },
        };
        (brief, refs)
    }
}

/// The largest `k` in `0..=n` with `fits(k)`, for a `fits` that only turns false as `k` grows; 0
/// when nothing fits.
fn largest(n: usize, fits: impl Fn(usize) -> bool) -> usize {
    if fits(n) {
        return n;
    }
    let (mut lo, mut hi) = (0, n);
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if fits(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

fn measure(b: &Brief) -> usize {
    tokens(&serde_json::to_string(b).unwrap_or_default())
}

/// Build the brief for `inputs`, with `suggested` from the page type's layout
/// ([`crate::prefix::suggested`]).
pub fn build(inputs: &Inputs, suggested: &[&str]) -> NewsResult<Built> {
    let plan = plan(inputs, suggested);
    let n = plan.ranked.len();
    // Entries drop from the end of the ranking (protected ones rank first), so what is kept is always
    // the first `k` plus the protected: the largest `k` that fits is found by bisection. Facts and
    // series are measured without the source text, so text never costs a fact.
    let kept_for =
        |k: usize| -> Vec<bool> { (0..n).map(|i| i < k || plan.ranked[i].protected).collect() };
    let k = largest(n, |k| {
        measure(&plan.assemble(&kept_for(k), &[]).0) <= TARGET_TOKENS
    });
    let kept = kept_for(k);
    let trimmed: Vec<String> = (k..n)
        .rev()
        .filter(|&i| !kept[i])
        .map(|i| match &plan.ranked[i].entry {
            Entry::Fact { fact, .. } => fact.reference.clone(),
            Entry::Series { series, .. } => series.reference.clone(),
        })
        .collect();
    let full: Vec<String> = inputs
        .documents
        .iter()
        .flat_map(|d| d.blocks.iter())
        .map(|b| b.text.trim().to_owned())
        .filter(|t| !t.is_empty())
        .collect();
    let (mut brief, mut refs) = plan.assemble(&kept, &full);
    let mut condense = None;
    if measure(&brief) > TARGET_TOKENS && !full.is_empty() {
        condense = Some(Condense {
            story_id: inputs.story_id,
            page_type: inputs.page_type.clone(),
            blocks: inputs
                .documents
                .iter()
                .flat_map(|d| d.blocks.iter().map(|b| (d.doc_id, b.block_no)))
                .collect(),
        });
        let text = extract(inputs);
        let m = largest(text.len(), |m| {
            measure(&plan.assemble(&kept, &text[..m]).0) <= TARGET_TOKENS
        });
        (brief, refs) = plan.assemble(&kept, &text[..m]);
    }
    let size = measure(&brief);
    if size > HARD_CAP_TOKENS {
        return Err(NewsError::new(
            ErrorCode::BudgetExceeded,
            format!(
                "the brief for story {} is about {size} tokens after trimming, over the {HARD_CAP_TOKENS}-token cap",
                inputs.story_id
            ),
        )
        .with_details(serde_json::json!({"tokens": size, "trimmed": trimmed})));
    }
    Ok(Built {
        brief,
        refs,
        tokens: size,
        trimmed,
        condense,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn pt(id: &str, start: (i32, u32, u32), end: (i32, u32, u32), t: PeriodType) -> Point {
        Point {
            dp_id: id.into(),
            series_key: "s".into(),
            label: "L".into(),
            value_text: "1".into(),
            unit: "Percent".into(),
            currency: None,
            scale: None,
            seasonal_adj: None,
            period_label: "label".into(),
            period_start: NaiveDate::from_ymd_opt(start.0, start.1, start.2),
            period_end: NaiveDate::from_ymd_opt(end.0, end.1, end.2),
            period_type: t,
            fiscal_year: None,
            fiscal_period: None,
            vintage: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        }
    }

    #[test]
    fn period_refs_come_from_dates() {
        use PeriodType::*;
        let r = |s, e, t| period_ref(&pt("dp_a", s, e, t));
        assert_eq!(r((2026, 10, 1), (2026, 10, 31), Duration), "pd_2026_10");
        assert_eq!(r((2024, 2, 1), (2024, 2, 29), Duration), "pd_2024_02");
        assert_eq!(r((2025, 1, 1), (2025, 12, 31), Duration), "pd_2025");
        assert_eq!(r((2026, 9, 16), (2026, 9, 16), Duration), "pd_2026_09_16");
        assert_eq!(r((2026, 9, 30), (2026, 9, 30), Instant), "pd_2026_09_30");
        assert_eq!(
            r((2023, 10, 1), (2024, 9, 28), Duration),
            "pd_20231001_20240928"
        );
        let mut undated = pt("dp_a", (1, 1, 1), (1, 1, 1), Duration);
        (undated.period_start, undated.period_end) = (None, None);
        let id = period_ref(&undated);
        assert!(news_core::refs::is_ref(&id) && id.len() == 13, "{id}");
        for id in ["pd_2026_10", "pd_20231001_20240928"] {
            assert!(news_core::refs::is_ref(id));
        }
    }

    #[test]
    fn cut_stops_at_a_sentence_or_a_word() {
        assert_eq!(cut("Short.", 300), "Short.");
        assert_eq!(cut("One two. Three four five.", 15), "One two.");
        assert_eq!(cut("One two three four", 12), "One two");
        assert_eq!(cut("ééééé", 3), "é");
    }

    #[test]
    fn largest_bisects_to_the_last_fit() {
        for n in 0..20 {
            for limit in 0..=n {
                assert_eq!(largest(n, |k| k <= limit), limit, "{n} {limit}");
            }
        }
        assert_eq!(largest(5, |_| false), 0);
    }

    #[test]
    fn tokens_round_up() {
        assert_eq!(tokens(""), 0);
        assert_eq!(tokens("abcd"), 2);
    }
}
