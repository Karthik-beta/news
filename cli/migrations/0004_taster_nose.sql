-- Taster (Bureau §3.6), Nose and Desk (§3.7, §4), the TD's taxonomy, candidates, clusters and queue.
-- The TD's `clusters` and `queue` overlap with `stories` and `run_plans`/`jobs`; both exist until the
-- Phase 4 ADRs settle which survive (TD, "Data model").

create table if not exists taxonomy (
  tax_id     bigserial primary key,
  parent_id  bigint references taxonomy,
  slug       text not null check (slug ~ '^[a-z0-9]+(-[a-z0-9]+)*$'),
  name       text not null,
  sort_order int not null default 0,
  unique nulls not distinct (parent_id, slug)
);

create table if not exists stories (
  story_id     bigserial primary key,
  event_key    text unique,
  title        text,
  first_seen   timestamptz not null,
  last_seen    timestamptz not null,
  member_count int not null default 1,
  entity_ids   bigint[] not null default '{}',
  page_id      text,
  status       text not null default 'open' check (status in ('open', 'covered', 'closed'))
);

create table if not exists candidates (
  cand_id      bigserial primary key,
  source_id    text not null references sources,
  url          text,
  title        text,
  event_key    text,
  story_id     bigint references stories,
  doc_ids      bigint[] not null default '{}',
  dp_count     int not null default 0,
  entity_ids   bigint[] not null default '{}',
  page_type    text,
  page_type_by text not null check (page_type_by in ('rule', 'rule-unsure', 'ai', 'human')),
  categories   text[],
  region       text,
  published_at timestamptz,
  first_seen   timestamptz not null default now(),
  status       text not null check (status in ('new', 'grouped', 'scored', 'selected', 'briefed', 'written', 'published', 'skipped'))
);
create index if not exists candidates_story on candidates (story_id);
create index if not exists candidates_event_key on candidates (event_key) where event_key is not null;
create index if not exists candidates_status on candidates (status, first_seen);

-- TD 'clusters': groups of candidates that are the same story.
create table if not exists clusters (
  cluster_id bigserial primary key,
  story_id   bigint references stories,
  label      text,
  created_at timestamptz not null default now()
);

create table if not exists embeddings (
  owner_kind text not null check (owner_kind in ('candidate', 'story', 'page')),
  owner_id   bigint not null,
  model      text not null,
  v          halfvec(384) not null,
  created_at timestamptz not null default now(),
  primary key (owner_kind, owner_id, model)
);
create index if not exists embeddings_v_hnsw on embeddings using hnsw (v halfvec_cosine_ops);

-- Event base weights (E), stored as data (§4.2). Starting values from the design; tuned by Huddle.
create table if not exists event_weights (
  event_type text primary key,
  weight     numeric(4,3) not null check (weight between 0 and 1),
  note       text not null,
  updated_at timestamptz not null default now()
);
insert into event_weights (event_type, weight, note) values
  ('policy_major',        1.0,  'Policy decision: Fed, ECB, BoE, BoJ, RBI, PBoC, RBA, BoC'),
  ('report_flagship',     0.95, 'Flagship institutional report (IMF WEO/GFSR/Fiscal Monitor, OECD EO, World Bank GEP, BIS AR)'),
  ('macro_headline',      0.9,  'Headline macro in target regions: CPI, GDP, employment'),
  ('policy_other',        0.8,  'Other central-bank decisions'),
  ('company_results',     0.7,  'Company results (10-K, 10-Q, 20-F, ESEF annual report, 8-K Item 2.02)'),
  ('macro_other',         0.6,  'Other macro releases; other institutional reports'),
  ('agency_science',      0.6,  'Agency science release (NASA, ESA, NOAA ...)'),
  ('paper_peer_reviewed', 0.5,  'Peer-reviewed paper'),
  ('news_story',          0.5,  'News story (primary-source-backed)'),
  ('preprint',            0.3,  'Preprint'),
  ('filing_8k_other',     0.3,  'Other 8-K items')
on conflict (event_type) do nothing;

create table if not exists scores (
  story_id       bigint not null references stories,
  computed_at    timestamptz not null,
  scorer_version text not null,
  components     jsonb not null,
  total          numeric(5,2) not null check (total between 0 and 100),
  explain        text not null,
  primary key (story_id, computed_at)
);

-- The Budget: stories chosen for a run.
create table if not exists run_plans (
  run_id   uuid not null,
  story_id bigint not null references stories,
  rank     int not null,
  tier     smallint not null check (tier in (1, 2)),
  bucket   text not null,
  adjusted numeric(5,2) not null,
  reason   text not null,
  action   text not null check (action in ('new', 'update')),
  primary key (run_id, story_id)
);

-- TD 'queue': selected items with tier, priority, status and attempts.
create table if not exists queue (
  queue_id   bigserial primary key,
  story_id   bigint references stories,
  run_id     uuid,
  tier       smallint not null check (tier in (1, 2)),
  priority   smallint not null default 2,
  status     text not null default 'queued',
  attempts   int not null default 0,
  created_at timestamptz not null default now()
);
