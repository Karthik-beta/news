# ADR 0008: Chief in the Bureau service

- Status: proposed (architect, 16 November 2026; Karthik answered its three open questions the same day)
- Date: 2026-11-16
- Issue: #10 (Chief, the service host, the Hotline, Rota and the Lineup); touches #29 (service and Board shell) and #30 (retire start tickets)
- Builds on: ADR 0002 (CLI core), ADR 0003 (Dispatch), ADR 0007 (Bureau service and Board)
- Context documents: Bureau Design §5.4–§5.9, §6.2–§6.4, §7.4, §8.1, §9.3; the service spike `docs/spikes/bureau-service.md` (branch `spike/bureau-service`)
- Contracts: [`contracts/hotline/`](../../contracts/hotline/README.md)

## Context

ADR 0007 moved control of News from a Claude Code chat into an always-on Windows service with one control path, the Hotline, and one control surface, Board. #10 builds the heart of it: Chief (the run engine), Rota (the scheduler), the Lineup (the heavy-slot queue), the Hotline server, and the service host that runs them with the Dispatch workers. #29 builds install, update, Board and secrets on top; #30 removes the first design's start tickets, prompt hook and `CLAUDECODE` guard first.

The Rust core on `phase-5/cli/core` (#5) has the queue (`news-dispatch`), the store, the envelope and error codes, and a `runs`/`run_steps`/`approvals` schema shaped for the retired design (start tickets, no `queued`/`pausing`/`skipped`, origins `prompt_hook|terminal`). Its claim does not yet look at the run a job belongs to. The workspace forbids `unsafe`, and Windows control needs it.

This record fixes the interfaces the #10 builder, the #29 builder and later subsystem PRs build against, so they can work in parallel.

## Decision

### 1. What the service spike settled

From `docs/spikes/bureau-service.md` (branch `spike/bureau-service`, head 7413057, 16 Nov 2026; release builds, on a laptop whose CPU was saturated by other agents, so times are pessimistic):

| Question (ADR 0007, "To confirm in the spike") | Finding | What #10 does with it |
| --- | --- | --- |
| `claude -p` under the service's account uses Karthik's subscription login | Yes, modelled with a fresh log-on environment (`CreateEnvironmentBlock`, no inheritance) cut to the §9.3 allow-list, inside a job, at Medium and at Low integrity. Login lives in `%USERPROFILE%\.claude\.credentials.json`. Not yet run under the real SCM install (the agent was not elevated). A call took **30–80 s wall for about 2 s of API time** (start-up on a loaded CPU) | #29's first install records it on the PR. If the login fails there, #10's AI path stops and Karthik picks the fallback (the issue's rule). Leases for AI jobs stay 10 minutes; the start-up tax is measured on an idle laptop in the Phase 7 lean-call spike |
| Service log-on as Karthik | Unproven: install needs elevation (self-elevate through UAC), `SeServiceLogonRight` (`LsaAddAccountRights`), and his **Microsoft-account password, not his PIN**; passwordless sign-in is on (`DevicePasswordLessBuildVersion = 2`) | #29's risk, with the fallback in §9 ("If the service cannot log on") |
| Usage-limit error shape and reset time | **Not settled**: no limit was hit | One classifier function, `news_ghost::claude::classify` (§8.4 here), with a provisional rule: a non-zero exit with `is_error: true` and an unknown `subtype` is a usage limit with no known reset; Dispatch pauses `ai` for the default hour, then one call probes. The first real limit fixes the shape in that one function |
| Hotline cost | Server-side checks per connection (pid, image, command line, job, impersonation) **0.50–0.65 ms**; client open → server-image check → request → reply **1.6–3.9 ms**; the DACL read back holds only his SID | Checks run on every connection and every H command; no caching needed |
| Spawn cost | `CreateProcess` 81 ms, assign to job 0.07 ms, `NtResumeProcess` 0.03 ms; a Toolhelp thread walk to resume costs 195–390 ms | Resume with `NtResumeProcess` (or the thread handle from `CreateProcessAsUserW`), never Toolhelp |
| Board in `news.exe` | Go on eframe 0.37 (glow). Linking it adds 9 static GUI DLL imports, **+270 ms on every CLI call** unless delay-loaded; a console-subsystem `news board` started at log-on opens a console window unless the manifest sets `consoleAllocationPolicy` `detached` | #29's build rules (ADR 0007, "Amended after the spike") |
| Job Object membership reaches every process a call starts; `IsProcessInJob` sees it | Yes, three levels down and through `claude.exe` → Git Bash. `IsProcessInJob(p, NULL)` is useless (every desktop process is in some job): always pass the service's own job | The root marker job (§8.2) |
| A marked process cannot leave | Not through `CREATE_BREAKAWAY_FROM_JOB`, **but it can through WMI** (`Win32_Process.Create` runs the child outside the job, and its H-tier request was accepted) | Every `claude -p` starts at **Low integrity** inside the job: the kernel then refuses it the pipe, WMI refuses it process creation, and UIPI blocks input to Board (spike item 3). Drafting calls stay tool-less as well |
| ClickHouse and Docker from the service | Reachable with the allow-listed environment; Docker finds its context through `%USERPROFILE%\.docker` | Docker Desktop runs only while Karthik is signed in, so the service waits for PostgreSQL (§9). Decided (Karthik, 16 Nov 2026): no boot-time Docker engine; scheduled runs that fall while he is signed out wait for sign-in (§7.1) |
| Pipe security | `D:P(A;;GA;;;<SID>)` read back exactly; other SIDs and Low-integrity clients get error 5 from the kernel; a second `FILE_FLAG_FIRST_PIPE_INSTANCE` server gets error 5 | As specified in §8.2 |
| `news service run` started from a terminal | `StartServiceCtrlDispatcher` fails with 1063 at once | `news service run` falls back to console mode on 1063 |

Install facts (elevation, `SeServiceLogonRight`, the Microsoft-account password, delayed auto-start with restart on failure, install doubling as "change password") and the Board build rules belong to #29; ADR 0007's "Amended after the spike" section records them.

### 2. Crates

Five new crates; two existing crates change. Arrows are `depends on`.

```
news (binary)
 ├─▶ news-service ─┬─▶ news-hotline ──▶ news-win*
 │                 ├─▶ news-rota ──▶ news-chief ─┬─▶ news-dispatch ──▶ news-store
 │                 │                             └─▶ news-standing ──▶ news-store
 │                 └─▶ news-win*   (and directly news-chief, news-standing, news-dispatch)
 └─▶ subsystem crates (news-ghost, news-press, …) ──▶ news-core only
news-board (#29) ──▶ news-hotline   (never news-store or news-dispatch: Board talks only to the service)
every crate ──▶ news-core (no I/O)        * the only crate allowed `unsafe`
```

| Crate | Status | Holds |
| --- | --- | --- |
| `news-core` | changed | Adds `clock` (`Clock` trait, `SystemClock`, `ManualClock`), `spawn` (`Spawner` trait and `SpawnRequest`, implemented by the service), and `JobCtx` on `JobInput` (`cancel: CancellationToken`, `spawner: Arc<dyn Spawner>`). The `CLAUDECODE` check is gone (#30). Still no database, network or clock access of its own |
| `news-dispatch` | changed | The claim's run check (§4.3); `jobs.step_seq`; `cancel_run_jobs`, `revoke_run_leases`; heartbeat touches the run in its own statement (§4.4); the worker loop as a library (#30) with a per-job `CancellationToken`, a per-pool pre-claim hook and an outcome channel to Chief |
| `news-standing` | **new** | Standing: the versioned table, the key registry (types, defaults, docs), typed reads, `set` with optimistic concurrency, history (§10) |
| `news-chief` | filled (was a skeleton) | Preset declarations, the run state machine, step drivers, gates and approvals, `authorize()`, the Lineup, pause and cancel, the Wrap, recovery, the status board and its next-action rules, code-rendered text. No Windows code: it calls a `ProcessControl` trait the service implements |
| `news-rota` | **new** | The scheduler: preset schedules from Standing, missed slots, quiet hours, the quota guard. Implements Chief's `StartPolicy` and the `ai` pool's pre-claim hook |
| `news-hotline` | **new** | The protocol: frames, request/reply/event types, the command table (name, tier, who may send it), the contracts in `contracts/hotline/` embedded at build time and validated, the peer admission rule, the client (Board, CLI) and a transport-generic server loop |
| `news-win` | **new** | The one crate exempt from `unsafe_code = "forbid"`, because the pipe's DACL, client identity, Job Objects, Low-integrity tokens and `NtResumeProcess` exist only as raw Win32 and ntdll calls; keeping them in one crate keeps the memory-safety review to one place. Every Win32 call: the pipe's security descriptor, peer inspection (pid, token SID, image path, command line, integrity, job membership), Job Objects, Low-integrity process creation. Its own `[lints]` table: `unsafe_code = "deny"`, `#[allow(unsafe_code)]` per function with a `// SAFETY:` comment, safe functions out. Compiles to stubs returning `Unsupported` off Windows. Uses `windows-sys` 0.61 (already in the lock file). #29 adds the SCM, LSA and Credential Manager calls here too |
| `news-service` | **new** | `news service run`: startup, the singleton lock, recovery, the pools, Chief's and Rota's loops, the Hotline server's command handlers, the event bus, the log ring, `Spawner` and `ProcessControl` over `news-win` |

Rules:

- ADR 0002 item 1 ("each subsystem crate depends only on `news-core`") still holds for subsystem crates. `news-chief`, `news-rota`, `news-standing` and `news-service` are orchestration crates and may depend on the store and the queue. A unit test reads `cargo metadata` and fails if a subsystem crate depends on anything but `news-core`, if `news-board` depends on `news-store` or `news-dispatch`, or if any crate but `news-win` lacks `unsafe_code = "forbid"`.
- Step and job handlers are bound to their owning crates in the binary (`crates/news/src/registry.rs`), as job kinds are today, so `news-chief` never depends on a subsystem crate.
- Only `news-service` calls Chief's control functions (`ChiefControl`). `bureau-single-entry` (§12) checks that only `news-chief` inserts into `runs`.

### 3. The run state machine

Statuses are Bureau §5.5's. `pausing` keeps the heavy slot until the run's work has stopped. `waiting_approval` also covers a form step (Huddle waits for Karthik the same way).

| Status | Holds the heavy slot | Jobs claimable | Live (one per preset) |
| --- | --- | --- | --- |
| `queued` | no | no | yes |
| `running` | yes (if heavy) | yes | yes |
| `waiting_ai` | yes (if heavy) | yes | yes |
| `waiting_approval` | no | no | yes |
| `pausing` | yes (if heavy) | **no** | yes |
| `paused` | no | no | yes |
| `succeeded`, `failed`, `cancelled`, `skipped` | no | no | no; always with a Wrap |

Transitions. Anything not in this table is refused (`precondition`, exit 4). One function, `chief::transition(tx, run, from: &[Status], to, cause)`, performs every change; a table test checks every pair.

| From | To | Cause | Who |
| --- | --- | --- | --- |
| (new) | `running` | Start; light preset, or slot free and no rank-0 run `queued` (otherwise the slot counts as busy); `StartPolicy` allows | Board (`run.start`); Rota |
| (new) | `queued` | Heavy, slot busy, `if_busy: "queue"` | Board (`run.start`) |
| (new) | `queued` (rank 0) | Start after pausing the holder | Board (`run.start_after_pause`) |
| (new) | `queued` with `start_by` | Scheduled heavy run, slot busy or quiet hours "don't start" | Rota |
| (new) | `skipped` | Scheduled run whose preset is live, past its late limit (§7.1), or held by the quota guard | Rota |
| `queued` | `running` | Slot free, oldest by (`lineup_rank`, `queued_at`), `StartPolicy` allows; `start_by` cleared | Chief (Lineup tick) |
| `queued` | `skipped` | `start_by` passed | Chief (Lineup tick) |
| `queued` | `cancelled` | Cancel queued | Board (`lineup.cancel`, `run.cancel`) |
| `running` ⇄ `waiting_ai` | | The current step is `ai_batch` or `draft` (and back when it finishes) | Chief |
| `running`, `waiting_ai` | `waiting_approval` | An approval or form step opened its gates or form | Chief |
| `waiting_approval` | `running` | Every open gate of the step answered, or the form submitted; light, or slot free | Board (`gate.*`, `huddle.answer`) |
| `waiting_approval` | `queued` | Same, heavy, slot busy (no `start_by`) | Board |
| `running`, `waiting_ai` | `pausing` (`drain`) | Pause; pause the holder for a new start; usage limit; quota reserve; service update | Board (`run.pause`, `run.start_after_pause`); Chief; #29's update |
| `running`, `waiting_ai` | `pausing` (`now`) | Pause on a stalled run (`last_seen_at` older than `chief.stall_minutes`) | Board (`run.pause`) |
| `pausing` (`drain`) | `pausing` (`now`) | Pause again once it has stalled | Board (`run.pause`) |
| `running`, `waiting_ai`, `pausing` | `pausing` (`cancel`) | Cancel | Board (`run.cancel`) |
| `pausing` | `paused` | Its work has stopped (§4) | Chief |
| `pausing` (`cancel`) | `cancelled` | Its work has stopped; Wrap | Chief |
| `waiting_approval`, `paused` | `cancelled` | Cancel (nothing is running); Wrap | Board (`run.cancel`) |
| `paused` | `running` / `queued` | Resume (queued when heavy and the slot is busy) | Board (`run.resume`); Chief for `usage_limit` at `resume_at` and for `quota` when the guard clears |
| `running`, `waiting_ai` | `succeeded` | Last step done; Wrap | Chief |
| `running`, `waiting_ai` | `failed` | Preflight blockers, a step error, or a bug; Wrap | Chief |

`paused_reason` is `karthik | usage_limit | quota | update` (`quiet_hours` stays in the check for the design's sake but nothing sets it: quiet hours hold starts, never a running run). `stop_mode` (`drain | now | cancel`) is set exactly while `pausing`.

Pause takes effect at the **next safe boundary**: the end of an inline code step, or, for a step that fans out jobs, as soon as the jobs already running finish; no new job of the run is claimed once it is `pausing`. A Catchup chunk is a yield point inside its step. This is the design's "next step boundary" made precise: an Edition writing 112 pages pauses after the calls in flight, not after all 112. Resume re-enters the same step, whose finished jobs are skipped by key (§6.4).

### 4. Pausing, cancelling and the claim

#### 4.1 The procedure

1. **`pausing`**, in its own transaction: `update runs set status = 'pausing', stop_mode = $m, paused_reason = $r where run_id = $1 and status = any($from)`. From this commit on, no claim takes a job of the run (§4.3).
2. **Stop the work.**
   - `drain`: wait until the run has no job in `running` **and** its driver is not inside an inline code step (the driver checks for `pausing` only between steps and between yields, so an inline step always finishes first; woken by the worker outcome channel and the driver, polled every second). Only then does step 3 run and free the slot.
   - `now` and `cancel`: (a) `ProcessControl::terminate_run(run)` terminates the run's Job Object (every `claude -p`, Docling or renderer process of the run) and waits until it is empty; (b) cancel the run's worker tasks (their `CancellationToken`, then `abort`) and await every one, with no timeout. While any task has not stopped, the run stays `pausing` and keeps the slot; Board shows which job and for how long. (c) `PublishGuard::wait_idle(run)` waits for a publish already sent to return or time out (Press, #20; a no-op until then).
3. **Finish**, in one transaction, locking the run row first: move the run's `running` jobs to `retryable` with no holder (`lease_until`, `locked_by`, `lease_id` null; `run_after = $now`); then `paused`, or for `cancel` mark its live jobs `cancelled`, call Press's reconcile hook and write the Wrap (`cancelled`). The slot is free at this commit.

A worker that reports after step 3 holds a `lease_id` that no longer exists: its heartbeat, completion or failure is `conflict` (ADR 0003), so nothing it computed is recorded; whatever it wrote before is keyed (§5.8) and the re-run is safe.

#### 4.2 Lock order

Every transaction that touches more than one of these takes them in this order and never waits for an earlier one while holding a later one: **`pools` → `runs` → `gates`/`approvals` → `run_steps` → `jobs`**.

#### 4.3 The claim

`news_dispatch::queue::claim` gains Bureau §5.4's step 3 exactly:

```sql
and (j.run_id is null or exists (select 1 from runs r
       where r.run_id = j.run_id and r.status in ('running', 'waiting_ai') for share of r))
```

The claim holds the run row `FOR SHARE` until it commits; the pause's step 1 needs the row's update lock (`FOR NO KEY UPDATE` conflicts with `FOR SHARE`; `FOR KEY SHARE` would not, so it must be `FOR SHARE`). So a pause waits for a claim already holding the row, and a claim that starts after the pause has the row waits, re-checks the committed row under READ COMMITTED, sees `pausing` and takes nothing. Every claim of the run either committed before `pausing` (its jobs are `running`, and step 2 or 3 deals with them) or finds nothing. Holding job rows of another run while waiting for this run's row cannot deadlock: the pause's step 3 locks only this run's `running` jobs, which are never claim candidates.

The claim also stops claiming `ai` jobs when the pre-claim hook says so (§7.3).

#### 4.4 Heartbeat

Today `heartbeat` updates the job and the run's `last_seen_at` in one statement, which takes `jobs` before `runs` and could deadlock against step 3. It becomes two statements without a shared transaction: the job update (returning `run_id`), then `update runs set last_seen_at = now() where run_id = $1`.

#### 4.5 Restart while `pausing`

At start, before any worker claims (§9), recovery completes every `pausing` run: the old service's tasks died with its process, and its Job Objects killed their processes when their last handle closed (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, spike item 3), so step 2 has nothing to wait for except Press's `sent_unknown` rows, which it marks for reconciliation; then step 3. A pause is never reversed by a restart.

### 5. The Lineup and the heavy slot

- **The slot is the index.** `runs_one_heavy` (unique, `where heavy and status in ('running','waiting_ai','pausing')`) is the only arbiter. Chief tries the transition; a unique violation means "busy". No in-memory flag.
- **The slot counts as busy for a new heavy start while any rank-0 run is `queued`**, even if no run holds it: a direct `run.start` (or a scheduled fire) does not take it, and gets the same answer as for a busy slot, joining the Lineup behind the rank-0 run if queued.
- **`run.start`** with the slot busy and `if_busy` absent or `"offer"` returns `conflict` with `details.reason = "queue_offer"`, `details.blocking` (the holder, its step and progress) and `details.offers` (`["queue", "pause_running"]`, or `["queue"]` when the holder is already `pausing`), plus code-rendered `text`. `retryable` stays false (ADR 0002 item 3; the §8.1 example shows true, which this corrects). Board then sends `run.start` with `if_busy: "queue"`, or `run.start_after_pause` naming the holder it showed; if the holder has changed, that is `conflict` with a fresh offer.
- **`run.start_after_pause`** in one transaction: lock the named holder, check it is still the holder and `running`/`waiting_ai`, set it `pausing` (`drain`, `karthik`), insert the new run `queued` with `lineup_rank = 0`. The reply's `position` is the run's real place in the Lineup order below: rank-0 runs come before rank-1 runs, oldest first, so it is 1 unless an earlier rank-0 run is still queued. A resume or approval while the holder is pausing finds the slot busy and queues at rank 1, so it never overtakes. The new run takes the slot when the holder's pause completes, unless an earlier rank-0 run goes first.
- **Promotion.** On every Chief tick (1 s, and when a run leaves a slot-holding status): first end every `queued` run whose `start_by < now` as `skipped` with a Wrap; then, if `StartPolicy` allows, try the first `queued` heavy run by (`lineup_rank`, `queued_at`, `run_id`) whose `start_by` is null or not passed. Chief clears `start_by` on a run's first start, so a run that rejoins later is never late.
- **Rejoining.** An approval while the slot is busy, and a Resume while it is busy, insert nothing: the run moves to `queued` (rank 1, `queued_at = now`, `start_by` null).
- **Light presets never queue.** They start beside a heavy run or are refused (`conflict`, `preset_live`; `precondition` from `StartPolicy`).
- **`lineup.cancel`** ends a `queued` run as `cancelled` with a Wrap that says so. `run.cancel` on a queued run does the same.

### 6. Steps

#### 6.1 Declaration

All 11 presets are declared in `news-chief/src/presets/` as data, one file each:

```rust
pub struct PresetDecl {
    pub id: &'static str,                 // "edition"
    pub name: &'static str,               // "Edition"
    pub version: u16,                     // bumped when its steps change; part of every input hash
    pub heavy: bool,
    pub args: &'static [ArgDecl],         // name, type, required, default, doc: Board builds its forms from these
    pub writes: &'static [Writes],        // Pages | HomeCuration | SourceState | TaskDrafts | Jobs | EvalResults | Config
    pub publishes: bool,
    pub preflight: Preflight,             // services, sitrep sources, quiet hours, quota, heavy-dev warning
    pub steps: &'static [StepDecl],
    pub reference: &'static str,          // "Bureau §8.3"
}
pub struct StepDecl {
    pub id: &'static str,                 // "write"
    pub title: &'static str,              // "Write pages" (Board and code-rendered text)
    pub kind: StepKind,                   // Code | AiBatch | Draft | Form | Approval
    pub handler: &'static str,            // "edition.write"; bound in the binary's registry
    pub gate: Option<&'static str>,       // approval steps: "plan", or a per-item family "report", "change"
    pub changes_to: Option<&'static str>, // approval steps: the step "Request changes" returns to
    pub g_tier: &'static [GCommand],      // G-tier commands this step may run
    pub cover: Cover,                     // what authorises them (§6.3)
}
pub enum Cover { None, Start, Gate(&'static str), GateOrStart(&'static str), PerItemGate(&'static str) }
```

The catalogue is checked by unit tests: ids unique and lower-case, every `G` step has a `cover` other than `None`, every `Gate` names an approval step earlier in the same preset, `changes_to` names an earlier step, heavy matches Bureau §8.1's table, and the JSON of `run.presets` matches a golden file.

#### 6.2 Execution contract

```rust
pub trait StepHandler: Send + Sync + 'static {
    /// Enter or re-enter the step. Called on first entry, after every restart and after every
    /// fan-in. Must be idempotent: everything it does is keyed (§6.4).
    fn run<'a>(&'a self, cx: &'a mut StepCx) -> BoxFuture<'a, Result<StepOutcome, StepError>>;
}
pub enum StepOutcome {
    Done(Value),               // output recorded; Chief moves to the next step
    Wait,                      // it enqueued jobs through cx.enqueue; call again when none of this step's jobs is live
    Yield(Value),              // a checkpoint (a Catchup chunk): output saved, a pause may take effect, then called again
    OpenGates(Vec<GateOpen>),  // approval step: gate, summary (with code-rendered text), preview, subject bytes
    OpenForm(FormOpen),        // form step (Huddle)
    Skip(String),              // e.g. the plan gate with review_plan off; the reason is recorded
}
pub enum StepError { Retry(String), Fail { class: ErrorClass, message: String } }
```

`StepCx` carries the run (id, short id, preset, args, `config` snapshot), the step (seq, id, attempt, previous output), the outputs of earlier steps, the database pool, the `Clock`, a `CancellationToken`, and `cx.enqueue`, which tags jobs with `run_id` and `step_seq`. A `Retry` is re-entered with backoff three times, then fails the run. A panic is caught and becomes `Fail { class: Bug }`.

Until their subsystem lands, every preset step's handler is a stub that fails with class `bug`, "not built yet (#NN)", so a real run ends `failed` with a Wrap at its first stubbed step. Chief's tests use a test catalogue with controllable handlers.

One driver task per live run advances it; the service holds a map of drivers, and the singleton lock (§9) guarantees one service per database.

#### 6.3 Gates, approvals and the G-tier rule

- An approval step's `OpenGates` inserts `gates` rows (`run_id, gate, round, step_seq, subject_sha, summary, preview`), at most one open per `(run_id, gate)`. `subject_sha` is SHA-256 over the exact bytes Board will display (the canonical JSON of the plan, draft or diff). Re-opening after a restart is `on conflict do nothing`.
- **`gate.approve | changes | reject`** lock the run, then the gate; the gate must be open and `round` must match (`precondition` otherwise); the sent `subject_sha` must equal the gate's (`gated` otherwise). The approval's foreign key covers `(run_id, gate, round, subject_sha)`, so the database itself refuses an approval for a hash the gate does not hold. `changes` needs a note; it reopens the drafting step named by `changes_to` with a new round. `reject` skips the gated work (for a per-item gate, that item) and continues. When the step's last open gate is answered, the run continues or joins the Lineup.
- **`authorize()`**, one function for every G-tier command (`news publish`, `news home publish`, `news eval promote`, `news corrections resolve --action update`, and the gated uses of `news scores adopt`, `news sources enable|disable`, `news jobs retry` above 50):

  ```rust
  pub async fn authorize<'t>(tx: &'t mut PgTransaction<'_>, req: GRequest<'_>) -> Result<Authorized<'t>, NewsError>;
  pub struct GRequest<'a> { pub run_id: Uuid, pub step_seq: i16, pub command: GCommand, pub item: ItemRef<'a> }
  ```

  It reads only PostgreSQL, inside the caller's transaction, and returns `gated` (exit 7) unless all three conditions of Bureau §5.5 hold: (1) the run is `running` (row locked `FOR SHARE`) and the step is its current `running` step and declares `command`; (2) `item` is in the run's Budget (the plan step's output) for `Cover::Start`, or in the approved subject for a gate; (3) the cover holds: `Start` (the run's origin is `board` or `schedule`), `Gate(g)` (an approval `approved`, `origin = 'board'`, `consumed_at` null, matching the gate's current round and hash), `GateOrStart(g)` (that approval, or the approval step recorded `Skip` with its reason), `PerItemGate(f)` (an approval for gate `f:<item>`).
- `Authorized<'t>` borrows the transaction and is not `Clone`. Press's outbox insert takes `&mut Authorized`, so an outbox row can only be written in the transaction that checked it.
- The approvals a step used get `consumed_at` in the transaction that marks the step `done`. A consumed approval authorises nothing.
- **The CLI forms of G-tier commands always return `gated`.** Chief runs G-tier work in-process, as library calls; no shell, agent or `claude -p` can run one. The table test in §12 runs every G-tier command from the CLI and through `authorize()` outside a live step.

#### 6.4 Idempotency (Bureau §5.8)

- **Step.** On first entry Chief records `input_hash` = SHA-256 of the canonical JSON of `{preset, preset_version, step, args, settings_version, the earlier outputs the step reads}`. A re-entry with a different hash is a bug (`failed`).
- **Jobs.** Handlers give every job its §5.8 key. `cx.enqueue` also skips a key that already has a `completed` job of this run (the live-key index alone would let a finished job be enqueued again), so a re-entered step enqueues only what is missing.
- **Gates** are keyed `(run_id, gate, round)`; **publishes** by page id, version and bundle SHA (Press); **Rota fires** by `(schedule_key, slot_at)`.
- **Steps are marked `done`** with their output, the approvals they consumed and the run's next `current_step` in one transaction. After a crash between a job completing and that transaction, the re-entered handler sees every job completed and returns `Done`.

#### 6.5 The Wrap

`chief::wrap::finish(tx, run, Terminal { status, reason }, now) -> Wrap` is the only writer of a terminal status. It builds the Wrap from PostgreSQL alone (steps and their `wrap` counts, approvals with origin and decision, jobs by state and class, tokens from AI job results, skipped or failure reason, Press's unresolved publishes), renders `text` by code, and writes `summary`, `status` and `finished_at` in the caller's transaction. After commit it emits a `run` event and a best-effort row to ClickHouse `runs`. The check constraint rejects a terminal status without a summary.

### 7. Rota

#### 7.1 Schedules

- A schedule is a Standing key `schedules.<id>` (`id` matches `^[a-z0-9-]{1,32}$`) with value `{"preset", "args", "cron", "enabled"}`. `cron` has five fields and is evaluated in `rota.time_zone` (an IANA name; unset means the laptop's zone, read at service start). `standing.set` validates the preset, its arguments and the expression.
- **Tick** every 30 seconds and at start, with `now` from the `Clock`. For each enabled schedule, the slots due are those after the lower bound and up to `now`, where the lower bound is the **later** of the latest `rota_fires.slot_at` for its key and the `created_at` of the Standing version that created or last changed the schedule (so a changed schedule never fires a slot from before the change, and an unchanged one never fires a slot twice). Only the **latest** due slot is considered; the older ones are counted in its row (`missed_before`).
- **The rules, in order**, for that latest slot (a slot is *late* when it is more than a minute old):
  1. *Missed-slot rule.* Late, and the next slot is under `rota.missed_slot_limit_minutes` away: recorded `missed`, no run row.
  2. *Late-limit rule* (only for slots rule 1 kept). `now > slot_at + lineup.queue_late_limit_minutes`: a run row ends `skipped` at once with a Wrap (reason `late`), heavy or light, even if the slot is free.
  3. Otherwise it fires.
- **Firing** is one transaction: insert `rota_fires (schedule_key, slot_at)` (the primary key makes it once only), then Chief's start with `origin = 'schedule'`, `schedule_key`, `slot_at`, the current `settings_version` and, for heavy runs, `start_by = slot_at + lineup.queue_late_limit_minutes`. The outcome (`started`, `queued`, `skipped` with reason) is written back and sent as a `schedule` event.
- **Every start checks `start_by` first.** Chief's start path, for a new scheduled run and for a Lineup promotion alike, ends a run whose `start_by` has passed as `skipped` with a Wrap instead of starting it, even when the slot is free.
- **Signed out** (Karthik, 16 Nov 2026). While Karthik is signed out, Docker Desktop and so PostgreSQL are down, and Rota cannot tick. At sign-in the service reaches PostgreSQL, recovers, and Rota's first tick applies the rules above in order: a slot dropped by the missed-slot rule is `missed`; one past the late limit is `skipped` with a Wrap; any other fires. So a scheduled run that fell while he was signed out runs if sign-in comes within the late limit, and otherwise is skipped and noted. There is no boot-time Docker engine.
- **Time zones.** A local time that does not exist (spring forward) fires at the first instant after the gap; one that occurs twice (fall back) fires once, at the first. Both are tested.

#### 7.2 Quiet hours

Keys `quiet_hours.enabled`, `.start`, `.end` (local `HH:MM`, may wrap midnight), `.mode` (`dont_start | reduced`), `.ai_slots`.

- `dont_start`: `StartPolicy` holds heavy starts during quiet hours. A scheduled heavy run queues with its `start_by` and starts when they end, or ends `skipped` if `start_by` passes first. A manual heavy Start gets `precondition` with code-rendered text (Bureau §6.4).
- `reduced`: heavy runs start on time. During quiet hours Rota sets `pools.max_running` for `ai` to `quiet_hours.ai_slots`, and back to `pools.ai.max_running` when they end; that applies to every run using the `ai` pool, a heavy run already going included.
- Light runs and the background pools are never held.

#### 7.3 The quota guard

Values are Karthik's settings; unset means no limit. `quota.bureau_daily_tokens`, `quota.bureau_weekly_tokens` (Bureau's own calls, from ClickHouse `ai_calls`) and `quota.reserve_daily_tokens`, `quota.reserve_weekly_tokens` (total Claude Code usage, from the OTel metric `claude_code.token.usage` in ClickHouse). A day is the local calendar day and a week the ISO week, in `rota.time_zone`. Tokens are input + output + cache creation; cache reads are not counted. One function, `rota::usage::tokens`, owns that rule. Readings are cached for 60 seconds; when ClickHouse cannot be read the guard allows and Problems says "usage unknown".

Hooks:

| Where | Call | Effect |
| --- | --- | --- |
| Every start (Board, Rota) and every Lineup promotion | `StartPolicy::check(preset, heavy, origin, now)` → `Allow` / `Hold(reason)` / `Refuse(reason)` | Board: `precondition` with text. Rota: cap or reserve → `skipped`; quiet hours → `queued`. Lineup: stays queued |
| Before every claim of the `ai` pool | `AiClaimHook::check(now)` → `Allow` / `Reserve` / `Cap` | No claim. `Reserve`: Chief sets runs with live `ai` jobs `pausing` (`drain`, `quota`) and resumes them when the hook allows again. `Cap`: Chief cancels the live `ai` jobs of running runs (`cancelled`, error `budget_exceeded`); their steps finish with what passed and the Wrap says so |
| Chief tick | `StartPolicy` again | Auto-resume of `quota` pauses |

#### 7.4 Usage limits

A job failing with `ai_usage_limit` already pauses pool `ai` until the reset (Dispatch). The worker reports the outcome to Chief, which sets every live run with live `ai` jobs `pausing` (`drain`, `usage_limit`, `resume_at` = the pool's `paused_until`) and emits `usage` (`limit_reached`). At `resume_at`, with the pool no longer paused, Chief resumes them (or queues heavy ones) and emits `limit_reset`.

### 8. The Hotline

#### 8.1 Transport and frames

- The pipe `\\.\pipe\news-hotline`, byte mode (frames carry their own length), up to 16 instances. Test servers use `\\.\pipe\news-hotline-test-<uuid>`, settable only through `news_hotline::ServerConfig::for_tests` (feature `test-support`, never in a release build; a golden test checks the release `--help`).
- A frame is a little-endian `u32` length, then that many bytes of UTF-8 JSON. Length 0 or above 1 MiB (1,048,576) closes the connection after a `usage` reply (with `id: null`). So does JSON that does not parse.
- A connection is either **requests** (one at a time; the client sends the next request after the reply; closed after 60 s idle) or **a subscription** (after `subscribe`, the server only sends events; no idle limit). Board keeps one of each; the CLI opens one per call.

#### 8.2 Security

- Pipe created with `first_pipe_instance(true)`, `reject_remote_clients(true)` and the security descriptor `D:P(A;;GA;;;<service SID>)` (spike item 2). The client opens it with `SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION`.
- **On connect**, the server reads the client pid (`GetNamedPipeClientProcessId`) and opens the process (`PROCESS_QUERY_LIMITED_INFORMATION`) for the life of the connection, so the pid cannot be reused under it. Then: the token SID by `ImpersonateNamedPipeClient` + `OpenThreadToken` + `RevertToSelf` on one thread with no `await` between; the integrity level; the image path (`QueryFullProcessImageNameW`); the command line (`NtQueryInformationProcess`, class 60); job membership (`IsProcessInJob` against the **root marker job**). A different SID, or integrity below Medium: logged and disconnected without a reply.
- **The root marker job.** At start the service creates one unnamed Job Object with `KILL_ON_JOB_CLOSE` and no breakaway flags. Every process it spawns for a run (AI or tool) is created suspended, assigned to the run's own job, which is nested in the root (Windows 8+ nested jobs), and resumed (`NtResumeProcess`, or the thread handle from `CreateProcessAsUserW`; never a Toolhelp walk). One `IsProcessInJob(p, root)` therefore covers every run. Every `claude -p` is created with a Low-integrity copy of the service token. If nesting fails on the laptop, the fallback is a registry of live run jobs checked one by one; the builder records which held.
- **Admission** is a pure function, table-tested on every combination:

  ```rust
  pub struct Peer { pub pid: u32, pub same_user: bool, pub integrity: Integrity, pub image_is_ours: bool, pub argv1_is_board: bool, pub in_marker_job: bool }
  pub enum Refusal { WrongUser, LowIntegrity, InAiJob, NotOurImage, NotBoard }
  pub fn admit(tier: Tier, peer: &Peer) -> Result<(), Refusal>
  ```

  R: any same-user, Medium-or-above peer (marked processes included). H: also `!in_marker_job`, then `image_is_ours` (the client's image path equals the service's own `current_exe()`, both final paths compared case-insensitively), then `argv1_is_board` (a hint, not a proof; spike item 2). Each H refusal replies `gated` with `details.refusal`, writes a `hotline_refusals` row, logs at `warn` and appears in Problems.
- **The client** (Board, CLI) checks the server the same way: `GetNamedPipeServerProcessId` → image path equal to its own `current_exe()`. Otherwise it sends nothing and reports the service as down.

#### 8.3 Messages

```json
{"v": 1, "id": "b-1734", "cmd": "run.start", "args": {"preset": "catchup", "args": {"from": "2026-01-01"}}}
{"v": 1, "id": "b-1734", "envelope": {"ok": false, "error": {"code": "conflict", "message": "…", "retryable": false, "details": {"reason": "queue_offer", "…": "…"}, "text": "…"}}}
{"v": 1, "event": "run", "seq": 412, "at": "2026-11-12T16:02:11Z", "data": {"…": "…"}}
```

- `envelope` is exactly the CLI envelope (`contracts/cli/envelope.schema.json`, Bureau §6.2): error codes and exit codes are the CLI's, `text` is code-rendered. A Hotline client that exits (the CLI) exits with the envelope's code.
- Unknown `cmd`: `usage` (exit 2). `args` that fail the command's input schema: `usage`, with the first schema error in `details`. Before the service is `up` (§9), every command except `service.ping` gets `upstream_unavailable` (exit 5, retryable) with text.
- **Versioning.** `v` is the protocol version (frames, the three message shapes). Another `v` gets `usage` with `details.supported = [1]`. Commands change only additively within `v` 1. Board and the service are one binary: Board compares `service.ping`'s `build` with its own and, on a mismatch (after an update), shows "Service updated: restart Board" and sends no H command. Every reply in every test is validated against its contract.
- **Events.** `subscribe` (`{"topics": [...]}`) replies `ok` with the current `seq`, then streams events. `seq` counts per service process from 1. A subscriber that falls more than 1,024 events behind gets one `lagged` event (`{"missed": n}`) and the stream continues; Board then reads `status` again. After a reconnect Board re-subscribes and reads `status`.

#### 8.4 Commands and topics

The contracts in `contracts/hotline/` are the list. Owner is the issue that implements the handler; #10 registers every command that has a contract and returns `precondition` ("built in #NN") for those it does not implement, as ADR 0002 does for CLI stubs. A reserved name without a contract is `usage` (unknown command) until its contract lands.

| Command | Tier | Owner |
| --- | --- | --- |
| `service.ping`, `status`, `run.show`, `run.list`, `run.presets`, `lineup.list`, `standing.get`, `standing.history`, `service.log`, `subscribe` | R | #10 |
| `run.start`, `run.start_after_pause`, `gate.approve`, `gate.changes`, `gate.reject`, `run.pause`, `run.resume`, `run.cancel`, `lineup.cancel`, `standing.set` | H | #10 |
| `docker.status` | R | #29 |
| `docker.start`, `docker.stop`, `service.update` | H | #29 |
| `huddle.answer` | H | #19 (name reserved; no contract yet) |
| `content.*` | H | later issues (names reserved) |

Topics: `run`, `gate`, `lineup`, `schedule`, `usage`, `standing`, `service` (#10); `source` (#9, #16) and `huddle` (#19) have contracts now and are emitted when their subsystems land. `lagged` is the bus's own.

`news_ghost::claude::classify(exit_code, stdout) -> ClaudeOutcome { Ok, UsageLimit { resets_at: Option<DateTime<Utc>> }, Failed }` is written in #10 (provisional rule in §1) because #10's usage-limit test needs it; #18 builds the rest of Ghost around it.

### 9. The service host

`news service run` (SCM entry; console mode when `StartServiceCtrlDispatcher` returns 1063, or with `--console`).

Start order:

1. Logging (the ring of 2,000 lines and the `service` topic), the event bus, the root marker job.
2. **The Hotline listener**, in state `starting`: `service.ping` answers; everything else is `upstream_unavailable`.
3. **Wait for PostgreSQL** (`waiting_for_database`, retry every 5 s with the reason in `service.ping`): Docker Desktop is up only while Karthik is signed in (spike item 4).
4. **Singleton**: a session advisory lock `(44, hashtext(current_schema()))` on a dedicated connection. If another service holds it, exit with that error. If the connection drops later, the lock is gone with it, so the service **fences itself before trying again**: it stops claiming, stops Chief's drivers and Rota, terminates every process in the root marker job and waits until the job is empty, cancels and awaits every worker task (as at stop, but without the 15 s grace), and only then goes back to step 3. Recovery after re-acquiring treats every `running` job as orphaned (nothing of this service is still working on one) and moves it to `retryable` with no holder, so any late report from before the loss names a `lease_id` that no longer exists and is refused (`conflict`).
5. Check migrations: pending ones are applied under the migration lock (the service is the updater's restart path, #29).
6. **Recovery** (`recovering`): complete `pausing` runs (§4.5); run the rescuer; resume runs `paused` for `usage_limit` whose `resume_at` has passed. Nothing claims before this ends.
7. Start the pools' workers (all six; `ai`, `render`, `publish` only ever hold jobs of live runs), Chief's drivers for every `running`/`waiting_ai` run (each re-enters its current step), Chief's tick (1 s), Rota's tick (30 s, missed slots first), Dispatch's rescuer and code schedules (30 s). State `up`.

Stop (SCM stop or shutdown, Ctrl-C in console): state `stopping`; H commands get `upstream_unavailable`; no new claims; in-flight jobs get 15 s, then the process exits. Live runs stay `running` and continue at the next start; their processes die with the root job.

**If the service cannot log on** (#29's first install fails with 1069 or 1326 because passwordless sign-in is on): Karthik first turns off "only allow Windows Hello sign-in" (Settings → Accounts → Sign-in options), re-runs `news service install`, and may turn it back on. If log-on still fails, the fallback is the same host in his session: `news service run --console` started hidden at his log-on by a per-user log-on entry (#29, `news service install --at-logon`). Nothing in #10 changes: same pipe, same singleton lock, same recovery. It costs little, because the data stack already runs only while he is signed in (Docker Desktop); what is lost is restart-on-failure from the SCM, which the log-on entry replaces with a restart loop in Board's "Start service" button.

`Spawner` (in `news-core`, implemented here) takes the program, arguments, an explicit environment (the caller builds the §9.3 allow-list; the service's own environment is never passed), a working folder, `Integrity::Low | Medium`, and the run and job. AI calls are `Low`; their writable folder is a per-run directory labelled Low. The OAuth-refresh risk the spike names is handled in #18: before an AI batch, a Medium-integrity no-op refreshes the login when `.credentials.json` is near expiry.

### 10. Standing (in #10, not #29)

Decided (Karthik, 16 Nov 2026): the Standing table and its get, history and set paths (`standing.get|history|set` over the Hotline, `news standing show|history` in the terminal) are built in #10, because `runs.settings_version` is a foreign key and Rota, the Lineup and the guard read settings. #29 keeps the Board settings screen, the TD "Configuration" keys, the per-step model keys, sources on or off and secrets.

- One row per version: the full map of explicitly set keys (`settings`), the `changes` (`[{key, old, new}]`, effective values), `who` (`board`; `seed` for version 1, which the migration inserts empty), `note`, `created_at`. The effective value of a key is its explicit value, else the registry default. A run's `config` snapshots the effective settings at start, with `settings_version`.
- `standing.set` (`{base_version, changes: [{key, value}], note?}`; `value: null` resets to the default) writes one new version. `base_version` must be the latest (`conflict` otherwise: Board shows what changed). Unknown keys, wrong types and invalid schedules are `validation_failed`. No key is ever a secret.
- Keys #10 registers: `rota.time_zone`, `rota.missed_slot_limit_minutes` (120), `lineup.queue_late_limit_minutes` (120), `chief.stall_minutes` (15), `pools.ai.max_running` (3; replaces `NEWS_PARALLEL`), `quiet_hours.*`, `quota.*`, `schedules.*`, and the next-action thresholds `next.*` (Bureau §8.1 rules 2, 6 and 7), `board.frontpage_due_days` (2), `board.huddle_every_days` (7), `board.rollcall_every_days` (7), `home.stale_days` (4).

### 11. The CLI

- `news run presets | status [<run>] | lineup | list [--active]` and `news standing show [<key>] | history [<key>]` are R and run **in-process** against PostgreSQL, through the same library functions the Hotline handlers call, so they work with the service down and give identical JSON. `news run status` adds the service state from a `service.ping` with a 300 ms timeout (`down` if none).
- Their `contracts/cli/` files `$ref` the matching `contracts/hotline/` output schemas, so there is one shape.
- `news service run [--console]` is added here; `install`, `uninstall` and `news board` are #29.
- No H-tier command has a CLI form (#30 removes them). G-tier CLI forms always return `gated` (§6.3).

### 12. Migrations

Chief's block is 0200–0299 (`cli/README.md`). The exact DDL is in the appendix.

| File | Does |
| --- | --- |
| `0200_standing.sql` | `standing` and its seed version 1 |
| `0201_chief_runs.sql` | One-time reshape: if `runs` has no `settings_version` column, refuse when `runs` has any row, else drop `approvals`, `run_steps`, `runs` (and `start_tickets` if #30 has not). Then create `runs`, `run_steps`, `gates`, `approvals` and their indexes as in the appendix. Nothing wrote these tables before #10, and nothing else references them, so a reshape is simpler and safer than a dozen guarded alters |
| `0202_jobs_step.sql` | `jobs.step_seq` and the `(run_id, step_seq, state)` index |
| `0203_rota_hotline.sql` | `rota_fires`, `hotline_refusals` |

Changes to `news-store`'s migration tests: the numbering test accepts the blocks of `cli/README.md`, not only 1–99; a `do $$ … $$` block or a `drop table` is allowed only in files named in a `RESHAPES` constant (`0201_chief_runs.sql`), and the re-run test still has to pass on it.

### 13. Tests

| Level | What |
| --- | --- |
| Unit | Transition table (every pair); `admit()` table (tier × same user × integrity × image × argv × job); frame codec (lengths 0, 1 MiB, 1 MiB + 1, truncated, bad UTF-8; a proptest round trip); the preset catalogue checks and its `run.presets` golden; next-action rules on fixture facts; Rota slot maths on a `ManualClock` (missed latest only, next slot under the limit, quiet hours crossing midnight, DST gap and overlap in `America/New_York` and `Pacific/Auckland`); the usage-limit classifier on fixtures; every Hotline contract against the 2020-12 meta-schema, and every reply produced in any test against its contract |
| PostgreSQL (`#[ignore]`d, own schema each, as ADR 0003) | **Claim/pause race, both orders, two real transactions:** (a) T1 runs the claim through `claim_in_tx` and holds it open; T2's `pausing` update is shown waiting in `pg_locks`; T1 commits; T2 completes; the claimed jobs are `running` and step 3 moves them to `retryable`. (b) T2 sets `pausing` and holds; T1's claim waits; T2 commits; T1 returns no job of that run. **Restart while `pausing`:** a run left `pausing` with `running` jobs; a new service instance completes the pause (jobs `retryable`, run `paused`) before its first claim (asserted by a claim hook that fails the test if called earlier), and only then frees the slot. **A task that does not stop** (a handler ignoring cancellation in `spawn_blocking`) keeps the run `pausing` and the slot held; a queued heavy run does not start until it returns. **Late completion** from a revoked lease, AI and code pool, is `conflict`. **Paused run's `retryable` job** is not claimed, and is after resume. **Lineup on a `ManualClock`:** a scheduled heavy run waits and starts when the slot frees; one past `start_by` ends `skipped` with a Wrap; manual Start returns `queue_offer`; "pause the running one" frees the slot after the in-flight jobs and starts the new run; `lineup.cancel`. **Rota:** missed slot once, latest only; none when the next is close; the lower bound is the later of the last fire and the schedule's last change (a schedule changed after its last fire fires no slot from before the change; an unchanged one never fires a slot twice); a slot past the late limit is `skipped` with a Wrap even with the slot free, and one dropped by the missed-slot rule is `missed` with no run row; **drain with an inline step**: a `pausing` (`drain`) run whose inline code step is still running with no jobs stays `pausing` and keeps the slot until the step returns; **lost singleton lock**: dropping the service's lock connection mid-run stops claims, empties the root job and awaits every task before re-acquiring, and a completion sent after the loss is `conflict`; quiet hours hold (`dont_start`) and shrink the pool (`reduced`); the guard stops starts at the cap, stops `ai` claims at the reserve, pauses with `quota` and resumes when usage falls (a fake `UsageSource`). **Gates:** approval for a changed hash refused (`gated`, and the foreign key refuses a direct insert); a consumed approval does not authorise a second step; **G-tier table**: every `GCommand` through `authorize()` outside a live step, and every G-tier CLI command, exit 7. **Wrap constraint**: an update to each terminal status without a summary fails. **Kill between steps**: the service process is killed after step *n* is `done`; a new one resumes at *n*+1 and no job of steps ≤ *n* runs again. **Golden status board**: the Bureau §8.1 example rendered byte for byte from a fixture database at `2026-11-12T13:40Z` in `America/New_York` |
| Windows (`#[cfg(windows)]`, new CI job `rust-windows` on `windows-latest`, free for this public repository) | The real pipe with a test name. A probe binary built from `news-hotline`'s tests acts as client; the test server's `image_is_ours` compares with the probe's path. Probe as `board` → H `ok`; as CLI → H exit 7, R `ok`; inside a test root marker job → H exit 7 and a `hotline_refusals` row; a second server on the name fails; a probe at Low integrity cannot open the pipe; a server whose DACL names another SID refuses the probe (the spike's way to model another user without a password). A fake `claude` (a small test binary that prints the provisional usage-limit JSON) run through the `Spawner`: the run pauses, the `ManualClock` passes the reset, and it resumes with no Hotline action |
| Repo (`scripts/tests/bureau-single-entry.test.ts`, Bun) | Every `insert into runs` (any case and spacing, in `.rs` and `.sql`) is under `cli/crates/news-chief/` or a test directory; also that no file outside `news-service` and tests calls `ChiefControl` |

### 14. What moves between issues

| Item | From | To | Why |
| --- | --- | --- | --- |
| Standing table, `standing.get|history|set`, `news standing show|history`, the Standing version test | #29 | #10 | Decided by Karthik. `runs.settings_version` references it; Rota and the Lineup read it. #29 keeps the Board settings screen and the TD config, model and source keys |
| `docker.*` and `service.update` handlers | — | #29 | #10 registers them as stubs; contracts are here |
| Removing `NEWS_PARALLEL` from `news db migrate` | — | #30 (or #10 if #30 has merged) | `pools.ai.max_running` is a Standing key now |
| `news work` as a library entry point; removing `CLAUDECODE` and the H-tier CLI forms | #30 | must merge before #10 starts | #10 builds on the library worker loop and assumes no H-tier CLI command exists |
| The crate name `news-board` | analytics pull (`traffic.pull`, #5) | the egui app (#29) | Decided by Karthik. #30 moves the `traffic.pull` kind and its stub handler, and `news stats`' stub reference, from `news-board` into `news-telemetry` (Ticker owns analytics ingestion): `news_dispatch::EXTRA_KINDS` owner, the binary's registry binding and the kinds-agree test follow. `news-board` is then free for #29's egui app |
| Board build rules: delay-load the 9 GUI DLLs (`/DELAYLOAD` + `delayimp.lib` in `crates/news/build.rs`), the `detached` console manifest (or `FreeConsole` at the top of `news board`), an optional `board` cargo feature off in CI | — | #29 | Spike item 5 |
| `news service install` self-elevation, `SeServiceLogonRight`, password prompt, the log-on fallback | — | #29 | Spike item 1; §9 |
| Ghost's call path, Low-integrity working folders, the OAuth pre-refresh | — | #18 | #10 provides `Spawner`, the marker and the classifier only |
| Bureau §5.5, §5.6, §8.1 edits (gates table, pause boundary, `retryable: false` on `queue_offer`, `stop_mode`, `lineup_rank`, `rota_fires`) | — | #10's PR | the issue's Docs list |

## Consequences

- #10 is large: about 4,000–5,000 lines of Rust and SQL plus tests *(estimate)*. It stays one PR because Chief, the Lineup and the Hotline share one state machine and one test fixture; the moves above keep it from growing.
- One crate holds every `unsafe` line, so the security review reads one crate for memory safety.
- Every `claude -p` runs at Low integrity: it cannot reach the Hotline at all, start a process through WMI, or write Karthik's files outside its Low folder. #18 must prove the login survives a token refresh before the first scheduled run.
- Service log-on with a password is unproven while passwordless sign-in is on; the log-on fallback (§9) keeps #10's design unchanged if it fails.
- Runs scheduled while Karthik is signed out wait for sign-in; there the missed-slot rule applies first (a dropped slot is `missed`), then the late limit (past it, `skipped` with a Wrap), and anything else runs (Karthik's choice; no boot-time Docker engine).
- Board must be restarted after a service update before it can act again.
- The data model gains `gates`, `rota_fires` and `hotline_refusals`; the design document is updated with the build.

## Alternatives considered

- *Alter the #5 tables in place with guarded alters.* About twelve statements, including check, not-null and index changes the migration lint does not allow today; a one-time guarded reshape is shorter, and safe because the tables are empty.
- *Keep gate state in `run_steps.output`.* No database-level link between an approval and the hash it answers; a `gates` table lets a foreign key refuse a mismatched approval.
- *Step-boundary pause as written in §8.1.* Pausing a 112-page write step would wait for every page. Stopping claims at once and draining in-flight jobs is the same guarantee, sooner.
- *Check every live run's Job Object for the marker.* Racy while jobs open and close; one root job with nested run jobs is one call.
- *Board identified by a token it presents.* Any process running as Karthik could read it; the image path is checked by the kernel, and Low integrity keeps AI calls off the pipe entirely.
- *`news run status` over the Hotline only.* It would not work with the service down, which is exactly when Karthik needs it.
- *Standing in #29 with a fixture table in #10.* Two schemas for the same data and a later migration of every run's `settings_version`.

## Open questions for Karthik

All answered on 16 Nov 2026:

1. **Standing in #10.** Yes: the table, the Hotline commands `standing.get`, `standing.history` and `standing.set` (set from Board only) and the terminal's read-only `news standing show|history` move from #29 into #10; #29 keeps the Board settings screen (§10, §14).
2. **Signed out.** Scheduled runs that fall while he is signed out wait until sign-in; there, the missed-slot rule applies first (a slot it drops is `missed`), then the late limit (past it, `skipped` with a Wrap), and anything else runs. No boot-time Docker engine (§7.1).
3. **The `news-board` name.** `traffic.pull` (and `news stats`' stub reference) move from `news-board` into `news-telemetry` in #30, so the egui app crate is `news-board` (§14).

## Changes to apply with the build

- **ADR 0007**: amended in this branch ("Amended by ADR 0008", "Amended after the spike"): contracts in `contracts/hotline/`; Low integrity for every AI call; install facts and the log-on fallback; Docker Desktop dependency; Board build rules; the `unsafe` crate; the Hotline numbers.
- **Bureau Design** §5.4 (claim and heartbeat as §4.3–§4.4), §5.5 (DDL as the appendix; pause boundary; `gates`), §5.6 (`rota_fires`, DST rule, day and week definitions), §8.1 (`queue_offer` `retryable: false`; `run_id` and `short_id` both in the status JSON).

## Appendix: DDL of the 0200 block

```sql
-- 0200_standing.sql
create table if not exists standing (
  version    integer primary key check (version >= 1),
  settings   jsonb not null check (jsonb_typeof(settings) = 'object'),   -- explicitly set keys; unset keys take the registry default
  changes    jsonb not null check (jsonb_typeof(changes) = 'array'),     -- [{"key", "old", "new"}], effective values
  who        text not null check (who in ('board', 'seed')),
  note       text,
  created_at timestamptz not null default now(),
  check ((version = 1) = (who = 'seed'))
);
insert into standing (version, settings, changes, who, note)
values (1, '{}', '[]', 'seed', 'registry defaults')
on conflict (version) do nothing;
```

```sql
-- 0201_chief_runs.sql
do $$
begin
  if to_regclass('runs') is not null and not exists (
       select 1 from information_schema.columns
        where table_schema = current_schema() and table_name = 'runs' and column_name = 'settings_version') then
    if exists (select 1 from runs) then
      raise exception 'runs has rows; the one-time reshape of ADR 0008 will not drop them';
    end if;
    drop table if exists approvals, run_steps, runs;
  end if;
end $$;
drop table if exists start_tickets;   -- ADR 0007; a no-op once #30 has dropped it

create table if not exists runs (
  run_id           uuid primary key,
  short_id         text unique not null check (short_id ~ '^r_[a-z0-9]{4,16}$'),
  preset           text not null,
  args             jsonb not null,
  heavy            boolean not null,
  origin           text not null check (origin in ('board', 'schedule')),
  schedule_key     text,
  slot_at          timestamptz,
  settings_version integer not null references standing (version),
  status           text not null check (status in ('queued', 'running', 'waiting_approval', 'waiting_ai', 'pausing',
                                                   'paused', 'succeeded', 'failed', 'cancelled', 'skipped')),
  lineup_rank      smallint not null default 1 check (lineup_rank in (0, 1)),
  queued_at        timestamptz,
  start_by         timestamptz,
  stop_mode        text check (stop_mode in ('drain', 'now', 'cancel')),
  paused_reason    text check (paused_reason in ('karthik', 'usage_limit', 'quiet_hours', 'quota', 'update')),
  resume_at        timestamptz,
  current_step     text,
  config           jsonb not null,
  budget           jsonb not null,
  created_at       timestamptz not null default now(),
  started_at       timestamptz,
  last_seen_at     timestamptz,
  finished_at      timestamptz,
  summary          jsonb,
  constraint runs_wrap_required check (status not in ('succeeded', 'failed', 'cancelled', 'skipped') or summary is not null),
  constraint runs_finished check ((status in ('succeeded', 'failed', 'cancelled', 'skipped')) = (finished_at is not null)),
  constraint runs_schedule check ((origin = 'schedule') = (schedule_key is not null and slot_at is not null)),
  constraint runs_queued check (status <> 'queued' or queued_at is not null),
  constraint runs_start_by check (start_by is null or (origin = 'schedule' and started_at is null)),
  constraint runs_started check (started_at is not null or status in ('queued', 'skipped', 'cancelled')),
  constraint runs_stop_mode check ((status = 'pausing') = (stop_mode is not null)),
  constraint runs_paused_reason check ((status in ('pausing', 'paused')) = (paused_reason is not null)),
  constraint runs_resume_at check (resume_at is null or status in ('pausing', 'paused'))
);
-- One live or queued run per preset; one heavy run holding the laptop (a pausing run keeps it until its work stops).
create unique index if not exists runs_one_per_preset on runs (preset)
  where status in ('queued', 'running', 'waiting_ai', 'waiting_approval', 'pausing', 'paused');
create unique index if not exists runs_one_heavy on runs ((true))
  where heavy and status in ('running', 'waiting_ai', 'pausing');
create index if not exists runs_lineup on runs (lineup_rank, queued_at, run_id) where status = 'queued';
create index if not exists runs_recent on runs (created_at desc);

create table if not exists run_steps (
  run_id      uuid not null references runs,
  seq         smallint not null check (seq >= 1),
  step        text not null,
  kind        text not null check (kind in ('code', 'ai_batch', 'draft', 'form', 'approval')),
  status      text not null check (status in ('pending', 'running', 'done', 'skipped', 'failed')),
  attempt     smallint not null default 0 check (attempt >= 0),
  input_hash  bytea check (input_hash is null or octet_length(input_hash) = 32),
  output      jsonb,
  error       jsonb,
  started_at  timestamptz,
  finished_at timestamptz,
  primary key (run_id, seq),
  unique (run_id, step)
);

create table if not exists gates (
  run_id      uuid not null,
  gate        text not null check (gate ~ '^[a-z]+(:[A-Za-z0-9_.-]{1,64})?$'),
  round       smallint not null check (round >= 1),
  step_seq    smallint not null,
  subject_sha bytea not null check (octet_length(subject_sha) = 32),
  summary     jsonb not null,
  preview     jsonb,
  opened_at   timestamptz not null,
  answered_at timestamptz,
  primary key (run_id, gate, round),
  unique (run_id, gate, round, subject_sha),
  foreign key (run_id, step_seq) references run_steps (run_id, seq)
);
create unique index if not exists gates_one_open on gates (run_id, gate) where answered_at is null;

create table if not exists approvals (
  run_id      uuid not null,
  gate        text not null,
  round       smallint not null,
  decision    text not null check (decision in ('approved', 'rejected', 'changes')),
  note        text,
  decided_at  timestamptz not null,
  subject_sha bytea not null,
  origin      text not null check (origin = 'board'),
  consumed_at timestamptz,
  primary key (run_id, gate, round),
  -- The approval answers exactly the hash the gate holds.
  foreign key (run_id, gate, round, subject_sha) references gates (run_id, gate, round, subject_sha),
  check (decision <> 'changes' or length(btrim(coalesce(note, ''))) > 0),
  check (consumed_at is null or decision = 'approved')
);
```

```sql
-- 0202_jobs_step.sql
alter table jobs add column if not exists step_seq smallint;
create index if not exists jobs_run_step on jobs (run_id, step_seq, state) where run_id is not null;
```

```sql
-- 0203_rota_hotline.sql
create table if not exists rota_fires (
  schedule_key     text not null,
  slot_at          timestamptz not null,
  outcome          text not null check (outcome in ('started', 'queued', 'skipped', 'missed')),
  reason           text,          -- preset_live | quota | next_slot_close | …
  missed_before    integer not null default 0 check (missed_before >= 0),
  run_id           uuid references runs,
  settings_version integer not null references standing (version),
  fired_at         timestamptz not null,
  primary key (schedule_key, slot_at),
  check ((outcome in ('started', 'queued', 'skipped')) = (run_id is not null))
);

create table if not exists hotline_refusals (
  refusal_id bigserial primary key,
  at         timestamptz not null default now(),
  cmd        text not null,
  refusal    text not null check (refusal in ('wrong_user', 'low_integrity', 'in_ai_job', 'not_our_image', 'not_board')),
  pid        integer,
  image      text,
  run_id     uuid,             -- when the process was in a run's job
  job_id     bigint
);
create index if not exists hotline_refusals_at on hotline_refusals (at);
```

`skipped` scheduled runs are rows in `runs` (with a Wrap), so Problems lists them with failed runs; a `missed` slot is only a `rota_fires` row.
