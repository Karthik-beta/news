//! End-to-end tests: run the real `news` binary and check exit codes, stdout and stderr.
//! No database, no network. Run with `cargo test -p news --test cli` (`bun scripts/test-cli.ts e2e`).
//!
//! Modules: `contract` (envelope, exit codes, stdout/stderr discipline), `stubs` (every command not
//! built yet), `contracts` (every file in contracts/cli against the JSON Schema 2020-12 meta-schema),
//! `components` (the component index), `secrets` (only the secrets guard names the write-API key).

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "cli/check.rs"]
mod check;
#[path = "cli/components.rs"]
mod components;
#[path = "cli/contract.rs"]
mod contract;
#[path = "cli/contracts.rs"]
mod contracts;
#[path = "cli/retired.rs"]
mod retired;
#[path = "cli/secrets.rs"]
mod secrets;
#[path = "cli/stubs.rs"]
mod stubs;

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The binary with a clean environment: no database, no telemetry, no agent-shell marker.
pub fn news() -> Command {
    let mut cmd = assert_cmd::cargo_bin_cmd!("news");
    // Output must not depend on the developer's terminal, a stray config or the local .env.
    cmd.env_remove("COLUMNS")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("DATABASE_URL")
        .env_remove("NEWS_DB_SCHEMA")
        .env_remove("NEWS_CH_HTTP")
        .env_remove("NEWS_OTEL_HTTP")
        .env_remove("NEWS_LOG")
        .env_remove("NEWS_RUN_ID")
        .env_remove("NEWS_JOB_ID")
        .env_remove("NEWS_COMPONENT_INDEX")
        .env("NO_COLOR", "1");
    cmd
}

/// Run `news` and return (envelope, exit code, stderr). Panics unless stdout is exactly one JSON
/// object on one line.
pub fn call(args: &[&str]) -> (Value, i32, String) {
    call_with(news().args(args))
}

pub fn call_with(cmd: &mut Command) -> (Value, i32, String) {
    let out = cmd.output().expect("news runs");
    let stdout = String::from_utf8(out.stdout).expect("stdout is UTF-8");
    let stderr = String::from_utf8(out.stderr).expect("stderr is UTF-8");
    assert!(stdout.ends_with('\n'), "no trailing newline: {stdout:?}");
    assert_eq!(
        stdout.matches('\n').count(),
        1,
        "stdout is not one line: {stdout:?}"
    );
    let env: Value = serde_json::from_str(&stdout).expect("stdout is JSON");
    assert!(env.is_object(), "stdout is not an object: {stdout}");
    (env, out.status.code().expect("exit code"), stderr)
}

/// Stdout of a successful run, exactly as printed (no per-platform normalisation).
fn stdout_of(args: &[&str]) -> String {
    let out = news()
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8(out).expect("stdout is UTF-8")
}

fn no_ansi() -> impl Predicate<str> {
    predicate::str::contains('\u{1b}').not()
}

fn assert_usage(args: &[&str], message: &str) {
    let (env, code, stderr) = call(args);
    assert_eq!(code, 2, "{args:?}");
    assert_eq!(env["ok"], false);
    assert_eq!(env["error"]["code"], "usage", "{args:?}: {env}");
    assert!(
        env["error"]["message"].as_str().unwrap().contains(message),
        "{args:?}: {env}"
    );
    assert!(!stderr.contains("panicked"), "{args:?}: {stderr}");
}

// --- `news version` -------------------------------------------------------------------------

#[test]
fn version_prints_the_envelope_with_the_version() {
    let (env, code, stderr) = call(&["version"]);
    assert_eq!(code, 0);
    assert_eq!(
        env,
        serde_json::json!({"ok": true, "data": {"news": VERSION}, "warnings": [], "truncated": false})
    );
    assert!(stderr.is_empty(), "{stderr}");
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
    let (env, _, _) = call(&["version"]);
    assert_eq!(env["data"]["news"], news_core_version());
}

fn news_core_version() -> &'static str {
    // news-core and news share the workspace version.
    VERSION
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

