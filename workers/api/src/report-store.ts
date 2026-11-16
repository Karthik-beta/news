// Where reader reports live: their own D1 database (binding REPORTS, schema in migrations/reports), never the
// DynamoDB table that publishing writes to, so a flood of reports cannot throttle a publish (ADR 0006).
// MemoryReportStore below has the same behaviour for unit tests; the integration suite runs D1ReportStore on
// Miniflare's local D1 with the real migration.

export interface StoredReport {
  id: string;
  page_id: string;
  page_path: string;
  kind: string;
  text: string;
  where?: string;
  email?: string;
  lang: string;
  /** ISO 8601 with milliseconds (Date.toISOString), so string order is time order. */
  created_at: string;
  /** Epoch seconds. */
  expires_at: number;
}

export interface ReportQuery {
  /** Receipt time to start from (normalised ISO timestamp). */
  after: string;
  /** With a next_after cursor: the last report ID returned, and only later ones follow. "" means "at or after". */
  afterId: string;
  /** Only reports received strictly before this time (the settling window, see listReports). */
  receivedBefore: string;
  /** Only reports whose expires_at is later than this (epoch seconds). */
  liveAt: number;
  limit: number;
}

export interface ReportStore {
  /** The UTC day (YYYY-MM-DD) on which this isolate saw the daily cap reached; refused without a query until it ends. */
  fullDay?: string;
  /** Stores the report unless `dailyCap` reports were already received that UTC day; false when the cap is hit. */
  add(report: StoredReport, dailyCap: number): Promise<boolean>;
  /** Reports matching the query, oldest first (by receipt time, then ID). */
  list(query: ReportQuery): Promise<StoredReport[]>;
  /** Deletes reports that expired at or before `now` (epoch seconds). */
  deleteExpired(now: number): Promise<void>;
}

const dayOf = (createdAt: string) => createdAt.slice(0, 10);

interface Row {
  id: string;
  page_id: string;
  page_path: string;
  kind: string;
  text: string;
  location: string | null;
  email: string | null;
  lang: string;
  created_at: string;
  expires_at: number;
}

const fromRow = (r: Row): StoredReport => ({
  id: r.id,
  page_id: r.page_id,
  page_path: r.page_path,
  kind: r.kind,
  text: r.text,
  ...(r.location ? { where: r.location } : {}),
  ...(r.email ? { email: r.email } : {}),
  lang: r.lang,
  created_at: r.created_at,
  expires_at: r.expires_at,
});

const stores = new WeakMap<D1Database, D1ReportStore>();

/** One store per database binding and isolate, so `fullDay` outlives a single request. */
export function d1ReportStore(db: D1Database): D1ReportStore {
  let store = stores.get(db);
  if (!store) {
    store = new D1ReportStore(db);
    stores.set(db, store);
  }
  return store;
}

export class D1ReportStore implements ReportStore {
  fullDay?: string;
  constructor(readonly db: D1Database) {}

  async add(r: StoredReport, dailyCap: number): Promise<boolean> {
    // One statement, so the count and the insert cannot interleave with another report: a real global cap.
    const res = await this.db
      .prepare(
        `INSERT INTO reports (id, created_at, day, page_id, page_path, kind, text, location, email, lang, expires_at)
         SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11
         WHERE (SELECT COUNT(*) FROM reports WHERE day = ?3) < ?12`,
      )
      .bind(
        r.id,
        r.created_at,
        dayOf(r.created_at),
        r.page_id,
        r.page_path,
        r.kind,
        r.text,
        r.where ?? null,
        r.email ?? null,
        r.lang,
        r.expires_at,
        dailyCap,
      )
      .run();
    return (res.meta.changes ?? 0) > 0;
  }

  async list(q: ReportQuery): Promise<StoredReport[]> {
    const { results } = await this.db
      .prepare(
        `SELECT id, page_id, page_path, kind, text, location, email, lang, created_at, expires_at FROM reports
         WHERE (created_at > ?1 OR (created_at = ?1 AND id > ?2)) AND created_at < ?3 AND expires_at > ?4
         ORDER BY created_at, id LIMIT ?5`,
      )
      .bind(q.after, q.afterId, q.receivedBefore, q.liveAt, q.limit)
      .all<Row>();
    return results.map(fromRow);
  }

  async deleteExpired(now: number): Promise<void> {
    await this.db.prepare("DELETE FROM reports WHERE expires_at <= ?1").bind(now).run();
  }
}

/** In-memory ReportStore for unit tests, with D1ReportStore's semantics. */
export class MemoryReportStore implements ReportStore {
  fullDay?: string;
  readonly rows: StoredReport[] = [];

  async add(r: StoredReport, dailyCap: number): Promise<boolean> {
    if (this.rows.filter((x) => dayOf(x.created_at) === dayOf(r.created_at)).length >= dailyCap) return false;
    this.rows.push(structuredClone(r));
    return true;
  }

  async list(q: ReportQuery): Promise<StoredReport[]> {
    return this.rows
      .filter(
        (r) =>
          (r.created_at > q.after || (r.created_at === q.after && r.id > q.afterId)) &&
          r.created_at < q.receivedBefore &&
          r.expires_at > q.liveAt,
      )
      .sort((a, b) => (a.created_at + a.id < b.created_at + b.id ? -1 : 1))
      .slice(0, q.limit)
      .map((r) => structuredClone(r));
  }

  async deleteExpired(now: number): Promise<void> {
    for (let i = this.rows.length - 1; i >= 0; i--)
      if ((this.rows[i] as StoredReport).expires_at <= now) this.rows.splice(i, 1);
  }
}
