# ADR 0003: Dispatch, the job engine

- Status: accepted
- Date: 2026-11-15
- Issue: #5 (Phase 5, wave 1)
- Context documents: Bureau Design §5.1–§5.9, §8.14, §9.3; decision D1 (custom Rust queue on PostgreSQL with River-style semantics; apalis-postgres as the fallback) and D8 (optional background worker)

## Context

Hundreds of small jobs per run go through pools with different limits on a 4-core laptop. Jobs must survive crashes, sleep and usage-limit stops, never run twice at once, and be reachable from Bun only through `news jobs`. PostgreSQL 17 is already running; no new server is wanted.

## Decision

**A small queue in PostgreSQL, owned by `news-dispatch`, with River's semantics.**

- **Model.** `jobs` (§5.4, plus a check that a job has a lease exactly when it is `running`), `pools` (seeded per §5.7; `ai` follows `NEWS_PARALLEL`), `schedules`. States: `available`, `scheduled`, `running`, `retryable`, `snoozed`, `completed`, `discarded`, `cancelled`.
- **Claim.** One transaction: lock the pool's row `FOR UPDATE` (claimers of one pool queue for a few milliseconds instead of skipping), return nothing if the pool is paused, then take ready jobs `ORDER BY priority, run_after, job_id FOR UPDATE SKIP LOCKED` with `LIMIT least(requested, max_running − running)`. Because the running count is read under the pool lock, `max_running` is exact under any number of workers; `SKIP LOCKED` keeps two claimers off the same job.
- **Slots** are counted from `state = 'running'`, so completing, failing, snoozing or rescuing a job frees its slot in the same statement. There is no counter to drift.
- **Leases and heartbeats.** A claim sets `lease_until`; the worker heartbeats every lease/3 (2 minutes for code kinds, 10 for AI). Only the holder (`locked_by`) can heartbeat, complete or fail a job; anything else is `conflict`.
- **Rescuer.** Whoever gets `pg_try_advisory_xact_lock(42)` locks every pool row in pool order (claimers hold at most one, so no deadlock), then moves expired `running` jobs to `retryable`, or `discarded` when their attempts are spent, recording a `transient` "lease expired" error. `news work` runs it every 30 seconds and at start.
- **Retries by error class** (`news_core::job::ErrorClass`): `transient` → `retryable` after full-jitter backoff `random(0, min(1 h, 30 s · 2^attempt))`, discarded after `max_attempts` (6); `rate_limited` → `snoozed` until `Retry-After` (default 60 s) with the attempt given back and `snoozes + 1`; `ai_usage_limit` → the pool's `paused_until` moves out (default 1 h) and the job is snoozed to then, attempt given back; every other class → `discarded` (the dead list). The last five errors are kept on the job.
- **Unique keys.** `jobs_unique_live` is a partial unique index over live states; enqueue is `INSERT … ON CONFLICT … DO NOTHING` and returns the existing `job_id` when the key is live.
- **Schedules.** Under `pg_try_advisory_xact_lock(43)`, every enabled schedule with `next_at <= now()` enqueues one job with unique key `schedule:<id>` and moves `next_at` to the first cron occurrence (UTC) after now. A laptop that slept through several runs gets one job, and a run still waiting is not doubled. The three seeded schedules start disabled until their handlers exist.
- **Registry.** `news_dispatch::KINDS` lists every kind with its pool, lease and owning subsystem; the binary binds owners to subsystem handlers. Unknown kinds fail as `bug`. A handler panic is caught (the handler runs in its own task) and recorded as `bug`.
- **`news work`** runs only `net`, `cpu` and `docling`; `ai`, `render` and `publish` are refused with exit 2. It forbids the write-API key before anything else and reports `write_api_key_reads` (always 0). Windows Task Scheduler can start it at log-on (`infra/windows/news-work.task.xml`).
- **Salvage and Karthik.** `news jobs retry` re-enqueues discarded jobs with fresh attempts; above 50 at once it is refused in an agent shell (`gated`). `news jobs pause|unpause` are H tier.

## Tests

PostgreSQL tests (each in its own schema) cover: 8 concurrent workers × 200 jobs across three pools, with the database's running count sampled throughout, never exceeding `max_running` and never claiming a job twice; a worker process that claims and dies, whose job the next worker's rescuer returns to `retryable`; a `rate_limited` snooze leaving `attempt` unchanged; concurrent enqueues of a live unique key returning one `job_id`; backoff bounds and discard at `max_attempts`; the dead list and retry; pool pause on a usage limit; heartbeats and holder checks; a schedule that slept through three days firing once; and a live `news work` round with a sentinel `NEWS_WRITE_API_KEY` that never reads it.

## Consequences

- About 1,000 lines of Rust and SQL that the project owns. If it proves unreliable, D1's fallback is apalis-postgres for the worker loop, keeping these tables.
- Bun (Ghost) shares the exact rules through `news jobs`, at the cost of one process per call; batches (`--limit`) keep that cheap.
- Capacity is enforced per pool only. Per-host rate limits are the Runner's job (#9).
