//! The full command tree: every command group of the Technical Design ("Agent toolbox") and Bureau
//! Design §6.4, plus `news db`. Help text is short on purpose: agents read it.

use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
// `bin_name` fixes the name in usage and help; clap would otherwise use argv[0] (`news.exe` on Windows).
#[command(
    name = "news",
    bin_name = "news",
    version = news_core::VERSION,
    about = "News agent toolbox",
    long_about = "News agent toolbox. Every command prints one JSON envelope on stdout: \
                  {\"ok\":true,\"data\":…,\"warnings\":[],\"truncated\":false} or {\"ok\":false,\"error\":{…}}. \
                  Exit codes: 0 ok, 2 usage, 3 not found, 4 conflict or precondition, 5 retryable, \
                  6 validation failed, 7 gated, 10 internal. Schemas: news schema <command>."
)]
pub struct Cli {
    #[command(flatten)]
    pub out: OutputArgs,
    #[command(subcommand)]
    pub command: Command,
}

/// Size discipline, the same for every command (Bureau §6.2).
#[derive(Args, Debug, Clone, Default)]
pub struct OutputArgs {
    /// Return at most N list items (default 20; `jobs claim`: how many to claim, default 1)
    #[arg(long, global = true, value_name = "N")]
    pub limit: Option<usize>,
    /// Keep only these keys of each item, comma-separated
    #[arg(long, global = true, value_name = "A,B")]
    pub fields: Option<String>,
    /// Continue a truncated list from its next_cursor
    #[arg(long, global = true, value_name = "CURSOR")]
    pub cursor: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Print the CLI version as JSON
    Version,
    /// Database: migrations and status
    #[command(subcommand)]
    Db(DbCmd),

    // --- Technical Design commands -----------------------------------------------------------
    /// Run source connectors; store new candidates
    Discover {
        /// Only this source
        #[arg(long)]
        source: Option<String>,
    },
    /// Score, cluster and select stories for a run (the Budget)
    Plan {
        #[arg(long)]
        run: String,
    },
    /// Download a story's source files and API data
    Fetch { story: String },
    /// Parse a story's snapshots into data points and text blocks
    Parse { story: String },
    /// Build the brief for a story
    Brief { story: String },
    /// List the component index, one line per component
    Components,
    /// One component's props, data shape and example
    Component {
        #[arg(value_name = "TYPE")]
        component_type: String,
    },
    /// Related pages and entities for a story
    Related { story: String },
    /// Store a Copy (page draft) for a story
    Save { story: String, file: String },
    /// Render a story's page bundle
    Render { story: String },
    /// Run the code checks on a story's page
    Check { story: String },
    /// Publish a story's page (G tier)
    Publish { story: String },
    /// Load a published page for revision
    Update { page: String },
    /// Delete working files for published items
    Prune,
    /// Run and coverage summary
    Stats,
    /// Home page curation (Frontpage)
    #[command(subcommand)]
    Home(HomeCmd),

