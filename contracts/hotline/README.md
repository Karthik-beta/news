# Hotline contracts

The Hotline is the Bureau service's control pipe, `\\.\pipe\news-hotline` ([ADR 0007](../../docs/adr/0007-bureau-service-and-board.md), [ADR 0008](../../docs/adr/0008-chief-in-the-bureau-service.md) §8). Board sends every H-tier action over it; Karthik's terminal reads status. These files are the protocol: the service, Board and the CLI build against them, the binary embeds them at build time, and every reply produced in a test is validated against them. JSON Schema 2020-12, `$id` under `https://news.karthik.nz/contracts/hotline/`.

## Files

| File | Describes |
| --- | --- |
| `request.schema.json` | A request frame: `{"v": 1, "id", "cmd", "args"}`; `args` is checked against the command's `$defs/input` |
| `reply.schema.json` | A reply frame: `{"v": 1, "id", "envelope"}`; `envelope` is the CLI envelope, `../cli/envelope.schema.json` (Bureau §6.2), whose `data` is the command's `$defs/output` |
| `event.schema.json` | An event frame on a subscribed connection: `{"v": 1, "event", "seq", "at", "data"}`; `data` is checked against `events/<topic>.schema.json#/$defs/data` |
| `common.schema.json` | Shared types: run ids, statuses, the run summary, open gates, Lineup entries, the Wrap, usage, Standing changes, preset declarations |
| `commands/<cmd>.schema.json` | One per command: `$defs/input` (the args), `$defs/output` (the envelope's data), extra `$defs` for structured `error.details` |
| `events/<topic>.schema.json` | One per event topic: `$defs/data` |
| `forms/<form>.schema.json` | One per form step, named by its handler: `$defs/answer`, what `form.submit` sends as `answer` |

`../cli/envelope.schema.json` arrives with #5 (`contracts/cli/`); until that merges the reply schema's `$ref` resolves only on that branch.

Annotations on each command (ignored by validators, read by the service and tests):

| Key | Meaning |
| --- | --- |
| `x-news-tier` | `R` (any same-user client at Medium integrity or above, including processes the service spawned) or `H` (Board only, never from a process in the service's AI job) |
| `x-hotline-from` | `["board", "terminal"]` or `["board"]` |
| `x-news-status` | The issue that implements the handler. The service registers every contracted command; one whose issue has not landed answers `precondition` naming it |
| `x-news-cli` | The terminal command that returns the same `data` in-process, or null |
| `x-news-errors` | Error codes the command can return besides `usage` and `internal` from the frame layer |

## Wire format

A frame is a little-endian `u32` length, then that many bytes of UTF-8 JSON, at most 1 MiB. A frame of length 0, over 1 MiB, or that does not parse gets a `usage` reply with `id: null` and the connection is closed.

```json
{"v": 1, "id": "b-1734", "cmd": "gate.approve", "args": {"run": "r_51d2", "gate": "plan", "round": 1, "subject_sha": "9c1e…"}}
{"v": 1, "id": "b-1734", "envelope": {"ok": true, "data": {"decision": "approved", "…": "…"}, "warnings": [], "truncated": false}}
{"v": 1, "event": "gate", "seq": 413, "at": "2026-11-12T16:02:11Z", "data": {"…": "…"}}
```

- One request at a time per connection; the reply comes before the next request is read. Request connections close after 60 seconds idle.
- `subscribe` turns a connection into an event stream; nothing else is read from it afterwards.
- Error and exit codes are the CLI's (`gated` = 7). A command sent by a client that may not send it gets `gated` with `details.refusal` (`in_ai_job`, `not_our_image`, `not_board`). A client under another user, or below Medium integrity, is disconnected without a reply.
- Before the service is `up`, every command except `service.ping` gets `upstream_unavailable` (exit 5, retryable).
- Unknown commands, and args that fail `$defs/input`, are `usage`. Reserved names without a contract (`content.*`) are unknown until their contracts land.

## Commands

| Command | Tier | From | Handler | Terminal form |
| --- | --- | --- | --- | --- |
| `service.ping` | R | Board, terminal | #10 | (inside `news run status`) |
| `status` | R | Board, terminal | #10 | `news run status` |
| `run.show` | R | Board, terminal | #10 | `news run status <run>` |
| `run.list` | R | Board, terminal | #10 | `news run list` |
| `run.presets` | R | Board, terminal | #10 | `news run presets` |
| `lineup.list` | R | Board, terminal | #10 | `news run lineup` |
| `standing.get` | R | Board, terminal | #10 | `news standing show` |
| `standing.history` | R | Board, terminal | #10 | `news standing history` |
| `service.log` | R | Board, terminal | #10 | — |
| `subscribe` | R | Board, terminal | #10 | — |
| `run.start` | H | Board | #10 | — |
| `run.start_after_pause` | H | Board | #10 | — |
| `gate.approve`, `gate.changes`, `gate.reject` | H | Board | #10 | — |
| `run.pause`, `run.resume`, `run.cancel` | H | Board | #10 | — |
| `lineup.cancel` | H | Board | #10 | — |
| `standing.set` | H | Board | #10 | — |
| `huddle.questions` | R | Board | #19 | — |
| `huddle.answer` | H | Board | #19 | — (the file form is `news scores label`) |
| `form.submit` | H | Board | #19 | — (completes a form step, such as Huddle's calibration, and continues the run) |
| `home.rollback` | H | Board | #20 | — |
| `docker.status` | R | Board, terminal | #29 | — |
| `docker.start`, `docker.stop` | H | Board | #29 | — |
| `service.update` | H | Board | #29 | — |

Terminal forms run in the calling process against PostgreSQL and return the same `data`; their `contracts/cli/` files `$ref` the `$defs/output` here.

## Events

`run`, `gate`, `lineup`, `schedule`, `usage`, `standing`, `service` (from #10); `source` (#9, #16) and `huddle` (#19) once those subsystems exist; `lagged` (`{"missed": n}`) when a subscriber fell more than 1,024 events behind. `seq` counts from 1 per service process: after a gap, a `lagged` event or a reconnect, Board reads `status` again.

## Versioning

`v` is the protocol version: frames and the three message shapes. Within `v` 1, commands and events change only additively. Board and the service come from one `news.exe`; Board compares `service.ping`'s `build` with its own and, after an update, asks Karthik to restart Board before it sends another H-tier command.

## Rules for the numbers

Every count, time, token figure and score in these messages is computed by code from PostgreSQL (or ClickHouse for usage). Every `text` field is rendered by code and shown verbatim by Board and the terminal. No model writes any of it.
