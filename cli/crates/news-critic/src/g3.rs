//! G3, story pairs (Bureau §3.6, §9.1): Karthik's same/different labels on pairs Taster drafted, and the
//! join threshold they imply. Cases are written by [`export`] from the database labels, never by hand;
//! every cosine comes from the stored embeddings.
//!
//! A pair is joined at threshold `t` when its cosine is at least `t` and the two share a resolved
//! entity (Taster's rule); a pair without a shared entity is never joined, so a same pair of that kind
//! always counts against recall.

use news_core::{NewsError, NewsResult};
use news_store::{PgPool, map_err};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

/// The labelled pair count G3 aims for (`pipeline/eval/g3/set.json`).
pub const TARGET: usize = 200;

/// Join precision G3 asks of the threshold when its `set.json` names none.
pub const DEFAULT_PRECISION: f64 = 0.97;

/// One labelled pair, as the metric sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Pair {
    pub cos: f64,
    pub shared_entity: bool,
    pub same: bool,
}

/// The labelled pairs of a loaded set's cases.
pub fn pairs_of(cases: &[Value]) -> Vec<Pair> {
    cases
        .iter()
        .filter_map(|c| {
            Some(Pair {
                cos: c["cos"].as_f64()?,
                shared_entity: c["shared_entity"].as_bool()?,
                same: c["label"]["same"].as_bool()?,
            })
        })
        .collect()
}

fn joined(p: &Pair, t: f64) -> bool {
    p.cos >= t && p.shared_entity
}

/// Precision and recall of "join" at threshold `t`; `None` when nothing is joined (precision) or no pair
/// is the same (recall).
pub fn precision_recall(pairs: &[Pair], t: f64) -> (Option<f64>, Option<f64>) {
    let joins = pairs.iter().filter(|p| joined(p, t)).count();
    let hits = pairs.iter().filter(|p| joined(p, t) && p.same).count();
    let same = pairs.iter().filter(|p| p.same).count();
    let ratio = |n: usize, d: usize| (d > 0).then(|| n as f64 / d as f64);
    (ratio(hits, joins), ratio(hits, same))
}

/// The lowest threshold whose join precision is at least `precision`: recall only falls as the
/// threshold rises, so the lowest one that is precise enough has the most recall. It is one of the
/// cosines of a same pair that shares an entity. `None` when no threshold is precise enough, or no
/// pair is labelled the same.
pub fn join_threshold(pairs: &[Pair], precision: f64) -> Option<f64> {
    let mut candidates: Vec<f64> = pairs
        .iter()
        .filter(|p| p.same && p.shared_entity)
        .map(|p| p.cos)
        .collect();
    candidates.sort_by(f64::total_cmp);
    candidates
        .into_iter()
        .find(|&t| precision_recall(pairs, t).0.is_some_and(|p| p >= precision))
}

/// A case file's text in the layout Biome keeps: objects open on their own lines, and an array of plain
/// values stays on one line when it fits.
pub fn format_json(v: &Value) -> String {
    let mut s = String::new();
    write_value(&mut s, v, 0, 0);
    s.push('\n');
    s
}