    // --- Bureau command groups -----------------------------------------------------------------
    /// Chief: preset runs and their status
    #[command(subcommand)]
    Run(RunCmd),
    /// Dispatch: the job queue
    #[command(subcommand)]
    Jobs(JobsCmd),
    /// Ghost: contracts for every AI step
    #[command(subcommand)]
    Ai(AiCmd),
    /// Contacts: the source registry
    #[command(subcommand)]
    Sources(SourcesCmd),
    /// Diary: the release calendar
    #[command(subcommand)]
    Calendar(CalendarCmd),
    /// Taster: stories
    #[command(subcommand)]
    Stories(StoriesCmd),
    /// Nose: scores and weights
    #[command(subcommand)]
    Scores(ScoresCmd),
    /// Roster: entities
    #[command(subcommand)]
    Entities(EntitiesCmd),
    /// Critic: evaluations
    #[command(subcommand)]
    Eval(EvalCmd),
    /// Huddle: the weekly review
    #[command(subcommand)]
    Review(ReviewCmd),
    /// Errata: reader error reports
    #[command(subcommand)]
    Corrections(CorrectionsCmd),
    /// Input and output JSON Schemas of a command
    Schema {
        /// The command, for example `jobs claim`; none lists them all
        command: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum DbCmd {
    /// Apply pending migrations (idempotent)
    Migrate,
    /// Applied and pending migrations
    Status,
}

#[derive(Subcommand, Debug)]
pub enum HomeCmd {
    /// Pages eligible for the home page
    Candidates,
    /// Store a draft curation
    Save { file: String },
    /// Code checks on a draft curation
    Check { draft: String },
    /// Render the home page locally with a draft
    Preview { draft: String },
    /// Publish an approved curation (G tier)
    Publish { draft: String },
    /// The live curation and its age
    Show,
}

#[derive(Subcommand, Debug)]
pub enum RunCmd {
    /// The preset catalogue
    Presets,
    /// The status board, or one run
    Status { run: Option<String> },
    /// Recent runs
    List {
        #[arg(long)]
        active: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum JobsCmd {
    /// Counts by pool and state, and recent jobs
    List {
        #[arg(long)]
        state: Option<String>,
        #[arg(long)]
        pool: Option<String>,
        #[arg(long)]
        run: Option<String>,
    },
    /// Discarded jobs grouped by error class and message
    Dead {
        #[arg(long)]
        class: Option<String>,
    },
    /// Claim ready jobs from a pool (Ghost); --limit sets how many
    Claim {
        #[arg(long)]
        pool: String,
        #[arg(long)]
        worker: String,
        /// Lease length; default 600 for ai, else the pool's longest kind lease
        #[arg(long, value_name = "SECS")]
        lease_seconds: Option<u64>,
    },
    /// Extend a running job's lease
    Heartbeat {
        job: i64,
        #[command(flatten)]
        holder: Holder,
        #[arg(long, value_name = "SECS")]
        lease_seconds: Option<u64>,
    },
    /// Mark a running job completed
    Complete {
        job: i64,
        /// Result JSON file, or - for stdin
        #[arg(long)]
        result: String,
        #[command(flatten)]
        holder: Holder,
    },
    /// Record a failure; the error class decides retry, snooze or discard
    Fail {
        job: i64,
        #[arg(long)]
        class: String,
        #[arg(long)]
        message: String,
        /// For rate_limited and ai_usage_limit: seconds to wait
        #[arg(long, value_name = "SECS")]
        retry_after: Option<u64>,
        #[command(flatten)]
        holder: Holder,
    },
    /// Re-enqueue a discarded job, or every discarded job of a class (more than 50 is gated)
    Retry {
        #[arg(required_unless_present = "class", conflicts_with = "class")]
        job: Option<i64>,
        #[arg(long)]
        class: Option<String>,
    },
}

/// The lease a claim handed out: heartbeat, complete and fail must name the worker and the lease id
/// (ADR 0003).
#[derive(Args, Debug)]
pub struct Holder {
    /// The worker name given to `jobs claim`
    #[arg(long)]
    pub worker: String,
    /// The job's `lease_id` from `jobs claim`
    #[arg(long, value_name = "LEASE_ID")]
    pub lease: uuid::Uuid,
    /// The job's `attempt` from `jobs claim`, recorded on the telemetry event; not part of the lease
    #[arg(long, value_parser = clap::value_parser!(i16).range(1..))]
    pub attempt: Option<i16>,
}

#[derive(Subcommand, Debug)]
pub enum AiCmd {
    /// Path of a job's input file
    Input { job: i64 },
    /// Validate and store a model output
    Accept { job: i64, file: String },
    /// JSON Schema of an AI step's output (A1–A12, brief)
    Schema { step: String },
    /// Apply a step's code fallback
    Fallback { job: i64 },
}

#[derive(Subcommand, Debug)]
pub enum SourcesCmd {
    /// All sources
    List,
    /// One source
    Show { source: String },
    /// Source health: green, amber or red with reasons
    Health {
        #[arg(long, default_value = "7d")]
        since: String,
    },
    /// One live discovery call, no writes
    Probe { source: String },
}

#[derive(Subcommand, Debug)]
pub enum CalendarCmd {
    /// Expected releases
    Upcoming {
        #[arg(long, default_value_t = 7)]
        days: u32,
        #[arg(long)]
        region: Option<String>,
    },
    /// Sync calendars from sources
    Sync,
    /// Releases that did not arrive
    Late,
}

#[derive(Subcommand, Debug)]
pub enum StoriesCmd {
    /// One story
    Show { story: String },
    /// A story's candidates
    Members { story: String },
    /// Stories similar to a text or candidate
    Similar {
        #[arg(long, required_unless_present = "cand", conflicts_with = "cand")]
        text: Option<String>,
        #[arg(long)]
        cand: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ScoresCmd {
    /// A story's score card (the Pitch)
    Explain { story: String },
    /// Current weights
    Weights,
    /// Fit new weights from labels; prints a diff
    Propose {
        #[arg(long)]
        from_labels: bool,
    },
    /// Record pairwise labels from Huddle
    Label {
        #[arg(long)]
        run: String,
        file: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum EntitiesCmd {
    /// Resolve a mention to an entity
    Resolve {
        text: String,
        #[arg(long)]
        kind: Option<String>,
    },
    /// One entity
    Show { entity: String },
    /// Mentions that did not resolve
    Unresolved,
}

#[derive(Subcommand, Debug)]
pub enum EvalCmd {
    /// Run an evaluation suite
    Run {
        suite: String,
        #[arg(long)]
        prompt_version: Option<String>,
    },
    /// An evaluation run's report
    Report { eval_run: String },
    /// Promote a prompt version (G tier)
    Promote {
        step: String,
        prompt_version: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum ReviewCmd {
    /// The Debrief: the week's statistics
    Weekly,
}

#[derive(Subcommand, Debug)]
pub enum CorrectionsCmd {
    /// Fetch new reader error reports
    Pull,
    /// One report
    Show { report: String },
    /// Resolve a report
    Resolve {
        report: String,
        #[arg(long, value_parser = ["update", "dismiss"])]
        action: String,
    },
}
