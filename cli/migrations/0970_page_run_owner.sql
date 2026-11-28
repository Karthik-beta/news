-- Press (#20, security review): the run that registered a draft. A run publishes only the drafts it
-- registered (and checked) itself, never one another run left waiting at its own gate.
alter table pages add column if not exists run_id uuid;
alter table page_drafts add column if not exists run_id uuid;
