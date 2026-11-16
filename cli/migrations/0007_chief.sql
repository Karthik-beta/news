-- Chief (Bureau §5.5): preset runs, their steps, approvals and start tickets. The logic is #10.

create table if not exists start_tickets (
  ticket_id   bigserial primary key,
  action      text not null default 'start' check (action in ('start', 'resume', 'pause')),
  preset      text not null,
  args        jsonb not null,
  canonical   text not null,
  prompt      text not null,
  match       text not null check (match in ('grammar', 'phrase', 'model', 'terminal')),
  state       text not null check (state in ('pending_confirm', 'issued', 'consumed', 'expired')),
  origin      text check (origin in ('prompt_hook', 'terminal')),
  created_at  timestamptz not null default now(),
  issued_at   timestamptz,
  consumed_at timestamptz,
  run_id      uuid
);

create table if not exists runs (
  run_id       uuid primary key,
  short_id     text unique not null check (short_id ~ '^r_[a-z0-9]+$'),
  preset       text not null,
  args         jsonb not null,
  heavy        boolean not null,
  origin       text not null,
  ticket_id    bigint not null references start_tickets,
  status       text not null check (status in ('running', 'waiting_approval', 'waiting_ai', 'paused', 'succeeded', 'failed', 'cancelled')),
  current_step text,
  config       jsonb not null,
  budget       jsonb not null,
  started_at   timestamptz not null,
  last_seen_at timestamptz not null,
  finished_at  timestamptz,
  summary      jsonb,
  -- Every run ends in a Wrap.
  constraint runs_wrap_required check (status not in ('succeeded', 'failed', 'cancelled') or summary is not null)
);
-- One live run per preset; one heavy run holding the laptop at a time (§8.1).
create unique index if not exists runs_one_per_preset on runs (preset)
  where status in ('running', 'waiting_ai', 'waiting_approval', 'paused');
create unique index if not exists runs_one_heavy on runs ((true))
  where heavy and status in ('running', 'waiting_ai');

create table if not exists run_steps (
  run_id      uuid not null references runs,
  seq         smallint not null,
  step        text not null,
  kind        text not null check (kind in ('code', 'ai_batch', 'session', 'approval')),
  status      text not null check (status in ('pending', 'running', 'done', 'skipped', 'failed')),
  input_hash  bytea,
  output      jsonb,
  started_at  timestamptz,
  finished_at timestamptz,
  primary key (run_id, seq)
);

create table if not exists approvals (
  run_id      uuid not null references runs,
  gate        text not null,
  round       smallint not null default 1,
  decision    text not null check (decision in ('approved', 'rejected', 'changes')),
  note        text,
  decided_at  timestamptz not null,
  subject_sha bytea not null,
  origin      text not null check (origin in ('prompt_hook', 'terminal')),
  consumed_at timestamptz,
  primary key (run_id, gate, round)
);
