-- Test harness only. This is NOT the production search schema (Phase 6 designs that, with FTS5 and sharding).
-- It exists so the integration tests can prove that D1 migrations apply and queries run inside workerd.
-- Keep comments free of semicolons: the migration reader splits statements on them.
CREATE TABLE harness_check (
  id INTEGER PRIMARY KEY,
  label TEXT NOT NULL UNIQUE
);
