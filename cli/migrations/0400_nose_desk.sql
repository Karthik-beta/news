-- Nose and Desk (#15, ADR 0012). Block 0400–0499 belongs to Taster, Nose and Desk.
-- `scores`, `run_plans` and `event_weights` exist since 0004; this adds the scoring inputs Taster
-- writes for each story (news_nose::score::StoryInput as JSON). Null until Taster has filled it, and
-- `news plan` scores only open stories that have it.

alter table stories add column if not exists score_input jsonb;

-- A score is keyed by story, hour and scorer version (§5.8), so a new version in the same hour keeps
-- its own row instead of the stale one.
alter table scores drop constraint if exists scores_pkey;
alter table scores add primary key (story_id, computed_at, scorer_version);
