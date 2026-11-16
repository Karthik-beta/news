# Spike: Bureau service, Hotline, spawn marker and Board

- Date: 16 November 2026. Laptop: Windows 11 Pro build 26300, 8 logical cores, Rust 1.100.0. Other agents were building and testing at the same time (CPU at 100 % during most runs), so wall times are pessimistic.
- For: issue #10 (Chief, the service, the Hotline, Rota, the Lineup) and "Build the Bureau service and Board shell"; ADR 0007 (PR #28).
- Code: `cli/spikes/` on branch `spike/bureau-service` (its own Cargo workspace, not linked into `news.exe`). `bureau-spike` covers items 1–4, `board-spike` item 5.

## Verdict

| # | Question | Verdict |
| --- | --- | --- |
| 1 | A Rust Windows service that installs, starts, stops and uninstalls | **Go**, with two install facts ADR 0007 must state (elevation; the log-on right and the Microsoft-account password). Lifecycle code works up to the SCM; the install itself was not run (agent not elevated). |
| 2 | Hotline locked to one user, local only, client identified | **Go** |
| 3 | Job Object spawn marker | **Go for what it claims, but it is escapable** through WMI from inside the job. Add a second marker: run AI calls at **Low integrity**, which closes the escape and makes the kernel refuse the pipe. |
| 4 | `claude -p` and the databases from the service's context | **Go** (console run with the service's environment); confirm once on the real install |
| 5 | Board on eframe in one `news.exe` | **Go**, with two link-time fixes so the CLI does not pay for the GUI: delay-load the GUI DLLs, and a "detached" console policy (or `FreeConsole`) so `news board` at log-on shows no console window. +6.4 MB. |

## 1. The Windows service

### What I built

`bureau-spike service install|start|stop|status|uninstall|run` on `windows-service` 0.8.1. `run` is the SCM entry point (`define_windows_service!`, a control handler for Stop and Shutdown, status Running → Stopped); its body is the Hotline server from item 2, on a Tokio runtime, stopped through a `watch` channel. `console` runs the same body without the SCM.

### Results

| Step | Result |
| --- | --- |
| `service install` from the agent's terminal (medium integrity, not elevated) | `OpenSCManager(SC_MANAGER_CREATE_SERVICE)` → **error 5, access denied**. Karthik's account is in Administrators, but a UAC-filtered token cannot create services. |
| `service status` (no admin needed) | error 1060, not installed: nothing was left behind |
| `service run` started from a terminal | `StartServiceCtrlDispatcher` → **error 1063** at once. `news service run` can use this to tell "started by the SCM" from "started in a terminal" and fall back to console mode for debugging. |
| `console` (the service body without the SCM) | Starts the Hotline in ~300 ms (debug build), serves, stops cleanly on its timer or Ctrl-C |

I did not install anything. `cli/spikes/bureau-spike/elevated-service-test.ps1` is a ready one-minute check for Karthik to run once in an elevated PowerShell: it installs `NewsBureauSpike` as `NT AUTHORITY\LocalService` (no password), starts it, calls the Hotline (R and H), stops it, deletes it, and prints the service log, which records the account, profile and environment the SCM gave the process.

### What installing "as Karthik's account" needs (for `news service install`)

1. **An elevated process.** Creating a service needs an administrator token. `news service install` should detect a filtered token and re-launch itself elevated (`ShellExecuteExW` with the `runas` verb; Karthik sees the UAC prompt) rather than fail with error 5.
2. **The account name and password.** `ServiceInfo.account_name = ".\Admin"` (the local account behind his Microsoft account, as `whoami` prints it) and `account_password` = **his Microsoft-account password**, not his PIN. He types it at a no-echo prompt (`SetConsoleMode` without `ENABLE_ECHO_INPUT`); the code passes it to `CreateServiceW` and zeroes the buffer. Nothing stores it except the SCM's LSA secret.
3. **The "Log on as a service" right.** `CreateServiceW` does not grant it (the Services console does, the API does not), and without it the first start fails with error 1069. `install` must call `LsaOpenPolicy` + `LsaAddAccountRights(SeServiceLogonRight)` for his SID, in the same elevated process.
4. **Passwordless sign-in is on.** `HKLM\…\PasswordLess\Device\DevicePasswordLessBuildVersion = 2` ("only allow Windows Hello sign-in for Microsoft accounts"). That setting hides the password tile at the lock screen; service log-on uses `LogonUser` with the password, which I expect still works, but this is **the one thing the spike could not prove**. If the first start fails with 1069 or 1326, he turns the setting off (Settings → Accounts → Sign-in options), installs, and can turn it back on.
5. **Start type** `Automatic (Delayed Start)` and recovery actions "restart after 1 minute" (`ChangeServiceConfig2W` with `SERVICE_CONFIG_DELAYED_AUTO_START_INFO` and `SERVICE_CONFIG_FAILURE_ACTIONS`; `windows-service` exposes both).
6. **A password change** stops the next log-on (error 1069). ADR 0007 already says Board shows the offline banner and he re-runs `news service install`; `install` should therefore also work as "update the password of an existing service" (`ChangeServiceConfigW`).

