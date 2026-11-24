# `news` CLI

One Rust binary that gives agents, Bun code and Karthik every operation on the local system (Technical Design, "Agent toolbox"; Bureau Design §6). Decisions: [ADR 0002](../docs/adr/0002-cli-core.md) (core and contracts), [ADR 0003](../docs/adr/0003-dispatch-job-engine.md) (Dispatch).

## Conventions (every command)

- **Envelope.** stdout carries exactly one JSON object on one line: `{"ok":true,"data":…,"warnings":[],"truncated":false}` or `{"ok":false,"error":{"code","message","retryable","hint"?,"details"?,"text"?}}`. Schema: [`contracts/cli/envelope.schema.json`](../contracts/cli/envelope.schema.json). `--help` and `--version` print plain text: they are not command results.
- **Logs** go to stderr, filtered by `NEWS_LOG` (default `warn`). On a usage error clap's message and usage also go to stderr; stdout still gets the envelope.
- **Exit codes** follow the error code:

  | Code | Exit | | Code | Exit |
  | --- | --- | --- | --- | --- |
  | (ok) | 0 | | `rate_limited`, `upstream_unavailable` | 5 (retryable) |
  | `usage` | 2 | | `validation_failed` | 6 |
  | `not_found` | 3 | | `gated` | 7 |
  | `conflict`, `precondition`, `licence_blocked`, `budget_exceeded` | 4 | | `internal` | 10 |

- **Size.** Lists go under `data.items` in a deterministic order. `--limit N` (default 20, max 500), `--cursor` (pass back `data.next_cursor`) and `--fields a,b` work on every command; `truncated` is `true` when a list was cut. `news jobs claim` uses `--limit` as the number of jobs to claim (default 1) and refuses `--cursor` and `--fields`, so a caller always sees every lease it took.
- **Text for Karthik** is rendered by code into `data.text` or `error.text`; the `/bureau` session prints it verbatim.
- **Telemetry.** Each call writes one row to ClickHouse `news.events` (`stage = cli`, `event` = the command path such as `jobs.claim`) with `run_id` and `job_id` from `NEWS_RUN_ID` / `NEWS_JOB_ID` (and `error_class` / `attempt` on `jobs fail`, `heartbeat`, `complete` and `retry --class`), and exports its span over OTLP to `NEWS_OTEL_HTTP`; the event carries the `trace_id`. Telemetry never fails a call; `NEWS_TELEMETRY=off` turns it off.
- **Tiers** (Bureau §6.3): R, W, G, recorded per command in its contract (`x-news-tier`). H-tier actions are Board's, sent over the Hotline (ADR 0007), so no H-tier command has a CLI form and nothing checks the environment of the calling shell. A G-tier form (`publish`, `home publish`, `eval promote`, `jobs retry` above 50 jobs, …) returns `gated` here; it runs only as the authorised step of a live run, inside the service (Chief, #10).
- **Contracts.** `news schema <command…>` prints a command's input and output schema; `news schema` lists them. `news ai schema <A1…A12|brief>` prints an AI step's output schema. Both serve the files in [`contracts/cli/`](../contracts/cli/), embedded at build time.
- **Numbers.** Values are stored and returned exactly as published (`value_text`, verbatim `unit`, `currency`, `scale`, `period_label`). Nothing converts currencies, units or fiscal periods.

## Stubs

Commands whose subsystem is not built yet return `precondition` (exit 4) with a hint naming the issue that builds it: Contacts, Wire (with `prune`), Clerk #9 (the libraries are built; the commands follow the Bureau service, #10/#29) · Chief #10 · Taster #15 · Ticker (`stats`) #16 · render, save, Proof #17 · Ghost #18 · Critic, Huddle, `scores propose|label` #19 · Press, home, Rewrite #20 · Errata #21. Built now: `version`, `db`, `components`, `component`, `schema`, `ai schema`, `jobs *`, `calendar upcoming|sync|late`, `entities resolve|show|unresolved` (#12; ADR 0009), `plan --run` and `scores explain|weights` (#15; ADR 0012), and `brief` and `related` (#18). `alias add` and `merge` are H-tier, so they have no command form.

## Crate map

