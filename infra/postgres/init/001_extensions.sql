-- Runs once, when the news-postgres volume is first created.
-- The schema itself (sources, candidates, clusters, queue, ledger, entities, edges, embeddings,
-- taxonomy, data_points) is designed in Phase 4/5 and applied by migrations, not here.
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE EXTENSION IF NOT EXISTS unaccent;
