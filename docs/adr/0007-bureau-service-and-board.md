# ADR 0007: Bureau service and Board

- Status: accepted (Karthik, 16 November 2026; open questions answered the same day: names, and H-tier actions from Board only)
- Date: 2026-11-16
- Issues: #10 (rescoped: Chief, the service, the Hotline, Rota and the queue) and the new "Bureau service and Board shell" issue (drafts in `docs/plans/2026-11-architecture-issue-changes.md`)
- Context documents: Bureau Design §1, §2.2, §5.5–§5.9, §6.3, §7.4, §8.1, §9.3, §11 (D17–D21); Technical Design "Local environment", "Agent orchestration", "Configuration and security"
- Supersedes: Bureau Design decisions D2, D8, D11 and D16; principle P8 as first written; the start tickets, the `UserPromptSubmit` prompt hook and the agent deny-list of the first Bureau design; ADR 0002 item 9; the `news work` and Task Scheduler parts of ADR 0003; the `/bureau curate home` wording in ADR 0005

## Context

The first Bureau design drove every workflow from a Claude Code chat: Karthik typed `/bureau …`, a prompt hook issued a one-time start ticket, and an interactive session looped on `news run next`. Background code jobs ran in `news work`, started by Windows Task Scheduler. A Bun web app was planned as the dashboard.

Karthik no longer wants to drive News by prompting. He wants one compiled Rust desktop app as the control surface for the whole project (runs, approvals, settings, analytics), backed by something that is always on. `claude -p` stays the AI harness. Runs must keep going with the app closed, and some runs should start on a schedule without him.

Two things follow. A chat session can no longer be the approval, so approvals and starts need a path that only Karthik can use. And a scheduled AI run is unattended, so P8 ("anything that calls a model runs only inside a preset Karthik started") has to change.

Everything else stays: PostgreSQL in Docker, the shared ClickHouse, the OTel collector, the whole cloud side, and the rules on numbers, units, fiscal periods and free tiers.

## Decision

1. **Bureau is a Windows service.** It logs on as Karthik's account, because `claude -p` uses his Claude login and Docker runs under his user. It hosts Chief, the Dispatch workers for every pool (replacing `news work` and Task Scheduler), Rota and Ghost. Runs continue with Board closed.
2. **Ghost is Rust, inside the service.** It spawns one `claude -p` per AI job, as before (Bureau Design §3.9). The 10-minute Bash limit and `--max-seconds` slicing go, because no chat session runs Ghost any more. The page renderer stays Bun + React/OpenUI, as a warm local render server that the service calls over HTTP on 127.0.0.1.
3. **Board is the desktop app.** Pure Rust on egui/eframe. It talks only to the service. It replaces the `/bureau` skill and the planned Bun web dashboard.
4. **One binary.** `news.exe` holds the CLI, the service (`news service run|install|uninstall`; updates are started from Board, `service.update`) and Board (`news board`), all built from the shared crates in `cli/`.
5. **Board opens minimised at log-on.** If the service is down, Board shows an offline banner, a "Start service" button and the last error. It follows the Windows light or dark setting, with a manual override. Its themes are generated from `design/tokens/tokens.json`.
6. **Laptop only.** Nothing is exposed to the network and there is no remote access. The Hotline rejects remote clients, and Plate and any other local listener bind to 127.0.0.1.
7. **`/bureau` is removed entirely:** the skill, the `UserPromptSubmit` prompt hook, start tickets (`start_tickets`), plain-English intent mapping (`news run intent`), `/bureau yes`, `news run hook-prompt`, `news run ticket`, and the agent deny-list built around them.
8. **One control path: the Hotline, a named pipe; high-tier actions from Board only.** Start, approve, request changes, reject, cancel, pause, resume, queue changes, Standing changes and every other H-tier action are accepted only over the Hotline and only from Board. Karthik's terminal (the `news` CLI as a Hotline client) may read status and use the low-risk tiers only (R over the Hotline; R and W in-process, as before). There is no `CLAUDECODE` guard. The security model is below. Approvals stay bound to `subject_sha`, and the G-tier rule stays. A run's origin is `board` or `schedule`.
9. **The `news` CLI stays** as the AI's toolbox inside `claude -p` (read and run-scoped commands only) and as Karthik's terminal tool for status and debugging. It sends no H-tier action.
10. **Heavy-slot collisions queue instead of refusing.** A scheduled heavy run waits for the slot. It is skipped, and the skip noted, if it would start more than about 2 hours late (a setting, starting at 2 hours). A manual Start while the slot is busy offers "Queue it" or "Pause the running one and start this". Board lists the queue and can cancel entries. This supersedes D16.
11. **Settings and secrets.** Settings live in Standing, a versioned PostgreSQL table: every change records who (always `board`), when, and the old and new value. Each run snapshots the settings it started with. Secrets live in Windows Credential Manager under Karthik's account; Board writes them there and nothing secret crosses the pipe. `.env` keeps only bootstrap values: how to reach PostgreSQL, ClickHouse and the OTel collector.
12. **Install and update.** `news service install` registers the service under Karthik's account; Windows asks for his password, which only he types. `news service uninstall` removes it. Both are run by Karthik in his own terminal because they come before the service exists; they are Windows administration, not Hotline actions. An update, started from Board (`service.update`), waits for a safe point (no AI call or publish in flight), swaps the binary, restarts the service, and live runs resume from PostgreSQL.

