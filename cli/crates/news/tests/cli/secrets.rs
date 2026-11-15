//! `news work` and the write-API key (Bureau §9.3): it refuses the ai, render and publish pools, and
//! nothing on its path can read `NEWS_WRITE_API_KEY`. The live sentinel run is in the Postgres tests.

use super::{call, call_with, news};
use std::path::{Path, PathBuf};

#[test]
fn work_refuses_ai_render_and_publish_pools_with_exit_2() {
    for pools in ["ai", "render", "publish", "net,ai", "cpu,publish"] {
        let (env, code, _) = call(&["work", "--pools", pools]);
        assert_eq!(code, 2, "{pools}: {env}");
        assert_eq!(env["error"]["code"], "usage", "{pools}");
        assert!(
            env["error"]["message"].as_str().unwrap().contains("never in news work"),
            "{pools}: {env}"
        );
    }
    let (env, code, _) = call(&["work", "--pools", "gpu"]);
    assert_eq!((code, env["error"]["code"].as_str()), (2, Some("usage")));
}

#[test]
fn work_refuses_bad_pools_before_reading_any_environment() {
    // A sentinel key and a bad database URL change nothing: the pool check comes first.
    let sentinel = "sentinel-write-key-0f3a";
    let out = news()
        .args(["work", "--pools", "publish"])
        .env("NEWS_WRITE_API_KEY", sentinel)
        .env("DATABASE_URL", "postgres://x:y@127.0.0.1:1/z")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let all = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(!all.contains(sentinel));
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n == "tests" || n == "target") {
                continue;
            }
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn only_the_secrets_guard_names_the_write_api_key() {
    // The variable is read in exactly one place, news-core/src/secrets.rs, behind the guard that
    // `news work` trips first thing; the subsystems that may publish (news-press) call the guard.
    let crates = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    rust_files(&crates, &mut files);
    let naming: Vec<String> = files
        .iter()
        .filter(|f| std::fs::read_to_string(f).unwrap().contains("NEWS_WRITE_API_KEY"))
        .map(|f| f.strip_prefix(&crates).unwrap().to_string_lossy().replace('\\', "/"))
        .collect();
    assert_eq!(naming, ["news-core/src/secrets.rs"], "{naming:?}");
    let reading: Vec<String> = files
        .iter()
        .filter(|f| {
            std::fs::read_to_string(f)
                .unwrap()
                .replace("forbid_write_api_key()", "")
                .contains("write_api_key()")
        })
        .map(|f| f.strip_prefix(&crates).unwrap().to_string_lossy().replace('\\', "/"))
        .filter(|f| f != "news-core/src/secrets.rs")
        .collect();
    assert!(
        reading.iter().all(|f| f.starts_with("news-press/")),
        "only Press may call write_api_key(): {reading:?}"
    );
}

#[test]
fn work_forbids_the_key_before_it_needs_a_database() {
    // With no DATABASE_URL `news work` fails (precondition), but only after forbidding the key; the
    // failure envelope must not mention or contain it.
    let sentinel = "sentinel-write-key-77aa";
    let (env, code, stderr) = call_with(
        news()
            .args(["work", "--pools", "net", "--once"])
            .env("NEWS_WRITE_API_KEY", sentinel),
    );
    assert_eq!(code, 4, "{env}");
    assert!(!env.to_string().contains(sentinel));
    assert!(!stderr.contains(sentinel));
}
