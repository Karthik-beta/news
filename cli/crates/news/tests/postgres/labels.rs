//! Karthik's labels against a real database (#19, #71): items drafted from the golden sets and from
//! Taster's embeddings, labelled by a test Board client over the Hotline (a terminal is refused, exit
//! 7), and exported as `pipeline/eval/g3/` and the rates in `judges.md`. The labels here are test data in
//! a scratch copy of `pipeline/eval/`; nothing is written to the repository.

use super::critic::{repo, scratch_root};
use super::fixture::{Fixture, call};
use super::huddle::{bureau, hotline};
use super::taster::{cand, n, source, t0};
use news_chief::Chief;
use news_core::ErrorCode;
use news_hotline::Peer;
use news_taster::embed::{DIM, MODEL};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;

fn terminal() -> Peer {
    Peer {
        argv1_is_board: false,
        ..Peer::board()
    }
}

fn chief(f: &Fixture) -> Chief {
    Chief::builder(f.pool.clone(), Arc::new(news_core::clock::SystemClock)).build()
}

fn in_root(f: &Fixture, root: &Path, args: &[&str]) -> (Value, i32) {
    let mut cmd = f.news();
    cmd.current_dir(root);
    let (env, code, _, err) = call(&mut cmd, args);
    assert!(code == 0 || env["ok"] == false, "{args:?}: {env} {err}");
    (env, code)
}

fn cases_in(dir: &Path) -> usize {
    std::fs::read_dir(dir).map_or(0, |d| {
        d.flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .count()
    })
}

