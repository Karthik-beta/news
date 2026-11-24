//! `news`: the agent toolbox CLI (Technical Design "Agent toolbox"; Bureau Design §6).
//!
//! Contract for every call (§6.2; ADR 0002):
//! * stdout carries exactly one JSON envelope line; logs and clap's own messages go to stderr;
//! * the exit code follows the error code (0, 2–7, 10);
//! * one `news.events` row per call, with `run_id` / `job_id` from `NEWS_RUN_ID` / `NEWS_JOB_ID`.
//!
//! `--help` and `--version` print plain text and exit 0: they are not command results.

mod cli;
mod commands;

use clap::error::ErrorKind;
use clap::{CommandFactory, FromArgMatches};
use cli::Cli;
use news_core::{Envelope, NewsError, Shape};
use news_telemetry::{ClickHouse, Event};
use serde_json::json;
use std::io::Write;
use std::process::ExitCode;
use std::time::Instant;
use tracing::Instrument;

/// The dotted subcommand path of parsed matches: `jobs.claim`, `version`.
fn command_path(m: &clap::ArgMatches) -> String {
    let mut parts = Vec::new();
    let mut cur = m;
    while let Some((name, sub)) = cur.subcommand() {
        parts.push(name.to_owned());
        cur = sub;
    }
    parts.join(".")
}

/// The envelope for a clap parse failure: `usage`, with clap's first line as the message.
fn usage_envelope(e: &clap::Error) -> Envelope {
    // clap's first paragraph, on one line: "the following required arguments were not provided: --worker <WORKER>".
    let rendered = e.render().to_string();
    let first: Vec<&str> = rendered
        .lines()
        .skip_while(|l| l.trim().is_empty())
        .take_while(|l| !l.trim().is_empty())
        .map(str::trim)
        .collect();
    let message = if e.kind() == ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand {
        "a subcommand is required".to_owned()
    } else if first.is_empty() {
        "invalid arguments".to_owned()
    } else {
        first.join(" ").trim_start_matches("error: ").to_owned()
    };
    Envelope::err(NewsError::usage(message).with_hint("news --help"))
}

/// Write the envelope line to `out` and flush it. Returns the exit code: a result that never reached
/// stdout (closed or full) is not a success, so a call that succeeded exits `internal` (10) instead.
fn deliver(out: &mut impl Write, env: &Envelope) -> i32 {
    match writeln!(out, "{}", env.to_line()).and_then(|()| out.flush()) {
        Ok(()) => env.exit_code(),
        Err(e) => {
            eprintln!("news: cannot write the result to stdout: {e}");
            match env.exit_code() {
                0 => news_core::ErrorCode::Internal.exit_code(),
                code => code,
            }
        }
    }
}

fn print(env: &Envelope) -> i32 {
    deliver(&mut std::io::stdout().lock(), env)
}

/// What a call says about one Dispatch job attempt, for the event's `error_class` and `attempt`.
#[derive(Debug, Default, Clone, PartialEq)]
struct JobContext {
    error_class: Option<String>,
    attempt: Option<i16>,
}

fn job_context(command: &cli::Command) -> JobContext {
    use cli::{Command, JobsCmd};
    match command {
        Command::Jobs(JobsCmd::Fail { class, holder, .. }) => JobContext {
            error_class: Some(class.clone()),
            attempt: holder.attempt,
        },
        Command::Jobs(JobsCmd::Heartbeat { holder, .. } | JobsCmd::Complete { holder, .. }) => {
            JobContext {
                error_class: None,
                attempt: holder.attempt,
            }
        }
        Command::Jobs(JobsCmd::Retry { class, .. }) => JobContext {
            error_class: class.clone(),
            attempt: None,
        },
        _ => JobContext::default(),
    }
}

fn emit(command: &str, env: &Envelope, started: Instant, ids: (String, String), job: &JobContext) {
    let Some(ch) = ClickHouse::from_env() else {
        return;
    };
    let (status, error, attrs) = match env {
        Envelope::Ok {
            warnings,
            truncated,
            ..
        } => (
            "ok".to_owned(),
            String::new(),
            json!({"exit": 0, "warnings": warnings.len(), "truncated": truncated}),
        ),
        Envelope::Err { error, .. } => (
            error.code.as_str().to_owned(),
            error.message.clone(),
            json!({"exit": error.exit_code()}),
        ),
    };
    let mut ev = Event::cli_call(command, |n| std::env::var(n).ok())
        .with_job(job.error_class.as_deref(), job.attempt)
        .finish(&status, &error, started.elapsed(), &attrs);
    (ev.trace_id, ev.span_id) = ids;
    if let Err(e) = ch.insert(&[ev]) {
        tracing::debug!(error = %e, "telemetry event not written");
    }
}

fn exit(code: i32) -> ExitCode {
    ExitCode::from(u8::try_from(code).unwrap_or(10))
}

