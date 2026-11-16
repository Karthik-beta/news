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

- **Size.** Lists go under `data.items` in a deterministic order. `--limit N` (default 20, max 500), `--cursor` (pass back `data.next_cursor`) and `--fields a,b` work on every command; `truncated` is `true` when a list was cut. `news jobs claim` uses `--limit` as the number of jobs to claim (default 1).
- **Text for Karthik** is rendered by code into `data.text` or `error.text`; the `/bureau` session prints it verbatim.
- **Telemetry.** Each call writes one row to ClickHouse `news.events` (`stage = cli`, `event` = the command path such as `jobs.claim`) with `run_id` and `job_id` from `NEWS_RUN_ID` / `NEWS_JOB_ID`, and exports its span over OTLP to `NEWS_OTEL_HTTP`; the event carries the `trace_id`. Telemetry never fails a call; `NEWS_TELEMETRY=off` turns it off.
- **Tiers** (Bureau §6.3): R, W, G, H, recorded per command in its contract (`x-news-tier`). H-tier commands refuse to run in a Claude Code tool shell (`CLAUDECODE` set) with `gated`; this is a guardrail, the boundary is the deny rules, start tickets and approvals (§5.5).
- **Contracts.** `news schema <command…>` prints a command's input and output schema; `news schema` lists them. `news ai schema <A1…A12|brief>` prints an AI step's output schema. Both serve the files in [`contracts/cli/`](../contracts/cli/), embedded at build time.
- **Numbers.** Values are stored and returned exactly as published (`value_text`, verbatim `unit`, `currency`, `scale`, `period_label`). Nothing converts currencies, units or fiscal periods.

## Stubs

Commands whose subsystem is not built yet return `precondition` (exit 4) with a hint naming the issue that builds it: Contacts, Wire, Clerk #9 · Chief #10 · Diary, Roster #12 · Taster, Nose #15 · Board (`stats`) #16 · render, save, Proof #17 · Fixer, Ghost #18 · Critic, Huddle #19 · Press, home, Rewrite #20 · Errata #21. Built now: `version`, `db`, `components`, `component`, `schema`, `ai schema`, `jobs *`, `work`.

## Crate map

| Crate | Holds |
| --- | --- |
| `news` | The binary: clap tree (`src/cli.rs`), routing and stubs (`src/commands/`), the job-kind → handler binding (`src/registry.rs`), contracts embedded by `build.rs` |
| `news-core` | Envelope, error codes and exit codes, output shaping, the job contract (`JobInput`, `JobFailure`, `ErrorClass`, `Handler`), reference tokens, tiers, the write-API key guard |
| `news-store` | PostgreSQL connection (`search_path` = `NEWS_DB_SCHEMA`, public), embedded migrations, error mapping |
| `news-telemetry` | Ticker: the `news.events` writer and `tracing` with the OTLP exporter |
| `news-dispatch` | Dispatch: queue operations, kinds registry, backoff, rescuer, schedules, the `news work` loop |
| `news-contacts`, `news-wire`, `news-clerk`, `news-diary`, `news-roster`, `news-taster`, `news-nose`, `news-fixer`, `news-ghost`, `news-proof`, `news-rewrite`, `news-press`, `news-chief`, `news-critic` | Subsystem skeletons: `ISSUE`, the job kinds they own and a stub `handle` (fails with class `bug`, "not built yet") |

Subsystem crates depend only on `news-core`; `news` wires them together. A new handler replaces its crate's `handle`; a new job kind is added to `news_dispatch::KINDS` and to the owning crate's `KINDS` (a test checks both lists agree).

## Database and migrations

- `DATABASE_URL` (from `.env`; the CLI does not read `.env` itself, so `bun run …` scripts or the shell pass it) and `NEWS_DB_SCHEMA` (default `news`). Extensions (`vector`, `pg_trgm`, `unaccent`) are installed once per database by `infra/postgres/init/`; migrations check for them.
- `news db migrate` creates the schema if needed and applies pending migrations; `news db status` lists applied and pending ones. `bun run setup` runs the migration.
- Files are `cli/migrations/NNNN_name.sql`, embedded in the binary. Every statement is idempotent (`create … if not exists`, seeds with `on conflict do nothing`, constraints inline). A later block changes an earlier table only with guarded forms that re-run as no-ops: `alter table … add column if not exists`, `drop column if exists`, `drop constraint if exists`, `alter column … set default` / `drop default`. A unit test refuses unguarded creates and any other `alter table`, and an integration test re-runs every file on a migrated schema and compares the catalogue. Never edit an applied migration: add a new one.
- **Blocks**, so parallel PRs never collide on a number:

  | Block | Owner |
  | --- | --- |
  | 0001–0099 | Core: every TD and Bureau §14 table (this PR, #5) |
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

Jobs live in `jobs`, pools in `pools` (seeded per §5.7; `ai` follows `NEWS_PARALLEL` on `news db migrate`), recurring jobs in `schedules` (UTC cron, seeded disabled). Ghost uses `news jobs claim|heartbeat|complete|fail`; Karthik uses `news jobs pause|unpause` and `retry` above 50 jobs. `news work --pools net,cpu,docling` is the optional background worker (`bun run work`; `infra/windows/news-work.task.xml` starts it at log-on); it refuses `ai`, `render` and `publish` (exit 2) and forbids itself the write-API key. `bun run work` (`scripts/work.ts`) starts the debug binary `bun run setup` builds with an allow-listed environment only: `DATABASE_URL`, `NEWS_DB_SCHEMA`, `NEWS_CH_*`, `NEWS_OTEL_HTTP`, `NEWS_LOG`, `NEWS_TELEMETRY`, plus what the OS needs to start a process. Details: ADR 0003.

## Component index

`news components` / `news component <name>` read `component-index.json` from `NEWS_COMPONENT_INDEX`, else `site/dist/component-index.json` in the nearest ancestor of the working directory. The shape is the site build's (#4, `buildComponentIndex` in `site/src/components/library.ts`), described by [`contracts/component-index.schema.json`](../contracts/component-index.schema.json): `{"version": 1, "budget", "tokens", "components": [{"name", "line", "tokens"}]}`. `news components` returns each `line`; `news component <name>` also returns the optional `description`, `props` (JSON Schema), `dataShape` and `example` when the index has them, and lists the absent ones in `missing` with a warning (#4/#7 add them). Anything else is `validation_failed`. Fixtures: `crates/news/tests/fixtures/component-index.json` (what #4 emits today) and `component-index.details.json` (with the optional fields).

## Tests

| Level | Command | What |
| --- | --- | --- |
| unit | `bun scripts/test-cli.ts unit` | In-crate tests; includes a walk of the whole command tree that every unbuilt leaf is a `precondition` stub |
| e2e | `bun scripts/test-cli.ts e2e` | The real binary: envelope and exit-code contract, golden help snapshots, every stub, contracts against the 2020-12 meta-schema, components, the telemetry event (a fake ClickHouse), the write-key guard |
| integration | `bun scripts/test-cli.ts integration` | `#[ignore]`d PostgreSQL tests against `news-postgres`: migrations (apply, re-run no-op), constraints, Dispatch concurrency (8 workers × 200 jobs), crash and rescue, snooze, unique keys, schedules, the `news work` sentinel. Each test makes and drops its own schema |

CI: `rust` (fmt, clippy `-D warnings`, test), `rust-postgres` (the integration level on pgvector), `cargo-deny`.