#[tokio::test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn judge_labels_come_from_board_and_the_rates_are_computed_into_judges_md() {
    let f = Fixture::migrated().await;
    let root = scratch_root("labels-judges");
    let g6 = cases_in(&root.join("pipeline/eval/g6/cases"));

    // Drafting stores every G6 page as an item, and no pair (there are no embeddings).
    let (env, code) = in_root(&f, &root, &["eval", "draft"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["judge_items_added"], g6, "{env}");
    assert_eq!(env["data"]["pairs"], 0);
    let (env, _) = in_root(&f, &root, &["eval", "draft"]);
    assert_eq!(
        env["data"]["judge_items_added"], 0,
        "drafting again adds nothing: {env}"
    );

    let b = bureau(&f, chief(&f));
    // A terminal may read the next items. They hold the text and the recorded figures and nothing else:
    // no seeded fault, no case id.
    let items = hotline(
        &b,
        terminal(),
        "eval.items",
        json!({"kind": "judge", "judge": "wrong_direction"}),
    )
    .await;
    assert_eq!(items["ok"], true, "{items}");
    let list = items["data"]["items"].as_array().unwrap().clone();
    assert_eq!(list.len(), 10);
    for i in &list {
        let keys: Vec<&str> = i.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["facts", "item", "text"], "{i}");
        assert!(!i["item"].as_str().unwrap().contains("g6-"));
    }
    assert!(
        list.iter()
            .all(|i| i["item"].as_str().unwrap().starts_with('j'))
    );
    // Tone judges A1 copy only, and G1 has stored no outputs here: nothing to label.
    let tone = hotline(
        &b,
        terminal(),
        "eval.items",
        json!({"kind": "judge", "judge": "tone"}),
    )
    .await;
    assert_eq!(tone["data"]["items"], json!([]), "{tone}");

    // Three labels from Board; the same call from a terminal is gated (exit 7) and stores nothing.
    let ids: Vec<&str> = list.iter().map(|i| i["item"].as_str().unwrap()).collect();
    let label = |item: &str, pass: bool| json!({"kind": "judge", "judge": "wrong_direction", "item": item, "pass": pass});
    let env = hotline(&b, terminal(), "eval.label", label(ids[0], true)).await;
    assert_eq!(env["error"]["details"]["refusal"], "not_board", "{env}");
    assert_eq!(
        news_hotline::error_code(&env).map(ErrorCode::exit_code),
        Some(7)
    );
    assert_eq!(f.scalar_i64("select count(*) from judge_labels").await, 0);
    for (item, pass) in [(ids[0], false), (ids[1], true), (ids[2], true)] {
        let env = hotline(&b, Peer::board(), "eval.label", label(item, pass)).await;
        assert_eq!(env["ok"], true, "{env}");
    }
    let env = hotline(&b, Peer::board(), "eval.label", label(ids[2], true)).await;
    assert_eq!(
        env["data"]["labelled"], 3,
        "labelling again replaces, it does not add: {env}"
    );
    // An item the judge does not judge, and one that does not exist.
    let env = hotline(
        &b,
        Peer::board(),
        "eval.label",
        json!({"kind": "judge", "judge": "tone", "item": ids[0], "pass": true}),
    )
    .await;
    assert_eq!(env["error"]["code"], "precondition", "{env}");
    let env = hotline(&b, Peer::board(), "eval.label", label("jnope", true)).await;
    assert_eq!(env["error"]["code"], "not_found", "{env}");
    // The next items leave out what is labelled, and progress counts it.
    let next = hotline(
        &b,
        Peer::board(),
        "eval.items",
        json!({"kind": "judge", "judge": "wrong_direction", "limit": 50}),
    )
    .await;
    assert_eq!(next["data"]["items"].as_array().unwrap().len(), g6 - 3);
    let j = &next["data"]["judges"][0];
    assert_eq!(
        (j["labelled"].as_i64(), j["waiting"].as_i64()),
        (Some(3), Some(g6 as i64 - 3))
    );

    // The judge's verdicts on the same items (what Audit's judges step will store): right on the first
    // two, wrong on the third. A pass is the positive class: Karthik passed two, the judge agreed on
    // one; he failed one, the judge agreed.
    for (item, pass) in [(ids[0], false), (ids[1], true), (ids[2], false)] {
        sqlx::query("insert into judge_verdicts (item_id, judge, model, pass) values ($1, 'wrong_direction', 'test-judge', $2)")
            .bind(item)
            .bind(pass)
            .execute(&f.pool)
            .await
            .unwrap();
    }
    let (env, code) = in_root(&f, &root, &["eval", "export"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["judge_labels"], 3);
    assert_eq!(env["data"]["written"], json!(["pipeline/eval/judges.md"]));
    let md = std::fs::read_to_string(root.join("pipeline/eval/judges.md")).unwrap();
    assert!(
        md.contains("| `wrong_direction` | A1 copy (G1), A7 (G6) | 3 | test-judge | 0.50 (2) | 1.00 (1) | Awaiting labels (3 of 100) |"),
        "{md}"
    );
    assert!(md.contains("| `tone` (banned words, verdicts, forecasts in our voice) | A1 copy (G1) | 0 | — | — | — |"), "{md}");
    assert!(
        md.contains("never edit by hand"),
        "the prose around the table is kept"
    );
    let (env, _) = in_root(&f, &root, &["eval", "export"]);
    assert_eq!(
        env["data"]["written"],
        json!([]),
        "exporting again changes nothing: {env}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// A 384-d vector literal: `c` on axis `i` and the rest of the unit length on axis `j`, so its cosine to
/// the unit vector on axis `i` is exactly `c`.
fn mix(i: usize, j: usize, c: f64) -> String {
    let mut v = vec![0.0f64; DIM];
    v[i] = c;
    v[j] = (1.0 - c * c).sqrt();
    format!(
        "[{}]",
        v.iter().map(f64::to_string).collect::<Vec<_>>().join(",")
    )
}

async fn embed(f: &Fixture, kind: &str, id: i64, v: &str) {
    sqlx::query(
        "insert into embeddings (owner_kind, owner_id, model, v) values ($1, $2::bigint::text, $3, $4::text::halfvec)",
    )
    .bind(kind)
    .bind(id)
    .bind(MODEL)
    .bind(v)
    .execute(&f.pool)
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn story_pairs_near_the_join_threshold_are_labelled_in_board_and_exported_as_g3() {
    let f = Fixture::migrated().await;
    let p = &f.pool;
    source(p, "wire-a", 2).await;
    // One candidate and one story per cosine; candidate i sits on axis i, so only its own story is near.
    // Taster's join threshold is 0.82: three pairs below it, three above, and one far outside the band.
    let cosines = [0.78, 0.80, 0.76, 0.84, 0.86, 0.88, 0.50];
    let mut made = Vec::new();
    for (i, c) in cosines.iter().enumerate() {
        let title = format!("Candidate {i}");
        let cid = cand(p, "wire-a", &title, None, None, t0()).await;
        let sid: i64 = sqlx::query_scalar(
            "insert into stories (title, first_seen, last_seen, entity_ids) values ($1, $2, $2, '{1}') returning story_id",
        )
        .bind(format!("Story {i}"))
        .bind(t0())
        .fetch_one(p)
        .await
        .unwrap();
        sqlx::query("update candidates set entity_ids = '{1}' where cand_id = $1")
            .bind(cid)
            .execute(p)
            .await
            .unwrap();
        let mut unit = vec![0.0f64; DIM];
        unit[i] = 1.0;
        let unit = format!(
            "[{}]",
            unit.iter()
                .map(f64::to_string)
                .collect::<Vec<_>>()
                .join(",")
        );
        embed(&f, "candidate", cid, &unit).await;
        embed(&f, "story", sid, &mix(i, 100 + i, *c)).await;
        made.push((cid, sid, *c));
    }
    let root = scratch_root("labels-pairs");

    // Four pairs: two nearest below the threshold and two nearest above, never the far one.
    let (env, code) = in_root(&f, &root, &["eval", "draft", "--pairs", "4"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(
        (
            env["data"]["pairs_added"].as_i64(),
            env["data"]["pairs"].as_i64()
        ),
        (Some(4), Some(4))
    );
    let drafted: Vec<String> = sqlx::query_scalar("select pair_id from story_pairs order by cos")
        .fetch_all(p)
        .await
        .unwrap();
    let want: Vec<String> = [0.78, 0.80, 0.84, 0.86]
        .iter()
        .map(|c| {
            let (cid, sid, _) = made.iter().find(|m| (m.2 - c).abs() < 1e-9).unwrap();
            format!("g3-c{cid}-s{sid}")
        })
        .collect();
    assert_eq!(drafted, want);

    let b = bureau(&f, chief(&f));
    let items = hotline(&b, terminal(), "eval.items", json!({"kind": "pair"})).await;
    assert_eq!(items["ok"], true, "{items}");
    let list = items["data"]["items"].as_array().unwrap().clone();
    assert_eq!(list.len(), 4);
    // Board sees both sides and never how close they are.
    for i in &list {
        let keys: Vec<&str> = i.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["a", "b", "item"], "{i}");
    }
    assert!(list.iter().all(
        |i| i["a"]["title"].as_str().unwrap().starts_with("Candidate")
            && i["b"]["title"].as_str().unwrap().starts_with("Story")
    ));

    // Karthik's labels, from Board (a terminal is gated, exit 7): the pairs above the threshold are the
    // same story, the ones below are different. Test data, in a scratch copy.
    let first = list[0]["item"].as_str().unwrap();
    let env = hotline(
        &b,
        terminal(),
        "eval.label",
        json!({"kind": "pair", "item": first, "same": true}),
    )
    .await;
    assert_eq!(
        news_hotline::error_code(&env).map(ErrorCode::exit_code),
        Some(7),
        "{env}"
    );
    assert_eq!(n(p, "select count(*) from pair_labels").await, 0);
    for i in &list {
        let id = i["item"].as_str().unwrap();
        let cos: f64 = sqlx::query_scalar("select cos from story_pairs where pair_id = $1")
            .bind(id)
            .fetch_one(p)
            .await
            .unwrap();
        let env = hotline(
            &b,
            Peer::board(),
            "eval.label",
            json!({"kind": "pair", "item": id, "same": cos > 0.82}),
        )
        .await;
        assert_eq!(env["ok"], true, "{env}");
    }
    let env = hotline(
        &b,
        Peer::board(),
        "eval.label",
        json!({"kind": "pair", "item": "g3-c999-s999", "same": true}),
    )
    .await;
    assert_eq!(env["error"]["code"], "not_found", "{env}");
    let none = hotline(&b, Peer::board(), "eval.items", json!({"kind": "pair"})).await;
    assert_eq!(none["data"]["items"], json!([]));
    assert_eq!(none["data"]["pairs"]["labelled"], 4);

    // Export: four cases, the first cases of an empty set keep version 1, and the set scores itself.
    let (env, code) = in_root(&f, &root, &["eval", "export"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(
        (
            env["data"]["g3_cases"].as_i64(),
            env["data"]["g3_version"].as_i64()
        ),
        (Some(4), Some(1))
    );
    let cases = root.join("pipeline/eval/g3/cases");
    assert_eq!(cases_in(&cases), 4);
    let text = std::fs::read_to_string(cases.join(format!("{}.json", want[3]))).unwrap();
    let case: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        (case["label"]["same"].clone(), case["label"]["by"].clone()),
        (json!(true), json!("karthik"))
    );
    assert_eq!(case["sides"]["a"]["kind"], "candidate");
    let (env, code) = in_root(&f, &root, &["eval", "run", "storymatch"]);
    assert_eq!(
        (code, env["data"]["failed"].as_i64()),
        (0, Some(0)),
        "{env}"
    );
    let id = env["data"]["eval_run"].as_str().unwrap().to_owned();
    let (env, _) = in_root(&f, &root, &["eval", "report", &id]);
    let m = &env["data"]["metrics"];
    // The lowest cosine that joins only same pairs is the 0.84 one (stored as a half-precision vector).
    assert!(
        (m["g3.join_threshold"].as_f64().unwrap() - 0.84).abs() < 0.005,
        "{m}"
    );
    assert_eq!(
        (
            m["g3.join_precision"].as_f64(),
            m["g3.join_recall"].as_f64()
        ),
        (Some(1.0), Some(1.0)),
        "{m}"
    );
    assert_eq!(
        (
            m["g3.pairs_same"].as_f64(),
            m["g3.pairs_different"].as_f64()
        ),
        (Some(2.0), Some(2.0))
    );
    let (env, _) = in_root(&f, &root, &["eval", "export"]);
    assert_eq!(env["data"]["written"], json!([]), "{env}");

    // Changing a label changes a case, so the set's version rises.
    sqlx::query("update pair_labels set same = not same where pair_id = $1")
        .bind(&want[0])
        .execute(p)
        .await
        .unwrap();
    let (env, _) = in_root(&f, &root, &["eval", "export"]);
    assert_eq!(env["data"]["g3_version"], 2, "{env}");
    assert_eq!(env["data"]["written"].as_array().unwrap().len(), 2, "{env}");
    assert!(
        std::fs::read_to_string(root.join("pipeline/eval/g3/set.json"))
            .unwrap()
            .contains("\"version\": 2")
    );
    // The repository's own G3 set is untouched.
    assert_eq!(cases_in(&repo().join("pipeline/eval/g3/cases")), 0);
    let _ = std::fs::remove_dir_all(&root);
}
