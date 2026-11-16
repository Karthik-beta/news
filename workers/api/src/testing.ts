// Test harness for the write API: the real handler with in-memory fakes for DynamoDB, R2, search and purge.
import { FakeTable } from "@news/worker-shared/dynamo/fake";
import { RecordingPurger } from "@news/worker-shared/purge";
import { FakeBucket } from "@news/worker-shared/testing/fake-r2";
import { RecordingSearchIndex } from "@news/worker-shared/testing/recording-search";
import { DEFAULT_REGION_VIEWS } from "@news/worker-shared/urls";
import { schemas } from "@news/worker-shared/validate";
import { expect } from "vitest";
import { handle } from "./app";
import type { Deps, Logger } from "./deps";

export const TEST_KEY = "test-write-key-0123456789abcdef-not-real";
export const ORIGIN = "https://news.test";

/** A rate limiter that allows `limit` calls per key, then refuses. */
export function countingLimiter(limit: number): RateLimit & { counts: Map<string, number> } {
  const counts = new Map<string, number>();
  return {
    counts,
    async limit({ key }: { key: string }) {
      const n = (counts.get(key) ?? 0) + 1;
      counts.set(key, n);
      return { success: n <= limit };
    },
  } as RateLimit & { counts: Map<string, number> };
}

export interface Harness {
  deps: Deps;
  table: FakeTable;
  bucket: FakeBucket;
  search: RecordingSearchIndex;
  purger: RecordingPurger;
  logs: { level: string; event: string; fields?: Record<string, unknown> }[];
  /** Moves the clock. */
  setNow(iso: string): void;
  call(method: string, path: string, init?: CallInit): Promise<Response>;
  /** Number of write calls made anywhere (DynamoDB, R2, search, purge) — for "writes nothing" assertions. */
  writeCount(): number;
}

export interface CallInit {
  body?: unknown;
  /** Raw body text (overrides `body`). */
  raw?: BodyInit;
  key?: string | null;
  auth?: string | null;
  contentType?: string | null;
  headers?: Record<string, string>;
}

export function harness(overrides: Partial<Deps> = {}): Harness {
  const table = new FakeTable();
  const bucket = new FakeBucket();
  const search = new RecordingSearchIndex();
  const purger = new RecordingPurger();
  const logs: Harness["logs"] = [];
  const log: Logger = {
    warn: (event, fields) => logs.push({ level: "warn", event, fields }),
    error: (event, fields) => logs.push({ level: "error", event, fields }),
  };
  let now = new Date("2026-11-12T10:00:00Z");
  const deps: Deps = {
    table: () => table,
    bucket: () => bucket.asBucket(),
    search,
    purger,
    writeKey: TEST_KEY,
    siteOrigin: ORIGIN,
    regions: [...DEFAULT_REGION_VIEWS],
    now: () => now,
    limiters: {},
    log,
    ...overrides,
  };
  let seq = 0;
  return {
    deps,
    table,
    bucket,
    search,
    purger,
    logs,
    setNow: (iso) => {
      now = new Date(iso);
    },
    writeCount: () => table.writes.length + bucket.writes.length + search.calls.length + purger.calls.length,
    call(method, path, init = {}) {
      const headers: Record<string, string> = { ...init.headers };
      if (init.auth !== null) headers.authorization = init.auth ?? `Bearer ${TEST_KEY}`;
      if (init.contentType !== null && !("content-type" in headers))
        headers["content-type"] = init.contentType ?? "application/json";
      if (init.key !== null && method !== "GET")
        headers["idempotency-key"] = init.key ?? `test-key-${++seq}-${crypto.randomUUID()}`;
      const body = init.raw ?? (init.body === undefined ? undefined : JSON.stringify(init.body));
      return handle(deps, new Request(`http://api.test${path}`, { method, headers, body }));
    },
  };
}

const validators: Record<string, (d: unknown) => boolean> = {
  page: schemas.validatePageItem,
  hub_entry: schemas.validateHubEntryItem,
  entity: schemas.validateEntityItem,
  path_claim: schemas.validatePathClaimItem,
  page_lock: schemas.validatePageLockItem,
  idempotency: schemas.validateIdempotencyItem,
  curation: schemas.validateCurationItem,
  taxonomy_node: schemas.validateTaxonomyItem,
  redirect: schemas.validateRedirectItem,
  report: schemas.validateReportItem,
};

/** Every stored item matches its schema in contracts/cloud/dynamodb-items.schema.json. */
export function expectItemsValid(items: Record<string, unknown>[]) {
  for (const item of items) {
    const validate = validators[String(item.type)];
    expect(validate, `no validator for item type ${String(item.type)}`).toBeDefined();
    expect(validate?.(item), `${String(item.PK)} / ${String(item.SK)}`).toBe(true);
  }
}
