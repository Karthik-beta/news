//! `news ai input|accept|fallback` on page jobs (`ai.write`, `ai.repair`), against a real `jobs` table, with a
//! fake Plate standing in for the Bun render server (its /check says the .oui parses).

use super::fixture::{Fixture, call};
use news_dispatch::queue::{self, NewJob};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../news-proof/tests/fixtures")
        .join(name)
}

/// An HTTP server whose /check says the .oui parses. Its literals are the one string the validators must see in
/// copy-digits.json's .oui (an unknown ref); a real Plate lists every literal it parsed.
pub(super) fn fake_plate() -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let mut buf = [0u8; 65536];
            let n = s.read(&mut buf).unwrap_or(0);
            let unknown = String::from_utf8_lossy(&buf[..n]).contains("dp_unknown");
            let body = if unknown {
                r#"{"errors":[],"components":[],"literals":[{"where":"oui:b","text":"dp_unknown","number":false}],"renderer":"fake"}"#
            } else {
                r#"{"errors":[],"components":[],"literals":[],"renderer":"fake"}"#
            };
            let _ = write!(
                s,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    url
}

async fn job(f: &Fixture, kind: &str, story: &str) -> String {
    let j = queue::enqueue(
        &f.pool,
        &NewJob::new(kind).pool("ai").args(json!({ "story": story })),
    )
    .await
    .unwrap();
    j.job_id.to_string()
}

fn read(p: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn page_jobs_get_their_input_and_their_copy_is_validated_stored_or_templated() {
    let f = Fixture::migrated().await;
    let dir = std::env::temp_dir().join(format!("news-ai-{}", f.schema));
    std::fs::create_dir_all(dir.join("briefs")).unwrap();
    std::fs::copy(fixture("brief.json"), dir.join("briefs/81234.json")).unwrap();
    let plate = fake_plate();
    let ai = |args: &[&str]| {
        let mut cmd = f.news();
        cmd.env("NEWS_WORK_DIR", &dir).env("NEWS_PLATE_URL", &plate);
        call(&mut cmd, args)
    };

    // A1's input is the brief.
    let write = job(&f, "ai.write", "81234").await;
    let (env, code, _, _) = ai(&["ai", "input", &write]);
    assert_eq!(code, 0, "{env}");
    assert!(
        env["data"]["path"]
            .as_str()
            .unwrap()
            .ends_with("81234.json")
    );

    // A Copy with typed numbers and an unknown ref is refused and kept for A2.
    let digits = fixture("copy-digits.json");
    let (env, code, _, _) = ai(&["ai", "accept", &write, digits.to_str().unwrap()]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(
        (
            env["data"]["valid"].as_bool(),
            env["data"]["stored"].as_bool()
        ),
        (Some(false), Some(false))
    );
    let rules: Vec<&str> = env["data"]["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["rule"].as_str().unwrap())
        .collect();
    assert!(
        rules.contains(&"numeric_literal") && rules.contains(&"refs"),
        "{rules:?}"
    );
    assert!(!dir.join("pages/81234/copy.json").exists());

    // A2's input: the brief, the refused Copy and its errors.
    let repair = job(&f, "ai.repair", "81234").await;
    let (env, code, _, _) = ai(&["ai", "input", &repair]);
    assert_eq!(code, 0, "{env}");
    let input = read(Path::new(env["data"]["path"].as_str().unwrap()));
    assert_eq!(input["brief"]["item"]["story_id"], 81234);
    assert_eq!(
        input["previous"]["headline"],
        "Annual inflation in America fell to 2.4% in October"
    );
    assert!(!input["errors"].as_array().unwrap().is_empty());

    // A clean Copy is stored.
    let (env, code, _, _) = ai(&[
        "ai",
        "accept",
        &repair,
        fixture("copy.json").to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["valid"], true);
    assert_eq!(env["data"]["stored"], true);
    let stored = read(&dir.join("pages/81234/copy.json"));
    assert_eq!(stored["source"], "ai");
    assert_eq!(stored["job"], repair.parse::<i64>().unwrap());
    assert_eq!(stored["draft_id"], env["data"]["draft_id"]);
    assert!(!dir.join("pages/81234/rejected.json").exists());

    // D10: the Tier 2 fallback stores a templated page.
    let (env, code, _, _) = ai(&["ai", "fallback", &repair]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["applied"], true);
    assert_eq!(env["data"]["result"]["flags"], json!(["templated"]));
    let stored = read(&dir.join("pages/81234/copy.json"));
    assert_eq!(
        (stored["source"].as_str(), stored["flags"].clone()),
        (Some("fallback"), json!(["templated"]))
    );

    // Other steps are Ghost's (#18).
    let classify = job(&f, "ai.classify", "81234").await;
    let (env, code, _, _) = ai(&["ai", "input", &classify]);
    assert_eq!(
        (code, env["error"]["code"].as_str()),
        (4, Some("precondition")),
        "{env}"
    );
    // A missing job.
    let (env, code, _, _) = ai(&["ai", "input", "999999"]);
    assert_eq!(
        (code, env["error"]["code"].as_str()),
        (3, Some("not_found")),
        "{env}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