fn main() -> ExitCode {
    // Before any reqwest client is built (Ticker, OTLP exporter, Runner); see the function's doc.
    news_wire::runner::install_tls_provider();
    // Board is a window: it needs the main thread and prints no envelope. It also needs `board` as
    // argv[1], which the Hotline checks (ADR 0007 decision 8), so it is not routed through clap.
    // `args_os`, not `args`: `args` panics on an argument that is not Unicode, before clap can turn it
    // into a usage error.
    let argv: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if argv.get(1).is_some_and(|a| a == "board") {
        let rest: Option<Vec<String>> = argv[2..]
            .iter()
            .map(|a| a.to_str().map(str::to_owned))
            .collect();
        return match rest {
            Some(rest) => exit(news_board::run(&rest)),
            None => {
                eprintln!("news board: an argument is not valid Unicode");
                exit(2)
            }
        };
    }
    // The write-API key is read from Credential Manager first (where Board puts it).
    news_core::secrets::set_credential_reader(news_win::cred_read);
    let started = Instant::now();
    let matches = match Cli::command().try_get_matches() {
        Ok(m) => m,
        Err(e) => {
            if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) {
                let _ = e.print();
                return exit(0);
            }
            // clap's full message (with usage) for humans on stderr; the envelope on stdout.
            eprint!("{}", e.render());
            let env = usage_envelope(&e);
            let code = print(&env);
            let none = (String::new(), String::new());
            emit("usage", &env, started, none, &JobContext::default());
            return exit(code);
        }
    };
    let path = command_path(&matches);
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(c) => c,
        Err(e) => {
            let env = usage_envelope(&e);
            let code = print(&env);
            let none = (String::new(), String::new());
            emit(&path, &env, started, none, &JobContext::default());
            return exit(code);
        }
    };
    let job = job_context(&cli.command);

    let guard = news_telemetry::init_tracing();
    let span = tracing::info_span!("news", command = %path);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build();
    let env = match runtime {
        Err(e) => Envelope::err(NewsError::internal(format!(
            "cannot start the runtime: {e}"
        ))),
        Ok(rt) => {
            let env = rt.block_on(execute(cli, &path).instrument(span.clone()));
            // Drop the runtime before the blocking telemetry clients run.
            drop(rt);
            env
        }
    };
    let ids = news_telemetry::current_ids(&span);
    drop(span);
    let code = print(&env);
    emit(&path, &env, started, ids, &job);
    drop(guard);
    exit(code)
}

