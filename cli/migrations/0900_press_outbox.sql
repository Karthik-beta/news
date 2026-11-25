-- Press (#20): what the outbox needs beyond 0005. `payload` carries an op's own body (the withdrawal
-- reason, the index status); `dead_at` marks a row the write API refused with a 4xx (class `bug`, never
-- retried); `last_error` is the reason, with the API's error code. Unsent and not dead means "to send";
-- `unknown_since` (0202) means the last send got no answer and the same key is sent again.
alter table publish_outbox add column if not exists payload jsonb, add column if not exists dead_at timestamptz, add column if not exists last_error text;