| Crate | Holds |
| --- | --- |
| `news` | The binary: clap tree (`src/cli.rs`), routing and stubs (`src/commands/`), the job-kind → handler binding (`src/registry.rs`), contracts embedded by `build.rs` |
| `news-core` | Envelope, error codes and exit codes, output shaping, the job contract (`JobInput`, `JobFailure`, `ErrorClass`, `Handler`), reference tokens, tiers, the write-API key guard |
| `news-store` | PostgreSQL connection (`search_path` = `NEWS_DB_SCHEMA`, public), embedded migrations, error mapping |
| `news-telemetry` | Ticker: the `news.events` writer, `tracing` with the OTLP exporter, and the analytics ingestion job (`traffic.pull`, a stub handler until #16) |
| `news-dispatch` | Dispatch: queue operations, kinds registry, backoff, rescuer, schedules, and the worker loop (`worker::run`) that the Bureau service hosts |
| `news-chief` | Chief (ADR 0008): the 11 presets as data, the run state machine, step drivers, the Lineup, pause and cancel, gates and `authorize()`, the Wrap, recovery and the status board |
| `news-standing` | Standing: Karthik's versioned settings, the key registry, history and the optimistic `set` |
| `news-rota` | Rota: preset schedules (missed slots, the late limit), quiet hours and the quota guard |
| `news-hotline` | The Hotline protocol: frames, the command table from `contracts/hotline/`, admission, the event bus, the server loop and the client |
| `news-service` | `news service run|install|uninstall|start|stop|status`: the singleton lock and its fence, recovery, the pools, Chief's and Rota's ticks, the Hotline handlers, `docker.*` and `service.update`. README in the crate |
| `news-win` | Every Win32 call (the pipe's security descriptor, peer identity, Job Objects, the SCM, Credential Manager, elevation, the log-on entry); the only crate allowed `unsafe`; stubs off Windows |
| `news-board` | Board, the egui desktop app (`news board`): the Hotline client, the three first screens, the token-generated theme, toasts, Credential Manager secrets. README in the crate; the window is `cfg(windows)` |
| `news-ledger` | Ledger rules, pure (ADR 0018): data points as published, `dp_id`, exact decimals and their style, the unit vocabulary, event keys, derived-point guards |
| `news-connectors` | The Stringers (`Connector` trait; SEC EDGAR, FRED, BLS and the second batch, ADR 0009): request descriptions and pure, versioned parsers. README in the crate; fixtures in `tests/fixtures/` |
| `news-contacts` | Source rows from `data/sources/*.toml`, the Contacts lint, `sync` into `sources`, the licence gate |
| `news-wire` | The Runner (robots.txt, token buckets, conditional GET, `Retry-After`, caps; `fake::FakeTransport` for tests), the Morgue, discover/fetch, `snapshots`/`fetch_state` |
| `news-clerk` | Rule-based classification and the taxonomy seed, candidates, storing data points (vintages, `supersedes`) and Roster step 1 entities, SimHash |
| `news-fixer` | Fixer (#18): the brief (`news_core::brief::Brief`, `brief.v1`) with its refs, context lines and size budget, A5 requests, the component index, and the cached prompt prefix per page type |
| `news-ghost` | Ghost (#18): runs every `ai.*` job as one lean `claude -p` (the §3.9 flags, an allow-listed environment, the AI Job Object on Windows), one A2 repair, usage-limit pauses, the per-run token ceiling and `ai_calls`. README in the crate; not yet installed by the service (needs `news ai accept`) |
| `news-proof` | Render and Proof (#17): the stored Copy and its validators, the templated fallback page, page bundles through Plate (`news save`, `news render`, `news ai accept`); the code checks (`news check`) come next |
| `news-rewrite`, `news-press`, `news-critic` | Subsystem skeletons: `ISSUE`, the job kinds they own and a stub `handle` (fails with class `bug`, "not built yet") |
| `news-taster` | Taster (#15): exact and SimHash duplicates, event keys, update detection, grey-zone A4 jobs, `stories.score_input` for Nose, H-tier `merge`/`split`; thresholds in `data/seeds/taster-v1.toml`; the embedding step is the `Joiner` seam (none until ADR 0011); the `story.detect` handler still refuses as not built |
| `news-nose` | Nose and Desk (#15, ADR 0012): story scores with the Pitch, the Budget (`run_plans`), scorer config v1 from `data/seeds/`; its job kinds still refuse as not built |
| `news-diary` | Diary (#12): ICS, FOMC HTML and seed-file calendars, derived earnings windows, the capture schedule, `calendar_events`; its job handlers refuse with `bug` until the Bureau service gives them a context |
| `news-roster` | Roster (#12): normalisation, the deterministic resolver, seed loaders (SEC, GLEIF, ROR, Wikidata), the unresolved queue, H-tier `add_alias` and `merge`; no job kinds |

Skeleton subsystem crates depend only on `news-core`; the #9 crates also use `news-ledger`, `news-connectors` and sqlx. `news` wires them together. Wire's and Clerk's job handlers still refuse with `bug` until the Bureau service gives them a context (#10, #29). A new handler replaces its crate's `handle`; a new job kind is added to `news_dispatch::KINDS` and to the owning crate's `KINDS` (a test checks both lists agree).

## Database and migrations

- `DATABASE_URL` (from `.env`; the CLI does not read `.env` itself, so `bun run …` scripts or the shell pass it) and `NEWS_DB_SCHEMA` (default `news`). Extensions (`vector`, `pg_trgm`, `unaccent`) are installed once per database by `infra/postgres/init/`; migrations check for them.
- `news db migrate` creates the schema if needed and applies pending migrations; `news db status` lists applied and pending ones. `bun run setup` runs the migration.
- Files are `cli/migrations/NNNN_name.sql`, embedded in the binary. Every statement is idempotent (`create … if not exists`, seeds with `on conflict do nothing`, constraints inline). A later block changes an earlier table only with guarded forms that re-run as no-ops: `alter table … add column if not exists`, `drop column if exists`, `drop constraint if exists`, `alter column … set default` / `drop default`; a table nobody needs any more goes with `drop table if exists`, after the columns that reference it. A unit test refuses unguarded creates and any other `alter table`, and an integration test re-runs every file on a migrated schema and compares the catalogue. Never edit an applied migration: add a new one.
- **Blocks**, so parallel PRs never collide on a number:

  | Block | Owner |
  | --- | --- |
  | 0001–0099 | Core: every TD and Bureau §14 table (#5); 0009 drops `start_tickets` and `runs.ticket_id` (#30) |
  | 0100–0199 | Contacts, Wire, Clerk, Ledger (#9) |
  | 0200–0299 | Chief (#10) |
  | 0300–0399 | Diary, Roster (#12) |
  | 0400–0499 | Taster, Nose, Desk (#15) |
  | 0500–0599 | Board, Ticker (#16) |
  | 0600–0699 | Render, Proof (#17) |
  | 0700–0799 | Fixer, Ghost (#18) |
  | 0800–0899 | Critic, Huddle (#19) |
  | 0900–0999 | Press, home, Rewrite (#20), Errata (#21) |

  Migrations run in version order, so **a lower block must never depend on a higher block's tables** (no foreign key, view, index or function on them): a fresh database applies the lower file first, and the table it names does not exist yet. A block may depend on the core block (0001–0099) and on lower blocks only; a link between two subsystems goes in the higher block of the two.

## Dispatch in short

Jobs live in `jobs`, pools in `pools` (seeded per §5.7; the service sizes `ai` from the Standing key `pools.ai.max_running`, or `quiet_hours.ai_slots` in reduced quiet hours), recurring jobs in `schedules` (UTC cron, seeded disabled). Ghost uses `news jobs claim|heartbeat|complete|fail`; `news jobs retry` takes a job or a class, and a class above 50 jobs is `gated` (Salvage's approved step or a Board action). Pausing and unpausing a pool are Board actions and have no CLI form. There is no `news work` command: the Bureau service hosts the worker loop (`news_dispatch::worker::run`; `WorkOptions::background` is the `net`, `cpu` and `docling` pools that run all the time, #10), and neither the loop nor any handler of those pools reads the write-API key (a sentinel test runs the loop to prove it). Details: ADR 0003.

## Component index

`news components` / `news component <name>` read `component-index.json` from `NEWS_COMPONENT_INDEX`, else `site/dist/component-index.json` in the nearest ancestor of the working directory. The shape is the site build's (#4, `buildComponentIndex` in `site/src/components/library.ts`), described by [`contracts/component-index.schema.json`](../contracts/component-index.schema.json): `{"version": 1, "budget", "tokens", "components": [{"name", "line", "tokens"}]}`. `news components` returns each `line`; `news component <name>` also returns the optional `description`, `props` (JSON Schema), `dataShape` and `example` when the index has them, and lists the absent ones in `missing` with a warning (#4/#7 add them). Anything else is `validation_failed`. Fixtures: `crates/news/tests/fixtures/component-index.json` (what #4 emits today) and `component-index.details.json` (with the optional fields).

## Tests

| Level | Command | What |
| --- | --- | --- |
| unit | `bun scripts/test-cli.ts unit` | In-crate tests; includes a walk of the whole command tree that every unbuilt leaf is a `precondition` stub |
| e2e | `bun scripts/test-cli.ts e2e` | The real binary: envelope and exit-code contract, golden help snapshots, every stub, contracts against the 2020-12 meta-schema, components, the telemetry event (a fake ClickHouse), that only the secrets guard names the write-API key |
| integration | `bun scripts/test-cli.ts integration` | `#[ignore]`d PostgreSQL tests against `news-postgres`: migrations (apply, re-run no-op), constraints, Dispatch concurrency (8 workers × 200 jobs), crash and rescue, snooze, unique keys, schedules, the sentinel run of the background pools. Each test makes and drops its own schema |

CI: `rust` (fmt, clippy `-D warnings`, test), `rust-postgres` (the integration level on pgvector), `cargo-deny`.