async fn execute(cli: Cli, path: &str) -> Envelope {
    let shape = match Shape::parse(
        cli.out.limit,
        cli.out.fields.as_deref(),
        cli.out.cursor.as_deref(),
    ) {
        Ok(s) => s,
        Err(e) => return Envelope::err(e),
    };
    match commands::run(cli, path, &shape).await {
        Ok(reply) => {
            let mut shaped = shape.apply_with(reply.data, reply.default_limit);
            let mut warnings = reply.warnings;
            warnings.append(&mut shaped.warnings);
            Envelope::ok_with(shaped.data, warnings, shaped.truncated)
        }
        Err(e) => {
            tracing::debug!(code = %e.code, "command failed");
            Envelope::err(e)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use clap::Parser;
    use news_core::ErrorCode;

    fn parse_error<const N: usize>(args: [&str; N]) -> clap::Error {
        match Cli::try_parse_from(args) {
            Ok(_) => panic!("{args:?} should not parse"),
            Err(e) => e,
        }
    }

    /// A stdout that is closed or full: every write or flush fails.
    struct Broken {
        fail_on_flush_only: bool,
    }
    impl Write for Broken {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if self.fail_on_flush_only {
                Ok(buf.len())
            } else {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
    }

    #[test]
    fn a_result_that_cannot_reach_stdout_is_not_a_success() {
        let ok = Envelope::ok(json!({"claimed": 1}));
        let mut sink = Vec::new();
        assert_eq!(deliver(&mut sink, &ok), 0);
        assert_eq!(sink, format!("{}\n", ok.to_line()).into_bytes());
        for fail_on_flush_only in [false, true] {
            let mut out = Broken { fail_on_flush_only };
            assert_eq!(
                deliver(&mut out, &ok),
                10,
                "flush only: {fail_on_flush_only}"
            );
            // A call that already failed keeps its own exit code.
            let err = Envelope::err(NewsError::usage("bad"));
            assert_eq!(deliver(&mut out, &err), 2);
        }
    }

    #[test]
    fn cli_definition_is_valid() {
        // clap's own consistency checks: duplicate names, conflicting flags, bad help text.
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_version_subcommand() {
        let cli = Cli::try_parse_from(["news", "version"]).expect("`news version` parses");
        assert!(matches!(cli.command, cli::Command::Version));
    }

    #[test]
    fn missing_subcommand_is_an_error_not_a_default() {
        let err = parse_error(["news"]);
        assert_eq!(
            err.kind(),
            ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
        assert_eq!(err.exit_code(), 2);
        assert_eq!(usage_envelope(&err).exit_code(), 2);
    }

    #[test]
    fn unknown_subcommand_is_rejected() {
        let err = parse_error(["news", "frobnicate"]);
        assert_eq!(err.kind(), ErrorKind::InvalidSubcommand);
        let env = usage_envelope(&err);
        assert_eq!(env.exit_code(), 2);
        assert!(
            env.to_line().contains("unrecognized subcommand"),
            "{}",
            env.to_line()
        );
    }

    #[test]
    fn subcommand_names_are_case_sensitive() {
        assert_eq!(
            parse_error(["news", "VERSION"]).kind(),
            ErrorKind::InvalidSubcommand
        );
    }

    #[test]
    fn version_subcommand_takes_no_arguments() {
        assert_eq!(
            parse_error(["news", "version", "extra"]).kind(),
            ErrorKind::UnknownArgument
        );
        assert_eq!(
            parse_error(["news", "version", "--bogus"]).kind(),
            ErrorKind::UnknownArgument
        );
    }

    #[test]
    fn version_flag_reports_the_core_version() {
        let err = parse_error(["news", "--version"]);
        assert_eq!(err.kind(), ErrorKind::DisplayVersion);
        assert_eq!(err.exit_code(), 0);
        assert_eq!(
            err.to_string().trim_end(),
            format!("news {}", news_core::VERSION)
        );
    }

    /// Every leaf command, with placeholder values for its required arguments.
    fn leaves() -> Vec<(String, Vec<String>)> {
        fn walk(cmd: &clap::Command, prefix: Vec<String>, out: &mut Vec<(String, Vec<String>)>) {
            let subs: Vec<_> = cmd
                .get_subcommands()
                .filter(|s| s.get_name() != "help")
                .collect();
            if subs.is_empty() {
                let mut argv = prefix.clone();
                for a in cmd.get_arguments() {
                    if !a.is_required_set() || a.is_global_set() {
                        continue;
                    }
                    let value = a
                        .get_possible_values()
                        .first()
                        .map(|v| v.get_name().to_owned())
                        .unwrap_or_else(|| "1".to_owned());
                    match a.get_long() {
                        Some(long) => {
                            argv.push(format!("--{long}"));
                            argv.push(value);
                        }
                        None => argv.push(value),
                    }
                }
                out.push((prefix.join("."), argv));
                return;
            }
            for s in subs {
                let mut p = prefix.clone();
                p.push(s.get_name().to_owned());
                walk(s, p, out);
            }
        }
        let mut out = Vec::new();
        walk(&Cli::command(), Vec::new(), &mut out);
        out
    }

    #[tokio::test]
    async fn every_command_that_is_not_built_is_a_precondition_stub() {
        let mut stubs = 0;
        for (path, argv) in leaves() {
            if commands::BUILT.contains(&path.as_str()) {
                continue;
            }
            // `stories similar` needs one of --text/--cand; `jobs retry` one of job/--class.
            let mut full = vec!["news".to_owned()];
            full.extend(argv);
            if path == "stories.similar" {
                full.extend(["--text".into(), "x".into()]);
            }
            // `--action update` is the G-tier form (always gated); `dismiss` is the stub.
            if path == "corrections.resolve" {
                full = full
                    .into_iter()
                    .map(|a| if a == "update" { "dismiss".into() } else { a })
                    .collect();
            }
            let cli = Cli::try_parse_from(&full).unwrap_or_else(|e| panic!("{full:?}: {e}"));
            let err = commands::run(cli, &path, &Shape::default())
                .await
                .expect_err(&path);
            assert_eq!(err.code, ErrorCode::Precondition, "{path}");
            assert_eq!(err.exit_code(), 4, "{path}");
            assert!(
                err.hint.as_deref().unwrap_or("").contains('#'),
                "{path}: no issue"
            );
            stubs += 1;
        }
        assert!(stubs >= 10, "only {stubs} stubs");
    }

    #[test]
    fn built_commands_exist() {
        let paths: Vec<String> = leaves().into_iter().map(|(p, _)| p).collect();
        for b in commands::BUILT {
            assert!(paths.iter().any(|p| p == b), "{b}");
        }
    }

    #[test]
    fn every_bureau_command_has_a_contract_except_publish() {
        let have = commands::schema_commands();
        for (path, _) in leaves() {
            // `board` opens a window and prints no envelope: it has no contract.
            if path == "publish" || path == "board" {
                continue;
            }
            assert!(
                have.contains(&path),
                "no contracts/cli/commands/{path}.schema.json"
            );
        }
    }
}
