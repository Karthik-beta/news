# Spikes

Throwaway proofs, not part of `news.exe`. Their own Cargo workspace (so `cli/`'s `unsafe_code = "forbid"` does not apply). Findings: `docs/spikes/bureau-service.md`.

| Crate | Proves | Run (from `cli/spikes`) |
| --- | --- | --- |
| `bureau-spike` | Service lifecycle, the Hotline pipe and its client checks, the Job Object marker, Low-integrity calls, `claude -p` and the databases from a service-like environment | `cargo run -p bureau-spike -- console --spawn job-demo` (also `wmi-escape`, `low-il`, `claude-ping`, `claude-tool`, `claude-ping-low`); `-- probe --dotenv ..\..\.env`; `-- client --cmd status`; service lifecycle: `bureau-spike\elevated-service-test.ps1` in an elevated PowerShell |
| `board-spike` | eframe Board: token colours, light/dark, start minimised, tray, toast, size and start-up | `cargo run --release -p board-spike -- --minimized`; features `tray`, `toast`, `detached-console`. `board-spike/news-board-link.patch` links it into `news` for the size test |

The `claude-*` scenarios make real (tiny, Haiku) `claude -p` calls on the subscription.
