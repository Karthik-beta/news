//! Command routing. Built commands live in their own modules; every other command is a stub that
//! returns `precondition` (exit 4) naming the issue that builds it.

mod calendar;
mod components;
mod db;
mod entities;
mod fixer;
mod home;
mod jobs;
mod nose;
mod run;
mod schema;
mod sources;
mod stories;

use crate::cli::*;
use news_core::shape::DEFAULT_LIMIT;
use news_core::{NewsError, NewsResult, Shape};
use serde_json::Value;

/// What a command returns before shaping.
#[derive(Debug)]
pub struct Reply {
    pub data: Value,
    pub warnings: Vec<String>,
    /// Items when no `--limit` is given.
    pub default_limit: usize,
}

impl Reply {
    pub fn new(data: Value) -> Self {
        Self {
            data,
            warnings: Vec::new(),
            default_limit: DEFAULT_LIMIT,
        }
    }
    pub fn default_limit(mut self, n: usize) -> Self {
        self.default_limit = n;
        self
    }
}

fn stub(path: &str, issue: &str) -> NewsResult<Reply> {
    Err(NewsError::not_built(&path.replace('.', " "), issue))
}

/// Run one parsed command. `path` is the dotted command path (`jobs.claim`).
pub async fn run(cli: Cli, path: &str, shape: &Shape) -> NewsResult<Reply> {
    use Command as C;
    match cli.command {
        C::Version => Ok(Reply::new(
            serde_json::json!({ "news": news_core::VERSION }),
        )),
        C::Db(cmd) => db::run(cmd).await,
        C::Components => components::list(),
        C::Component { component_type } => components::show(&component_type),
        C::Schema { command } => schema::command(&command),
        C::Jobs(cmd) => jobs::run(cmd, shape).await,
        C::Ai(AiCmd::Schema { step }) => schema::ai(&step),

        // Not built yet: each names the issue that builds its subsystem.
        C::Sources(cmd) => sources::run(cmd).await,
        // `prune` deletes working files; Wire owns them and the `work.prune` job (#9).
        C::Discover { .. } | C::Fetch { .. } | C::Prune => stub(path, news_wire::ISSUE),
        C::Parse { .. } => stub(path, news_clerk::ISSUE),
        C::Run(cmd) => run::run(cmd).await,
        C::Standing(cmd) => run::standing(cmd).await,
        C::Service(cmd) => run::service(cmd).await,
        // `main` runs Board before the parser; this arm is for completeness.
        C::Board(_) => Err(NewsError::usage(
            "news board opens a window and is run directly",
        )),
        // G tier: the CLI forms are always gated (ADR 0008 §6.3).
        C::Publish { .. } => run::gated("publish"),
        C::Home(HomeCmd::Publish { .. }) => run::gated("home publish"),
        C::Eval(EvalCmd::Promote { .. }) => run::gated("eval promote"),
        C::Corrections(CorrectionsCmd::Resolve { action, .. }) if action == "update" => {
            run::gated("corrections resolve --action update")
        }
        C::Calendar(cmd) => calendar::run(cmd).await,
        C::Entities(cmd) => entities::run(cmd).await,
        C::Related { story } => fixer::related(&story).await,
        C::Stories(cmd) => stories::run(cmd).await,
        C::Plan { run } => nose::plan(&run).await,
        C::Scores(cmd) => nose::scores(cmd, path).await,
        C::Stats => stub(path, news_telemetry::ISSUE),
        C::Save { .. } | C::Render { .. } | C::Check { .. } => stub(path, news_proof::ISSUE),
        C::Brief { story } => fixer::brief(&story).await,
        C::Ai(_) => stub(path, news_ghost::ISSUE),
        C::Eval(_) | C::Review(_) => stub(path, news_critic::ISSUE),
        C::Update { .. } => stub(path, news_rewrite::ISSUE),
        C::Home(cmd) => home::run(cmd).await,
        C::Corrections(_) => stub(path, "#21"),
    }
}

/// Commands that do real work in this build. Everything else is a stub.
#[cfg(test)]
pub const BUILT: &[&str] = &[
    "version",
    "db.migrate",
    "db.status",
    "components",
    "component",
    "schema",
    "jobs.list",
    "jobs.dead",
    "jobs.claim",
    "jobs.heartbeat",
    "jobs.complete",
    "jobs.fail",
    "jobs.retry",
    "ai.schema",
    "run.presets",
    "run.status",
    "run.lineup",
    "run.list",
    "standing.show",
    "standing.history",
    "service.run",
    "service.install",
    "service.uninstall",
    "service.stage",
    "service.start",
    "service.stop",
    "service.status",
    "board",
    "publish",
    "home.publish",
    "home.candidates",
    "home.save",
    "home.check",
    "home.preview",
    "home.show",
    "eval.promote",
    "calendar.upcoming",
    "calendar.sync",
    "calendar.late",
    "entities.resolve",
    "entities.show",
    "entities.unresolved",
    "sources.list",
    "sources.show",
    "sources.health",
    "sources.probe",
    "sources.sync",
    "plan",
    "scores.explain",
    "scores.weights",
    "stories.show",
    "stories.members",
    "stories.similar",
    "brief",
    "related",
];

/// Dotted names of the commands that have a contract file.
#[cfg(test)]
pub fn schema_commands() -> Vec<String> {
    schema::commands()
}

/// Open the database from `DATABASE_URL` / `NEWS_DB_SCHEMA`; the schema must exist, so queries never
/// fall back to `public`.
pub async fn database(max: u32) -> NewsResult<(news_store::PgPool, news_store::DbConfig)> {
    let (pool, cfg) = open_database(max).await?;
    news_store::require_schema(&pool, &cfg.schema).await?;
    Ok((pool, cfg))
}

/// [`database`] without the schema check, for `db migrate` (which creates it) and `db status`.
pub async fn open_database(max: u32) -> NewsResult<(news_store::PgPool, news_store::DbConfig)> {
    let cfg = news_store::DbConfig::from_env()?;
    let pool = news_store::connect(&cfg, max).await?;
    Ok((pool, cfg))
}

/// Read a JSON file, or stdin for `-`.
pub fn read_json(path: &str) -> NewsResult<Value> {
    let text = if path == "-" {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)
            .map_err(|e| NewsError::usage(format!("cannot read stdin: {e}")))?;
        s
    } else {
        std::fs::read_to_string(path)
            .map_err(|e| NewsError::not_found(format!("cannot read {path}: {e}")))?
    };
    serde_json::from_str(&text)
        .map_err(|e| NewsError::validation(format!("{path} is not valid JSON: {e}")))
}
