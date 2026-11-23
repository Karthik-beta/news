-- Taster (#15 follow-up). Which data points a candidate came from, so a story's score input can read
-- their values, prior values and revisions (`dp_count` only says how many). Null on candidates stored
-- before this column; a structured story whose candidates have none stays unscored.

alter table candidates add column if not exists dp_ids text[];
