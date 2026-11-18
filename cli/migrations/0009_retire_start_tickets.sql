-- Retire start tickets (ADR 0007, issue #30). Runs are started from Board over the Hotline, which
-- authenticates the client, so there is nothing for a ticket to prove: `runs.ticket_id` and the
-- `start_tickets` table go. The `origin` values and constraints of `runs` and `approvals` are reshaped
-- by Chief's block (0200–0299, #10), which replaces those tables.
--
-- Dropping the column first removes the foreign key to the table. Both statements are no-ops when
-- run again.

alter table runs drop column if exists ticket_id;
drop table if exists start_tickets;
