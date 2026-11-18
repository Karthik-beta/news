-- Reader error reports (ADR 0006), in their own D1 database (binding REPORTS), so report traffic can never spend the
-- publishing table's DynamoDB write units. Holds the report text, page, kind, optional "where" and e-mail, language
-- and receipt time, never the IP address or any other request detail. Rows expire 180 days after receipt
-- (`expires_at`, epoch seconds): listing skips expired rows and the Errata pull deletes them.
-- `day` (YYYY-MM-DD of receipt, UTC) backs the global daily cap, checked in the same INSERT that stores a report.
-- Keep comments free of semicolons: the migration reader splits statements on them.
CREATE TABLE reports (
  id TEXT PRIMARY KEY NOT NULL,
  created_at TEXT NOT NULL,
  day TEXT NOT NULL,
  page_id TEXT NOT NULL,
  page_path TEXT NOT NULL,
  kind TEXT NOT NULL,
  text TEXT NOT NULL,
  location TEXT,
  email TEXT,
  lang TEXT NOT NULL,
  expires_at INTEGER NOT NULL
);

CREATE INDEX reports_by_receipt ON reports (created_at, id);

CREATE INDEX reports_by_day ON reports (day);
