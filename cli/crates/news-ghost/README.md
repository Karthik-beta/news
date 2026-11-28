# news-ghost

Ghost runs every `ai.*` job (Bureau Design §3.9, §7; ADR 0007, ADR 0008), inside the Bureau service. Dispatch's worker claims the job and renews its lease; `news_ghost::handle` runs one lean `claude -p`:

| What | Where |
| --- | --- |
| The §3.9 flag set, `pipeline/claude/page-call.settings.json` | `claude::argv` |
| The child environment, built from an allow-list: no `ANTHROPIC_API_KEY`, AWS, Cloudflare, database or write-API secret; `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1`, `NEWS_SPAWNED_BY` (a label only) and `OTEL_RESOURCE_ATTRIBUTES` per job | `claude::child_env` |
| Windows: created suspended, put in the service's AI Job Object, then resumed (the spawn marker); no Job Object, no call | `Ghost::spawn`, `news_win::Job::adopt_suspended` |
| The output goes to `news ai accept`; an A1 Copy gets one A2 repair (errors, previous Copy, brief), then `ai_invalid_output` | `AcceptSink`, `Ghost::run` |
| A usage-limit reply is `ai_usage_limit` with `retry_after` = the reset: Dispatch pauses the `ai` pool, Chief pauses the runs and resumes them on its own | `claude::classify` |
| A run at `quota.ai_run_tokens` starts no new call (the job completes as `skipped: token_ceiling`) | `Ghost::run`, `store::Limits` |
| One `ai_calls` row per call (PostgreSQL, migration `0750`), mirrored to ClickHouse `news.ai_calls` | `store::PgStore` |
| Batches enqueued grouped by prefix file, so the prompt cache stays warm | `order_by_prefix` |

Job arguments, paths relative to the repository root (no absolute paths or `..`; `input` must resolve under `work/`, `prefix` under `work/prefix/`, `schema` under `contracts/`): `{"input", "schema", "prefix"?, "step"?}`; the step defaults from the kind (`ai.write` A1, `ai.repair` A2, `ai.classify` A3, `ai.storymatch` A4, `ai.condense` A5, `ai.spotcheck` A7). The model comes from Standing (`models.main`, `models.light`, `models.curator`).

The usage-limit reply's shape is not settled (ADR 0008 §1). `claude::classify` reads it conservatively and is the one place to fix it after the first real limit.

## Tests

`cargo test -p news-ghost` runs Ghost against `fake-claude` (`src/bin/fake_claude.rs`, built only with the `fake-claude` feature, which a dev-dependency turns on for tests), a test-only stand-in that records its argv, environment and stdin and replies as its input asks. Never the real CLI, no network. The Job Object tests are `cfg(windows)`.

## Wiring left

- The service installs Ghost (`news_ghost::install`) once `news ai accept` exists as a library call: an `AcceptSink` over it, a `PgStore` on the service's pool (with `ClickHouse::from_env()`), the root marker job from `host::run` as `job`, and the `claude.exe` path. Until then the handler refuses before any call (`bug`).
- Not here yet: per-run nested Job Objects (so a pause can terminate one run's calls), the OAuth pre-refresh. (Each call already runs at Low integrity in a Low-labelled folder, ADR 0008 §8.2, #89.)
- The flag set follows Bureau §3.9 as written; the doc gives no value for `--setting-sources`, so calls pass the empty list (load no settings source); the lean-call spike must confirm that value (ADR 0014 lists what it measures).
