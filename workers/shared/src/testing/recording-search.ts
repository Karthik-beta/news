// A SearchIndex that records calls, for unit tests. Integration tests use real D1 (Miniflare) instead.
import type { PageRecord } from "../records";
import { MissingShardError, type SearchIndex } from "../search";

export class RecordingSearchIndex implements SearchIndex {
  readonly rows = new Map<string, PageRecord>();
  readonly calls: { op: "index" | "remove" | "status"; page_id: string }[] = [];
  /** Years with a bound shard. */
  years = new Set([2025, 2026, 2027]);

  assertShard(publishedAt: string): void {
    const year = Number(publishedAt.slice(0, 4));
    if (!this.years.has(year)) throw new MissingShardError(year);
  }

  async index(page: PageRecord): Promise<void> {
    this.calls.push({ op: "index", page_id: page.page_id });
    this.rows.set(page.page_id, structuredClone(page));
  }

  async remove(page: Pick<PageRecord, "page_id">): Promise<void> {
    this.calls.push({ op: "remove", page_id: page.page_id });
    this.rows.delete(page.page_id);
  }

  async setIndexStatus(page: Pick<PageRecord, "page_id">, status: "index" | "noindex"): Promise<void> {
    this.calls.push({ op: "status", page_id: page.page_id });
    const row = this.rows.get(page.page_id);
    if (row) row.index_status = status;
  }
}
