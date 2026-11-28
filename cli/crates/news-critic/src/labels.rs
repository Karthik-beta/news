//! Karthik's labels (Bureau §9.1): what Board's labelling screens show and store, and the rates code
//! computes from them.
//!
//! * A12 judges, one failure mode at a time: Karthik reads a stored output (a G6 page, or a G1 output a
//!   version stored) and says pass or fail. About 100 labels per judge; the judge's own verdicts on the
//!   same outputs are compared with them, and its true-positive and true-negative rates are written
//!   into `pipeline/eval/judges.md` by [`judges_table`], never typed.
//! * G3 story pairs: same or different, drafted by `news_taster::pairs`, exported by [`crate::g3`].
//!
//! Items hold no ground truth (a G6 page's seeded fault, a pair's cosine), and their ids are opaque, so
//! a label is blind. Nothing here fills in a label.

use crate::golden::{self, render};
use news_core::{NewsError, NewsResult};
use news_store::{PgPool, map_err};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

/// Labels a judge needs before its rates are worth reading (Bureau §9.1: about 100).
pub const TARGET: usize = 100;

/// One A12 judge: a failure mode and the question Karthik answers about a text.
pub struct Judge {
    pub id: &'static str,
    pub name: &'static str,
    /// The gloss `judges.md` gives after the id, if any.
    pub gloss: Option<&'static str>,
    /// The golden sets whose outputs it judges.
    pub sets: &'static [&'static str],
    pub question: &'static str,
}

pub const JUDGES: [Judge; 5] = [
    Judge {
        id: "wrong_direction",
        name: "Wrong direction",
        gloss: None,
        sets: &["g1", "g6"],
        question: "Does any sentence say a figure rose where the figures show it fell, or fell where they show it rose?",
    },
    Judge {
        id: "unsupported_claim",
        name: "Unsupported claim",
        gloss: None,
        sets: &["g1", "g6"],
        question: "Does the text state anything the recorded figures do not support?",
    },
    Judge {
        id: "attribution",
        name: "Attribution",
        gloss: Some("misattributed quote"),
        sets: &["g1", "g6"],
        question: "Is any statement or quote credited to someone other than who the source says said it?",
    },
    Judge {
        id: "missing_context",
        name: "Missing context",
        gloss: None,
        sets: &["g1", "g6"],
        question: "Does the text leave out context the figures need to be read fairly, such as a period, a base, a restatement or a unit?",
    },
    Judge {
        id: "tone",
        name: "Tone",
        gloss: Some("banned words, verdicts, forecasts in our voice"),
        sets: &["g1"],
        question: "Does the text use a banned word, give a verdict, or make a forecast in our own voice?",
    },
];

pub fn judge(id: &str) -> NewsResult<&'static Judge> {
    JUDGES
        .iter()
        .find(|j| j.id == id)
        .ok_or_else(|| NewsError::usage(format!("{id:?} is not an A12 judge")))
}

fn sets(j: &Judge) -> Vec<String> {
    j.sets.iter().map(|s| (*s).to_owned()).collect()
}

/// Where a set's golden outputs came from, as the item says: `g1-…@v3` or the G6 case.
const ITEM_ID: &str = "'j' || left(md5($1::text || '@' || $2::text), 10)";

