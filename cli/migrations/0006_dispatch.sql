-- Dispatch (Bureau §5.4, §5.6, §5.7): the job queue, pools and recurring schedules.

create table if not exists pools (
  pool         text primary key check (pool in ('net', 'cpu', 'docling', 'render', 'ai', 'publish')),
  max_running  smallint not null check (max_running >= 0),
  paused_until timestamptz,
  note         text
);
-- §5.7 on a 4-core laptop. `ai` follows NEWS_PARALLEL: `news db migrate` updates it from the env.
insert into pools (pool, max_running, note) values
  ('net',     6, 'network-bound; per-host buckets in the Runner'),
  ('cpu',     2, 'parse, XBRL, embed, score, brief'),
  ('docling', 1, 'heavy; one at a time'),
  ('render',  2, 'one warm Bun render server serves both slots'),
  ('ai',      3, 'NEWS_PARALLEL'),
  ('publish', 1, 'paced by the write-API token bucket')
on conflict (pool) do nothing;

create table if not exists jobs (
  job_id       bigserial primary key,
  kind         text not null,
  pool         text not null references pools,
  run_id       uuid,
  item_ref     text,
  args         jsonb not null default '{}',
  unique_key   text,
  priority     smallint not null default 2 check (priority between 0 and 4),
  state        text not null default 'available'
               check (state in ('available', 'scheduled', 'running', 'retryable', 'snoozed', 'completed', 'discarded', 'cancelled')),
  attempt      smallint not null default 0 check (attempt >= 0),
  max_attempts smallint not null default 6 check (max_attempts >= 1),
  snoozes      smallint not null default 0 check (snoozes >= 0),
  run_after    timestamptz not null default now(),
  lease_until  timestamptz,
  locked_by    text,
  error_class  text,
  errors       jsonb not null default '[]',
  result       jsonb,
  created_at   timestamptz not null default now(),
  finished_at  timestamptz,
  -- A running job always has a lease; a finished one never does.
  check ((state = 'running') = (lease_until is not null))
);
-- Unique jobs: one live job per key (River's unique jobs).
create unique index if not exists jobs_unique_live on jobs (unique_key)
  where unique_key is not null and state not in ('completed', 'discarded', 'cancelled');
-- The claim's scan.
create index if not exists jobs_ready on jobs (pool, priority, run_after)
  where state in ('available', 'scheduled', 'retryable', 'snoozed');
-- Capacity count and the rescuer's scan.
create index if not exists jobs_running on jobs (pool, lease_until) where state = 'running';
create index if not exists jobs_dead on jobs (error_class, job_id) where state = 'discarded';
create index if not exists jobs_run on jobs (run_id) where run_id is not null;

-- Recurring jobs (§5.6): cron in UTC; a laptop that slept runs each schedule once at wake.
create table if not exists schedules (
  schedule_id      text primary key,
  kind             text not null,
  pool             text not null references pools,
  args             jsonb not null default '{}',
  cron             text not null,
  next_at          timestamptz not null,
  enabled          boolean not null default true,
  last_enqueued_at timestamptz,
  last_job_id      bigint,
  note             text
);
-- Seeded disabled: their handlers land with the subsystems (#9 onwards), and the worker itself is
-- optional (decision D8). Source discovery schedules are added per source by Contacts.
insert into schedules (schedule_id, kind, pool, cron, next_at, enabled, note) values
  ('calendar.sync', 'calendar.sync', 'net', '0 6 * * *',  now(), false, 'Diary: daily 06:00 UTC'),
  ('work.prune',    'work.prune',    'cpu', '30 3 * * *', now(), false, 'working files and snapshot retention'),
  ('traffic.pull',  'traffic.pull',  'net', '0 5 * * *',  now(), false, 'Cloudflare Web Analytics into traffic_daily')
on conflict (schedule_id) do nothing;
