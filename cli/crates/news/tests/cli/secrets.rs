//! The write-API key (Bureau §9.3): one function names the variable, and only Press calls it. The live
//! sentinel run of the service's background pools is in the Postgres tests (`dispatch.rs`).

use super::call;
use std::path::{Path, PathBuf};

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
    // The variable is read in exactly one place, news-core/src/secrets.rs, behind the guard that counts
    // every read; the subsystem that may publish (news-press) is the only caller.
    let crates = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    rust_files(&crates, &mut files);
    let naming: Vec<String> = files
        .iter()
        .filter(|f| {
            std::fs::read_to_string(f)
                .unwrap()
                .contains("NEWS_WRITE_API_KEY")
        })
        .map(|f| {
            f.strip_prefix(&crates)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
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
        .map(|f| {
            f.strip_prefix(&crates)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .filter(|f| f != "news-core/src/secrets.rs")
        .collect();
    assert!(
        reading.iter().all(|f| f.starts_with("news-press/")),
        "only Press may call write_api_key(): {reading:?}"
    );
}

#[test]
fn there_is_no_work_command_any_more() {
    // The worker loop runs inside the Bureau service (ADR 0007); no terminal command starts it.
    let (env, code, _) = call(&["work", "--pools", "net", "--once"]);
    assert_eq!(code, 2, "{env}");
    assert_eq!(env["error"]["code"], "usage");
    assert!(
        env["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unrecognized subcommand 'work'"),
        "{env}"
    );
}
