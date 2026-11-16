-- Search index for one shard (one D1 database per publication year, ADR 0004 and TD "Search").
-- Indexes title, summary, entities, tags, categories, region and date: about 2-4 KB per page, never body text.
-- `categories` holds every category and its parents, space-padded (" economy economy/inflation "), so a filter is an
-- exact token match. Keep comments free of semicolons: the migration reader splits statements on them.
CREATE TABLE pages (
  page_id TEXT PRIMARY KEY NOT NULL,
  path TEXT NOT NULL,
  lang TEXT NOT NULL,
  title TEXT NOT NULL,
  summary TEXT NOT NULL,
  entities TEXT NOT NULL,
  tags TEXT NOT NULL,
  categories TEXT NOT NULL,
  region TEXT NOT NULL,
  page_type TEXT NOT NULL,
  tier INTEGER NOT NULL,
  published_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  index_status TEXT NOT NULL
);

CREATE INDEX pages_published_at ON pages (published_at);

CREATE VIRTUAL TABLE pages_fts USING fts5(
  title,
  summary,
  entities,
  tags,
  categories,
  content = 'pages',
  content_rowid = 'rowid',
  tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TRIGGER pages_ai AFTER INSERT ON pages BEGIN
  INSERT INTO pages_fts (rowid, title, summary, entities, tags, categories)
  VALUES (new.rowid, new.title, new.summary, new.entities, new.tags, new.categories);
END;

CREATE TRIGGER pages_ad AFTER DELETE ON pages BEGIN
  INSERT INTO pages_fts (pages_fts, rowid, title, summary, entities, tags, categories)
  VALUES ('delete', old.rowid, old.title, old.summary, old.entities, old.tags, old.categories);
END;

-- Only a change to an indexed column rewrites the FTS row: an index_status or date change costs one row write.
CREATE TRIGGER pages_au AFTER UPDATE OF title, summary, entities, tags, categories ON pages BEGIN
  INSERT INTO pages_fts (pages_fts, rowid, title, summary, entities, tags, categories)
  VALUES ('delete', old.rowid, old.title, old.summary, old.entities, old.tags, old.categories);
  INSERT INTO pages_fts (rowid, title, summary, entities, tags, categories)
  VALUES (new.rowid, new.title, new.summary, new.entities, new.tags, new.categories);
END;