What differs inside the real service (to check in the first install's log; the spike's service writes all of it): session 0, no desktop, so no window, tray or toast (Board does those); the user profile *is* loaded for a user-account service, so `%USERPROFILE%`, `%APPDATA%` and HKCU are his; the environment is built for his log-on (see item 4), not inherited from a terminal.

## 2. The Hotline

### What I built

`hotline.rs`: a Tokio named-pipe server (`tokio::net::windows::named_pipe::ServerOptions` with `first_pipe_instance(true)`, `reject_remote_clients(true)` and `create_with_security_attributes_raw`), framing as ADR 0007 (u32 LE length + JSON, 1 MiB cap). The security descriptor is built from SDDL `D:P(A;;GA;;;<SID>)`: a protected DACL with one ACE. Per connection the server reads:

- the client PID (`GetNamedPipeClientProcessId`) → `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` → image path (`QueryFullProcessImageNameW`, from the kernel) and command line (`NtQueryInformationProcess` class 60, from the client's memory);
- the client SID, by `ImpersonateNamedPipeClient` + `OpenThreadToken` + `RevertToSelf`, synchronously after the first read (impersonation is per thread; no `await` in between);
- AI-job membership (`IsProcessInJob` against the service's own job handle, item 3).

The client opens the pipe with `security_qos_flags(SECURITY_IDENTIFICATION)`, so the server can read its SID but cannot act as it, and checks the server with `GetNamedPipeServerProcessId` → image path.

### Results

| Check | Result |
| --- | --- |
| DACL as created (read back with `GetSecurityInfo`) | `O:<Karthik>D:P(A;;FA;;;<Karthik>)`: owner Karthik, protected, one ACE, full access to his SID. `Get-Acl \\.\pipe\news-hotline-spike` (an independent reader) prints the same. No Everyone, Users, Administrators or SYSTEM entry. |
| A different SID is refused | Server started with the DACL granting only `S-1-5-19` (LocalService), then only an unknown domain SID `S-1-5-21-1-2-3-1001`; the client running as Karthik gets **`CreateFile` error 5** both times, before the server sees anything. That is the kernel enforcing the DACL, which is exactly what another user would hit. (I could not log on as a second user without a password, so the "other user" is modelled by taking Karthik out of the DACL.) |
| A second server on the same name | **error 5**: `FILE_FLAG_FIRST_PIPE_INSTANCE` stops a squatter from posing as the service while it runs. (Before the service starts, a squatter could create the name first; the client's server-image check covers that.) |
| Low-integrity client (item 3) | **error 5**: the pipe carries the default Medium mandatory label, so Low processes cannot open it for writing |
| Terminal `client --cmd status` (R) | `ok` |
| Terminal `client --cmd run.start` (H) | `gated:not_board` |
| `board --cmd run.start` (H, same image, argv[1] = `board`) | `ok` |
| Server-side checks per connection (PID, image, command line, job, impersonation), release build | 0.50–0.65 ms (8 runs; one outlier at 151 ms while the CPU was saturated) |
| Round trip, client open → server-image check → request → reply, release build | 1.6–3.9 ms |

### Notes for the build

- `IsProcessInJob(p, NULL)` ("in any job?") is useless as a marker: **every** process here was already in some job (the desktop app, Windows Terminal and many launchers use jobs). Always pass the service's own job handle.
- The command-line check (`argv[1] == "board"`) is a hint, not a proof: the client could rewrite its own command line, and any process running as Karthik can start `news.exe board`. The image path is reliable. This is the residual risk ADR 0007 already names; item 3 shows the realistic way round it and how to close it.
- The owner of an object gets `READ_CONTROL` and `WRITE_DAC` implicitly; the owner is Karthik, who is also the only grantee, so nothing changes, but do not create the pipe from a process running as someone else.
- Production needs `unsafe` for these calls, and the workspace has `unsafe_code = "forbid"`. Put them in one small platform crate (for example `news-win`) with its own `[lints]` table (`unsafe_code = "deny"` plus `#[allow]` per function and a `// SAFETY:` comment), safe functions out, and keep every other crate on `forbid`.

## 3. The spawn marker

### What I built

`job.rs` + scenarios in `console --spawn …`. A Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` and no breakaway flags. Children are started with `std::process::Command` (so pipes and the environment work as usual) with `CREATE_SUSPENDED | CREATE_NO_WINDOW`, assigned with `AssignProcessToJobObject`, then resumed. `Command` hides the main-thread handle, so resuming needs either a Toolhelp thread snapshot (documented) or `NtResumeProcess` on the process handle (ntdll, undocumented but stable since XP).

### Results

| Check | Result |
| --- | --- |
| Child in the job | `IsProcessInJob` = true |
| Child spawns with `CREATE_BREAKAWAY_FROM_JOB` | **error 5**: cannot leave |
| Grandchild three levels down (`job-child` → `cmd.exe` → `bureau-spike job-check` → `bureau-spike client`/`board`) | Server sees `in_ai_job = true` for both; R `status` → `ok`, H `run.start` from a process started as `board` → **`gated:ai_job`** |
| Real `claude -p` with the Bash tool (`claude.exe` → Git Bash → `bureau-spike job-check` → client) | Server sees `in_ai_job = true`; R ok, H **gated**. `claude.exe` runs normally inside a no-breakaway job. |
| **Escape through WMI**: inside the job, `Invoke-CimMethod Win32_Process Create` starting `bureau-spike board --cmd run.start` | The new process is created by `WmiPrvSE`, **outside the job** (`in_ai_job = false`, not even in any job), passes the image and `board` checks, and its H-tier request is **accepted**. The job marker alone does not hold against code that can run a shell. |
| Same three tests from a **Low-integrity** process in the job | Hotline open → **error 5**; WMI `Win32_Process.Create` → **"Access denied"**; writing `%TEMP%` (Medium) → **error 5** |
| `claude -p` at Low integrity in the job | **Works**: `pong`, subscription login, 32 s wall |
| Spawn cost: suspend → assign → resume | `CreateProcess` 81 ms release (60–240 ms debug, loaded CPU); assign ≈ 0.07 ms; resume with `NtResumeProcess` 0.03–0.04 ms; with a Toolhelp snapshot **195–390 ms** (it walks every thread on the machine) |

### Recommendation

1. Keep the job (it is free, kills the whole tree on cancel or service exit, and is what `IsProcessInJob` checks).
2. **Also start every `claude -p` at Low integrity** (`DuplicateTokenEx` of the service token, `SetTokenInformation(TokenIntegrityLevel, S-1-16-4096)`, `CreateProcessAsUserW`; no privilege needed for a lowered copy of your own token). Then the kernel refuses the Hotline to the call and everything it starts (the pipe's Medium label), WMI refuses process creation, UIPI blocks synthetic input to Board's window, and the call cannot write Karthik's files. This turns "tool-less calls" from the only guard into defence in depth, and closes the WMI escape above.
   - Cost: the call must read, not write, under `%USERPROFILE%\.claude`. A ping worked; the risk is the OAuth refresh, which rewrites `.credentials.json`. If the refresh token rotates and the Low process cannot save the new one, the stored login could go stale. Mitigation: before each AI batch, the service (Medium) checks `expiresAt` in `.credentials.json` and, if it is near, runs one Medium `claude -p` no-op to refresh; or give Ghost its own `CLAUDE_CONFIG_DIR` labelled Low with its own login. Test over a full token lifetime before relying on it.
   - Writable places for Low calls (work files): a per-run folder labelled Low (`icacls … /setintegritylevel low`) or `%USERPROFILE%\AppData\LocalLow\news\`.
3. Use `NtResumeProcess` (or `CreateProcessW` / `CreateProcessAsUserW` directly, which return the thread handle), never a Toolhelp walk. An alternative with no suspended window at all is `PROC_THREAD_ATTRIBUTE_JOB_LIST` in `STARTUPINFOEX`; it needs `CreateProcessW` by hand.
4. Optional hardening once tool calls exist: `JOBOBJECT_BASIC_UI_RESTRICTIONS` (`JOB_OBJECT_UILIMIT_HANDLES` and friends) and an `ActiveProcessLimit`.

## 4. `claude -p`, PostgreSQL, ClickHouse and Docker from the service's context

### What differs in the service, and how I modelled it

The service runs as Karthik in session 0 with his profile loaded. Its environment is not a terminal's: the SCM builds a log-on environment for the account (system + user variables, profile folders), with nothing inherited from a shell. I modelled that with `CreateEnvironmentBlock(token, bInherit = FALSE)` (what Windows builds for a fresh log-on), then cut it to the allow-list of Bureau Design §9.3 (26 names: `Path`, `PATHEXT`, `SystemRoot`, `USERPROFILE`, `APPDATA`, `LOCALAPPDATA`, `TEMP`, …). Children got only that (`env_clear()`), cwd an empty temp folder, and ran inside the AI job.

| Difference | Finding |
| --- | --- |
| Environment | The agent's own process had 110 variables, 56 of them from the desktop app (`CLAUDECODE`, `CLAUDE_CODE_*`, `ANTHROPIC_BASE_URL`, `DISABLE_AUTOUPDATER`, …). The fresh log-on block has 55 and none of those. Ghost must build the child environment itself from the allow-list, as the design says; never pass the service's own environment through. |
| PATH | The fresh PATH has the nvm4w folder (where `claude.cmd` lives), Docker, Git, Cargo and Bun. Ghost should still not rely on PATH: resolve `claude.exe` once at install (`…\nodejs\node_modules\@anthropic-ai\claude-code\bin\claude.exe`, behind the `claude.cmd` shim) and store the absolute path in Standing. Calling `claude.exe` directly avoids a `cmd.exe` layer and batch-file argument quoting. |
| Credentials store | On Windows Claude Code keeps the login in `%USERPROFILE%\.claude\.credentials.json` (present), not in Credential Manager, so the service as Karthik reads it with no extra step. |
| `CLAUDE_CONFIG_DIR` | Not set anywhere (user, machine). Default `%USERPROFILE%\.claude` is used. |
| `ANTHROPIC_API_KEY` | Not set; removed by `env_clear()` anyway |

### Results

| Probe (allow-listed fresh log-on environment) | Result |
| --- | --- |
| `claude -p --model haiku --tools "" --strict-mcp-config …` "reply pong", in the job | `pong`, `is_error: false`, subscription login, `claude-haiku-5-5`, $0.0006. Wall **52.6 s and 64.3 s**; `duration_ms` 11.4 s and 13.4 s; `duration_api_ms` 2.3 s and 1.9 s. The same command straight from PowerShell with the agent's environment: 78.9 s wall. So the job and the clean environment cost nothing measurable; the wall time is start-up on a CPU at 100 %. |
| Same, at Low integrity | `pong`, wall 32.1 s |
| Same with the Bash tool | Works: 2 turns, 57 s wall, $0.0013 |
| PostgreSQL (`DATABASE_URL` from the main checkout's `.env`, not printed) | Connected, `current_user = news`, PostgreSQL 17.8, 964 ms connect + query |
| Shared ClickHouse over HTTP on 127.0.0.1:8123 (`NEWS_CH_*`) | `SELECT version(), currentUser()` → 26.8.36.3, `news`, 312 ms |
| Docker CLI with only the allow-listed environment | `docker ps` lists `news-postgres`, `news-otel`, `news-dynamodb` and the shared stack; 3.5 s. It finds the `desktop-linux` context through `%USERPROFILE%\.docker`. Karthik is in `docker-users`, which the Docker Desktop pipe admits. |

### Not settled here

- **The usage-limit error.** Not triggered; its shape and reset time still need a real limit hit (Phase 7 lean-call spike, or the first long run). Ghost should treat a non-zero exit with `is_error: true` and an unknown `subtype` as "pause the `ai` pool and surface in Board" until then.
- **Docker Desktop is a per-user app.** It starts when Karthik signs in, not at boot. The service starts at boot (delayed auto-start), so for a while after boot, and whenever he is signed out, PostgreSQL and the shared ClickHouse are down. Rota and the workers must treat "database unreachable" as "wait and retry", not as a failure, and Board's Docker screen should say why. ADR 0007's "runs continue with Board closed" holds; "runs continue while signed out" does not, unless Docker Desktop's engine runs without a sign-in.
- **Start-up time of `claude -p`.** 30–80 s wall for a two-second API call on this busy laptop. With `NEWS_PARALLEL = 3` and 60–150 s per A1 call this is a fixed tax worth measuring on an idle machine in the Phase 7 lean-call spike (which flags cut it; `--bare` is not an option because it skips the subscription login).

## 5. Board on eframe

### What I built

`board-spike` (lib + bin; `run(args)` is the whole app, so it links into `news.exe` unchanged). eframe 0.37 with the **glow** renderer only (`default-features = false`, features `glow`, `default_fonts`, `accesskit`). At start it parses `design/tokens/tokens.json` (`include_str!`), builds one `egui::Visuals` per mode from the `color` group (light = `$value`, dark = `$extensions["news.dark"]`: paper, wash, rule, ink, ink-2, brand, control-border, focus, selection) and registers both with `set_visuals_of`. `ThemePreference::System` follows Windows; a System / Light / Dark switch is the manual override. Options: `--minimized`, `--tray` (feature `tray`), `--toast` (feature `toast`), `--exit-after-first-frame --report FILE` for timing. Then I linked it into the real `news` CLI as `news board` (`cli/spikes/board-spike/news-board-link.patch`, applied only for the measurement and reverted).

eframe 0.37 notes for the builders: the `App` trait is now `ui(&mut self, ui: &mut Ui, frame)` plus an optional `logic(ctx, frame)` that also runs while the window is hidden; `ViewportBuilder` has no `with_minimized`.

### Results

| Check | Result |
| --- | --- |
| Token colours | Applied in both modes from `tokens.json`; no hand-typed hex |
| Windows light/dark | `ctx.system_theme()` = `Some(Dark)`, matching `HKCU\…\Personalize\AppsUseLightTheme = 0`. winit follows changes at run time; the manual override works through `ctx.set_theme` |
| Start minimised without a flash | Create the viewport with `with_visible(false)`, then in the first `logic()` send `ViewportCommand::Minimized(true)` and `Visible(true)`. Measured: the report is written from the first `logic()` and `ui()` never ran, so nothing was painted before the minimise |
| Tray | `tray-icon` 0.26.1 builds and runs on eframe's winit loop (icon from RGBA; click → `Minimized(false)` + `Focus`, woken by `TrayIconEvent::set_event_handler` → `request_repaint`). Not clicked by hand in this spike |
| Toast | `tauri-winrt-notification` 0.8.1: `show()` returned `Ok(())` from inside `news board` (one test toast appeared with the PowerShell app id) |
| Binary size, standalone Board | 6.69 MB (`board-spike.exe`, release, same profile as `cli/`) |
| **`news.exe` size** | baseline **7.61 MB** (7,607,808 B) → with Board **14.01 MB** (+6.41 MB, +84 %) → with tray and toast **14.11 MB** (+0.10 MB more) |
| Dependencies | +3,162 lines in `cli/Cargo.lock`; the spike workspace alone resolves 415 packages |
| Release build time (loaded laptop) | `news` from scratch 16 m 31 s; adding Board 13 m 58 s more; changing only `news`'s build script 7 m 54 s (thin LTO relinks everything) |
| `news board`: process start → first painted frame | `main()` → first frame **0.75–2.1 s** in most runs (up to 8 s when the CPU was saturated); process age at `main()` 0.2–13 s, which is the machine, not Board: `cmd /c exit` took 0.4–5.4 s at the same time. Re-measure on an idle laptop; I expect well under 1 s |
| **Cost to every CLI call** | Linking eframe adds 9 static DLL imports to `news.exe`: `user32`, `gdi32`, `opengl32`, `uiautomationcore`, `dwmapi`, `imm32`, `uxtheme`, `ole32`, `oleaut32` (`dumpbin /dependents`). The Windows loader maps them for `news --version` too. Interleaved runs: median 592 → 860 ms (+270 ms, n = 15) at moderate load; lost in noise at full load. With `/DELAYLOAD` for those 9 DLLs (+ `delayimp.lib`) all nine move to the delay-load table, `news board` still works, and the CLI path no longer loads them |
| Console window | `news.exe` is a console-subsystem program, so started from Explorer or the Run key it opens a console window next to Board (`GetConsoleWindow` ≠ null in every Start-Process run). With an embedded manifest `<consoleAllocationPolicy>detached</consoleAllocationPolicy>` (Windows 11 24H2+; this laptop is build 26300) the same start has **no console** (`console_window: false`) |

Not verified: how a detached-policy `news.exe` behaves when typed in an interactive terminal (the agent's shell has no console). Check in Windows Terminal that `news run status` still prints; if it does not, drop the manifest and call `FreeConsole()` at the top of `news board` when `GetConsoleProcessList` reports only this process (a brief flash at log-on).

### Recommendation

Go: eframe 0.37 on glow, Board in `news.exe` as decided, plus delay-loaded GUI DLLs and the console policy above. Toasts from `tauri-winrt-notification`, tray from `tray-icon`. If the +6.4 MB or the 8–14-minute release link become a nuisance during development, the cheap escape is a cargo feature `board` (default on for the installed build, off for CI and test builds), not a second binary.

## Recommended crates (pinned)

| Use | Crate | Version | Notes |
| --- | --- | --- | --- |
| Service lifecycle and SCM entry | `windows-service` | 0.8.1 | MIT/Apache-2.0; covers create, delete, start, stop, status, delayed start, failure actions, account and password |
| Win32 calls (pipe identity, jobs, tokens, LSA, console) | `windows-sys` | 0.61.2 | Already in `cli/Cargo.lock`; no new version |
| Named pipe server and client | `tokio` (`net` feature) | 1.x (lock: 1.54.1) | Already a workspace dependency; `ServerOptions` has every flag ADR 0007 needs |
| Board | `eframe` + `egui` (`glow`, `default_fonts`, `accesskit`; no `wgpu`) | 0.37.0 | MIT/Apache-2.0 |
| Charts in Board (later) | `egui_plot` | match egui 0.37 | not built in this spike |
| Tray | `tray-icon` | 0.26.1 | MIT/Apache-2.0 |
| Toasts | `tauri-winrt-notification` | 0.8.1 | MIT/Apache-2.0; `on_activated` for clicks while Board runs. Register an AUMID under `HKCU\Software\Classes\AppUserModelId\News.Board` (no admin) so toasts say "News Board", not "Windows PowerShell"; for clicks when Board is closed, use protocol activation (`news-board://open/run/81234`, registered under HKCU) that runs `news board --open …`. |
| Secrets (not spiked) | `windows-sys` `CredWriteW`/`CredReadW`, or `keyring` | — | either is fine; `windows-sys` adds nothing |

## Changes to ADR 0007

1. **Spawn marker: add Low integrity.** "Every `claude -p` is started at Low integrity inside the AI job." Keep the job. Rationale and the measured WMI escape are in item 3. Move "If the spike shows that a marked process can start a process outside its job through another service (for example WMI), the drafting calls must stay tool-less" from the residual risks into the decision: the spike showed it, so drafting calls stay tool-less **and** every call runs at Low integrity.
2. **Install facts** for decision 12: `news service install` self-elevates (UAC), grants `SeServiceLogonRight`, takes the Microsoft-account password (not the PIN), uses delayed auto-start with restart-on-failure, and doubles as "change password". Note that passwordless sign-in is on and may need to be off during install.
3. **Docker Desktop dependency.** State that the data stack is up only while Karthik is signed in (Docker Desktop), and that the service waits for it.
4. **One binary, two subsystems.** Decision 4 stands, with two build rules: the GUI DLLs are delay-loaded (`/DELAYLOAD` + `delayimp.lib` in `cli/crates/news/build.rs`), so the hundreds of CLI calls per run inside `claude -p` do not load OpenGL, user32 and UI Automation; and `news board` at log-on must not open a console window (manifest `consoleAllocationPolicy=detached`, or `FreeConsole` first thing in `board`). Decision 5's log-on start is a `HKCU\…\Run` entry `news.exe board --minimized`.
5. **`unsafe` boundary.** One platform crate is exempt from `unsafe_code = "forbid"` (item 2).
6. **Naming.** `cli/crates/news-board` today is the analytics-pull crate (`traffic.pull`, "Board: the local analytics dashboard"); the egui app needs that name, or a new one, in the Board shell PR.

## Risks

- Same-user boundary: everything here protects against a model acting on injected text, not against malware running as Karthik (ADR 0007 already says so).
- Passwordless sign-in and service log-on (item 1.4): unproven until the first install.
- OAuth refresh under Low integrity (item 3): unproven over a token lifetime.
- `NtResumeProcess` and `NtQueryInformationProcess` are ntdll exports, stable for 20 years but not in the Win32 docs; both have documented fallbacks (thread handle from `CreateProcessW`; the command line is only a hint anyway).
- Docker Desktop not running before sign-in (item 4).
