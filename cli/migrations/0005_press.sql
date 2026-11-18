-- Press (Bureau §3.12), the TD's ledger and home_curations, and analytics (§8.14).

create table if not exists pages (
  page_id          text primary key,
  slug             text unique not null,
  story_id         bigint references stories,
  page_type        text,
  tier             smallint check (tier in (1, 2)),
  version          int not null check (version >= 1),
  status           text not null check (status in ('draft', 'ready', 'published', 'withdrawn')),
  published_at     timestamptz,
  updated_at       timestamptz,
  index_status     text not null default 'noindex' check (index_status in ('noindex', 'index')),
  renderer_version text,
  bundle_sha       bytea,
  oui_sha          bytea,
  flags            text[] not null default '{}'
);

create table if not exists page_refs (
  page_id text not null references pages,
  version int not null,
  ref     text not null check (ref ~ '^(dp|dv|sr|pd|q|cx)_[a-z0-9]+(_[a-z0-9]+)*$'),
  primary key (page_id, version, ref)
);
create index if not exists page_refs_ref on page_refs (ref);

create table if not exists publish_outbox (
  outbox_id       bigserial primary key,
  page_id         text not null,
  version         int not null,
  bundle_sha      bytea not null,
  idempotency_key text not null unique,
  op              text not null check (op in ('put_page', 'withdraw', 'index_promote', 'home_curation')),
  created_at      timestamptz not null default now(),
  sent_at         timestamptz,
  response        jsonb,
  attempts        int not null default 0
);
create index if not exists publish_outbox_unsent on publish_outbox (outbox_id) where sent_at is null;

-- TD 'ledger': stops reprocessing (source URL, content hash, page URL, publish date, version).
create table if not exists ledger (
  ledger_id    bigserial primary key,
  source_url   text not null,
  content_sha  bytea not null,
  page_url     text,
  published_at timestamptz,
  version      int,
  unique (source_url, content_sha)
);

-- TD 'home_curations': drafts and published curations (Frontpage).
create table if not exists home_curations (
  draft_id      bigserial primary key,
  run_id        uuid,
  status        text not null default 'draft' check (status in ('draft', 'checked', 'approved', 'published', 'rejected', 'superseded')),
  version       int unique,
  slots         jsonb not null,
  figures       text[] not null default '{}',
  rationale     text,
  check_results jsonb,
  created_at    timestamptz not null default now(),
  approved_at   timestamptz,
  published_at  timestamptz
);

-- Daily site traffic pulled from Cloudflare Web Analytics (§8.14), for later ranking (§4.8).
create table if not exists traffic_daily (
  day       date not null,
  page_id   text not null,
  views     integer not null check (views >= 0),
  visitors  integer check (visitors >= 0),
  source    text not null default 'cloudflare-web-analytics',
  pulled_at timestamptz not null default now(),
  primary key (day, page_id)
);
