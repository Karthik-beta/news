//! CI guard: no test opens a network socket.
//!
//! The only code that can reach the network is `ReqwestTransport` (news-wire/src/runner.rs); the
//! telemetry exporter and the Postgres client only talk to loopback services. This test fails if any
//! test file constructs the live transport, uses reqwest or opens a socket directly, so tests can only
//! run through `fake::FakeTransport` and recorded fixtures.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name()
                .is_some_and(|n| n != "target" && n != "fixtures")
            {
                rust_files(&p, out);
            }
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// The `#[cfg(test)]` part of a source file, or the whole file for files under `tests/`.
fn test_code(path: &Path, text: &str) -> String {
    let under_tests = path.components().any(|c| c.as_os_str() == "tests");
    if under_tests {
        text.to_owned()
    } else {
        text.split_once("#[cfg(test)]")
            .map(|(_, t)| t.to_owned())
            .unwrap_or_default()
    }
}

/// Patterns that open a socket or resolve a name. Built from pieces so this file does not match
/// itself.
fn forbidden() -> Vec<String> {
    [
        ["Reqwest", "Transport::new"],
        ["reqwest", "::"],
        ["TcpStream", "::connect"],
        ["TcpSocket", "::new"],
        ["UdpSocket", "::bind"],
        ["UdpSocket", "::connect"],
        ["lookup", "_host"],
        ["to_socket", "_addrs"],
        ["ureq", "::"],
        ["hyper", "::client"],
        ["isahc", "::"],
        ["attohttpc", "::"],
        ["surf", "::"],
    ]
    .iter()
    .map(|p| p.concat())
    .collect()
}

/// Every forbidden pattern a file's test code contains.
fn offenders(path: &Path, text: &str) -> Vec<String> {
    let code = test_code(path, text);
    forbidden()
        .into_iter()
        .filter(|pat| code.contains(pat.as_str()))
        .map(|pat| format!("{} uses {pat}", path.display()))
        .collect()
}

#[test]
fn no_test_can_open_a_network_socket() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    rust_files(&crates, &mut files);
    assert!(files.len() > 20, "found only {} files", files.len());
    let offenders: Vec<String> = files
        .iter()
        .flat_map(|f| offenders(f, &std::fs::read_to_string(f).unwrap()))
        .collect();
    assert!(
        offenders.is_empty(),
        "tests must not touch the network:\n{}",
        offenders.join("\n")
    );
}

/// The guard itself: if it stopped seeing a live transport it would pass for the wrong reason.
#[test]
fn the_guard_catches_what_it_is_meant_to_catch() {
    let live = [
        "let t = news_wire::runner::Reqwest",
        "Transport::new().unwrap();",
    ]
    .concat();
    let connect = [
        "let s = std::net::TcpStream",
        "::connect(\"example.com:443\");",
    ]
    .concat();
    let dns = ["\"example.com:443\".to_socket", "_addrs()"].concat();
    // Any file under `tests/` is all test code.
    for code in [&live, &connect, &dns] {
        let hits = offenders(Path::new("crates/x/tests/a.rs"), code);
        assert_eq!(hits.len(), 1, "{code}: {hits:?}");
    }
    // In a source file only what follows `#[cfg(test)]` counts: the live transport's own
    // definition must not trip it, a test module that uses it must.
    let src = format!("fn prod() {{ {live} }}\n#[cfg(test)]\nmod tests {{ fn t() {{}} }}\n");
    assert!(offenders(Path::new("crates/x/src/lib.rs"), &src).is_empty());
    let src = format!("fn prod() {{}}\n#[cfg(test)]\nmod tests {{ fn t() {{ {live} }} }}\n");
    assert_eq!(offenders(Path::new("crates/x/src/lib.rs"), &src).len(), 1);
    // Loopback servers (a listener the code under test talks to) and the fake transport are fine.
    let fine =
        "let l = std::net::TcpListener::bind(\"127.0.0.1:0\"); let f = FakeTransport::new();";
    assert!(offenders(Path::new("crates/x/tests/a.rs"), fine).is_empty());
    // Fixtures are never scanned, and the walk does see the Rust tests of every crate.
    let mut files = Vec::new();
    rust_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join(".."),
        &mut files,
    );
    for needle in [
        "news-wire/tests/runner.rs",
        "news-connectors/tests/stringers.rs",
    ] {
        assert!(
            files.iter().any(|f| f
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/")
                .ends_with(needle)),
            "{needle} is not scanned"
        );
    }
}
