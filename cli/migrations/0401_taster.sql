-- Taster (#15). A merged-away story keeps its row and event key, so a later candidate with that key
-- still finds it and follows this redirect to the surviving story.
create table if not exists story_redirects (
  from_story bigint primary key references stories,
  into_story bigint not null references stories check (into_story <> from_story),
  merged_at  timestamptz not null default now()
);
