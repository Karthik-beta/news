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

-- `prompt_promotions`: what Audit's `promote` step (`news eval promote`, G tier) wrote after its
-- approval. The live version of a step is its latest row; one row per run and step, so a re-entered
-- step writes nothing twice.
create table if not exists prompt_promotions (
  promotion_id   bigserial primary key,
  step           text not null check (step in ('A1', 'A3', 'A7', 'A8')),
  prompt_version text not null,
  eval_run_id    bigint not null references eval_runs (eval_run_id),
  run_id         uuid not null references runs (run_id),
  promoted_at    timestamptz not null default now(),
  unique (run_id, step)
);
create index if not exists prompt_promotions_step on prompt_promotions (step, promoted_at);
