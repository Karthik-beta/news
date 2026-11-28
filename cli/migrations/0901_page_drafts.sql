-- Press (#20): the next version of a live page, while it waits to be sent. `pages` keeps the live version
-- (status `published`) until the outbox row for the draft is sent; then the draft replaces it in one
-- transaction. A rejected or dead revision deletes the draft and the live page stays as it was.
create table if not exists page_drafts (
  page_id          text primary key references pages,
  version          int not null check (version >= 2),
  bundle_sha       bytea not null,
  oui_sha          bytea,
  renderer_version text,
  tier             smallint check (tier in (1, 2)),
  page_type        text,
  flags            text[] not null default '{}',
  state            text not null default 'draft' check (state in ('draft', 'ready')),
  updated_at       timestamptz
);
