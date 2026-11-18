# news-board

Board, the desktop app ([ADR 0007](../../../docs/adr/0007-bureau-service-and-board.md), decision 3): egui inside `news.exe`, started by `news board`. It talks only to the Bureau service over the Hotline; it opens no listener and reads no database.

## Run it

```
news board                       open the window
news board --minimized           open in the taskbar (the log-on entry uses this)
news board --open gate:r_51d2.plan   open on an item (what a toast's click runs)
news board autostart on|off|status   Board's log-on entry (HKCU Run) for your user
```

`news service install` also adds the log-on entry; `uninstall` removes it. The window is Windows-only (eframe on the glow renderer, `cfg(windows)`); the rest of the crate builds and is tested everywhere.

## What it holds

| Module | Does |
| --- | --- |
| `link` | The Hotline client: a thread with a Tokio runtime, one request connection and one event connection, reconnects, a `Connect` trait so tests run against an in-memory server. The real connector checks that the pipe's server is the installed `news.exe` and sends nothing otherwise (offline banner). |
| `app` | State and the actions behind every control. Every H-tier command is one method called by one click; the reply's code-rendered `text` is what Board shows. If `service.ping`'s `build` differs from Board's, Board sends no H-tier command until restarted. |
| `logic` | Forms into arguments: `run.start` args from the preset declaration, one `standing.set` from edited settings, a gate answer bound to the `subject_sha` and round shown. Values go as typed; nothing converts a number, unit or period. |
| `ui` | The window: navigation, offline banner (Start service through the SCM, the last error), Runs & approvals, Settings & schedules, Docker health. |
| `theme` | egui visuals and styles from `design/tokens/tokens.json`. `build.rs` generates `tokens.rs` (light and dark palettes, type sizes, spacing, radii, borders); a test fails on any literal colour in `src/`. Follows the Windows light or dark setting, with a manual override. |
| `toast` | Toasts from the event stream (gate waiting, run failed, usage limit, source red, Huddle questions); a click opens Board on that item. |
| `secrets` | Windows Credential Manager under `news/<name>` (`news_core::secrets::SECRETS`). A secret goes from its field to `CredWriteW` and nowhere else: no Hotline command carries one, `Debug` is masked, the field is emptied on save. |

## Screens (Bureau §8.1, step 1)

- **Runs & approvals**: the status board's "next" line; gates with their summary, preview, hash and round and Approve, Request changes (a note is required) and Reject; paused runs with Resume; running runs with Pause and Cancel (asks twice); the Lineup with Remove; a Start form built from `run.presets` (the heavy-slot busy reply becomes "Queue it" / "Pause the running one and start this"); run detail with the Wrap; problems, including refused control attempts.
- **Settings & schedules**: every Standing key with its default, edit, reset and history; schedules as JSON checked before sending; one Save is one `standing.set` with `base_version` (a `conflict` reloads); secrets entry; the service's build and `Update the service`.
- **Docker health**: the News containers with Start and Stop, the shared ClickHouse health only.

## Not yet

Screens of steps 2–5 (#16, #18, #21 and later), a single-instance guard, a registered toast app id (toasts show under the PowerShell id) and toast clicks while Board is closed (Board shows what it missed from the status board when it opens).

## Tests

`cargo test -p news-board`: unit tests for the forms, theme generation and the no-literal-colours lint, and the app against an in-memory Hotline (`news_hotline::serve_conn` with a fake service): connect and build check, gate answers bound to the hash shown, CLI-style peers refused as `gated`, the queue offer, set and cancel, events into toasts, the offline banner, and the secret sentinel. Every screen is drawn headless in both themes. `crates/news/tests/postgres/board.rs` drives Board's client against the real service on PostgreSQL.