fn write_value(out: &mut String, v: &Value, indent: usize, prefix: usize) {
    match v {
        Value::Object(m) if !m.is_empty() => {
            out.push_str("{\n");
            let last = m.len() - 1;
            for (i, (k, x)) in m.iter().enumerate() {
                let key = format!("{}: ", Value::String(k.clone()));
                out.push_str(&" ".repeat(indent + 2));
                out.push_str(&key);
                write_value(out, x, indent + 2, indent + 2 + key.len());
                out.push_str(if i < last { ",\n" } else { "\n" });
            }
            out.push_str(&" ".repeat(indent));
            out.push('}');
        }
        Value::Array(a) if !a.is_empty() && a.iter().all(|x| !x.is_object() && !x.is_array()) => {
            let flat = format!(
                "[{}]",
                a.iter()
                    .map(Value::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            if prefix + flat.chars().count() < 120 {
                out.push_str(&flat);
            } else {
                out.push_str("[\n");
                let last = a.len() - 1;
                for (i, x) in a.iter().enumerate() {
                    out.push_str(&" ".repeat(indent + 2));
                    out.push_str(&x.to_string());
                    out.push_str(if i < last { ",\n" } else { "\n" });
                }
                out.push_str(&" ".repeat(indent));
                out.push(']');
            }
        }
        other => out.push_str(&other.to_string()),
    }
}

type Row = (
    String,
    f64,
    bool,
    String,
    chrono::DateTime<chrono::Utc>,
    bool,
    Option<String>,
);

/// `(candidate, story)` of a pair id, only when the id is exactly `g3-c<n>-s<n>` as Taster writes it.
/// The id names a file, so anything else is refused before a path is made from it.
pub fn parse_pair_id(id: &str) -> Option<(i64, i64)> {
    let (c, s) = id.strip_prefix("g3-c")?.split_once("-s")?;
    if ![c, s]
        .iter()
        .all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let (c, s) = (c.parse::<i64>().ok()?, s.parse::<i64>().ok()?);
    (format!("g3-c{c}-s{s}") == id).then_some((c, s))
}

/// What [`export`] did.
#[derive(Debug, Clone, PartialEq)]
pub struct Exported {
    pub cases: usize,
    pub version: u32,
    /// Paths relative to the repository root that were written.
    pub written: Vec<String>,
}

/// Write the labelled pairs as `pipeline/eval/g3/cases/<id>.json` and raise `set.json`'s `version` when
/// the cases changed from ones already there (the first cases of an empty set keep it). A case is only
/// rewritten when its text differs; nothing is removed. The set is loaded and checked afterwards, so
/// a file that breaks its schema fails here.
pub async fn export(db: &PgPool, root: &Path) -> NewsResult<Exported> {
    let rows: Vec<Row> = sqlx::query_as(
        "select p.pair_id, p.cos, p.shared_entity, p.model, p.drafted_at, l.same, p.sides->'a'->>'source'
               from story_pairs p join pair_labels l using (pair_id) order by p.pair_id",
    )
    .fetch_all(db)
    .await
    .map_err(map_err)?;
    let dir = root.join("pipeline/eval/g3");
    let cases = dir.join("cases");
    std::fs::create_dir_all(&cases).map_err(io(&cases))?;
    let mut had_cases = false;
    for e in std::fs::read_dir(&cases).map_err(io(&cases))?.flatten() {
        had_cases |= e.path().extension().is_some_and(|x| x == "json");
    }
    // Every id is checked before any file is written. A case holds ids, the source id, the cosine and the
    // label, never the outlet's title or text: the repository is public, and that text stays in PostgreSQL.
    let mut files = Vec::new();
    for (id, cos, shared, model, drafted, same, source) in &rows {
        let (cand, story) = parse_pair_id(id).ok_or_else(|| {
            NewsError::validation(format!("{id:?} is not a G3 pair id (g3-c<n>-s<n>)"))
        })?;
        let text = format_json(&json!({
            "id": id, "candidate": {"id": cand, "source_id": source}, "story": {"id": story},
            "cos": cos, "shared_entity": shared, "label": {"same": same, "by": "karthik"},
            "provenance": {"source": "Taster embeddings", "model": model, "drafted_at": drafted},
        }));
        files.push((id, text));
    }
    let mut written = Vec::new();
    for (id, text) in files {
        let path = cases.join(format!("{id}.json"));
        if std::fs::read_to_string(&path).ok().as_deref() != Some(&text) {
            std::fs::write(&path, text).map_err(io(&path))?;
            written.push(format!("pipeline/eval/g3/cases/{id}.json"));
        }
    }
    let manifest_path = dir.join("set.json");
    let manifest = std::fs::read_to_string(&manifest_path).map_err(io(&manifest_path))?;
    let mut version = serde_json::from_str::<Value>(&manifest)
        .ok()
        .and_then(|m| m["version"].as_u64())
        .ok_or_else(|| NewsError::validation("pipeline/eval/g3/set.json has no version"))?
        as u32;
    if !written.is_empty() && had_cases {
        let old = format!("\"version\": {version}");
        version += 1;
        let bumped = manifest.replacen(&old, &format!("\"version\": {version}"), 1);
        std::fs::write(&manifest_path, bumped).map_err(io(&manifest_path))?;
        written.push("pipeline/eval/g3/set.json".to_owned());
    }
    let set = crate::golden::load(root, "g3")?;
    if let Some((id, why)) = set.problems.first() {
        return Err(NewsError::validation(format!(
            "exported G3 case {id} is invalid: {why}"
        )));
    }
    Ok(Exported {
        cases: rows.len(),
        version,
        written,
    })
}

fn io(p: &Path) -> impl Fn(std::io::Error) -> NewsError + '_ {
    move |e| NewsError::internal(format!("{}: {e}", p.display()))
}

/// The G3 metrics of a set's cases: the pair counts, the threshold the labels imply for `precision`, and
/// join precision and recall at the live `join` threshold. A metric that has no data is left out.
pub fn metrics(pairs: &[Pair], precision: f64, live_join: Option<f64>) -> BTreeMap<String, f64> {
    let mut m = BTreeMap::new();
    m.insert(
        "pairs_same".to_owned(),
        pairs.iter().filter(|p| p.same).count() as f64,
    );
    m.insert(
        "pairs_different".to_owned(),
        pairs.iter().filter(|p| !p.same).count() as f64,
    );
    if let Some(t) = join_threshold(pairs, precision) {
        m.insert("join_threshold".to_owned(), t);
    }
    if let Some(t) = live_join {
        let (p, r) = precision_recall(pairs, t);
        if let Some(p) = p {
            m.insert("join_precision".to_owned(), p);
        }
        if let Some(r) = r {
            m.insert("join_recall".to_owned(), r);
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(cos: f64, shared: bool, same: bool) -> Pair {
        Pair {
            cos,
            shared_entity: shared,
            same,
        }
    }

    /// Labelled pairs made up for the arithmetic only; they are not G3 cases.
    fn sample() -> Vec<Pair> {
        vec![
            p(0.95, true, true),
            p(0.90, true, true),
            p(0.86, true, true),
            p(0.84, true, false), // a wrong join at 0.84 and below
            p(0.83, true, true),
            p(0.78, true, true),
            p(0.76, true, false),
            p(0.92, false, true), // never joined: no shared entity
            p(0.70, true, false),
        ]
    }

    #[test]
    fn the_threshold_is_the_lowest_one_that_is_precise_enough() {
        let pairs = sample();
        // At 0.86: joins 0.95, 0.90, 0.86 (all same): precision 1, recall 3 of 6 same pairs.
        assert_eq!(join_threshold(&pairs, 0.97), Some(0.86));
        assert_eq!(precision_recall(&pairs, 0.86), (Some(1.0), Some(0.5)));
        // At 0.83 the wrong join at 0.84 is in: 4 of 5.
        assert_eq!(precision_recall(&pairs, 0.83), (Some(0.8), Some(4.0 / 6.0)));
        // A looser bar takes the lower threshold: 0.78 joins 0.95..0.78, one wrong of six.
        assert_eq!(join_threshold(&pairs, 0.8), Some(0.78));
        // Nothing is precise enough when every join is wrong.
        assert_eq!(
            join_threshold(&[p(0.9, true, false), p(0.8, true, true)], 1.0),
            None
        );
    }

    #[test]
    fn no_labels_and_no_same_pairs_give_no_threshold() {
        assert_eq!(join_threshold(&[], 0.97), None);
        assert_eq!(join_threshold(&[p(0.9, true, false)], 0.97), None);
        assert_eq!(precision_recall(&[], 0.8), (None, None));
        assert!(!metrics(&[], 0.97, Some(0.82)).contains_key("join_threshold"));
    }

    #[test]
    fn metrics_report_the_live_threshold_and_the_implied_one() {
        let m = metrics(&sample(), 0.97, Some(0.82));
        assert_eq!(m["pairs_same"], 6.0);
        assert_eq!(m["pairs_different"], 3.0);
        assert_eq!(m["join_threshold"], 0.86);
        // Live 0.82 joins 0.95, 0.90, 0.86, 0.84, 0.83: four of five right, four of six recalled.
        assert_eq!(m["join_precision"], 0.8);
        assert_eq!(m["join_recall"], 4.0 / 6.0);
    }

    #[test]
    fn pairs_are_read_from_cases_and_a_case_without_a_label_is_left_out() {
        let cases = vec![
            json!({"cos": 0.9, "shared_entity": true, "label": {"same": true, "by": "karthik"}}),
            json!({"cos": 0.8, "shared_entity": false}),
        ];
        assert_eq!(pairs_of(&cases), vec![p(0.9, true, true)]);
    }

    #[test]
    fn only_an_id_taster_writes_becomes_a_file_name() {
        assert_eq!(parse_pair_id("g3-c7-s12"), Some((7, 12)));
        for bad in [
            "",
            "g3-c7",
            "g3-c7-s",
            "g3-c-s1",
            "g3-c007-s12",
            "g3-c7-s12/",
            "g3-c7-s12.json",
            "../g3-c7-s12",
            "g3-c7-s1/../../evil",
            "g3-c7-s-1",
            "g3-c+7-s1",
            "g3-c99999999999999999999-s1",
            "g3-c1-s2\\x",
        ] {
            assert_eq!(parse_pair_id(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn json_keeps_biomes_layout() {
        let v =
            json!({"id": "g3-c1-s2", "list": ["a", "b"], "empty": [], "n": {"x": 1.5, "ok": true}});
        assert_eq!(
            format_json(&v),
            "{\n  \"empty\": [],\n  \"id\": \"g3-c1-s2\",\n  \"list\": [\"a\", \"b\"],\n  \"n\": {\n    \"ok\": true,\n    \"x\": 1.5\n  }\n}\n"
        );
        let long: Vec<String> = (0..12)
            .map(|i| format!("a title of some length {i}"))
            .collect();
        let out = format_json(&json!({ "m": long }));
        assert!(
            out.starts_with("{\n  \"m\": [\n    \"a title of some length 0\",\n"),
            "{out}"
        );
    }
}
