// Integration: the MCP tools inside workerd against DynamoDB Local (real SigV4 client), Miniflare's R2 and a D1 search
// shard with the real schema. Data is seeded the way the write API stores it. Needs `bun run infra:up`.
import { applyD1Migrations } from "cloudflare:test";
import { env } from "cloudflare:workers";
import { DynamoTable } from "@news/worker-shared/dynamo/client";
import { entityHub, HUB_LATEST, hubPk, hubSk, taxonomyNodeKey } from "@news/worker-shared/keys";
import { putPageFile } from "@news/worker-shared/r2";
import { Reader } from "@news/worker-shared/read";
import { indexPage, listShards } from "@news/worker-shared/search";
import { makePageRecord } from "@news/worker-shared/testing/fixtures";
import { schemas } from "@news/worker-shared/validate";
import { DynamoDbLocal, dynamoConfigFromEnv, uniqueTableName } from "@news/worker-test-support/dynamodb";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import type { Deps } from "../../src/deps";
import { handleMcp, MODERN_VERSION } from "../../src/server";
import { clientMeta } from "../../src/testing";

const local = dynamoConfigFromEnv(env as unknown as Record<string, unknown>);
const admin = new DynamoDbLocal(local);
const tableName = uniqueTableName("news-mcp-it");
const table = new DynamoTable({ ...local, table: tableName });
const ORIGIN = "https://news.test";

const deps: Deps = {
  reader: () => new Reader(table),
  bucket: () => env.PAGES,
  shards: () => listShards(env),
  siteOrigin: ORIGIN,
  allowedOrigins: new Set([ORIGIN]),
  log: { error: () => {} },
};

const page = makePageRecord({
  id: "it-mcp-page",
  title: "Euro area wages accelerate",
  categories: ["economy/labour"],
  region: "europe",
  entities: [{ id: "institution:ECB", name: "European Central Bank", kind: "institution" }],
  chart_ids: ["wages"],
});
page.files = [...page.files, "charts/wages.csv"].sort();

async function call(name: string, args: Record<string, unknown>) {
  const res = await handleMcp(
    deps,
    new Request("http://mcp.test/mcp", {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "mcp-protocol-version": MODERN_VERSION,
        "mcp-method": "tools/call",
        "mcp-name": name,
      },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        method: "tools/call",
        params: { name, arguments: args, _meta: clientMeta },
      }),
    }),
  );
  expect(res.status).toBe(200);
  return ((await res.json()) as { result: { isError: boolean; structuredContent: Record<string, unknown> } }).result;
}

beforeAll(async () => {
  await admin.createSingleTable(tableName);
  await applyD1Migrations(env.SEARCH_2026, env.SEARCH_MIGRATIONS);
  await table.put(page as never);
  await table.put({
    PK: `URL#${page.path}`,
    SK: "PAGE",
    type: "path_claim",
    lang: "en",
    page_id: page.page_id,
    path: page.path,
    created_at: page.published_at,
  });
  for (const hub of [HUB_LATEST, entityHub("institution:ECB")]) {
    await table.put({
      PK: hubPk(hub),
      SK: hubSk(page.published_at, page.page_id),
      type: "hub_entry",
      lang: "en",
      hub,
      page_id: page.page_id,
      path: page.path,
      title: page.title,
      summary: page.summary,
      page_type: page.page_type,
      tier: page.tier,
      region: page.region,
      published_at: page.published_at,
      updated_at: page.updated_at,
      index_status: page.index_status,
    });
  }
  await table.put({
    PK: "ENT#institution:ECB",
    SK: "META",
    type: "entity",
    lang: "en",
    id: "institution:ECB",
    name: "European Central Bank",
    kind: "institution",
    updated_at: page.updated_at,
  });
  await table.put({
    ...taxonomyNodeKey("economy"),
    type: "taxonomy_node",
    lang: "en",
    id: "economy",
    name: "Economy",
    parent: null,
    order: 1,
    updated_at: page.updated_at,
  });
  await putPageFile(
    env.PAGES,
    page.path,
    "index.md",
    "# Euro area wages accelerate\n\nNegotiated wages (ECB series).\n",
  );
  await putPageFile(
    env.PAGES,
    page.path,
    "data.json",
    '{"points":[{"id":"dp:wages","value":"4.4","source":"src:ecb"}]}',
  );
  await putPageFile(env.PAGES, page.path, "charts/wages.csv", "quarter,value\n2026-Q2,4.4\n");
  await indexPage(env.SEARCH_2026, page);
});
afterAll(async () => {
  await admin.deleteTableIfExists(tableName);
});

describe("MCP tools on real local storage", () => {
  it("search_pages finds the page through D1 FTS5", async () => {
    const r = await call("search_pages", { query: "wages euro", category: "economy" });
    expect(r.isError).toBe(false);
    expect(schemas.validateSearchPagesOutput(r.structuredContent)).toBe(true);
    expect(r.structuredContent.results).toEqual([
      expect.objectContaining({ page_id: "it-mcp-page", url: `${ORIGIN}/2026/11/it-mcp-page`, region: "europe" }),
    ]);
  });

  it("get_page by path, get_entity, list_categories and get_data read DynamoDB and R2", async () => {
    const pageMd = await call("get_page", { path: "/2026/11/it-mcp-page" });
    expect(pageMd.structuredContent).toMatchObject({ page_id: "it-mcp-page", truncated: false });
    expect(pageMd.structuredContent.markdown).toContain("Negotiated wages");

    const entity = await call("get_entity", { entity_id: "institution:ECB" });
    expect(schemas.validateGetEntityOutput(entity.structuredContent)).toBe(true);
    expect((entity.structuredContent.pages as unknown[]).length).toBe(1);

    const cats = await call("list_categories", {});
    expect(cats.structuredContent.categories).toEqual([
      { id: "economy", name: "Economy", parent: null, order: 1, lang: "en", url: `${ORIGIN}/economy` },
    ]);

    const data = await call("get_data", { page_id: "it-mcp-page", chart_id: "wages" });
    expect(data.structuredContent).toMatchObject({ media_type: "text/csv", content: "quarter,value\n2026-Q2,4.4\n" });
  });
});
