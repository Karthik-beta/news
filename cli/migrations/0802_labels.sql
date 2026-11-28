-- Karthik's labels (#19, #71; Bureau §9.1). Block 0800–0899 belongs to Critic and Huddle.
-- Board's labelling screens read the items below and write the labels over the Hotline (`eval.items`,
-- `eval.label`, H tier); `news eval export` turns them into `pipeline/eval/g3/` and the rates in
-- `pipeline/eval/judges.md`. Nothing here is ever filled in by code except the items to label.

-- Stored outputs offered to the A12 judges, drafted by `news eval draft` from the golden sets: a G6 page
-- (rendered from its refs) or a G1 output a version stored. `subject` is what Board shows and holds no
-- ground truth, so a label is blind: no seeded-fault kind, no validation result.
create table if not exists judge_items (
  item_id        text primary key,
  set            text not null check (set in ('g1', 'g6')),
  case_id        text not null,
  prompt_version text not null default '',
  subject        jsonb not null,
  drafted_at     timestamptz not null default now()
);

-- One label per item and failure mode: `pass` is true when the text has no such failure. Labelling again
-- replaces it.
create table if not exists judge_labels (
  item_id     text not null references judge_items (item_id) on delete cascade,
  judge       text not null check (judge in ('wrong_direction', 'unsupported_claim', 'attribution', 'missing_context', 'tone')),
  pass        boolean not null,
  labelled_at timestamptz not null default now(),
  primary key (item_id, judge)
);

-- The A12 judge's own verdicts on the same items, written by Audit's `judges` step once it exists. The
-- rates in judges.md compare these with `judge_labels`.
create table if not exists judge_verdicts (
  item_id     text not null references judge_items (item_id) on delete cascade,
  judge       text not null check (judge in ('wrong_direction', 'unsupported_claim', 'attribution', 'missing_context', 'tone')),
  model       text not null,
  pass        boolean not null,
  reason_code text,
  judged_at   timestamptz not null default now(),
  primary key (item_id, judge, model)
);

-- G3 candidates drafted by `news eval draft` (Taster's neighbours either side of the join threshold):
-- a candidate and a story, with the cosine and the shared-entity fact the join decision used. `sides`
-- is what Board shows (outlet text: it stays here and is never exported); `cos` and the rest are never
-- shown, so a label is not anchored on them. The id names a file, so it can only be `g3-c<n>-s<n>`.
create table if not exists story_pairs (
  pair_id       text primary key constraint story_pairs_pair_id_form check (pair_id ~ '^g3-c[0-9]+-s[0-9]+$'),
  sides         jsonb not null,
  cos           double precision not null,
  shared_entity boolean not null,
  model         text not null,
  drafted_at    timestamptz not null default now()
);

create table if not exists pair_labels (
  pair_id     text primary key references story_pairs (pair_id) on delete cascade,
  same        boolean not null,
  labelled_at timestamptz not null default now()
);