/// Every command group of the Technical Design ("Agent toolbox") and Bureau Design §6.4.
pub const GROUPS: &[&str] = &[
    "version",
    "db",
    "discover",
    "plan",
    "fetch",
    "parse",
    "brief",
    "components",
    "component",
    "related",
    "save",
    "render",
    "check",
    "publish",
    "update",
    "prune",
    "stats",
    "home",
    "run",
    "jobs",
    "ai",
    "sources",
    "calendar",
    "stories",
    "scores",
    "entities",
    "eval",
    "review",
    "corrections",
    "schema",
];

#[test]
fn help_lists_every_command_group() {
    let help = stdout_of(&["--help"]);
    for g in GROUPS {
        let re = predicate::str::is_match(format!(r"(?m)^  {g}\s{{2,}}\S")).unwrap();
        assert!(re.eval(&help), "`news --help` does not list {g}:\n{help}");
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
fn program_is_called_news_on_every_platform() {
    // clap would otherwise take the name from argv[0], which is `news.exe` on Windows.
    for args in [
        ["--help"].as_slice(),
        ["help"].as_slice(),
        ["version", "--help"].as_slice(),
        ["jobs", "--help"].as_slice(),
    ] {
        let out = stdout_of(args);
        assert!(out.contains("Usage: news "), "{args:?}: {out}");
        assert!(!out.contains("news.exe"), "{args:?}: {out}");
    }
    news().assert().code(2).stderr(
        predicate::str::contains("Usage: news [OPTIONS] <COMMAND>")
            .and(predicate::str::contains("news.exe").not()),
    );
}

#[test]
fn help_snapshot() {
    insta::assert_snapshot!("help", stdout_of(&["--help"]));
}

#[test]
fn version_help_snapshot() {
    insta::assert_snapshot!("version_help", stdout_of(&["version", "--help"]));
}

#[test]
fn group_help_snapshots() {
    // The Bureau groups built here, so a renamed flag shows up in review.
    insta::assert_snapshot!("jobs_help", stdout_of(&["jobs", "--help"]));
    insta::assert_snapshot!("run_help", stdout_of(&["run", "--help"]));
}

#[test]
fn groups_that_once_had_h_tier_forms_list_none() {
    // H-tier actions are Board's (ADR 0007): these groups show only their R, W and G commands.
    for group in ["home", "sources", "stories", "scores", "entities", "eval"] {
        insta::assert_snapshot!(format!("{group}_help"), stdout_of(&[group, "--help"]));
    }
}

// --- Malformed input: usage envelope, exit 2 ---------------------------------------------------

#[test]
fn no_arguments_is_a_usage_error() {
    // Agents calling `news` bare get the usage envelope on stdout, clap's usage on stderr, exit 2.
    let (env, code, stderr) = call(&[]);
    assert_eq!(code, 2);
    assert_eq!(env["error"]["code"], "usage");
    assert!(stderr.contains("Usage: news"), "{stderr}");
}

#[test]
fn a_group_without_its_subcommand_is_a_usage_error() {
    for g in ["jobs", "run", "home", "db"] {
        let args: Vec<&str> = g.split(' ').collect();
        assert_usage(&args, "subcommand is required");
    }
}

#[test]
fn unknown_subcommand_is_a_usage_error() {
    assert_usage(&["frobnicate"], "unrecognized subcommand 'frobnicate'");
}

#[test]
fn unknown_flag_is_a_usage_error() {
    for args in [["--bogus"].as_slice(), ["version", "--bogus"].as_slice()] {
        assert_usage(args, "unexpected argument '--bogus'");
    }
}

#[test]
fn version_rejects_extra_positional_arguments() {
    assert_usage(&["version", "extra"], "unexpected argument 'extra'");
}

#[test]
fn subcommands_are_case_sensitive() {
    assert_usage(&["VERSION"], "unrecognized subcommand");
    assert_usage(&["Version"], "unrecognized subcommand");
}

#[test]
fn empty_string_argument_is_rejected() {
    let (_, code, _) = call(&[""]);
    assert_eq!(code, 2);
}

#[test]
fn jobs_claim_refuses_cursor_and_fields() {
    // A claim takes jobs; paging or trimming its result would lose leases the caller never sees.
    // Checked before the database is opened (DATABASE_URL is unset here).
    for extra in [["--cursor", "c1"], ["--fields", "job_id"]] {
        let mut args = vec!["jobs", "claim", "--pool", "net", "--worker", "w"];
        args.extend(extra);
        assert_usage(&args, "jobs claim");
    }
}

#[test]
fn missing_required_arguments_are_usage_errors() {
    assert_usage(&["fetch"], "required");
    assert_usage(&["jobs", "claim", "--pool", "net"], "--worker");
    assert_usage(&["jobs", "fail", "1", "--class", "transient"], "--message");
}

#[test]
fn non_utf8_argument_is_rejected_without_a_panic() {
    #[cfg(unix)]
    let bad: std::ffi::OsString = {
        use std::os::unix::ffi::OsStrExt;
        std::ffi::OsStr::from_bytes(&[0xff, 0xfe]).to_owned()
    };
    // An unpaired surrogate is the Windows form of an argument that is not Unicode.
    #[cfg(windows)]
    let bad: std::ffi::OsString = {
        use std::os::windows::ffi::OsStringExt;
        std::ffi::OsString::from_wide(&[0xD800])
    };
    // In the subcommand's place, and after `board` (which `main` handles before clap).
    let board = std::ffi::OsString::from("board");
    for args in [vec![&bad], vec![&board, &bad]] {
        let assert = news().args(&args).assert().code(2);
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
        assert!(
            !stderr.contains("panicked"),
            "panic on a non-Unicode argument {args:?}: {stderr}"
        );
    }
}

// --- Output hygiene -----------------------------------------------------------------------

#[test]
fn piped_output_has_no_ansi_escapes() {
    for args in [vec!["version"], vec!["--help"], vec!["version", "--help"]] {
        let out = stdout_of(&args);
        assert!(no_ansi().eval(&out), "ANSI escape in stdout of {args:?}");
    }
    let out = news().arg("frobnicate").output().unwrap();
    for stream in [out.stdout, out.stderr] {
        assert!(
            no_ansi().eval(&String::from_utf8_lossy(&stream)),
            "ANSI escape in error output"
        );
    }
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

// --- Nose ---------------------------------------------------------------------------------

#[test]
fn scores_weights_prints_scorer_config_v1_without_a_database() {
    let (env, code, stderr) = call(&["scores", "weights"]);
    assert_eq!(code, 0, "{env} {stderr}");
    let d = &env["data"];
    assert_eq!(
        (d["live"].as_str(), d["version"].as_str()),
        (Some("seed"), Some("v1"))
    );
    assert_eq!(d["weights"]["I"], 0.30);
    assert_eq!(d["weights"]["G"], 0.10);
    assert_eq!(d["half_life_hours"]["report"], 96.0);
    assert_eq!(d["event_weights"]["policy_major"], 1.0);
    assert_eq!(d["coverage_targets"]["region"]["india"], 0.15);
}

#[test]
fn huddle_commands_refuse_bad_input_before_the_database() {
    for (args, code, kind) in [
        (vec!["scores", "adopt", "v-two"], 2, "usage"),
        (vec!["scores", "propose"], 2, "usage"),
        (
            vec!["scores", "label", "--run", "r_1", "no-such-file.json"],
            3,
            "not_found",
        ),
    ] {
        let (env, got, _) = call(&args);
        assert_eq!(
            (got, env["error"]["code"].as_str()),
            (code, Some(kind)),
            "{args:?}: {env}"
        );
    }
}

#[test]
fn plan_refuses_a_run_that_is_not_a_uuid_before_the_database() {
    let (env, code, _) = call(&["plan", "--run", "r_51d2"]);
    assert_eq!(
        (code, env["error"]["code"].as_str()),
        (2, Some("usage")),
        "{env}"
    );
}
