-- Critic (#19, Bureau §9.1, §8.11). Block 0800–0899 belongs to Critic and Huddle.
-- `eval_runs`: one row per `news eval run` (or Audit's metrics), with each set's metrics and the text
-- report code rendered. `prompt_version` is empty when only the golden sets were checked. `run_id` is
-- the Audit run that produced it, if any.

create table if not exists eval_runs (
  eval_run_id    bigserial primary key,
  suite          text not null,
  prompt_version text not null default '',
  run_id         uuid references runs (run_id),
  passed         integer not null check (passed >= 0),
  failed         integer not null check (failed >= 0),
  metrics        jsonb not null,
  report         text not null,
  created_at     timestamptz not null default now()
);
create index if not exists eval_runs_suite on eval_runs (suite, created_at);
