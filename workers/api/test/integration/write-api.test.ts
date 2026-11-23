// Integration: the write API inside workerd against Miniflare's local R2 and D1 (with the real search schema) and
// DynamoDB Local through the real SigV4 client. Needs `bun run infra:up`.
import { applyD1Migrations } from "cloudflare:test";
import { env } from "cloudflare:workers";
import { DynamoTable } from "@news/worker-shared/dynamo/client";
import { fromItem, type Item } from "@news/worker-shared/dynamo/codec";
import type { Cond } from "@news/worker-shared/dynamo/conditions";
import { tableContract } from "@news/worker-shared/dynamo/contract";
import type { BatchOp, Doc, Key, QueryInput, Table, TransactOp } from "@news/worker-shared/dynamo/table";
import { RecordingPurger } from "@news/worker-shared/purge";
import { readCapped } from "@news/worker-shared/r2";
import { D1SearchIndex, indexPage, listShards, searchPages } from "@news/worker-shared/search";
import {
  curationPages,
  makeBundle,
  makeCuration,
  makeMeta,
  makePageRecord,
} from "@news/worker-shared/testing/fixtures";
import { DEFAULT_REGION_VIEWS, homeUrls } from "@news/worker-shared/urls";
import { DynamoDbLocal, dynamoConfigFromEnv, uniqueTableName } from "@news/worker-test-support/dynamodb";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { handle } from "../../src/app";
import type { Deps } from "../../src/deps";
import { d1ReportStore } from "../../src/report-store";
import { expectItemsValid } from "../../src/testing";

const local = dynamoConfigFromEnv(env as unknown as Record<string, unknown>);
const admin = new DynamoDbLocal(local);
const tableName = uniqueTableName("news-api-it");
const KEY = "integration-write-key-0123456789abcdef";
const ORIGIN = "https://news.test";

/** The real client, counting write calls so a test can prove "writes nothing". */
class CountingTable implements Table {
  writes = 0;
  constructor(readonly inner: Table) {}
  get = (k: Key) => this.inner.get(k);
  batchGet = (k: Key[]) => this.inner.batchGet(k);
  query = (q: QueryInput) => this.inner.query(q);
  put = (i: Doc, c?: Cond) => {
    this.writes++;
    return this.inner.put(i, c);
  };
  delete = (k: Key, c?: Cond) => {
    this.writes++;
    return this.inner.delete(k, c);
  };
  transact = (ops: TransactOp[]) => {
    this.writes++;
    return this.inner.transact(ops);
  };
  batchWrite = (ops: BatchOp[]) => {
    this.writes++;
    return this.inner.batchWrite(ops);
  };
}

const real = new DynamoTable({
  table: tableName,
  region: local.region,
  endpoint: local.endpoint,
  accessKeyId: local.accessKeyId,
  secretAccessKey: local.secretAccessKey,
});
const table = new CountingTable(real);
const purger = new RecordingPurger();
let clock = new Date("2026-11-12T10:00:00Z");
const deps: Deps = {
  table: () => table,
  bucket: () => env.PAGES,
  search: new D1SearchIndex(env),
  reports: () => d1ReportStore(env.REPORTS),
  reportDailyCap: 200,
  purger,
  writeKey: KEY,
  siteOrigin: ORIGIN,
  regions: [...DEFAULT_REGION_VIEWS],
  now: () => clock,
  storageTimeoutMs: 10_000,
  limiters: {},
  log: { warn: () => {}, error: () => {} },
};

const call = (method: string, path: string, body?: unknown, key = `it-${crypto.randomUUID()}`) =>
  handle(
    deps,
    new Request(`http://api.test${path}`, {
      method,
      headers: { authorization: `Bearer ${KEY}`, "content-type": "application/json", "idempotency-key": key },
      body: body === undefined ? undefined : JSON.stringify(body),
    }),
  );

async function allItems(): Promise<Record<string, unknown>[]> {
  const out = await admin.call<{ Items: Item[] }>("Scan", { TableName: tableName, ConsistentRead: true });
  return out.Items.map(fromItem);
}

async function r2Keys(prefix: string): Promise<string[]> {
  return (await env.PAGES.list({ prefix })).objects.map((o) => o.key).sort();
}

beforeAll(async () => {
  await admin.createSingleTable(tableName);
  for (const shard of ["SEARCH_2025", "SEARCH_2026"] as const)
    await applyD1Migrations(env[shard], env.SEARCH_MIGRATIONS);
  await applyD1Migrations(env.REPORTS, env.REPORTS_MIGRATIONS);
});
afterAll(async () => {
  await admin.deleteTableIfExists(tableName);
});

// The same behaviour suite the in-memory fake passes, now against DynamoDB Local.
tableContract("DynamoTable on DynamoDB Local", () => real);

