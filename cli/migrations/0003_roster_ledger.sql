-- Roster (Bureau §3.5; TD "entities + edges") and Clerk's Ledger (§3.4): values stored exactly as
-- published. Nothing here converts a currency, unit, scale or fiscal period: `value_text`, `unit`,
-- `currency`, `scale` and `period_label` are verbatim, and `value` is the exact decimal of `value_text`.

create table if not exists entities (
  entity_id  bigserial primary key,
  kind       text not null check (kind in ('company', 'country', 'central_bank', 'institution', 'person', 'topic', 'agency', 'source', 'page')),
  name       text not null,
  prominence smallint not null default 3 check (prominence between 1 and 3),
  region     text,
  slug       text unique,
  attrs      jsonb not null default '{}'
);

create table if not exists entity_ids (
  scheme     text not null,
  value      text not null,
  entity_id  bigint not null references entities,
  valid_from date not null default '-infinity',
  valid_to   date,
  primary key (scheme, value, valid_from)
);
create index if not exists entity_ids_entity on entity_ids (entity_id);

create table if not exists entity_aliases (
  alias_norm text not null,
  entity_id  bigint not null references entities,
  source     text not null,
  weight     real not null default 1,
  primary key (alias_norm, entity_id)
);
create index if not exists entity_aliases_trgm on entity_aliases using gin (alias_norm gin_trgm_ops);

-- TD 'edges': the knowledge graph's links (src, rel, dst, weight, since).
create table if not exists edges (
  src    bigint not null references entities,
  rel    text not null,
  dst    bigint not null references entities,
  weight real not null default 1,
  since  date,
  primary key (src, rel, dst)
);
create index if not exists edges_dst on edges (dst, rel);

create table if not exists data_points (
  dp_id        text primary key check (dp_id ~ '^dp_[a-z0-9]+$'),
  context_key  text not null,
  series_key   text not null,
  entity_id    bigint references entities,
  concept      text not null,
  label        text not null,
  value_text   text not null,
  value        numeric,
  unit         text not null,
  currency     char(3),
  scale        text,
  decimals     smallint,
  period_label text not null,
  period_start date,
  period_end   date,
  period_type  text not null check (period_type in ('instant', 'duration')),
  fiscal_year  smallint,
  fiscal_period text,
  seasonal_adj text,
  vintage      timestamptz not null,
  supersedes   text references data_points,
  snapshot_sha bytea not null references snapshots,
  locator      jsonb not null,
  parser       text not null,
  retrieved_at timestamptz not null
);
create index if not exists data_points_series on data_points (series_key, period_end, vintage desc);
create index if not exists data_points_supersedes on data_points (supersedes) where supersedes is not null;

create table if not exists derived_points (
  dp_id    text primary key check (dp_id ~ '^dv_[a-z0-9]+$'),
  formula  text not null check (formula in ('pct_change(a,b)', 'pp_change(a,b)', 'diff(a,b)', 'share(a,total)')),
  inputs   text[] not null,
  value    numeric not null,
  unit     text not null,
  rounding smallint not null,
  guard    jsonb not null
);

create table if not exists documents (
  doc_id       bigserial primary key,
  snapshot_sha bytea not null references snapshots,
  kind         text not null check (kind in ('press_release', 'statement', 'filing_section', 'report_chapter', 'abstract')),
  title        text,
  lang         text not null default 'en',
  published_at timestamptz,
  text_len     int not null,
  simhash      bigint not null,
  content_sha  bytea not null
);
create index if not exists documents_content_sha on documents (content_sha);

create table if not exists text_blocks (
  doc_id   bigint not null references documents,
  block_no int not null,
  kind     text,
  page_no  int,
  text     text not null,
  primary key (doc_id, block_no)
);
