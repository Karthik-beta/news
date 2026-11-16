# Issue changes for the Bureau service and Board (November 2026)

Drafts only. Nothing here has been filed or edited on GitHub. Karthik reviews them, then the issues are edited or filed (gh CLI is fine for this repository; show the drafts first). The decision behind them is [ADR 0007](../adr/0007-bureau-service-and-board.md).

Karthik's answers of 16 Nov 2026 are applied throughout: the names **Rota** (scheduler), **Hotline** (control pipe), **Plate** (warm render server), **Lineup** (heavy runs waiting for the slot) and **Standing** (settings store, `news standing`); and **H-tier actions only from Board**. The terminal CLI over the Hotline reads status and uses the low-risk tiers only; there is no `CLAUDECODE` guard.

## Summary

| Issue | Change |
| --- | --- |
| #4 renderer | No change. Review checks that Plate, the warm local render server, binds 127.0.0.1 and can be called by the service |
| #5 Rust core | No change to the open PR. A follow-up issue (below) removes start tickets, `news run intent\|hook-prompt\|ticket` and the `CLAUDECODE` guard, and moves `news work` into the service |
| #6 (PR #26) cloud layer | No change; Board's content actions and Deploy use what it builds |
| #9 Wire, Contacts, Clerk, Ledger | Waits for the service step; its ADR becomes `docs/adr/0018-ledger-provenance.md` (was 0007) |
| #10 Chief | Rescoped: Chief, the service host, the Hotline, Rota and the Lineup, with no `/bureau`. Moves to wave 1b, before #9 |
| New | "Build the Bureau service and Board shell", wave 1b, after #10 |
| #16 | Board is an egui screen in `news.exe`: Analytics (step 4) and Content & sources (step 3); Rollcall's proposals are a `claude -p` draft approved in Board; depends on #18 and #19, so it moves to wave 6b, after #19 and before #21 |
| #18 | Ghost is Rust inside the service; no `--max-seconds`; Opus builds it |
| #19 | Huddle is a Board form with no AI; Audit and Huddle started from Board |
| #20 | Frontpage's curator is a `claude -p` draft; approval in Board; scheduled Editions |
| #21 | Errata, published-content actions, cloud usage, Deploy trigger and dev status in Board (step 5); launch runs from Board |
| #7, #8, #11–#15, #17 | No change beyond wording: #17 may say "on 127.0.0.1" for Plate |
| Board steps 2–5 | Step 2 goes with #18 or its own issue; step 3 and 4 into #16; step 5 into #21 (below) |

