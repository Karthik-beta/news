// Read layer over the single table: everything the SSR site, the MCP server and the write API read.
// Strongly consistent reads (the Table implementations ask for them), small pages, no scans.
import type { Doc, Key, Table } from "./dynamo/table";
import {
  CURATION_PK,
  curationLatestKey,
  curationVersionKey,
  entityKey,
  HUB_LATEST,
  hubPk,
  pageLatestKey,
  pagePk,
  pageVersionKey,
  pathClaimKey,
  redirectKey,
  TAXONOMY_PK,
  versionFromSk,
} from "./keys";
import type {
  CurationRecord,
  EntityRecord,
  HubEntry,
  PageRecord,
  PathClaim,
  RedirectRecord,
  TaxonomyNode,
} from "./records";

const as = <T>(doc: Doc | undefined): T | undefined => doc as unknown as T | undefined;

export interface Page<T> {
  items: T[];
  /** Opaque cursor for the next page, or null. */
  cursor: string | null;
}

/** Cursors are the last sort key, base64url-encoded; they only ever continue a query on the same partition. */
export function encodeCursor(key: Key | undefined): string | null {
  if (!key) return null;
  return btoa(key.SK).replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");
}

export function decodeCursor(pk: string, cursor: string | null | undefined): Key | undefined {
  if (!cursor) return undefined;
  if (!/^[A-Za-z0-9_-]{1,700}$/.test(cursor)) throw new RangeError("invalid cursor");
  try {
    const sk = atob(cursor.replaceAll("-", "+").replaceAll("_", "/"));
    return { PK: pk, SK: sk };
  } catch {
    throw new RangeError("invalid cursor");
  }
}

export class Reader {
  constructor(readonly table: Table) {}

  getPage(pageId: string): Promise<PageRecord | undefined> {
    return this.table.get(pageLatestKey(pageId)).then(as<PageRecord>);
  }

  getPageVersion(pageId: string, version: number): Promise<PageRecord | undefined> {
    return this.table.get(pageVersionKey(pageId, version)).then(as<PageRecord>);
  }

  /** Version numbers of a page, newest first. */
  async listPageVersions(pageId: string, limit = 20): Promise<number[]> {
    const res = await this.table.query({ pk: pagePk(pageId), sk: { beginsWith: "V#" }, forward: false, limit });
    return res.items.map((i) => versionFromSk(String(i.SK)));
  }

  /** True when the page has at least one stored version (whether or not it has a live LATEST record). */
  async hasAnyVersion(pageId: string): Promise<boolean> {
    const res = await this.table.query({ pk: pagePk(pageId), sk: { beginsWith: "V#" }, limit: 1 });
    return res.items.length > 0;
  }

  /** Live records of many pages, keyed by page ID. Missing pages are simply absent. */
  async getPages(pageIds: string[]): Promise<Map<string, PageRecord>> {
    const docs = await this.table.batchGet([...new Set(pageIds)].map(pageLatestKey));
    return new Map(docs.map((d) => [String(d.page_id), d as unknown as PageRecord]));
  }

  async getPageIdByPath(path: string): Promise<string | undefined> {
    const claim = as<PathClaim>(await this.table.get(pathClaimKey(path)));
    return claim?.page_id;
  }

  /** A hub's entries, newest first. */
  async listHub(hub: string, opts: { limit?: number; cursor?: string | null } = {}): Promise<Page<HubEntry>> {
    const pk = hubPk(hub);
    const res = await this.table.query({
      pk,
      forward: false,
      limit: Math.min(Math.max(opts.limit ?? 20, 1), 100),
      start: decodeCursor(pk, opts.cursor),
    });
    return { items: res.items as unknown as HubEntry[], cursor: encodeCursor(res.next) };
  }

  /** The newest pages on the site (the home page's automatic "Latest" list). */
  async latestPages(limit = 10): Promise<HubEntry[]> {
    return (await this.listHub(HUB_LATEST, { limit })).items;
  }

  getEntity(entityId: string): Promise<EntityRecord | undefined> {
    return this.table.get(entityKey(entityId)).then(as<EntityRecord>);
  }

  /** The whole category tree, ordered by parent then `order` then ID. */
  async listTaxonomy(): Promise<TaxonomyNode[]> {
    const nodes: TaxonomyNode[] = [];
    let start: Key | undefined;
    for (let pages = 0; pages < 20; pages++) {
      const res = await this.table.query({ pk: TAXONOMY_PK, sk: { beginsWith: "NODE#" }, start, limit: 500 });
      nodes.push(...(res.items as unknown as TaxonomyNode[]));
      start = res.next;
      if (!start) break;
    }
    return nodes.sort(
      (a, b) => (a.parent ?? "").localeCompare(b.parent ?? "") || a.order - b.order || a.id.localeCompare(b.id),
    );
  }

  getRedirect(path: string): Promise<RedirectRecord | undefined> {
    return this.table.get(redirectKey(path)).then(as<RedirectRecord>);
  }

  getLiveCuration(): Promise<CurationRecord | undefined> {
    return this.table.get(curationLatestKey()).then(as<CurationRecord>);
  }

  getCurationVersion(version: number): Promise<CurationRecord | undefined> {
    return this.table.get(curationVersionKey(version)).then(as<CurationRecord>);
  }

  /** The newest curation versions (highest first). */
  async recentCurationVersions(limit = 5): Promise<CurationRecord[]> {
    const res = await this.table.query({ pk: CURATION_PK, sk: { beginsWith: "V#" }, forward: false, limit });
    return res.items as unknown as CurationRecord[];
  }

  /** The highest curation version ever written (not necessarily the live one after a rollback), or 0. */
  async newestCurationVersion(): Promise<number> {
    const res = await this.table.query({ pk: CURATION_PK, sk: { beginsWith: "V#" }, forward: false, limit: 1 });
    const top = res.items[0];
    return top ? versionFromSk(String(top.SK)) : 0;
  }
}
