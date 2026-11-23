-- Ghost (#18): one row per `claude -p` call. The per-run token ceiling (`quota.ai_run_tokens`) sums
-- `tokens`, and Board's AI-call records read it; the same row is mirrored to ClickHouse `news.ai_calls`
-- (infra/clickhouse/003_ticker.sql) for the quota guard and dashboards. No foreign keys: rows outlive
-- pruned jobs.

create table if not exists ai_calls (
  call_id        bigserial primary key,
  at             timestamptz not null,
  run_id         uuid,
  job_id         bigint not null,
  step           text not null,                 -- A1 … A12
  model          text not null,
  prompt_version text not null default '',       -- the prefix file's name (its hash)
  tokens_in      bigint not null default 0,
  tokens_out     bigint not null default 0,
  cache_read     bigint not null default 0,
  cache_write_5m bigint not null default 0,
  cache_write_1h bigint not null default 0,
  tokens         bigint not null default 0,      -- input + output + cache writes (news_rota::tokens)
  turns          integer not null default 0,
  duration_ms    bigint not null default 0,
  subtype        text not null default '',       -- success, error_max_turns, usage_limit, spawn_failed, …
  validation     text not null default '',       -- valid, invalid:<rule>, or empty
  repair_round   smallint not null default 0,
  est_cost_usd   double precision not null default 0,
  input_file     text not null,
  output_file    text
);

create index if not exists ai_calls_run on ai_calls (run_id) where run_id is not null;