describe("publishing a page end to end", () => {
  it("writes R2, DynamoDB and D1, and a replay writes nothing", async () => {
    const bundle = await makeBundle(
      makeMeta({ id: "it-page", title: "Inflation cools in October", tags: ["cpi", "prices"] }),
    );
    const first = await call("PUT", "/v1/pages/it-page", bundle, "publish:it-page:1");
    expect(first.status).toBe(200);
    const firstText = await first.text();

    expect(await r2Keys("2026/11/it-page/")).toEqual(
      Object.keys(bundle.files)
        .map((f) => `2026/11/it-page/${f}`)
        .sort(),
    );
    const html = await env.PAGES.get("2026/11/it-page/index.html");
    expect(html?.httpMetadata?.contentType).toBe("text/html; charset=utf-8");
    expect(await html?.text()).toBe(bundle.files["index.html"]);

    const items = await allItems();
    expectItemsValid(items.filter((i) => i.type !== undefined));
    expect(items.some((i) => i.PK === "PAGE#it-page" && i.SK === "LATEST")).toBe(true);

    const hits = await searchPages(listShards(env), { query: "inflation octob" });
    expect(hits.map((h) => h.page_id)).toEqual(["it-page"]);
    expect(hits[0]).toMatchObject({ path: "/2026/11/it-page", lang: "en", region: "north-america" });

    const writesBefore = table.writes;
    const itemsBefore = JSON.stringify(await allItems());
    const replay = await call("PUT", "/v1/pages/it-page", bundle, "publish:it-page:1");
    expect(replay.status).toBe(200);
    expect(await replay.text()).toBe(firstText);
    expect(table.writes).toBe(writesBefore);
    expect(JSON.stringify(await allItems())).toBe(itemsBefore);

    const reused = await call(
      "PUT",
      "/v1/pages/it-page",
      await makeBundle(makeMeta({ id: "it-page", title: "Other" })),
      "publish:it-page:1",
    );
    expect(reused.status).toBe(409);
    expect(table.writes).toBe(writesBefore);
  });

  it("search filters by category (with parents), region, type and date, and shards by year", async () => {
    await call(
      "PUT",
      "/v1/pages/it-old",
      await makeBundle(
        makeMeta({
          id: "it-old",
          path: "/2025/03/it-old",
          title: "Inflation in India eases",
          categories: ["economy/inflation"],
          region: "india",
          published_at: "2025-03-01T00:00:00Z",
          updated_at: "2025-03-01T00:00:00Z",
        }),
      ),
    );
    const shards = listShards(env);
    const ids = async (p: Parameters<typeof searchPages>[1]) =>
      (await searchPages(shards, p)).map((h) => h.page_id).sort();
    expect(await ids({ query: "inflation" })).toEqual(["it-old", "it-page"]);
    expect(await ids({ query: "inflation", category: "economy" })).toEqual(["it-old", "it-page"]);
    expect(await ids({ query: "inflation", category: "econ" })).toEqual([]);
    expect(await ids({ query: "inflation", region: "india" })).toEqual(["it-old"]);
    expect(await ids({ query: "inflation", from: "2026-01-01" })).toEqual(["it-page"]);
    expect(await ids({ query: "inflation", to: "2025-12-31" })).toEqual(["it-old"]);
    expect(await ids({ query: "inflation", page_type: "earnings" })).toEqual([]);
    const old = await env.SEARCH_2025.prepare("SELECT page_id FROM pages").all();
    expect(old.results).toEqual([{ page_id: "it-old" }]);
    // Hostile input is only ever words inside quotes.
    expect(await ids({ query: '") OR 1=1; DROP TABLE pages; --' })).toEqual([]);
    expect((await env.SEARCH_2026.prepare("SELECT COUNT(*) AS n FROM pages").first<{ n: number }>())?.n).toBe(1);
  });

  it("withdrawal removes the search row and the public files", async () => {
    const res = await call("POST", "/v1/pages/it-old/withdraw", { reason: "Integration test withdrawal" });
    expect(res.status).toBe(200);
    expect(await r2Keys("2025/03/it-old/")).toEqual([]);
    expect(await searchPages(listShards(env), { query: "india" })).toEqual([]);
    const latest = (await allItems()).find((i) => i.PK === "PAGE#it-old" && i.SK === "LATEST");
    expect(latest).toMatchObject({ status: "withdrawn" });
  });

  it("readCapped reads at most the cap from real R2", async () => {
    await env.PAGES.put("cap/test.md", "é".repeat(100));
    const capped = await readCapped(env.PAGES, "cap/test.md", 51);
    expect(capped).toMatchObject({ truncated: true, size: 200 });
    expect(capped?.text).toBe("é".repeat(25));
    expect(await readCapped(env.PAGES, "cap/test.md", 10_000)).toMatchObject({
      truncated: false,
      text: "é".repeat(100),
    });
    expect(await readCapped(env.PAGES, "cap/none.md", 10)).toBeUndefined();
  });
});

