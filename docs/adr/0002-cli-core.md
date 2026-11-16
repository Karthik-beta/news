# ADR 0002: The `news` CLI core and its contracts

- Status: accepted
- Date: 2026-11-15
- Issue: #5 (Phase 5, wave 1)
- Amended by: ADR 0007 (item 9; item 2's command tree)
- Context documents: Technical Design, "Agent toolbox" and "Data model"; Bureau Design §3, §6, §9.2, §11 (D3), §14

## Context

Every later laptop PR (connectors, Chief, Ghost, Proof, Press) builds on one Rust binary. Agents call it from Claude Code and Bun calls it from the pipeline, so its output, errors and exit codes are a contract. Decision D3 settled on a CLI, not an MCP server, with exportable schemas. Numbers on pages must come from source data by code, never converted.

## Decision

1. **One binary, many crates.** `news` holds the clap tree and routing. Shared rules live in `news-core` (no I/O), PostgreSQL in `news-store`, telemetry in `news-telemetry`, the queue in `news-dispatch`, and each Bureau subsystem has its own crate that depends only on `news-core`. A subsystem PR touches its crate and a line of routing, nothing else.
2. **Full command tree now, stubs where unbuilt.** Every command group of the TD and Bureau §6.4 parses today. An unbuilt command returns `precondition` (exit 4) with the issue that builds it, so agents and skills can be written against the final surface and fail predictably.
3. **The envelope.** stdout carries exactly one compact JSON object per call, even for usage errors (clap's text goes to stderr). `--help` and `--version` stay plain text. Error codes are the eleven of §6.2; exit codes follow them (2 usage, 3 not found, 4 conflict/precondition/licence/budget, 5 retryable, 6 validation, 7 gated, 10 internal). `retryable` is true exactly for exit 5.
4. **Generic size discipline.** Lists go under `data.items`; `--limit`, `--fields` and `--cursor` are global options applied after the command runs, so every command gets them the same way. Cursors are offsets (`c<n>`), opaque to callers; commands still bound their queries by `offset + limit + 1`.
5. **Contracts are files.** `contracts/cli/` holds the envelope, one schema per command (input = the arguments as an object, output = `data`, plus `x-news-tier`, `x-news-status`, `x-news-errors`) and the AI step outputs (`brief.v1`, `page-draft.v1`, A3–A7, A9, A12). The binary embeds them at build time and serves them with `news schema` / `news ai schema`, so there is one copy. Tests validate every file against the JSON Schema 2020-12 meta-schema. `news home *` and `news publish` contracts come with #20.
6. **One schema, migrations in blocks.** All tables live in one PostgreSQL schema (`news`, or a throwaway one in tests) reached through `search_path`; migration files are unqualified, idempotent statement by statement, embedded with sqlx, and numbered in per-subsystem blocks (cli/README.md). Migrations 0001–0099 create every TD and Bureau §14 table with the constraints of §3–§5, including `jobs_unique_live`, `jobs_ready`, `runs_one_per_preset`, `runs_one_heavy`, the HNSW index on `halfvec(384)` and the Wrap check. The TD's `clusters`, `queue` and `ledger` are created as the TD describes, alongside their Bureau counterparts, until a later ADR retires them.
7. **Telemetry that never breaks a call.** One `news.events` row per call (`stage = cli`, `event` = command path, `run_id` / `job_id` from the environment, `error_class` / `attempt` from the arguments of the `jobs` commands that report on one attempt, `trace_id` / `span_id` from the call's span; a call that fails to parse writes a `usage` event too), written over HTTP to ClickHouse after the command finishes; spans exported over OTLP/HTTP. A dead ClickHouse or collector costs at most a 2-second timeout and is logged at `debug`. `003_ticker.sql` adds the §9.2 columns and the `ai_calls`, `fetches`, `scores`, `check_results` and `runs` tables.
8. **Secrets.** The CLI never prints `DATABASE_URL` or ClickHouse credentials. `NEWS_WRITE_API_KEY` is named in exactly one function (`news_core::secrets::write_api_key`); `news work` disables it before anything else, and a test fails if any other crate names the variable or any crate but Press calls the function.
9. **Tiers as a guardrail.** H-tier commands refuse in a Claude Code tool shell (`CLAUDECODE`), returning `gated`. The real boundary stays in Chief (start tickets, approvals bound to `subject_sha`, the G-tier rule, #10) and the deny rules. The check is an environment variable, so an agent that runs `env -u CLAUDECODE news …`, `CLAUDECODE= news …` or an equivalent (`unset`, `Remove-Item Env:`, a wrapper script) gets past it: the deny rules must cover those forms for every H-tier command, not only the bare command.

   Notes for #10 (Chief):
   - `news run hook-prompt` is the `UserPromptSubmit` hook, which Claude Code starts with `CLAUDECODE` set. It must do its work in-process (match the prompt, issue the start ticket) and must not shell out to other H-tier `news` commands, which would refuse; its own H-tier check has to recognise the hook invocation instead of relying on `CLAUDECODE`.
   - `news jobs retry` above 50 jobs is H tier today (refused in an agent shell). Under Salvage's approved step it must become G: allowed when the run's approval for that step covers it, refused otherwise.

## Consequences

- Golden tests pin the help text, every stub and the envelope; renaming a command or flag shows up in review.
- Telemetry and OTLP add `reqwest` and the OpenTelemetry crates; no TLS is compiled in, because every service is on loopback.
- The `version` command now prints the envelope (`data.news`), not a bare object.
- Shaping after the fact means a command that returns a huge list does the work before it is cut; list commands therefore bound their own queries.

## Alternatives considered

- *Per-command shaping options*: duplicated, and easy to forget on a new command.
- *Hand-written schemas in Rust (schemars)*: the contracts would only exist inside the binary; files are reviewable, diffable and usable from Bun.
- *A schema per subsystem*: cross-subsystem joins (pages ↔ data points ↔ stories) would need qualified names everywhere; one schema per database is simpler for one user.

## Amended by ADR 0007

- **Item 9, new wording.** **Tiers.** H-tier commands are actions only Karthik takes, and only from Board: the Bureau service accepts them over its named pipe (the Hotline) only from the installed `news.exe` running as `news board`, and never from a process it spawned for `claude -p` (ADR 0007). The CLI has no H-tier form; in the terminal it reads status and runs R and W commands. There is no `CLAUDECODE` check. The real boundary stays in Chief (approvals bound to `subject_sha`, the G-tier rule).
- **Item 2, added.** The `news run intent`, `news run hook-prompt` and `news run ticket` stubs are removed, and `news service run|install|uninstall` and `news board` are added; service updates are a Board action, not a CLI command (ADR 0007).
- **Item 6, added.** `start_tickets` is dropped and Standing, a versioned settings table, is added in a later migration block (ADR 0007); the `runs` constraints change as in Bureau Design §5.5.

## Applied in code by #30

- **Item 9.** The `CLAUDECODE` check and `tier::require_human` are gone, and with them every CLI form of an H-tier action: `news run start|approve|changes|reject|resume|pause|cancel`, `news home rollback`, `news jobs pause|unpause`, `news sources enable|disable`, `news scores adopt`, `news stories merge|split`, `news entities alias add|merge` and `news eval label` (an H-tier command in Bureau Design §6.4 that the issue did not list; its contract was H too). Their contracts, stubs and golden help entries are removed, and a repository test fails if any source file names the variable. `news jobs retry --class` above 50 jobs answers `gated` to every caller until Chief's `authorize()` and the Board action exist (#10). `news run next` and `news run submit` are retired too: Chief advances runs inside the service, and drafts arrive through `news ai accept` (ADR 0007).
- **Item 2.** `news run intent|hook-prompt|ticket` and `news work` are removed from the tree, with their contracts in `contracts/cli/commands/`.
- **Item 6.** Migration `0009_retire_start_tickets.sql` drops `runs.ticket_id` and `start_tickets` (guarded, so it re-runs as a no-op). The `origin` values and constraints of `runs` and `approvals` are left to Chief's reshape of those tables (`0201`, #10).
- **Item 8.** `news work` no longer exists to forbid the key; the sentinel test now runs the worker loop's library entry point (ADR 0003).
- **Item 1.** `traffic.pull` and its stub handler moved from `news-board` into `news-telemetry` (Ticker), and the `news-board` crate was removed, so the name is free for the desktop app (#29). The binding of job kinds to handlers is now in the `news` package's library target (`news::registry`).
