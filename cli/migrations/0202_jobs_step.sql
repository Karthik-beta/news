-- Jobs know the step that enqueued them (ADR 0008 §6.2), so a re-entered step enqueues only what is
-- missing and a pause can wait for one run's jobs.
alter table jobs add column if not exists step_seq smallint;
create index if not exists jobs_run_step on jobs (run_id, step_seq, state) where run_id is not null;

-- The `ai` pool's size is the Standing key `pools.ai.max_running` now (ADR 0008 §10), which the service
-- applies. 0006 is left as it was (editing an applied migration breaks its checksum); only the seed
-- note changes here.
update pools set note = 'Standing: pools.ai.max_running' where pool = 'ai' and note = 'NEWS_PARALLEL';

-- Publishes of a run (Press, #20). A row that was sent but has no answer is marked `unknown_since` when
-- its run pauses or the service restarts, and reconciled when the run resumes (ADR 0008 §4.1, §4.5).
alter table publish_outbox add column if not exists run_id uuid, add column if not exists unknown_since timestamptz;
create index if not exists publish_outbox_run on publish_outbox (run_id) where run_id is not null;