Scheduling (Rota), the Lineup, quiet hours, missed schedules, the usage-limit pause and the quota guard are part of the same change and are specified in Bureau Design §5.6, §5.9 and §7.4.

**Names** (Karthik, 16 Nov 2026; newsroom roles): **Rota**, the scheduler; **Hotline**, the control pipe; **Plate**, the warm Bun render server; **Lineup**, the heavy runs waiting for the slot; **Standing**, the versioned settings store, with the command `news standing`.

## The control interface

This is what Board, the CLI and the service build against. The contract files live in `contracts/control/` (one JSON Schema per command and per event, JSON Schema 2020-12), embedded in the binary like `contracts/cli/` (ADR 0002 item 5).

**The Hotline.** The pipe `\\.\pipe\news-hotline`. Messages are UTF-8 JSON, each preceded by its length as a little-endian `u32`, at most 1 MiB.

**Request and reply.** A request is `{"v": 1, "id": "<client id>", "cmd": "<command>", "args": {…}}`. The reply carries the same `id` and the CLI envelope of Bureau Design §6.2 (`ok`, `data` or `error`, code-rendered `text`). Error codes and their meaning are the CLI's; `gated` is returned for any command the caller may not send.

**Events.** `{"cmd": "subscribe", "args": {"topics": […]}}` turns the connection into a stream of `{"event": "<topic>", "at": "…", "data": {…}}`. Topics: `run` (status, step and progress changes), `gate` (a gate opened or was answered), `lineup`, `schedule` (fired, skipped, missed), `usage` (limit reached, reset, cap reached), `source` (health changed), `huddle` (questions ready), `service` (log lines at `info` and above), `standing`. Board raises toasts from these (below).

**Commands.** H-tier commands are accepted only from Board. The terminal may send the R commands; it has no CLI form of an H command.

| Command | Does | Tier | Accepted from |
| --- | --- | --- | --- |
| `status` | The status board: running, waiting, queued, due, problems, next | R | Board; terminal (`news run status`) |
| `run.show` | One run: steps, progress, budget, open gate with its summary and preview, Wrap | R | Board; terminal (`news run status <run>`) |
| `run.start` | Start a preset with arguments; returns `queue_offer` when the heavy slot is busy | H | Board only |
| `run.start_after_pause` | Pause the run holding the heavy slot at its next step boundary, then start this one | H | Board only |
| `gate.approve` / `gate.changes` / `gate.reject` | Answer an open gate; `args` carry `run`, `gate`, `round` and the `subject_sha` Board showed; `changes` needs a note | H | Board only |
| `run.pause` / `run.resume` / `run.cancel` | As in Bureau Design §8.1 | H | Board only |
| `lineup.list` / `lineup.cancel` | The Lineup | R / H | list: Board, terminal (`news run lineup`); cancel: Board only |
| `standing.get` / `standing.history` / `standing.set` | Read Standing and its history; change one or more keys in one new version | R / R / H | get and history: Board, terminal (`news standing show|history`); set: Board only |
| `huddle.answer` | Store Karthik's pairwise answers from the Huddle form | H | Board only |
| `content.*` | The other H-tier actions of Bureau Design §6.4: home curation rollback, withdraw or correct a page, `news sources enable|disable`, `news jobs retry` above 50, `news jobs pause|unpause`, `news scores adopt`, `news stories merge|split`, `news entities alias` | H | Board only |
| `docker.status` / `docker.start` / `docker.stop` | Health of the News containers and the shared ClickHouse; start or stop the News containers only | R / H / H | status: Board, terminal; start and stop: Board only |
| `service.log` | Recent service log lines | R | Board; terminal |
| `service.update` | As in decision 12 | H | Board only |

