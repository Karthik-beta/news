-- Chief (ADR 0008 §3, §12): one-time reshape of the #5 run tables into the Bureau service's shape.
-- Nothing wrote `runs` before #10, so a `runs` without `settings_version` that has rows is refused,
-- never dropped. This file is in news-store's RESHAPES, the only files allowed a `do` block or a
-- `drop table`.

do $$
begin
  if to_regclass('runs') is not null and not exists (
       select 1 from information_schema.columns
        where table_schema = current_schema() and table_name = 'runs' and column_name = 'settings_version') then
    if exists (select 1 from runs) then
      raise exception 'runs has rows; the one-time reshape of ADR 0008 will not drop them';
    end if;
    drop table if exists approvals, run_steps, runs;
  end if;
end $$;
drop table if exists start_tickets;   -- ADR 0007; a no-op once #30 has dropped it

create table if not exists runs (
  run_id           uuid primary key,
  short_id         text unique not null check (short_id ~ '^r_[a-z0-9]{4,16}$'),
  preset           text not null,
  args             jsonb not null,
  heavy            boolean not null,
  origin           text not null check (origin in ('board', 'schedule')),
  schedule_key     text,
  slot_at          timestamptz,
  settings_version integer not null references standing (version),
  status           text not null check (status in ('queued', 'running', 'waiting_approval', 'waiting_ai', 'pausing',
                                                   'paused', 'succeeded', 'failed', 'cancelled', 'skipped')),
  lineup_rank      smallint not null default 1 check (lineup_rank in (0, 1)),
  queued_at        timestamptz,
  start_by         timestamptz,
  stop_mode        text check (stop_mode in ('drain', 'now', 'cancel')),
  paused_reason    text check (paused_reason in ('karthik', 'usage_limit', 'quiet_hours', 'quota', 'update')),
  resume_at        timestamptz,
  current_step     text,
  config           jsonb not null,
  budget           jsonb not null,
  created_at       timestamptz not null default now(),
  started_at       timestamptz,
  last_seen_at     timestamptz,
  finished_at      timestamptz,
  summary          jsonb,
  constraint runs_wrap_required check (status not in ('succeeded', 'failed', 'cancelled', 'skipped') or summary is not null),
  constraint runs_finished check ((status in ('succeeded', 'failed', 'cancelled', 'skipped')) = (finished_at is not null)),
  constraint runs_schedule check ((origin = 'schedule') = (schedule_key is not null and slot_at is not null)),
  constraint runs_queued check (status <> 'queued' or queued_at is not null),
  constraint runs_start_by check (start_by is null or (origin = 'schedule' and started_at is null)),
  constraint runs_started check (started_at is not null or status in ('queued', 'skipped', 'cancelled')),
  constraint runs_stop_mode check ((status = 'pausing') = (stop_mode is not null)),
  constraint runs_paused_reason check ((status in ('pausing', 'paused')) = (paused_reason is not null)),
  constraint runs_resume_at check (resume_at is null or status in ('pausing', 'paused'))
);
-- One live or queued run per preset; one heavy run holding the laptop (a pausing run keeps it until its work stops).
create unique index if not exists runs_one_per_preset on runs (preset)
  where status in ('queued', 'running', 'waiting_ai', 'waiting_approval', 'pausing', 'paused');
create unique index if not exists runs_one_heavy on runs ((true))
  where heavy and status in ('running', 'waiting_ai', 'pausing');
create index if not exists runs_lineup on runs (lineup_rank, queued_at, run_id) where status = 'queued';
create index if not exists runs_recent on runs (created_at desc);

create table if not exists run_steps (
  run_id      uuid not null references runs,
  seq         smallint not null check (seq >= 1),
  step        text not null,
  kind        text not null check (kind in ('code', 'ai_batch', 'draft', 'form', 'approval')),
  status      text not null check (status in ('pending', 'running', 'done', 'skipped', 'failed')),
  attempt     smallint not null default 0 check (attempt >= 0),
  input_hash  bytea check (input_hash is null or octet_length(input_hash) = 32),
  output      jsonb,
  error       jsonb,
  started_at  timestamptz,
  finished_at timestamptz,
  primary key (run_id, seq),
  unique (run_id, step)
);

create table if not exists gates (
  run_id      uuid not null,
  gate        text not null check (gate ~ '^[a-z]+(:[A-Za-z0-9_.-]{1,64})?$'),
  round       smallint not null check (round >= 1),
  step_seq    smallint not null,
  subject_sha bytea not null check (octet_length(subject_sha) = 32),
  summary     jsonb not null,
  preview     jsonb,
  opened_at   timestamptz not null,
  answered_at timestamptz,
  primary key (run_id, gate, round),
  unique (run_id, gate, round, subject_sha),
  foreign key (run_id, step_seq) references run_steps (run_id, seq)
);
create unique index if not exists gates_one_open on gates (run_id, gate) where answered_at is null;

create table if not exists approvals (
  run_id      uuid not null,
  gate        text not null,
  round       smallint not null,
  decision    text not null check (decision in ('approved', 'rejected', 'changes')),
  note        text,
  decided_at  timestamptz not null,
  subject_sha bytea not null,
  origin      text not null check (origin = 'board'),
  consumed_at timestamptz,
  primary key (run_id, gate, round),
  -- The approval answers exactly the hash the gate holds.
  foreign key (run_id, gate, round, subject_sha) references gates (run_id, gate, round, subject_sha),
  check (decision <> 'changes' or length(btrim(coalesce(note, ''))) > 0),
  check (consumed_at is null or decision = 'approved')
);
