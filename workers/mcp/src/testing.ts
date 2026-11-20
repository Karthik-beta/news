// Test harness for the MCP server: the real handler over in-memory DynamoDB, R2 and a recording D1 stand-in.
import { FakeTable } from "@news/worker-shared/dynamo/fake";
import { entityHub, HUB_LATEST, hubPk, hubSk, taxonomyNodeKey } from "@news/worker-shared/keys";
import { Reader } from "@news/worker-shared/read";
import type { PageRecord } from "@news/worker-shared/records";
import type { SearchShard } from "@news/worker-shared/search";
import { FakeBucket } from "@news/worker-shared/testing/fake-r2";
import { makePageRecord } from "@news/worker-shared/testing/fixtures";
import type { Deps } from "./deps";
import { handleMcp, MODERN_VERSION } from "./server";

export const ORIGIN = "https://news.test";
/** R2 metadata of a file written by the publish path for version 1 (the fixture page's live version). */
export const STAMP_V1 = { customMetadata: { version: "1", bundle_sha256: "a".repeat(64) } };

/**
 * A D1 stand-in for unit tests: records every statement and answers search queries from a fixed row list (no
 * filtering, but the statement's LIMIT is honoured).
 */
export class RecordingD1 {
  readonly statements: string[] = [];
  constructor(readonly rows: Record<string, unknown>[]) {}
  prepare(sql: string) {
    this.statements.push(sql);
    const limit = /\bLIMIT (\d+)\s*$/.exec(sql)?.[1];
    const rows = limit === undefined ? this.rows : this.rows.slice(0, Number(limit));
    const stmt = {
      bind: () => stmt,
      all: async () => ({ results: rows, success: true, meta: {} }),
      first: async () => rows[0] ?? null,
      run: async () => ({ success: true, meta: {} }),
    };
    return stmt;
  }
  asD1(): D1Database {
    return this as unknown as D1Database;
  }
}

export function seedSite(table: FakeTable, bucket: FakeBucket): PageRecord {
  const page = makePageRecord(
    { id: "us-cpi-october", title: "US inflation cools in October", chart_ids: ["cpi-trend"] },
    { files: ["charts/cpi-trend.csv", "data.json", "index.html", "index.md", "meta.json", "page.oui", "sources.json"] },
  );
  const withdrawn = makePageRecord({ id: "old-page", path: "/2026/10/old-page" }, { status: "withdrawn" });
  table.seed(page as never, withdrawn as never, {
    PK: "URL#/2026/11/us-cpi-october",
    SK: "PAGE",
    type: "path_claim",
    lang: "en",
    page_id: page.page_id,
    path: page.path,
    created_at: page.published_at,
  });
  for (const hub of [HUB_LATEST, entityHub("country:US")]) {
    table.seed({
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
  table.seed(
    {
      PK: "ENT#country:US",
      SK: "META",
      type: "entity",
      lang: "en",
      id: "country:US",
      name: "United States",
      kind: "country",
      updated_at: page.updated_at,
    },
    {
      ...taxonomyNodeKey("economy"),
      type: "taxonomy_node",
      lang: "en",
      id: "economy",
      name: "Economy",
      parent: null,
      order: 1,
      updated_at: page.updated_at,
    },
    {
      ...taxonomyNodeKey("economy/inflation"),
      type: "taxonomy_node",
      lang: "en",
      id: "economy/inflation",
      name: "Inflation",
      parent: "economy",
      order: 1,
      updated_at: page.updated_at,
    },
  );
  bucket.put(
    "2026/11/us-cpi-october/index.md",
    "# US inflation cools in October\n\nConsumer prices rose 0.2% (BLS).\n",
    STAMP_V1,
  );
  bucket.put(
    "2026/11/us-cpi-october/data.json",
    '{"points":[{"id":"dp:cpi-yoy","value":"2.4","unit":"percent","source":"src:bls"}]}',
    STAMP_V1,
  );
  bucket.put("2026/11/us-cpi-october/charts/cpi-trend.csv", "period,value\n2026-09,2.6\n2026-10,2.4\n", STAMP_V1);
  bucket.writes.length = 0;
  return page;
}

export function harness(overrides: Partial<Deps> = {}) {
  const table = new FakeTable();
  const bucket = new FakeBucket();
  const page = seedSite(table, bucket);
  const d1 = new RecordingD1([
    {
      page_id: page.page_id,
      path: page.path,
      lang: "en",
      title: page.title,
      summary: page.summary,
      categories: " economy economy/inflation ",
      region: page.region,
      page_type: page.page_type,
      published_at: page.published_at,
    },
  ]);
  const shards: SearchShard[] = [{ year: 2026, db: d1.asD1() }];
  const errors: string[] = [];
  const deps: Deps = {
    reader: () => new Reader(table),
    bucket: () => bucket.asBucket(),
    shards: () => shards,
    siteOrigin: ORIGIN,
    allowedOrigins: new Set([ORIGIN]),
    log: { error: (event) => errors.push(event) },
    ...overrides,
  };
  return { deps, table, bucket, d1, page, errors };
}

export const clientMeta = {
  "io.modelcontextprotocol/protocolVersion": MODERN_VERSION,
  "io.modelcontextprotocol/clientInfo": { name: "test-client", version: "1.0.0" },
  "io.modelcontextprotocol/clientCapabilities": {},
};

export interface RpcInit {
  id?: string | number | null;
  params?: Record<string, unknown>;
  headers?: Record<string, string | null>;
  /** Leave out _meta (modern requests need it). */
  noMeta?: boolean;
  raw?: BodyInit;
  method?: string;
}

/** A well-formed 2026-07-28 request; `init` breaks it in the ways a test needs. */
export function rpc(deps: Deps, method: string, init: RpcInit = {}): Promise<Response> {
  const params = { ...(init.params ?? {}), ...(init.noMeta ? {} : { _meta: clientMeta }) };
  const body = { jsonrpc: "2.0", ...(init.id === null ? {} : { id: init.id ?? 1 }), method, params };
  const headers: Record<string, string> = {
    "content-type": "application/json",
    accept: "application/json, text/event-stream",
    "mcp-protocol-version": MODERN_VERSION,
    "mcp-method": method,
  };
  if (typeof init.params?.name === "string") headers["mcp-name"] = init.params.name;
  for (const [k, v] of Object.entries(init.headers ?? {})) {
    if (v === null) delete headers[k];
    else headers[k] = v;
  }
  return handleMcp(
    deps,
    new Request("http://mcp.test/mcp", {
      method: init.method ?? "POST",
      headers,
      body: (init.method ?? "POST") === "POST" ? (init.raw ?? JSON.stringify(body)) : undefined,
    }),
  );
}

export const callTool = (deps: Deps, name: string, args: Record<string, unknown> = {}, init: RpcInit = {}) =>
  rpc(deps, "tools/call", { ...init, params: { name, arguments: args } });
