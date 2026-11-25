// Test doubles for the SSR site: a fixture set in a FakeTable, a SiteData over it, and an in-memory Cache API.
import { FakeTable } from "@news/worker-shared/dynamo/fake";
import type { Doc } from "@news/worker-shared/dynamo/table";
import { Reader } from "@news/worker-shared/read";
import emptyLatest from "../../fixtures/data/empty-latest/items.json";
import normal from "../../fixtures/data/normal/items.json";
import region from "../../fixtures/data/region/items.json";
import withdrawnLead from "../../fixtures/data/withdrawn-lead/items.json";
import type { SiteData } from "../../src/data";
import { TaxonomyCache } from "../../src/server/context";
import type { EdgeCache } from "../../src/server/serve";
import type { Ctx } from "../../src/server/views";

export const FIXTURE_ITEMS: Record<string, Doc[]> = {
  normal: normal as unknown as Doc[],
  "withdrawn-lead": withdrawnLead as unknown as Doc[],
  region: region as unknown as Doc[],
  "empty-latest": emptyLatest as unknown as Doc[],
};

export const ORIGIN = "https://news.karthik.nz";
export const REGIONS = ["north-america", "europe", "india", "china", "asia", "australia", "global"];
/** The moment the fixture sets are made around (their newest page is a few hours before it). */
export const FIXTURE_NOW = new Date("2025-01-15T16:00:00.000Z");

export const tableOf = (items: Doc[] = FIXTURE_ITEMS.normal as Doc[]): FakeTable => {
  const table = new FakeTable();
  table.seed(...items);
  return table;
};

/** A SiteData over `table`. `files` are the page files R2 would hold, by "<path> <file>". */
export function dataOf(table: FakeTable, files: Record<string, unknown> = {}): SiteData {
  const reader = new Reader(table);
  return {
    reader,
    curation: () => reader.getLiveCuration(),
    shards: [],
    async pageJson(path, file) {
      return files[`${path} ${file}`];
    },
    async pageText(path, file) {
      return files[`${path} ${file}`] as string | undefined;
    },
  };
}

export function ctxOf(table: FakeTable, over: Partial<Ctx> & { files?: Record<string, unknown> } = {}): Ctx {
  const data = dataOf(table, over.files);
  const cache = new TaxonomyCache();
  return { data, regions: REGIONS, now: FIXTURE_NOW, taxonomy: () => cache.get(data.reader), ...over };
}

/** An in-memory Cache API that honours the stored `cache-control: max-age` against a settable clock. */
export class MemoryCache implements EdgeCache {
  readonly store = new Map<string, { res: Response; expires: number }>();
  puts: string[] = [];
  constructor(private readonly clock: () => number) {}

  async match(request: Request): Promise<Response | undefined> {
    const hit = this.store.get(request.url);
    if (!hit) return undefined;
    if (this.clock() >= hit.expires) {
      this.store.delete(request.url);
      return undefined;
    }
    return hit.res.clone();
  }

  async put(request: Request, response: Response): Promise<void> {
    const maxAge = Number(/max-age=(\d+)/.exec(response.headers.get("cache-control") ?? "")?.[1] ?? 0);
    this.puts.push(request.url);
    this.store.set(request.url, { res: response.clone(), expires: this.clock() + maxAge * 1000 });
  }
}
