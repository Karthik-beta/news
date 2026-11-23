-- Chief's status board (#10, Bureau §8.1 "Problems"). Why a source is amber or red, since when, and the
-- Sitrep (Rollcall run) that last saw it. Written with `health` by whoever grades it; null until then.

alter table sources add column if not exists health_reason text;
alter table sources add column if not exists health_since date;
alter table sources add column if not exists health_sitrep text;
