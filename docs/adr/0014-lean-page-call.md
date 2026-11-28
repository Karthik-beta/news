# ADR 0014: The lean page call

- Status: proposed (the lean-call spike has not run; its measurements are still to come)
- Date: 2026-11-28
- Issue: #18 (Phase 7): Fixer and Ghost, briefs, prefixes and lean page calls
- Context documents: Bureau Design §3.8 (Fixer), §3.9 (Reporter: the call shape and why each flag), §7 (AI budget), §9.3 (secrets); Technical Design "Keeping each claude -p call lean"; ADR 0007; ADR 0008 §1 (the service spike), §8.2 (the spawn marker and Low integrity); `cli/crates/news-ghost/README.md`

## Context

Every AI step (A1–A12) is one `claude -p` call that Ghost starts inside the Bureau service. Each call must be lean (no tools, no MCP servers, no hooks, plugins or memory files, at most two turns), safe (no secret in its environment, unable to send a control command) and cheap (a prefix that is the same bytes for every call of a page type, so back-to-back calls read it from the prompt cache). Bureau §3.9 gives the flag set from the Claude Code documentation and marks the settings isolation "unverified"; it asks a spike to confirm the flags, the settings isolation, the cache TTL and the cache-read ratio across back-to-back calls.

That spike needs Karthik's real `claude` login and spends his plan's quota, so no agent can run it on its own. Fixer (#80), Ghost (#77) and the Low-integrity spawn (#89) were built on the documented flags. This record writes down what is implemented and what is already known, and lists what the spike must measure. When the spike has run, its numbers go into the table below and the status becomes accepted.

## What is already settled

From the service spike (ADR 0008 §1), not from this one:

- `claude -p` started by the service under Karthik's account uses his subscription login (`%USERPROFILE%\.claude\.credentials.json`), from an environment cut to the allow-list, inside a Job Object, at Medium and at Low integrity.
- The usage-limit reply's shape and reset time are **not** settled; `news_ghost::claude::classify` reads it conservatively (ADR 0008 §8.4).

From the Claude Code documentation as cited in Bureau §3.9 (documented, not measured here): `--tools ""` disables every built-in tool; `--strict-mcp-config` with an empty config stops project MCP servers loading; `--json-schema` makes the harness validate the result; `--max-budget-usd` is a client-side estimate; `ANTHROPIC_API_KEY`, when set, replaces the subscription; `--bare` never reads subscription credentials and must not be used; `-p` main-conversation requests get a 1-hour cache TTL on a subscription.

## Decision: the call as built

`news_ghost::claude::argv` is the one place the flag set lives:

```
claude -p --model <models.*> --tools "" --strict-mcp-config --mcp-config {"mcpServers":{}}
  --settings <root>/pipeline/claude/page-call.settings.json --setting-sources ""
  --disable-slash-commands --no-session-persistence
  --append-system-prompt-file <root>/work/prefix/<page-type>.<hash12>.md
  --json-schema <schema JSON> --max-turns 2 --max-budget-usd 0.60 --output-format json
  < <brief or repair input>
```

`tests/ghost.rs` (`the_argv_is_the_golden_call_and_a_valid_copy_is_accepted`) pins this argv against the fake `claude`.

Around the flags:

- **Environment.** Built from an allow-list (`claude::ENV_ALLOW`), never by removing names: no `ANTHROPIC_API_KEY`, AWS, Cloudflare, database, ClickHouse, write-API, GitHub, proxy or OTLP-header variable. Ghost sets `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1`, `NEWS_SPAWNED_BY` (a log label) and `OTEL_RESOURCE_ATTRIBUTES` per job.
- **Process.** Created suspended at Low integrity, put in the service's AI Job Object, then resumed; its working folder is a per-job, Low-labelled folder outside the repository, so no project `CLAUDE.md` or `.claude/` settings are in reach (ADR 0008 §8.2).
- **Settings file.** `pipeline/claude/page-call.settings.json`: `disableAllHooks`, no project MCP servers, and every tool denied as a second layer behind `--tools ""`.
- **Prefix.** Built by `news_fixer::prefix::build`: instructions, `news-brand`, one `page-*` skill, the component index and the page type's pre-selected specs, CRLF-normalised, written as `<page-type>.<hash12>.md`. The same inputs give the same file name in any process (`prefix_hash_is_stable_across_runs`). Ghost accepts a prefix only from `work/prefix/`, so a brief and its untrusted `source_text` can never become the system prompt. Batches are enqueued grouped by prefix file (`order_by_prefix`) so calls with one prefix run back to back.

### Differences from Bureau §3.9

| §3.9 says | Built | Why |
|---|---|---|
| `--settings` with no `--setting-sources` value | `--setting-sources ""` (load no user, project or local settings) | Without it, Karthik's user settings, `~/.claude/CLAUDE.md` and rules could load into page calls; the spike must confirm the empty list is accepted and means "none" |
| Schema from `pipeline/schemas/page-draft.v1.json` | From `contracts/cli/ai/page-draft.v1.schema.json`, confined to `contracts/` | The contracts folder is where every AI schema lives |
| Relative paths from the repository | Absolute paths resolved from the repository root (symlinks resolved, `..` refused) | The call runs in a per-job folder outside the repository |
| Medium integrity implied | Low integrity, Low-labelled work folder | ADR 0008 §1: a marked process can leave its job through WMI unless it runs at Low |

## What the spike measures

Run with Karthik's login, from the service (or the same allow-listed environment at Low integrity), on one page type's real prefix and three to five real briefs, back to back, then again after 10 and after 70 minutes.

| Question | Result |
|---|---|
| Cache-read ratio across back-to-back calls with one prefix (`cache_read_input_tokens` ÷ all input tokens, from `usage` in the JSON result) | **To be measured.** The design's estimate (Bureau §7.2, A1 row, marked speculative: ~8k cached of ~11k input, about 0.7) is an estimate, not a measurement |
| Cache TTL actually observed (a hit after 10 minutes, after 70 minutes) | To be measured; documented as 1 hour for `-p` main-conversation requests |
| Prefix size in tokens per page type (target 6–10k) | To be measured against the real component index |
| The flag set runs as written (`--tools ""` accepted, `--setting-sources ""` accepted and loads no source) | To be confirmed |
| No tool runs, no hook fires, no MCP server starts, no memory file (`~/.claude/CLAUDE.md`, rules) is loaded | To be confirmed, from a `--output-format stream-json --verbose` init line (tools, MCP servers, memory) |
| The call bills the subscription (`apiKeySource` in the init line is `none`, even with a user-settings `env.ANTHROPIC_API_KEY` or `apiKeyHelper` present) | To be confirmed |
| Turns and wall time per call; how `--json-schema` reports a retried result | To be measured |

If a flag behaves differently, the fix goes in `claude::argv` (and its golden test) and this table records the difference.

## Consequences

- Ghost and Fixer are built and tested without the real CLI: every test uses the fake `claude`, so a change in Claude Code's flags is caught only by the spike or the first real run.
- Until the spike runs, the isolation rests on documented flag behaviour plus the second layers (allow-listed environment, settings deny list, per-job folder outside the repository, Low integrity). Low integrity blocks writes, not reads: a call that somehow got a tool could read Karthik's files, so the spike's "no tool runs" check matters.
- The issue's box "ADR 0014 records the spike's measured cache-read ratio and confirmed flags" stays open until the table above is filled in.

## Alternatives considered

- **Write a ratio from the design's estimate.** Rejected: an estimate in a measurement table would be read as a measurement.
- **`--bare`.** Rejected: it never reads subscription credentials (Bureau §3.9).
- **Workflow agents or sub-agents for page writing.** Rejected in §3.9: their cache TTL is 5 minutes against 1 hour for `-p` main-conversation calls, and they bring tools.
