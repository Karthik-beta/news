-- Contacts (Bureau §3.1), Wire's Morgue (§3.2) and Diary (§3.3).

create table if not exists sources (
  source_id        text primary key,
  name             text not null,
  publisher        text not null,
  connector        text not null,
  base_url         text not null,
  source_tier      smallint not null check (source_tier between 1 and 4),
  licence          text not null check (licence in ('public-domain', 'cc-by-4.0', 'custom', 'unclear')),
  attribution      text not null,
  redistribution   boolean not null,
  image_policy     text not null,
  auth_env         text,                -- NAME of the env var holding a free key, never the key
  rate_per_sec     numeric(6,3) not null check (rate_per_sec > 0),
  max_concurrency  smallint not null default 1 check (max_concurrency >= 1),
  daily_quota      integer check (daily_quota is null or daily_quota > 0),
  robots_respect   boolean not null default true,
  regions          text[] not null,
  categories       text[] not null,
  calendar_kind    text check (calendar_kind in ('ics', 'api', 'html', 'derived')),
  enabled          boolean not null default true,
  health           text not null default 'unknown' check (health in ('unknown', 'green', 'amber', 'red')),
  notes            text
);

create table if not exists snapshots (
  sha256        bytea primary key check (octet_length(sha256) = 32),
  source_id     text not null references sources,
  url           text not null,
  fetched_at    timestamptz not null,
  http_status   smallint not null,
  content_type  text,
  bytes         bigint not null check (bytes >= 0),
  etag          text,
  last_modified text,
  storage_path  text not null,
  retain_until  timestamptz
);
create index if not exists snapshots_source_fetched on snapshots (source_id, fetched_at desc);

create table if not exists fetch_state (
  url_hash           bytea primary key,
  url                text not null,
  source_id          text not null references sources,
  etag               text,
  last_modified      text,
  last_status        smallint,
  last_sha256        bytea,
  last_fetched       timestamptz,
  next_due           timestamptz,
  consecutive_errors smallint not null default 0
);
create index if not exists fetch_state_due on fetch_state (next_due);

create table if not exists calendar_events (
  cal_id      bigserial primary key,
  source_id   text not null references sources,
  event_key   text not null unique,
  title       text not null,
  expected_at timestamptz,
  expected_day date not null,
  precision   text not null check (precision in ('exact', 'day', 'window')),
  window_days smallint not null default 0,
  origin      text not null check (origin in ('ics', 'api', 'html', 'derived')),
  status      text not null default 'expected' check (status in ('expected', 'captured', 'late', 'cancelled')),
  captured_at timestamptz
);
create index if not exists calendar_events_day on calendar_events (expected_day, status);
