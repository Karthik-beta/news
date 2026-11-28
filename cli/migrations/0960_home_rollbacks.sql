-- Home rollback (#20; ADR 0005): one row per rollback attempt, written before the request is sent. The
-- write API keeps an Idempotency-Key for 7 days and replays its stored reply, so a key may name only one
-- attempt: v1 -> v2 -> v1 must not send the first key again. A resend after a crash finds the open row
-- for the same version and the same live draft (`from_draft`) and reuses its key.
create table if not exists home_rollbacks (
  rollback_id     bigserial primary key,
  version         int not null,
  from_draft      bigint not null,
  idempotency_key text not null unique default ('home-rollback:' || gen_random_uuid()::text),
  created_at      timestamptz not null default now(),
  sent_at         timestamptz,
  response        jsonb,
  done_at         timestamptz
);
create unique index if not exists home_rollbacks_open on home_rollbacks (version, from_draft) where done_at is null;
