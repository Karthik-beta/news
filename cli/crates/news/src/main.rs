//! `news`: the agent toolbox CLI. Commands return compact JSON for agents.
//! Phase 0 skeleton: only `version` works; the toolbox is built in Phase 5.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "news", version = news_core::VERSION, about = "News agent toolbox")]
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
