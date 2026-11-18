-- Rota's fires (ADR 0008 §7.1: once per schedule slot) and the Hotline's refused H-tier requests (§8.2).

create table if not exists rota_fires (
  schedule_key     text not null,
  slot_at          timestamptz not null,
  outcome          text not null check (outcome in ('started', 'queued', 'skipped', 'missed')),
  reason           text,          -- preset_live | quota | next_slot_close | late | …
  missed_before    integer not null default 0 check (missed_before >= 0),
  run_id           uuid references runs,
  settings_version integer not null references standing (version),
  fired_at         timestamptz not null,
  primary key (schedule_key, slot_at),
  check ((outcome in ('started', 'queued', 'skipped')) = (run_id is not null))
);

create table if not exists hotline_refusals (
  refusal_id bigserial primary key,
  at         timestamptz not null default now(),
  cmd        text not null,
  refusal    text not null check (refusal in ('wrong_user', 'low_integrity', 'in_ai_job', 'not_our_image', 'not_board')),
  pid        integer,
  image      text,
  run_id     uuid,             -- when the process was in a run's job
  job_id     bigint
);
create index if not exists hotline_refusals_at on hotline_refusals (at);
