-- Nose and Desk (#15, ADR 0012). Block 0400–0499 belongs to Taster, Nose and Desk.
-- `scores`, `run_plans` and `event_weights` exist since 0004; this adds the scoring inputs Taster
-- writes for each story (news_nose::score::StoryInput as JSON). Null until Taster has filled it, and
-- `news plan` scores only open stories that have it.

alter table stories add column if not exists score_input jsonb;
