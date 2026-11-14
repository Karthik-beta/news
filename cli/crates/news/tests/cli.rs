//! End-to-end tests: run the real `news` binary and check exit codes, stdout and stderr.
//! No database, no network. Run with `cargo test -p news --test cli`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn news() -> Command {
    let mut cmd = assert_cmd::cargo_bin_cmd!("news");
    // Output must not depend on the developer's terminal or a stray config.
    cmd.env_remove("COLUMNS")
        .env_remove("CLICOLOR_FORCE")
        .env("NO_COLOR", "1");
    cmd
}

/// Stdout of a successful run. clap prints the binary's file name in `Usage:`, which is `news.exe` on
/// Windows; that is folded to `news` so snapshots and comparisons are the same on every platform.
fn stdout_of(args: &[&str]) -> String {
    let out = news()
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8(out)
        .expect("stdout is UTF-8")
        .replace("news.exe", "news")
}

fn no_ansi() -> impl Predicate<str> {
    predicate::str::contains('\u{1b}').not()
}

// --- `news version` -------------------------------------------------------------------------

#[test]
fn version_prints_a_json_object_with_one_news_key() {
    let out = stdout_of(&["version"]);
    let json: Value = serde_json::from_str(&out).expect("stdout is valid JSON");
    let object = json.as_object().expect("stdout is a JSON object");
    assert_eq!(object.len(), 1, "unexpected extra keys: {object:?}");
    assert_eq!(object.get("news"), Some(&Value::String(VERSION.to_owned())));
}

#[test]
fn version_json_is_one_compact_line_ending_in_a_newline() {
    let out = stdout_of(&["version"]);
    assert!(out.ends_with('\n'), "no trailing newline: {out:?}");
    assert_eq!(
        out.matches('\n').count(),
        1,
        "output spans several lines: {out:?}"
    );
    assert!(
        !out.contains(": ") && !out.contains(", "),
        "JSON is not compact: {out:?}"
    );
}

#[test]
fn version_matches_the_core_crate_version() {
    let json: Value = serde_json::from_str(&stdout_of(&["version"])).unwrap();
    assert_eq!(json["news"], news_core::VERSION);
}

#[test]
fn version_writes_nothing_to_stderr_and_exits_zero() {
    news()
        .arg("version")
        .assert()
        .code(0)
        .stderr(predicate::str::is_empty());
}

#[test]
fn version_is_idempotent() {
    assert_eq!(stdout_of(&["version"]), stdout_of(&["version"]));
}

#[test]
fn version_ignores_database_and_pipeline_environment() {
    // `version` must work on a bare machine: no DB, no network, no config.
    let plain = stdout_of(&["version"]);
    let out = news()
        .arg("version")
        .env("DATABASE_URL", "postgres://nobody:nothing@127.0.0.1:1/none")
        .env("NEWS_PG_PASSWORD", "not-a-real-password")
        .env("NEWS_TARGET_PAGES", "not-a-number")
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    assert_eq!(String::from_utf8(out).unwrap(), plain);
}

#[test]
fn version_does_not_echo_environment_secrets() {
    let secret = "s3cr3t-value-that-must-not-appear";
    news()
        .arg("version")
        .env("NEWS_PG_PASSWORD", secret)
        .env(
            "DATABASE_URL",
            format!("postgres://news:{secret}@127.0.0.1:5433/news"),
        )
        .assert()
        .success()
        .stdout(predicate::str::contains(secret).not())
        .stderr(predicate::str::contains(secret).not());
}

// --- `--version`, `--help` -------------------------------------------------------------------

#[test]
fn version_flag_prints_name_and_version() {
    for flag in ["--version", "-V"] {
        news()
            .arg(flag)
            .assert()
            .success()
            .stdout(format!("news {VERSION}\n"))
            .stderr(predicate::str::is_empty());
    }
}

#[test]
fn help_lists_the_version_command() {
    for flag in ["--help", "-h"] {
        news()
            .arg(flag)
            .assert()
            .success()
            .stdout(
                predicate::str::contains("News agent toolbox")
                    .and(predicate::str::contains("Usage: news"))
                    .and(
                        predicate::str::is_match(
                            r"(?m)^\s+version\s+Print the CLI version as JSON$",
                        )
                        .unwrap(),
                    ),
            )
            .stderr(predicate::str::is_empty());
    }
}

#[test]
fn help_subcommand_matches_help_flag() {
    assert_eq!(stdout_of(&["help"]), stdout_of(&["--help"]));
    assert_eq!(
        stdout_of(&["help", "version"]),
        stdout_of(&["version", "--help"])
    );
}

#[test]
fn version_command_help_describes_itself() {
    let help = stdout_of(&["version", "--help"]);
    assert!(help.starts_with("Print the CLI version as JSON"), "{help}");
    assert!(help.contains("Usage: news version"), "{help}");
}

#[test]
fn help_snapshot() {
    insta::assert_snapshot!("help", stdout_of(&["--help"]));
}

#[test]
fn version_help_snapshot() {
    insta::assert_snapshot!("version_help", stdout_of(&["version", "--help"]));
}

// --- Malformed input ----------------------------------------------------------------------

#[test]
fn no_arguments_is_a_usage_error() {
    // Agents calling `news` bare get usage on stderr and exit code 2, never a JSON success.
    news()
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("Usage: news"));
}

#[test]
fn unknown_subcommand_is_a_usage_error() {
    news()
        .arg("frobnicate")
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "unrecognized subcommand 'frobnicate'",
        ));
}

#[test]
fn unknown_flag_is_a_usage_error() {
    for args in [["--bogus"].as_slice(), ["version", "--bogus"].as_slice()] {
        news()
            .args(args)
            .assert()
            .code(2)
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains("unexpected argument '--bogus'"));
    }
}

#[test]
fn version_rejects_extra_positional_arguments() {
    news()
        .args(["version", "extra"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("unexpected argument 'extra'"));
}

#[test]
fn subcommands_are_case_sensitive() {
    news()
        .arg("VERSION")
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty());
    news()
        .arg("Version")
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty());
}

#[test]
fn empty_string_argument_is_rejected() {
    news()
        .arg("")
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty());
}

#[test]
fn non_utf8_argument_is_rejected_without_a_panic() {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let bad = std::ffi::OsStr::from_bytes(&[0xff, 0xfe]);
        let assert = news().arg(bad).assert().code(2);
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
        assert!(
            !stderr.contains("panicked"),
            "panic on invalid UTF-8: {stderr}"
        );
    }
    // Windows arguments are UTF-16 and round-trip lossily; the Unix case above is the real check.
}

// --- Output hygiene -----------------------------------------------------------------------

#[test]
fn piped_output_has_no_ansi_escapes() {
    for args in [vec!["version"], vec!["--help"], vec!["version", "--help"]] {
        let out = stdout_of(&args);
        assert!(no_ansi().eval(&out), "ANSI escape in stdout of {args:?}");
    }
    let stderr = String::from_utf8_lossy(&news().arg("frobnicate").assert().get_output().stderr)
        .into_owned();
    assert!(no_ansi().eval(&stderr), "ANSI escape in error output");
}

#[test]
fn errors_never_produce_a_panic_message() {
    for args in [vec![], vec!["nope"], vec!["version", "x", "y"], vec!["--"]] {
        let assert = news().args(&args).assert().failure();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
        assert!(!stderr.contains("panicked"), "{args:?} panicked: {stderr}");
        assert_eq!(assert.get_output().status.code(), Some(2), "{args:?}");
    }
}
