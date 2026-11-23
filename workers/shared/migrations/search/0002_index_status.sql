-- The sitemaps list only pages with index_status = 'index' (ADR 0010): this index lets those queries read only those rows.
CREATE INDEX pages_index_status ON pages (index_status, published_at);