/// The recorded figures a brief gives, as the source states them.
fn facts(brief: &Value) -> Vec<String> {
    brief["facts"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|f| {
                    let unit = f["unit"].as_str().filter(|u| !u.is_empty());
                    format!(
                        "{}: {}{} ({})",
                        f["label"].as_str().unwrap_or_default(),
                        f["value"].as_str().unwrap_or_default(),
                        unit.map(|u| format!(" {u}")).unwrap_or_default(),
                        f["period"].as_str().unwrap_or_default()
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// What `news eval draft` found for the judges: items now stored, and how many were new.
pub struct Items {
    pub added: usize,
    pub total: i64,
}

/// Store the outputs the judges are checked on: every G6 page (filled from its brief) and every output
/// a version stored for G1. An item already stored is left as it is, with its labels.
pub async fn draft_items(db: &PgPool, root: &Path) -> NewsResult<Items> {
    let mut found: Vec<(String, String, Value)> = Vec::new();
    let g6 = golden::load(root, "g6")?;
    for c in &g6.cases {
        let brief = &g6.briefs[c["brief_case"].as_str().unwrap_or_default()];
        let page = render(c["page"].as_str().unwrap_or_default(), brief)
            .map_err(|e| NewsError::validation(format!("{}: {e}", c["id"])))?;
        found.push((
            c["id"].as_str().unwrap_or_default().to_owned(),
            String::new(),
            json!({"text": page, "facts": facts(brief)}),
        ));
    }
    let g1 = golden::load(root, "g1")?;
    let outputs = root.join("pipeline/eval/g1/outputs");
    let mut versions: Vec<String> = std::fs::read_dir(&outputs)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.strip_suffix(".jsonl").map(str::to_owned)
        })
        .collect();
    versions.sort();
    for v in versions {
        for o in golden::outputs(root, "g1", &v)? {
            let (Some(out), Some(case)) = (
                o.output,
                g1.cases.iter().find(|c| c["id"] == o.case.as_str()),
            ) else {
                continue;
            };
            found.push((
                o.case.clone(),
                v.clone(),
                json!({"text": serde_json::to_string_pretty(&out).unwrap_or_default(),
                       "facts": facts(&case["brief"])}),
            ));
        }
    }
    let mut added = 0;
    for (case, version, subject) in found {
        let set = &case[..2];
        let done = sqlx::query(sqlx::AssertSqlSafe(format!(
            "insert into judge_items (item_id, set, case_id, prompt_version, subject)
             values ({ITEM_ID}, $3, $1, $4, $5) on conflict (item_id) do nothing"
        )))
        .bind(&case)
        .bind(&version)
        .bind(set)
        .bind(&version)
        .bind(subject)
        .execute(db)
        .await
        .map_err(map_err)?;
        added += done.rows_affected() as usize;
    }
    let total = sqlx::query_scalar("select count(*) from judge_items")
        .fetch_one(db)
        .await
        .map_err(map_err)?;
    Ok(Items { added, total })
}

/// `eval.items`: the next `limit` unlabelled items of a judge, or of G3, and every label set's progress.
pub async fn items(
    db: &PgPool,
    kind: &str,
    judge_id: Option<&str>,
    limit: i64,
) -> NewsResult<Value> {
    let chosen = match (kind, judge_id) {
        ("judge", Some(id)) => Some(judge(id)?),
        ("judge", None) => return Err(NewsError::usage("eval.items for a judge needs judge")),
        ("pair", _) => None,
        _ => return Err(NewsError::usage(format!("unknown kind {kind:?}"))),
    };
    let items: Vec<Value> = match chosen {
        Some(j) => sqlx::query_as::<_, (String, Value)>(
            "select i.item_id, i.subject from judge_items i
              where i.set = any($1)
                and not exists (select 1 from judge_labels l where l.item_id = i.item_id and l.judge = $2)
              order by i.item_id limit $3",
        )
        .bind(sets(j))
        .bind(j.id)
        .bind(limit)
        .fetch_all(db)
        .await
        .map_err(map_err)?
        .into_iter()
        .map(|(item, s)| json!({"item": item, "text": s["text"], "facts": s["facts"]}))
        .collect(),
        None => sqlx::query_as::<_, (String, Value)>(
            "select p.pair_id, p.sides from story_pairs p
              where not exists (select 1 from pair_labels l where l.pair_id = p.pair_id)
              order by md5(p.pair_id) limit $1",
        )
        .bind(limit)
        .fetch_all(db)
        .await
        .map_err(map_err)?
        .into_iter()
        .map(|(item, s)| json!({"item": item, "a": s["a"], "b": s["b"]}))
        .collect(),
    };
    let mut judges = Vec::new();
    let mut text = String::new();
    for j in &JUDGES {
        let (labelled, waiting) = progress(db, j).await?;
        text.push_str(&format!(
            "{}: {labelled} of {TARGET} labelled, {waiting} waiting.\n",
            j.name
        ));
        judges.push(json!({"id": j.id, "name": j.name, "question": j.question,
                           "labelled": labelled, "waiting": waiting, "target": TARGET}));
    }
    let (pl, pw) = pair_progress(db).await?;
    text.push_str(&format!(
        "Story pairs: {pl} of {} labelled, {pw} waiting.",
        crate::g3::TARGET
    ));
    Ok(
        json!({"kind": kind, "judge": chosen.map(|j| j.id), "items": items, "judges": judges,
              "pairs": {"labelled": pl, "waiting": pw, "target": crate::g3::TARGET}, "text": text}),
    )
}

async fn progress(db: &PgPool, j: &Judge) -> NewsResult<(i64, i64)> {
    sqlx::query_as(
        "select count(*) filter (where exists (select 1 from judge_labels l where l.item_id = i.item_id and l.judge = $2)),
                count(*) filter (where not exists (select 1 from judge_labels l where l.item_id = i.item_id and l.judge = $2))
           from judge_items i where i.set = any($1)",
    )
    .bind(sets(j))
    .bind(j.id)
    .fetch_one(db)
    .await
    .map_err(map_err)
}

async fn pair_progress(db: &PgPool) -> NewsResult<(i64, i64)> {
    sqlx::query_as(
        "select count(*) filter (where exists (select 1 from pair_labels l where l.pair_id = p.pair_id)),
                count(*) filter (where not exists (select 1 from pair_labels l where l.pair_id = p.pair_id))
           from story_pairs p",
    )
    .fetch_one(db)
    .await
    .map_err(map_err)
}

/// `eval.label` for a judge: store `pass` for the item under the failure mode. The item must be one the
/// judge judges.
pub async fn label_judge(db: &PgPool, judge_id: &str, item: &str, pass: bool) -> NewsResult<Value> {
    let j = judge(judge_id)?;
    let set: Option<String> = sqlx::query_scalar("select set from judge_items where item_id = $1")
        .bind(item)
        .fetch_optional(db)
        .await
        .map_err(map_err)?;
    let set = set.ok_or_else(|| NewsError::not_found(format!("no item {item:?} to label")))?;
    if !j.sets.contains(&set.as_str()) {
        return Err(NewsError::precondition(format!(
            "the {} judge does not judge {} outputs",
            j.name,
            set.to_uppercase()
        )));
    }
    sqlx::query(
        "insert into judge_labels (item_id, judge, pass) values ($1, $2, $3)
         on conflict (item_id, judge) do update set pass = excluded.pass, labelled_at = now()",
    )
    .bind(item)
    .bind(j.id)
    .bind(pass)
    .execute(db)
    .await
    .map_err(map_err)?;
    let (labelled, _) = progress(db, j).await?;
    Ok(
        json!({"kind": "judge", "item": item, "labelled": labelled, "target": TARGET,
              "text": format!("{}: {labelled} of {TARGET} labelled.", j.name)}),
    )
}

/// `eval.label` for a pair: store whether the two are the same story.
pub async fn label_pair(db: &PgPool, pair: &str, same: bool) -> NewsResult<Value> {
    let found: Option<String> =
        sqlx::query_scalar("select pair_id from story_pairs where pair_id = $1")
            .bind(pair)
            .fetch_optional(db)
            .await
            .map_err(map_err)?;
    if found.is_none() {
        return Err(NewsError::not_found(format!("no pair {pair:?} to label")));
    }
    sqlx::query(
        "insert into pair_labels (pair_id, same) values ($1, $2)
         on conflict (pair_id) do update set same = excluded.same, labelled_at = now()",
    )
    .bind(pair)
    .bind(same)
    .execute(db)
    .await
    .map_err(map_err)?;
    let (labelled, _) = pair_progress(db).await?;
    Ok(
        json!({"kind": "pair", "item": pair, "labelled": labelled, "target": crate::g3::TARGET,
              "text": format!("Story pairs: {labelled} of {} labelled.", crate::g3::TARGET)}),
    )
}

/// A judge's agreement with Karthik's labels on the items it also judged.
#[derive(Debug, Clone, PartialEq)]
pub struct Rates {
    /// Items Karthik labelled that the judge also judged.
    pub judged: usize,
    /// Of Karthik's passes, the share the judge also passed.
    pub tpr: Option<f64>,
    pub tnr: Option<f64>,
    /// Karthik's passes and fails among the judged items, the denominators of the two rates.
    pub passes: usize,
    pub fails: usize,
}

/// True-positive and true-negative rates from `(Karthik's pass, the judge's pass)` pairs. A pass is the
/// positive class, as the judge's own output is `{"pass": bool}`: the TPR is the share of Karthik's passes
/// the judge also passes, the TNR the share of his fails it also fails.
pub fn rates(pairs: &[(bool, bool)]) -> Rates {
    let passes = pairs.iter().filter(|(k, _)| *k).count();
    let fails = pairs.len() - passes;
    let right = |karthik: bool| {
        pairs
            .iter()
            .filter(|(k, j)| *k == karthik && j == k)
            .count()
    };
    let share = |n: usize, d: usize| (d > 0).then(|| n as f64 / d as f64);
    Rates {
        judged: pairs.len(),
        tpr: share(right(true), passes),
        tnr: share(right(false), fails),
        passes,
        fails,
    }
}

fn rate(r: Option<f64>, n: usize) -> String {
    r.map_or_else(|| "—".to_owned(), |r| format!("{r:.2} ({n})"))
}

/// The table in `judges.md`, from the database: one row per judge and model that has judged, and one
/// row for a judge nothing has judged yet. Every figure is computed here.
pub async fn judges_table(db: &PgPool) -> NewsResult<String> {
    let counts: BTreeMap<String, i64> = sqlx::query_as::<_, (String, i64)>(
        "select judge, count(*) from judge_labels group by judge",
    )
    .fetch_all(db)
    .await
    .map_err(map_err)?
    .into_iter()
    .collect();
    let rows: Vec<(String, String, bool, bool)> = sqlx::query_as(
        "select l.judge, v.model, l.pass, v.pass
           from judge_labels l join judge_verdicts v on v.item_id = l.item_id and v.judge = l.judge",
    )
    .fetch_all(db)
    .await
    .map_err(map_err)?;
    let mut t = String::from(
        "| Judge (failure mode) | Judges | Karthik's labels | Model | True-positive rate | True-negative rate | Status |\n\
         | --- | --- | --- | --- | --- | --- | --- |\n",
    );
    for j in &JUDGES {
        let labels = counts.get(j.id).copied().unwrap_or(0) as usize;
        let name = j
            .gloss
            .map_or_else(|| format!("`{}`", j.id), |g| format!("`{}` ({g})", j.id));
        let judges = j
            .sets
            .iter()
            .map(|s| match *s {
                "g1" => "A1 copy (G1)",
                _ => "A7 (G6)",
            })
            .collect::<Vec<_>>()
            .join(", ");
        let mut models: BTreeMap<&str, Vec<(bool, bool)>> = BTreeMap::new();
        for (judge_id, model, k, v) in &rows {
            if judge_id == j.id {
                models.entry(model).or_default().push((*k, *v));
            }
        }
        let status = if labels < TARGET {
            format!("Awaiting labels ({labels} of {TARGET})")
        } else if models.is_empty() {
            "Awaiting judge".to_owned()
        } else {
            "Measured".to_owned()
        };
        if models.is_empty() {
            t.push_str(&format!(
                "| {name} | {judges} | {labels} | — | — | — | {status} |\n"
            ));
        }
        for (model, pairs) in models {
            let r = rates(&pairs);
            t.push_str(&format!(
                "| {name} | {judges} | {labels} | {model} | {} | {} | {status} |\n",
                rate(r.tpr, r.passes),
                rate(r.tnr, r.fails)
            ));
        }
    }
    Ok(t)
}

const START: &str = "<!-- rates:start";
const END: &str = "<!-- rates:end -->";

/// `md` with the table between its `rates` markers replaced.
pub fn splice(md: &str, table: &str) -> NewsResult<String> {
    let bad = || {
        NewsError::validation("judges.md has no <!-- rates:start --> … <!-- rates:end --> block")
    };
    let start = md.find(START).ok_or_else(bad)?;
    let after = start + md[start..].find("-->").ok_or_else(bad)? + 3;
    let end = md.find(END).filter(|e| *e >= after).ok_or_else(bad)?;
    Ok(format!("{}\n\n{table}\n{}", &md[..after], &md[end..]))
}

/// Write the table into `pipeline/eval/judges.md`; `true` when the file changed.
pub async fn export_judges(db: &PgPool, root: &Path) -> NewsResult<bool> {
    let path = root.join("pipeline/eval/judges.md");
    let old = std::fs::read_to_string(&path)
        .map_err(|e| NewsError::not_found(format!("{}: {e}", path.display())))?;
    let new = splice(&old, &judges_table(db).await?)?;
    if new == old {
        return Ok(false);
    }
    std::fs::write(&path, new)
        .map_err(|e| NewsError::internal(format!("{}: {e}", path.display())))?;
    Ok(true)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn rates_are_shares_of_karthiks_passes_and_fails_the_judge_agreed_with() {
        // (Karthik, judge): 4 passes of which 3 agreed, 2 fails of which 1 agreed.
        let r = rates(&[
            (true, true),
            (true, true),
            (true, true),
            (true, false),
            (false, false),
            (false, true),
        ]);
        assert_eq!((r.judged, r.passes, r.fails), (6, 4, 2));
        assert_eq!((r.tpr, r.tnr), (Some(0.75), Some(0.5)));
        // No fails among the judged items: the TNR is not defined, not 1.
        let r = rates(&[(true, true)]);
        assert_eq!((r.tpr, r.tnr), (Some(1.0), None));
        assert_eq!(rates(&[]).tpr, None);
    }

    #[test]
    fn the_table_goes_between_its_markers_and_nothing_else_moves() {
        let md = "# A12\n\nprose\n\n<!-- rates:start (code writes this) -->\n\nold\n\n<!-- rates:end -->\n\nafter\n";
        let out = splice(md, "| t |\n").unwrap();
        assert_eq!(
            out,
            "# A12\n\nprose\n\n<!-- rates:start (code writes this) -->\n\n| t |\n\n<!-- rates:end -->\n\nafter\n"
        );
        assert_eq!(
            splice(&out, "| t |\n").unwrap(),
            out,
            "splicing again changes nothing"
        );
        assert!(splice("no markers", "x").is_err());
    }

    #[test]
    fn every_judge_has_a_question_and_the_sets_the_doc_names() {
        let ids: Vec<&str> = JUDGES.iter().map(|j| j.id).collect();
        assert_eq!(
            ids,
            [
                "wrong_direction",
                "unsupported_claim",
                "attribution",
                "missing_context",
                "tone"
            ]
        );
        assert!(JUDGES.iter().all(|j| j.question.ends_with('?')));
        assert_eq!(judge("tone").unwrap().sets, ["g1"]);
        assert!(judge("vibes").is_err());
    }

    #[test]
    fn facts_are_listed_as_the_brief_states_them() {
        let brief = json!({"facts": [
            {"label": "Revenue", "value": "94036000000", "unit": "USD", "period": "2026-10-17"},
            {"label": "Shares", "value": "7", "unit": "", "period": "Q4 FY26"}]});
        assert_eq!(
            facts(&brief),
            [
                "Revenue: 94036000000 USD (2026-10-17)",
                "Shares: 7 (Q4 FY26)"
            ]
        );
    }
}