**Waves.** A new wave 1b sits between wave 1 (#4, #5, #6) and wave 2: #10, then the new issue. #9 now depends on both. #7 and #8 do not depend on the service and can run alongside wave 1b, within the limit of three coding agents. Later waves keep their numbers, except #16, which moves to a new wave 6b after #18 and #19.

**ADR numbers.** 0007 is now "Bureau service and Board". #9's "ledger and provenance" record moves to 0018 (the next free number after #21's 0017). #10 keeps 0008, retitled "Chief in the Bureau service". #5's ADRs 0002 and 0003 and #6's ADR 0005 get the amendments listed in ADR 0007.

---

## #10, rescoped

**Title:** Build Chief inside the Bureau service, with the Hotline, Rota and the Lineup

### Goal

Make Chief, running inside the always-on Bureau service, the only way a preset runs: starts, gate answers and every other H-tier action only over the Hotline (a named pipe locked to Karthik's user) and only from Board, approvals bound to `subject_sha`, the G-tier rule in code, one heavy run at a time with a queue, schedules that count as Karthik's approval, and a Wrap at the end of every run (Bureau §5.5, §5.6, §6.3, §8.1, §9.3; ADR 0007).

### Phase

Phase 5 — Rust news CLI and connectors

### Areas

bureau, cli

### Builder model

Opus 5.5

### Size

L (about 2–5k lines)

### Wave and dependencies

Wave 1b · #5 (and the #5 follow-up below)

### Scope

- Spike first (findings in ADR 0008): `claude -p` started by a process running as a Windows service under Karthik's account uses his subscription login; the usage-limit error's shape and reset time; Job Object membership reaches every process a `claude -p` call starts and `IsProcessInJob` sees it; the shared ClickHouse and Docker are reachable from the service.
- `news service run`: the service host process (also runnable in the foreground for development and tests). It hosts Chief, Rota and the Dispatch workers for every pool (the rescuer and the code-job schedule tick every 30 seconds), and continues live runs from PostgreSQL after a restart. Windows registration, install and update are in the new issue.
- Preset declarations in Rust for all 11 presets (Edition, Flash, Catchup, Frontpage, Rollcall, Salvage, Followup, Errata, Audit, Huddle, Reprint): steps and their kinds (`code`, `ai_batch`, `draft`, `form`, `approval`), arguments, heavy flag, writes, publishes, gates. Step handlers bound to subsystem handlers (stubs until their PRs land).
- Migrations: `runs` as in Bureau §5.5 (`origin` `board|schedule`, `schedule_key`, `settings_version`, `queued`, `skipped`, `start_by`, `paused_reason`), `run_steps` kinds, `approvals.origin` `board`; drop `start_tickets` if the #5 follow-up has not.
- The Lineup: scheduled runs queue with `start_by` (setting `queue_late_limit`, starting at 2 hours) and end as `skipped` with a Wrap after it; a manual start returns `queue_offer`; "pause the running one and start this"; resume and approved gates queue when the slot is busy; `lineup.cancel`.
- Rota: preset schedules read from settings (Standing itself comes with the new issue; until then a fixture table), missed slots (latest only, none if the next is under `missed_slot_limit`), quiet hours (Karthik's setting: "don't start", where due heavy runs wait until quiet hours end, or "reduced parallelism", where they start on time with fewer `ai` slots; either way within `queue_late_limit`), the quota guard (daily and weekly Bureau cap and reserve; values are Karthik's settings).
- Usage limits: pause the `ai` pool and the run until the reset time, then resume on their own.
- The Hotline (`contracts/control/`, ADR 0007): DACL for Karthik's SID only, `FILE_FLAG_FIRST_PIPE_INSTANCE`, `PIPE_REJECT_REMOTE_CLIENTS`, client token check, length-prefixed JSON with the CLI envelope, the command table and the event topics. H-tier commands accepted only from Board: the client must be the installed `news.exe` running as `news board`; anything else gets `gated`. The spawn marker: an AI Job Object without breakaway; everything but reads from processes in it returns `gated` and is logged. No `CLAUDECODE` guard.
- `news run presets|status|lineup|list` and `news standing show|history` as in Bureau §6.4 (the terminal's read-only view); start, gate answers, pause, resume, cancel, `lineup.cancel` and `standing.set` exist only as Hotline commands that Board sends, with no CLI form. The status board (Running, Waiting for you, Queued, Due, Problems, Next; the 10 next-action rules as data; code-rendered `text`).
- `authorize()` for every G-tier command (the three conditions of §5.5): an ungated step is authorised by the run's start; approvals are consumed when their step finishes; outbox rows are inserted in the same transaction as their check. One Wrap function.
- CI test in `scripts/tests/`: `bureau-single-entry` (only Chief's sources and tests insert into `runs`).

### Out of scope

Windows service registration, install, update, settings UI and secrets, and Board (the new issue). The bodies of preset steps (#15–#21). Ghost (#18). No `/bureau` skill, prompt hook, start tickets or `.claude/settings.json` deny rules.

### Agents

- Branch `phase-5/bureau/chief-service`.
- `spike` (Opus): the service spike.
- `architect` (Opus): ADR 0008 and `contracts/control/`.
- `feature-builder` run on Opus: Chief, Rota, the Lineup and the Hotline.
- `test-writer` (Sonnet): golden outputs.
- `security-reviewer` (Opus) and `reviewer` (Opus), then CodeRabbit. Karthik merges.

### Done when

- [ ] ADR 0008 records the spike: login under the service account, the usage-limit error and reset time, Job Object inheritance, ClickHouse and Docker reachability. If the login fails, the PR stops and Karthik decides the fallback.
- [ ] An H-tier command from any client other than Board (the CLI, a test script, a process in the AI Job Object) exits 7 (`gated`) and is logged; the same command from a test Board client succeeds. A read from the terminal succeeds (tests with a fake `claude`).
- [ ] The Hotline refuses a client under another user, and a second server instance cannot be created (tests).
- [ ] An approval for a changed draft is refused (`gated`), and a consumed approval cannot authorise a second step (tests).
- [ ] A table test runs every G-tier command outside a live run and expects exit 7.
- [ ] Lineup tests on a virtual clock: a scheduled heavy run waits and starts when the slot frees; one past `start_by` ends `skipped` with a Wrap; a manual start returns `queue_offer`; "pause the running one" frees the slot at the next step boundary; a stalled-run pause revokes the leases first and a late completion from a revoked lease is rejected; `lineup.cancel` works.
- [ ] Scheduler tests on a virtual clock: a missed slot runs once, latest only, and not when the next slot is under the limit; quiet hours hold a heavy run in "don't start" mode and start it with fewer slots in "reduced parallelism" mode; the quota guard stops new starts at the cap.
- [ ] A usage-limit reply pauses the run and it resumes on its own at the reset time (fake `claude`).
- [ ] A golden test reproduces the Bureau §8.1 example status board from a fixture database.
- [ ] A database constraint test shows every terminal status has a Wrap. After the service is killed between steps, it resumes without repeating finished steps.
- [ ] `bureau-single-entry` passes in CI.
- [ ] The security review is posted with no open high findings.

### Docs

- `docs/adr/0008-chief-in-the-service.md`; `contracts/control/README.md`; `cli/crates/news-chief/README.md`.
- Bureau §5.5, §5.6 and §8.1 if the build differs from the design.

_Drafted with AI assistance (Claude); reviewed by Karthik before filing._

---

## New issue: Build the Bureau service and Board shell

**Title:** Build the Bureau service and Board shell

### Goal

Install Bureau as a Windows service and give Karthik Board, the egui desktop app in `news.exe`, with its first three screens: Runs & approvals, Settings & schedules and Docker health, so every later feature lands in Board as it is built (ADR 0007 decisions 1–12; Bureau §8.1 screens step 1).

### Phase

Phase 5 — Rust news CLI and connectors

### Areas

bureau, cli, design

### Builder model

Sonnet 5.5

### Size

L (about 2–5k lines)

### Wave and dependencies

Wave 1b · #5, #10

### Scope

- `news service install|uninstall`, run by Karthik in his own terminal: registers the service under his account (Windows asks for his password; the agent never handles it), sets restart on failure, and removes it. Update, started from Board (`service.update`), waits for a safe point (no AI call or publish in flight), swaps the binary, restarts, and runs resume from PostgreSQL.
- Standing: the versioned PostgreSQL settings table (who, always `board`; when; old and new value), keys from TD "Configuration" (the former environment variables), preset schedules, quiet hours, the Bureau cap and reserve, models per step, sources on or off; `standing.get|history|set` over the Hotline (set from Board only); `news standing show|history` in the terminal. `.env` keeps only bootstrap values.
- Secrets: Board writes them to Windows Credential Manager under Karthik's account; the service reads them when a step needs one; nothing secret crosses the pipe or is stored in PostgreSQL.
- `news board` (egui/eframe): opens minimised at log-on (a log-on entry for Karthik's user); talks only to the service over the Hotline, checking the server is the installed service; offline banner with "Start service" and the last error; follows the Windows light or dark setting with a manual override; themes generated from `design/tokens/tokens.json` by a build step, never by hand.
- Screens (Bureau §8.1, step 1):
  - Runs & approvals: the status board; Start forms built from `news run presets`; pause, resume, cancel; gate answers (Approve, Request changes with a note, Reject) bound to the `subject_sha` shown; the queue offer; run detail and the Wrap; the Lineup.
  - Settings & schedules: every setting and its history; schedules; secrets entry.
  - Docker health: the News containers with Start and Stop; the shared ClickHouse health-checked only.
- Toasts raised by Board from the service's event stream (gate waiting, run failed, usage limit, source red, Huddle due); clicking one opens Board on that item (`news board --open <kind>:<id>`).
- Remove the `dashboard/` skeleton and its workspace, CI, Dependabot and README entries.

### Out of scope

Chief, the Hotline server, Rota and the Lineup (#10). Board screens steps 2–5 (#16, #18, #21 and later issues). Any network listener.

### Agents

- Branch `phase-5/bureau/service-board-shell`.
- `feature-builder` run on Opus: install, update, Standing and Credential Manager (service work, ADR 0007).
- `feature-builder` (Sonnet): Board and its three screens.
- `security-reviewer` (Opus): install, secrets, the Hotline client and the Board-only check.
- `reviewer` (Opus); a different Opus agent checks the token styling (`news-design-system`).
- CodeRabbit. Karthik merges.

### Done when

- [ ] Karthik runs `news service install`, types his password, and the service starts at boot under his account; `uninstall` removes it (manual check, recorded on the PR).
- [ ] An update started from Board during a fixture run waits for the safe point, restarts, and the run continues without repeating finished steps (test).
- [ ] Every Standing change writes a new version with who, when, old and new value, and a run records the version it started with (tests).
- [ ] A secret written through Board is in Credential Manager and appears in no PostgreSQL table, log line or Hotline message (sentinel test).
- [ ] Board starts, approves, requests changes on and cancels a fixture run through the service, and the same actions sent by the CLI are refused (`gated`); with the service stopped it shows the offline banner, and "Start service" starts it.
- [ ] Board's colours, type and spacing come only from the generated token theme (a lint fails on literal colours in Board code); light, dark and the manual override checked by the styling reviewer.
- [ ] A fixture gate raises a toast, and clicking it opens Board on that gate.
- [ ] `dashboard/` is gone and `bun run typecheck`, `bun run test` and CI pass without it.
- [ ] The security review is posted with no open high findings.

### Docs

- `cli/crates/news-board/README.md`; `cli/crates/news-service/README.md`.
- TD "The Bureau service and Board" and "Settings" if the build differs; `README.md` layout and commands; `CLAUDE.md` Layout (drop the `dashboard/` note).

_Drafted with AI assistance (Claude); reviewed by Karthik before filing._

---

## #5 follow-up issue

**Title:** Retire start tickets and the CLAUDECODE guard; move news work into the service

### Goal

Bring the Rust core in line with ADR 0007 once #5 is merged: no start tickets, no prompt-hook commands, no environment guard, and the background worker hosted by the service instead of Task Scheduler.

### Phase

Phase 5 — Rust news CLI and connectors

### Areas

cli

### Builder model

Opus 5.5

### Size

M (about 1–2k lines)

### Wave and dependencies

Wave 1b · #5 (before #10 starts, or folded into #10 if Karthik prefers)

### Scope

- Migration: drop `start_tickets`; drop `runs.ticket_id`; `runs.origin` and `approvals.origin` become the checked enums of Bureau §5.5 (or leave the `runs` changes to #10 and only drop the ticket table here).
- Remove `news run intent`, `news run hook-prompt` and `news run ticket`, their contracts in `contracts/cli/` and their golden help entries. `news run next` and `news run submit` are retired too, because Chief advances runs inside the service and drafts arrive through `news ai accept` (confirm with Karthik; the spec names only the first three).
- Remove the `CLAUDECODE` check (ADR 0002 item 9 as amended) and the CLI forms of the H-tier commands (`news run start|approve|changes|reject|resume|pause|cancel`, `news home rollback`, `news jobs pause|unpause`, `news sources enable|disable`, `news scores adopt`, `news stories merge|split`, `news entities alias`): H-tier actions are Board actions over the Hotline (#10). Where a command also runs as Chief's G-tier step, keep it for that path only.
- Move `news work`: the worker loop becomes a library entry point that `news service run` hosts (#10); the `news work` command and `infra/windows/news-work.task.xml` are removed; the "never reads the write-API key" sentinel test moves to the service's background pools.
- `.env.example`: keep only bootstrap values (PostgreSQL, ClickHouse, OTel); the rest are listed as settings keys in a comment pointing at TD "Settings".
- Apply the ADR 0007 amendments to `docs/adr/0002-cli-core.md` and `docs/adr/0003-dispatch-job-engine.md` if the #5 branch has merged without them.

### Out of scope

The service, the Hotline and Chief (#10); Board (new issue).

### Agents

- Branch `phase-5/cli/retire-tickets`.
- `feature-builder` run on Opus; `test-writer` (Sonnet) for the golden help; `reviewer` (Opus); CodeRabbit.

### Done when

- [ ] Migrations apply to an empty database and to one with #5's schema; `start_tickets` is gone (test).
- [ ] The golden `news --help` snapshot no longer lists `intent`, `hook-prompt` or `ticket` (and `next`, `submit` if agreed).
- [ ] No source file mentions `CLAUDECODE` (CI grep), and the golden help lists no H-tier command.
- [ ] `news work` and the Task Scheduler template are gone; the background-pool sentinel test passes from the library entry point.
- [ ] CI's `rust` job is green.

### Docs

- `cli/README.md`; ADRs 0002 and 0003 amendment notes; `.env.example`.

_Drafted with AI assistance (Claude); reviewed by Karthik before filing._

---

## #9: small edit (ADR renumbered)

- **Wave and dependencies:** `Wave 2 · #5` → `Wave 2 · #5, #10, and "Build the Bureau service and Board shell"`.
- **Agents:** `architect (Opus): ADR 0007.` → `architect (Opus): ADR 0018.`
- **Docs:** `docs/adr/0007-ledger-provenance.md` → `docs/adr/0018-ledger-provenance.md`.

---

## #16: edits

- **Title:** "Build Ticker and Board, and wire the Rollcall preset" → "Build Ticker and Board's Content and Analytics screens, and wire Rollcall".
- **Goal:** replace "show it on the local Board (localhost only)" with "show it in Board, the egui desktop app in `news.exe` (Analytics and Content & sources, Bureau §8.1 steps 3 and 4)". Replace "TD 'Local analytics dashboard'" with "TD 'Board'".
- **Wave and dependencies:** `Wave 5 · #8, #9, #10` → `Wave 6b · #9, #10, #18, #19, "Build the Bureau service and Board shell"`: Rollcall's proposals call `claude -p` through Ghost (#18) and use the A11 skill (#19), so #16 moves to a new wave 6b, after #19 (wave 6) and before #21 (wave 7) (#8 was for the React chart components, which Board no longer uses).
- **Scope, Board bullet:** replace "Board (`dashboard/`), drawn with the design system's chart components and server-rendered" with "Board screens in `cli/` (egui, egui_plot charts styled by the token theme): Content & sources (stories and scores, pages, source health, the Diary, dead jobs and retry) and Analytics (the panels below, read from ClickHouse)". Keep the panel list; add "Bureau usage against the cap, the reserve and the total (Bureau §7.4)".
- **Scope, Rollcall bullet:** replace "`sources health` → `probe` → session step (`news run submit`) → `actions` approval → gated disable" with "`sources health` → `probe` → proposals draft (A11, `claude -p` through Ghost, no tools) → `actions` approval in Board → gated disable. Rollcall runs weekly on Karthik's schedule and waits at its gate".
- **Agents:** `component-tester (Opus): Board's panels and charts` → `reviewer (Opus) for Board; a different Opus agent checks the token styling`.
- **Done when:** replace "Board listens only on 127.0.0.1; a test fails on any other bind address" with "Board opens no network listener (test)". Replace "golden HTML tests" with "golden view-model tests (every value computed by code)". Replace the component-testing line with "A different Opus agent signs off the token styling of both screens in light and dark". Rollcall line: "the summary is submitted" → "the proposals draft is shown in Board".
- **Docs:** `dashboard/README.md` → `cli/crates/news-board/README.md` (screens).

## #18: edits

- **Goal:** add "Ghost runs inside the Bureau service, in Rust (ADR 0007)".
- **Wave and dependencies:** add "Build the Bureau service and Board shell".
- **Builder model:** stays Opus 5.5 (service work).
- **Scope, lean-call spike:** keep the flag set, settings isolation and cache measurements; remove "the shape of the usage-limit error" (confirmed in #10's spike).
- **Scope, Ghost:** replace "Ghost (`pipeline/run-ai.ts`)" with "Ghost (Rust, a crate hosted by the service)"; replace "heartbeats; `--max-seconds`" with "heartbeats"; add "spawns each `claude -p` suspended into the service's AI Job Object, then resumes it (the spawn marker)"; replace "On a usage limit, pauses the `ai` pool and the run" with "On a usage limit, pauses the `ai` pool and the run until the reset time and resumes on its own"; add "feeds `ai_calls` for the quota guard and Board's AI-call records".
- **Scope, new bullet:** "Board screens step 2: the live service log and one record per `claude -p` call (step, model, tokens, time, result, error; brief and output viewable)" (Sonnet, with a different Opus agent checking token styling), unless Karthik prefers its own issue.
- **Out of scope:** replace "A8 and A11, which run in the interactive session (#10, #19)" with "the A8 and A11 skills' prose (#19)".
- **Agents:** `feature-builder (Sonnet): Ghost` → `feature-builder run on Opus: Ghost`; add `feature-builder (Sonnet): Board step 2`.
- **Done when:** remove "`--max-seconds` stops claiming and lets in-flight calls finish"; replace "a usage-limit reply makes `news run next` return `paused_usage`" with "a usage-limit reply pauses the run and it resumes at the reset time"; add "a `claude -p` child and a process it starts are both in the AI Job Object (test)".
- **Docs:** `pipeline/README.md` → `cli/crates/news-ghost/README.md`; `pipeline/README.md` keeps the call settings and prefixes.

## #19: edits

- **Scope, skills:** `news-home-curation (A8)` stays, as a `claude -p` draft skill with inputs built by code; add "the A11 Rollcall proposals skill".
- **Scope, Huddle:** replace "The Huddle preset end to end" with "Huddle as a Board form with no AI: about 30 pairwise questions chosen by code, the Search Console status and the 5-page reading sample entered by Karthik; answers over the Hotline from Board (`huddle.answer`) and stored with `news scores label`; the Debrief and the proposed changes shown in Board, one approval per change. Huddle runs weekly on Karthik's schedule and a toast reminds him".
- **Done when:** replace "Karthik runs `/bureau run evals suite=writer`" with "Karthik starts Audit (`suite=writer`) from Board". Replace "Audit and Huddle run end to end on a fixture database with fake `claude`" with "Audit and Huddle run end to end on a fixture database with fake `claude` and a test Board client answering the form over the Hotline".
- **Areas:** add `cli` if the Huddle form lands in `cli/crates/news-board`.

## #20: edits

- **Wave and dependencies:** add "Build the Bureau service and Board shell".
- **Scope, home commands:** `rollback` (H-tier) → "rollback (H-tier, a Board action over the Hotline; no CLI form)".
- **Scope, preset step bodies:** add "Frontpage's curator as a draft step (A8 through Ghost, inputs built by code, up to two fix rounds), its preview shown in Board, and the `publish` gate answered in Board; scheduled Editions publish the pages that pass Proof, with `review_plan` per schedule".
- **Done when:** in the Frontpage line, "(approve, changes, reject)" → "(approve, request changes with a note, reject, all from a test Board client over the Hotline)"; add "Value-only revisions published at step 10 are only those planned in the Budget at step 4 (test)"; add "A scheduled Edition on a fixture schedule runs end to end without a gate, and one with `review_plan` on stops at `plan`". "`rollback` is H-tier only" → "`rollback` is accepted only from Board".

## #21: edits

- **Scope, prerequisites:** add "the read-only tokens for Board: Cloudflare analytics and usage; AWS IAM read-only for DynamoDB and CloudWatch metrics; a GitHub fine-grained token for this repository with read access to pull requests, issues and checks, and **Actions: read and write**, which is needed to dispatch the deploy workflow (Karthik approved Board running it). Actions write also allows other Actions operations, such as re-running or cancelling workflow runs; Board only dispatches the deploy workflow on `main`. Karthik enters the tokens in Board (Credential Manager)".
- **Scope, CD:** add "the deploy workflow accepts `workflow_dispatch` on `main`, so Board can trigger it".
- **Scope, pullers:** "feed Board" → "feed Board's cloud usage view (step 5)".
- **Scope, new bullet:** "Board screens step 5: published-content actions (withdraw or correct a page, home curation rollback, Errata reports); cloud free-tier usage and alarms; Deploy buttons that trigger the GitHub Actions deploy workflow for `main` and show its history (no local wrangler deploys); development status (PRs, CI, issues; read-only)".
- **Scope, Errata:** "A gate for each report" → "A gate for each report, answered in Board".
- **Scope, launch:** "`/bureau run daily target=10`, then a 100-page run" → "an Edition with `target=10` started from Board, then a 100-page run, then a scheduled Edition". "The first `/bureau curate home`" → "The first Frontpage, approved in Board".
- **Agents:** add `feature-builder (Sonnet): Board step 5`; `security-reviewer (Opus)` also covers the tokens and the Deploy trigger; a different Opus agent checks the token styling.
- **Done when:** "`/bureau run daily` publishes 100+ pages" → "An Edition started from Board publishes 100+ pages". "Karthik approves a `/bureau curate home` draft" → "Karthik approves a Frontpage draft in Board". Add "Deploy from Board runs the workflow on `main` and its history shows the run; the security review records each token's permissions, including what GitHub's Actions write allows beyond dispatch".

## Board screens steps 2–5

| Step | Screen | Where |
| --- | --- | --- |
| 1 | Runs & approvals, Settings & schedules, Docker health | The new issue |
| 2 | Service log and AI-call records | #18 (or its own issue, if Karthik prefers) |
| 3 | Content & sources | #16 |
| 4 | Analytics | #16 |
| 5 | Whole project: content actions, cloud usage, Deploy, dev status | #21 |

## Only Karthik can do

- Type his Windows password at `news service install`.
- Create the Cloudflare and AWS read-only tokens and the GitHub token (read access plus Actions read and write, to dispatch Deploy) and enter them in Board.
- Approve any paid service (none is planned).
- Choose a name for the spawn marker, if he wants one (the other names are settled).
- Set the cap, reserve, quiet-hour and schedule values in Board later (the development-session question is settled: H-tier actions only from Board).
