-- Roster (Bureau §3.5, step 5): mentions the deterministic resolver could not place, shown in the
-- Huddle workflow. One row per normalised mention and expected kind; `seen` counts repeats and
-- `context` keeps one example of where it appeared. Block 0300–0399 belongs to Diary and Roster.

create table if not exists unresolved_mentions (
  mention_id      bigserial primary key,
  mention_norm    text not null,
  mention         text not null,
  kind            text,
  reason          text not null check (reason in ('no_match', 'ambiguous', 'below_threshold')),
  candidates      jsonb not null default '[]',
  context         text not null default '',
  seen            integer not null default 1 check (seen >= 1),
  first_seen      timestamptz not null default now(),
  last_seen       timestamptz not null default now(),
  status          text not null default 'open' check (status in ('open', 'resolved', 'dismissed')),
  resolved_entity bigint references entities,
  unique nulls not distinct (mention_norm, kind)
);
create index if not exists unresolved_mentions_open on unresolved_mentions (status, seen desc, mention_id);
