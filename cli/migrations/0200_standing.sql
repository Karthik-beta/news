-- Standing (ADR 0008 §10): Karthik's settings, one row per version. Every change is a new version with
-- who, when, and the old and new effective values; each run records the version it started with.

create table if not exists standing (
  version    integer primary key check (version >= 1),
  settings   jsonb not null check (jsonb_typeof(settings) = 'object'),   -- explicitly set keys; unset keys take the registry default
  changes    jsonb not null check (jsonb_typeof(changes) = 'array'),     -- [{"key", "old", "new"}], effective values
  who        text not null check (who in ('board', 'seed')),
  note       text,
  created_at timestamptz not null default now(),
  check ((version = 1) = (who = 'seed'))
);
insert into standing (version, settings, changes, who, note)
values (1, '{}', '[]', 'seed', 'registry defaults')
on conflict (version) do nothing;