The read-only (R) and run-scoped (W) commands of Bureau Design §6.4 do not go through the pipe. They keep running in the calling process against PostgreSQL, so `claude -p` calls and tests use them as before.

## Security model

### The Hotline

- The service creates the pipe with a security descriptor whose DACL grants read and write to Karthik's SID only (the service's own account). There is no entry for Everyone, Users or Administrators.
- The first instance is created with `FILE_FLAG_FIRST_PIPE_INSTANCE`, so no other process can create the name first and pose as the service. Every instance uses `PIPE_REJECT_REMOTE_CLIENTS`, so the pipe cannot be reached over the network.
- On each connection the service reads the client's process id (`GetNamedPipeClientProcessId`) and checks, by impersonating the client, that its token is Karthik's SID. Anything else is disconnected.
- **H-tier actions only from Board.** For an H-tier command the service also checks that the client process is Board: its image is the installed `news.exe` and it was started as `news board` (read from the client process), and it is not in an AI job (below). Board sends an H-tier command only when Karthik clicks the control for it; it has no script or command interface. Anything else gets `gated`, and the attempt is logged and shown in Board's Problems. So the terminal, a script, a development Claude Code session or a `claude -p` call can read status but cannot start, approve, cancel or change anything.
- Board and the CLI check the server side too: `GetNamedPipeServerProcessId` must name the running Bureau service's process, whose image is the installed `news.exe`. If not, they show the offline banner and send nothing.

### The spawn marker