describe("the search schema", () => {
  it("an index-status change rewrites the row, not the full-text index", async () => {
    const db = env.SEARCH_2025;
    const page = makePageRecord({
      id: "trigger-page",
      title: "Trigger probe page",
      published_at: "2025-03-01T00:00:00.000Z",
      updated_at: "2025-03-01T00:00:00.000Z",
    });
    await indexPage(db, page);
    const status = await db
      .prepare("UPDATE pages SET index_status = ?2 WHERE page_id = ?1")
      .bind("trigger-page", "index")
      .run();
    // The row plus the pages_index_status index entry (migration 0002); a full-text trigger would add more.
    expect(status.meta.rows_written).toBe(2);
    expect((await searchPages([{ year: 2025, db }], { query: "trigger probe" })).map((h) => h.page_id)).toEqual([
      "trigger-page",
    ]);
    await indexPage(db, { ...page, title: "Renamed probe page" });
    expect((await searchPages([{ year: 2025, db }], { query: "renamed" })).map((h) => h.page_id)).toEqual([
      "trigger-page",
    ]);
    expect(await searchPages([{ year: 2025, db }], { query: "trigger" })).toEqual([]);
  });
});

describe("home curation on DynamoDB Local", () => {
  it("publishes with a transaction, rolls back, and purges exactly home and region views", async () => {
    for (const p of curationPages()) await real.put(p as never);
    purger.calls.length = 0;
    const v1 = await call("PUT", "/v1/home/curation", makeCuration());
    expect(v1.status).toBe(200);
    const v2body = makeCuration();
    v2body.slots.lead.figure_id = "dp:cpi-mom";
    expect(await (await call("PUT", "/v1/home/curation", v2body)).json()).toMatchObject({
      version: 2,
      live_version: 2,
    });
    const rollback = await call("POST", "/v1/home/curation/rollback", { version: 1 });
    expect(await rollback.json()).toMatchObject({ version: 2, live_version: 1 });
    const latest = (await allItems()).find((i) => i.PK === "HOME#CURATION" && i.SK === "LATEST");
    expect(latest).toMatchObject({ version: 1 });
    expectItemsValid((await allItems()).filter((i) => i.PK === "HOME#CURATION"));
    expect(purger.calls).toEqual([1, 2, 3].map(() => homeUrls(ORIGIN, deps.regions)));
  });
});

describe("reports in their own D1 database", () => {
  it("stores an intake without request details, never touching DynamoDB, and lists it for Errata", async () => {
    const writesBefore = table.writes;
    const res = await handle(
      deps,
      new Request("http://api.test/v1/reports", {
        method: "POST",
        headers: { "content-type": "application/json", "cf-connecting-ip": "203.0.113.77", origin: ORIGIN },
        body: JSON.stringify({ page_id: "it-page", text: "Integration report text here." }),
      }),
    );
    expect(res.status).toBe(202);
    expect(table.writes).toBe(writesBefore);
    const stored = (await env.REPORTS.prepare("SELECT * FROM reports").all()).results;
    expect(stored).toHaveLength(1);
    expect(JSON.stringify(stored)).not.toContain("203.0.113.77");
    clock = new Date("2026-11-12T10:05:00Z");
    const list = await call("GET", "/v1/reports?after=2026-11-01T00:00:00Z");
    expect(((await list.json()) as { reports: unknown[] }).reports).toHaveLength(1);
    clock = new Date("2026-11-12T10:00:00Z");
  });

  it("enforces the daily cap in the database, lists by receipt with a cursor, and drops expired reports", async () => {
    const store = d1ReportStore(env.REPORTS);
    await env.REPORTS.prepare("DELETE FROM reports").run();
    const report = (id: string, created_at: string, expires_at = 2_000_000_000) => ({
      id: `r_${id.padStart(32, "0")}`,
      page_id: "it-page",
      page_path: "/2026/11/it-page",
      kind: "figure",
      text: "A figure looks wrong here.",
      where: "Second chart",
      lang: "en",
      created_at,
      expires_at,
    });
    expect(await store.add(report("1", "2026-11-10T08:00:00.000Z"), 2)).toBe(true);
    expect(await store.add(report("2", "2026-11-10T09:00:00.000Z"), 2)).toBe(true);
    expect(await store.add(report("3", "2026-11-10T10:00:00.000Z"), 2)).toBe(false);
    expect(await store.add(report("4", "2026-11-11T08:00:00.000Z", 1_000), 2)).toBe(true);
    const query = {
      after: "2026-11-01T00:00:00.000Z",
      afterId: "",
      receivedBefore: "2026-11-12T00:00:00.000Z",
      limit: 10,
    };
    const all = await store.list({ ...query, liveAt: 1_500 });
    expect(all.map((r) => r.id.slice(-1))).toEqual(["1", "2"]);
    expect(all[0]).toMatchObject({ where: "Second chart" });
    const next = await store.list({
      ...query,
      after: "2026-11-10T08:00:00.000Z",
      afterId: all[0]?.id ?? "",
      liveAt: 1_500,
    });
    expect(next.map((r) => r.id.slice(-1))).toEqual(["2"]);
    await store.deleteExpired(1_500);
    expect((await env.REPORTS.prepare("SELECT COUNT(*) AS n FROM reports").first<{ n: number }>())?.n).toBe(2);
  });
});
