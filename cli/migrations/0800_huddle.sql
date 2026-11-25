-- Huddle (#19, Bureau §4.7, §8.12). Block 0800–0899 belongs to Critic and Huddle.
-- `score_labels`: Karthik's pairwise answers ("which deserved a page first?"), each with the component
-- values the two stories had when he answered, so a later re-score cannot change what he judged.
-- `scorer_versions`: the weight sets calibration proposes and Karthik adopts. v1 is the seed file and
-- has no row; a version is live once `adopted`, and at most one proposal is open at a time.

create table if not exists score_labels (
  label_id   bigserial primary key,
  run_id     uuid not null references runs (run_id),
  story_a    bigint not null references stories (story_id),
  story_b    bigint not null references stories (story_id),
  winner     text not null check (winner in ('a', 'b')),
  x_a        jsonb not null,
  x_b        jsonb not null,
  created_at timestamptz not null default now(),
  check (story_a < story_b),
  unique (run_id, story_a, story_b)
);
create index if not exists score_labels_created on score_labels (created_at);

create table if not exists scorer_versions (
  version    text primary key check (version ~ '^v[0-9]+$'),
  n          integer not null unique check (n >= 2),
  parent     text not null,
  weights    jsonb not null,
  status     text not null check (status in ('proposed', 'adopted')),
  labels     integer not null check (labels >= 0),
  created_at timestamptz not null default now(),
  adopted_at timestamptz,
  check ((status = 'adopted') = (adopted_at is not null))
);
create unique index if not exists scorer_versions_one_proposed on scorer_versions ((true)) where status = 'proposed';