- Every `claude -p` process the service starts is created suspended, assigned to a Windows Job Object that the service owns, then resumed. The job is created without `JOB_OBJECT_LIMIT_BREAKAWAY_OK` and `JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK`, so every process the call starts (a tool shell, a `news` command) is in the same job and cannot leave it. Being in that job is the marker.
- For every command except reads, the service checks the client process with `IsProcessInJob` against its AI jobs. A client in one gets `gated` (even a copy of Board, which a tool shell could otherwise start), and the attempt is logged and shown in Board's Problems. Read and run-scoped commands from marked processes run as normal: they are the AI's toolbox.
- The child also gets `NEWS_SPAWNED_BY=<run>:<job>` in its environment. That label is for logs and telemetry only; the check never relies on it, because an environment variable can be removed by the process that holds it.
- Page calls get no tools (`--tools ""`, Bureau Design §3.9). Drafting calls (the Frontpage curator A8 and Rollcall's proposals A11) take inputs built by code and get no tools either; if the spike shows one needs a tool, it gets only the read-only `news` commands its step needs. So an injected instruction has no shell with which to look for a way out of the job.

### Approvals and starts

- Approvals are bound to `subject_sha` as before: Board sends the hash of the plan, draft or diff it displayed, and the service records the decision only if it equals the hash of what the gate holds now. A changed draft needs a new answer.
- Every approval records its origin, always `board`. The G-tier rule of Bureau Design §5.5 is unchanged, except that an ungated step is now authorised by the run's start: a Start from Board, or a schedule Karthik set in Board.
- A schedule is a setting. Creating or changing one goes through `standing.set`, from Board only, so it carries the same protection as an approval, and its version is recorded on every run it starts.

### Residual risks

- Any other software running as Karthik's user can open the Hotline and read status, as before. To act, it would have to drive Board itself (for example by synthetic clicks) or replace the installed `news.exe`: this protects against a model acting on its own or on injected text, not against malware on the laptop.
- A Claude Code development session runs as Karthik and is not in a marked job. Its agents can use the CLI to read status and run R and W commands, but cannot send any H-tier action, because those are accepted only from Board (Karthik's answer, 16 Nov 2026; there is no `CLAUDECODE` guard). An agent that started `news board` and drove it with synthetic input would still be acting as Karthik's user; development agents are not given tools for desktop automation.
- A process that can read the PostgreSQL credentials could write rows directly. The bootstrap values in `.env` are on Karthik's disk, as before.
- If the spike shows that a marked process can start a process outside its job through another service (for example WMI), the drafting calls must stay tool-less.

## Notifications

A Windows service runs in session 0 and cannot show toasts, so Board raises them from the event stream: a gate waiting, a run failed, a usage limit reached, a source turned red, Huddle questions ready. Clicking one opens Board on that item (`news board --open <kind>:<id>`). If Board was not running, it shows what it missed from the status board when it opens.

## What this supersedes

| Superseded | By |
| --- | --- |
| P8 "Unattended means code only" | P8 as rewritten in Bureau Design §1: a Start or a schedule Karthik set is the approval to start; gated presets still stop and wait in Board |
| D2 (one user-invoked `/bureau` skill driving Chief) | Decisions 1, 3, 7 and 8 above; Bureau Design D17 |
| D8 (optional background worker via Task Scheduler) | Decision 1: the service hosts the Dispatch workers, always on; Bureau Design D17 |
| D11 (model for the preset session) | There is no session. The drafting steps' models are per-step settings, starting at Sonnet 5.5 |
| D16 (refuse, never queue, a second heavy run) | Decision 10; Bureau Design D20 |
| Start tickets, the prompt hook, the agent deny-list for G- and H-tier commands | Decision 8 and the security model above; Bureau Design D18 |
| ADR 0002 item 9 (the `CLAUDECODE` guard) | H-tier actions from Board only, the Hotline and the spawn marker; amendment below |
| ADR 0003, `news work` started by Task Scheduler (`infra/windows/news-work.task.xml`) | The Dispatch workers hosted in the service; amendment below |
| ADR 0005 context: Frontpage started with `/bureau curate home` | Frontpage on Karthik's schedule or from Board; amendment below |

## Amendments to apply on other branches

ADRs 0001–0006 are on unmerged branches. Apply these there, as an "Amended by ADR 0007" note at the end of each record, so the original text stays readable. No change to ADR 0001, 0004 or 0006.

**`docs/adr/0002-cli-core.md`** (branch `phase-5/cli/core`)

- Header: add `- Amended by: ADR 0007 (item 9; item 2's command tree)`.
- Item 9, new wording: "**Tiers.** H-tier commands are actions only Karthik takes, and only from Board: the Bureau service accepts them over its named pipe (the Hotline) only from the installed `news.exe` running as `news board`, and never from a process it spawned for `claude -p` (ADR 0007). The CLI has no H-tier form; in the terminal it reads status and runs R and W commands. There is no `CLAUDECODE` check. The real boundary stays in Chief (approvals bound to `subject_sha`, the G-tier rule)."
- Item 2, add a sentence: "The `news run intent`, `news run hook-prompt` and `news run ticket` stubs are removed, and `news service run|install|uninstall` and `news board` are added; service updates are a Board action, not a CLI command (ADR 0007)."
- Item 6, add a sentence: "`start_tickets` is dropped and Standing, a versioned settings table, is added in a later migration block (ADR 0007); the `runs` constraints change as in Bureau Design §5.5."

**`docs/adr/0003-dispatch-job-engine.md`** (branch `phase-5/cli/core`)

- Header: add `- Amended by: ADR 0007 (the background worker)`.
- The **`news work`** bullet, new wording: "**Workers in the service.** The Bureau service hosts the Dispatch workers for every pool (ADR 0007). The `net`, `cpu` and `docling` pools run all the time; `ai`, `render` and `publish` jobs exist only inside a live run. Only Press reads the write-API key, from Windows Credential Manager, inside a step the G-tier rule authorises. `news work` and `infra/windows/news-work.task.xml` are retired; the rescuer and the schedule tick run inside the service every 30 seconds and at start."
- The **Salvage and Karthik** bullet, new wording: "`news jobs retry` re-enqueues discarded jobs with fresh attempts; above 50 at once it runs only as Salvage's gated step or as an H-tier action from Board. `news jobs pause|unpause` are H tier, from Board only."
- The **Schedules** bullet, add: "Preset schedules are settings and are evaluated by Rota, the service's scheduler (Bureau Design §5.6); this table keeps the code-only recurring jobs."
- Tests paragraph: replace "a live `news work` round with a sentinel `NEWS_WRITE_API_KEY` that never reads it" with "a live round of the service's background pools with a sentinel write-API key that is never read".

**`docs/adr/0005-home-curation.md`** (branch `phase-6/workers/data-api-mcp`)

- Context, first sentence, new wording: "The home page is curated every 2–3 days by the Frontpage preset, which runs on Karthik's schedule or from Board and waits for his approval in Board (TD "Home page curation", Bureau Design 8.6, ADR 0007)."

**`docs/adr/README.md`** (created by #4 on `phase-1/site/renderer`): add the index line

`| [0007](0007-bureau-service-and-board.md) | Bureau service and Board | accepted | 2026-11-16 |`

## Consequences

- Runs no longer need a chat window, a Bash time limit or a session model. Ghost is Rust, so the job rules live in one language.
- Karthik can be away while runs happen. The quota guard, quiet hours and auto-publish only on a Proof pass (Bureau Design §5.6, §7.4, §8.3) are what keep unattended runs safe.
- The service runs with Karthik's rights. A password change stops it logging on; Board's offline banner shows the last error, and he re-enters the password with `news service install`.
- The `news` binary grows a GUI dependency (egui, eframe, egui_plot). Board is a separate subcommand, so CLI calls inside `claude -p` do not start it.
- Board's screens are built in order (Bureau Design §8.1): the shell with Runs & approvals, Settings & schedules and Docker health comes right after wave 1 (#4, #5, #6) and before the connectors (#9); later screens land as their features are built.
- Builders: service work (Chief, the pipe, control security, Rota, Ghost) is built by Opus and gets an Opus security review. Board screens are built by Sonnet, reviewed by Opus, and a different Opus agent checks the token styling.
- The `dashboard/` skeleton and its workspace entries are removed by the Board shell PR.
- ADR numbers planned in the issues move: #9's ledger and provenance record, planned as 0007, moves to 0018.

## Alternatives considered

- *Keep `/bureau` and add a service.* Two control surfaces, and the prompt hook was never confirmed to see the typed text. Karthik chose one.
- *A Bun web dashboard on localhost.* A second language for the control surface and a listening port to protect; Karthik chose Rust only.
- *Tauri or another webview app.* Lets the design system's React charts be reused, but adds a webview and a JavaScript build to the binary. Karthik chose "leaner, Rust code only".
- *An HTTP or gRPC API on 127.0.0.1 for control.* Any local process, under any user, can connect to a loopback port, and a browser page can reach it. A named pipe can be locked to one user and to local clients.
- *Task Scheduler plus the CLI, no service.* No always-on host for runs, queue, usage-limit resume or events for toasts.
- *Run the service as LocalSystem or a virtual account.* `claude -p` needs Karthik's Claude login and his Docker access.
- *An environment variable as the spawn marker.* A process can remove its own environment variables; Job Object membership cannot be dropped without breakaway rights.

## Open questions for Karthik

- **Development sessions.** Answered 16 Nov 2026: H-tier actions are accepted only from Board; the terminal reads status and uses the low-risk tiers; no `CLAUDECODE` guard.
- **Deferred settings** (he sets them in Board later): the Bureau daily and weekly token cap and the reserve, quiet-hour times and whether quiet hours hold heavy runs or run them at reduced parallelism, and the default schedule times.

## To confirm in the spike (in #10)

- `claude -p` uses Karthik's subscription login when started by the service under his account (session 0, his profile), and the shape of the usage-limit error and its reset time.
- Job Object membership is inherited by every process a `claude -p` call starts, and `IsProcessInJob` sees it from the service.
- The shared ClickHouse and Docker are reachable from the service as Karthik's user.

## References (not re-checked on 16 Nov 2026)

- Named pipe security and access rights: https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights
- `CreateNamedPipe` (`PIPE_REJECT_REMOTE_CLIENTS`, `FILE_FLAG_FIRST_PIPE_INSTANCE`): https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea
- `GetNamedPipeClientProcessId`: https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid
- Job Objects: https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects ; `IsProcessInJob`: https://learn.microsoft.com/en-us/windows/win32/api/jobapi/nf-jobapi-isprocessinjob
- Service user accounts: https://learn.microsoft.com/en-us/windows/win32/services/service-user-accounts
- Credential Manager, `CredWriteW`: https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credwritew
- egui and eframe: https://github.com/emilk/egui
