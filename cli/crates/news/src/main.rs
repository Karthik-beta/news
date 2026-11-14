//! `news`: the agent toolbox CLI. Commands return compact JSON for agents.
//! Phase 0 skeleton: only `version` works; the toolbox is built in Phase 5.

use clap::{Parser, Subcommand};

#[derive(Parser)]
// `bin_name` fixes the name in usage and help; clap would otherwise use argv[0] (`news.exe` on Windows).
#[command(
    name = "news",
    bin_name = "news",
    version = news_core::VERSION,
    about = "News agent toolbox"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the CLI version as JSON.
    Version,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Version => {
            println!("{}", serde_json::json!({ "news": news_core::VERSION }));
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use clap::error::ErrorKind;

    fn parse_error<const N: usize>(args: [&str; N]) -> clap::Error {
        match Cli::try_parse_from(args) {
            Ok(_) => panic!("{args:?} should not parse"),
            Err(e) => e,
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
        assert!(matches!(cli.command, Command::Version));
    }

    #[test]
    fn missing_subcommand_is_an_error_not_a_default() {
        let err = parse_error(["news"]);
        assert_eq!(
            err.kind(),
            ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
        assert_eq!(err.exit_code(), 2);
    }

    #[test]
    fn unknown_subcommand_is_rejected() {
        let err = parse_error(["news", "frobnicate"]);
        assert_eq!(err.kind(), ErrorKind::InvalidSubcommand);
        assert_eq!(err.exit_code(), 2);
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
}
