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

#[test]
fn no_test_can_open_a_network_socket() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    rust_files(&crates, &mut files);
    assert!(files.len() > 20, "found only {} files", files.len());
    // Built from pieces so this file does not match itself.
    let forbidden = [
        ["Reqwest", "Transport::new"].concat(),
        ["reqwest", "::"].concat(),
        ["TcpStream", "::connect"].concat(),
        ["UdpSocket", "::bind"].concat(),
        ["ureq", "::"].concat(),
    ];
    let mut offenders = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        let code = test_code(f, &text);
        for pat in &forbidden {
            if code.contains(pat.as_str()) {
                offenders.push(format!("{} uses {pat}", f.display()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "tests must not touch the network:\n{}",
        offenders.join("\n")
    );
}
