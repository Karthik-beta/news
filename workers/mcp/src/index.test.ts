// MCP conformance tests against spec revision 2026-07-28 (inside workerd): server/discover, initialize (legacy,
// stateless), tools/list and tools/call; header and _meta validation; transport rules; read-only tools; size caps.
import { exports } from "cloudflare:workers";
import { schemas } from "@news/worker-shared/validate";
import { describe, expect, it } from "vitest";
import { ERR, LEGACY_VERSIONS, MODERN_VERSION, SERVER_INFO } from "./server";
import { callTool, harness, ORIGIN, rpc } from "./testing";
import { CAPS, type TOOL_LIST } from "./tools";

// Responses are checked field by field below; a loose type keeps the assertions readable.
// biome-ignore lint/suspicious/noExplicitAny: test-only view of arbitrary JSON-RPC results
type Loose = any;
type Rpc = {
  jsonrpc: string;
  id?: unknown;
  result?: Record<string, Loose>;
  error?: { code: number; message: string; data?: Loose };
};
const body = async (res: Response) => (await res.json()) as Rpc;

async function expectRpcError(res: Response, status: number, code: number): Promise<Rpc> {
  expect(res.status).toBe(status);
  expect(res.headers.get("content-type")).toContain("application/json");
  const b = await body(res);
  expect(b.jsonrpc).toBe("2.0");
  expect(b.error?.code).toBe(code);
  expect(b.result).toBeUndefined();
  return b;
}

describe("entry point", () => {
  it("health and 404 outside /mcp", async () => {
    expect((await exports.default.fetch(new Request("http://x/health"))).status).toBe(200);
    expect((await exports.default.fetch(new Request("http://x/sse"))).status).toBe(404);
  });

  it("tools/list works with no data bindings at all (nothing is read)", async () => {
    const { deps } = harness();
    const listed = await body(await rpc(deps, "tools/list"));
    const res = await exports.default.fetch(
      new Request("http://x/mcp", {
        method: "POST",
        headers: {
          "content-type": "application/json",
          "mcp-protocol-version": MODERN_VERSION,
          "mcp-method": "tools/list",
        },
        body: JSON.stringify({
          jsonrpc: "2.0",
          id: 1,
          method: "tools/list",
          params: { _meta: (await import("./testing")).clientMeta },
        }),
      }),
    );
    expect(res.status).toBe(200);
    expect((await body(res)).result?.tools).toEqual(listed.result?.tools);
  });
});

describe("server/discover (2026-07-28)", () => {
  it("returns versions, capabilities and server info", async () => {
    const res = await rpc(harness().deps, "server/discover", { id: "discover-1" });
    expect(res.status).toBe(200);
    const b = await body(res);
    expect(b.id).toBe("discover-1");
    expect(b.result).toMatchObject({
      resultType: "complete",
      supportedVersions: [MODERN_VERSION, ...LEGACY_VERSIONS],
      capabilities: { tools: { listChanged: false } },
      _meta: { "io.modelcontextprotocol/serverInfo": SERVER_INFO },
    });
    expect(typeof b.result?.instructions).toBe("string");
  });
});

describe("initialize (earlier revisions; answered without a session)", () => {
  it.each([
    ["2025-11-25", "2025-11-25"],
    ["2025-06-18", "2025-06-18"],
    ["2024-11-05", "2025-11-25"],
    ["2026-07-28", "2025-11-25"],
  ])("a client asking for %s gets %s", async (requested, expected) => {
    const res = await rpc(harness().deps, "initialize", {
      noMeta: true,
      headers: { "mcp-protocol-version": null, "mcp-method": null },
      params: { protocolVersion: requested, capabilities: {}, clientInfo: { name: "legacy", version: "1" } },
    });
    expect(res.status).toBe(200);
    expect(res.headers.get("mcp-session-id")).toBeNull();
    const b = await body(res);
    expect(b.result).toEqual({
      protocolVersion: expected,
      capabilities: { tools: { listChanged: false } },
      serverInfo: SERVER_INFO,
      instructions: expect.any(String),
    });
  });

  it("then serves tools/list and tools/call for the negotiated legacy version, statelessly", async () => {
    const { deps } = harness();
    const legacy = {
      noMeta: true,
      headers: { "mcp-protocol-version": "2025-11-25", "mcp-method": null, "mcp-name": null },
    };
    expect((await rpc(deps, "notifications/initialized", { ...legacy, id: null })).status).toBe(202);
    const list = await body(await rpc(deps, "tools/list", legacy));
    expect(list.result?.tools).toHaveLength(5);
    expect(list.result?.resultType).toBeUndefined();
    const call = await body(await callTool(deps, "list_categories", {}, legacy));
    expect(call.result?.isError).toBe(false);
  });

  it("an initialize carrying 2026-07-28 _meta is refused with the supported versions", async () => {
    const b = await expectRpcError(
      await rpc(harness().deps, "initialize", { params: { protocolVersion: MODERN_VERSION } }),
      404,
      ERR.methodNotFound,
    );
    expect(b.error?.data?.supported).toContain(MODERN_VERSION);
  });

  it("ignores an Mcp-Session-Id header and never sets one", async () => {
    const res = await rpc(harness().deps, "tools/list", { headers: { "mcp-session-id": "abc123" } });
    expect(res.status).toBe(200);
    expect(res.headers.get("mcp-session-id")).toBeNull();
  });
});

describe("tools/list", () => {
  it("lists the five read-only tools in a fixed order with valid schemas", async () => {
    const b = await body(await rpc(harness().deps, "tools/list"));
    expect(b.result?.resultType).toBe("complete");
    expect(b.result?.cacheScope).toBe("public");
    const tools = b.result?.tools as typeof TOOL_LIST;
    expect(tools.map((t) => t.name)).toEqual(["search_pages", "get_page", "list_categories", "get_entity", "get_data"]);
    for (const t of tools) {
      expect(t.name).toMatch(/^[A-Za-z0-9_.-]{1,128}$/);
      expect(t.inputSchema).toMatchObject({ type: "object" });
      expect(JSON.stringify(t.inputSchema)).not.toContain("$ref");
      expect(t.annotations).toMatchObject({ readOnlyHint: true, destructiveHint: false });
      expect(t.description.length).toBeGreaterThan(20);
    }
  });
});

describe("tools/call", () => {
  const outputs: Record<string, (d: unknown) => boolean> = {
    search_pages: schemas.validateSearchPagesOutput,
    get_page: schemas.validateGetPageOutput,
    list_categories: schemas.validateListCategoriesOutput,
    get_entity: schemas.validateGetEntityOutput,
    get_data: schemas.validateGetDataOutput,
  };

  it.each([
    ["search_pages", { query: "inflation", region: "north-america" }],
    ["get_page", { page_id: "us-cpi-october" }],
    ["get_page", { path: "/2026/11/us-cpi-october", format: "json" }],
    ["list_categories", {}],
    ["get_entity", { entity_id: "country:US" }],
    ["get_data", { page_id: "us-cpi-october" }],
    ["get_data", { page_id: "us-cpi-october", chart_id: "cpi-trend" }],
  ])("%s %j returns structured content that matches its output schema", async (name, args) => {
    const res = await callTool(harness().deps, name, args);
    expect(res.status).toBe(200);
    const b = await body(res);
    expect(b.result?.resultType).toBe("complete");
    expect(b.result?.isError).toBe(false);
    expect(b.result?.content?.[0]?.type).toBe("text");
    expect(outputs[name]?.(b.result?.structuredContent), JSON.stringify(b.result?.structuredContent)).toBe(true);
  });

  it("returns figures exactly as published", async () => {
    const b = await body(await callTool(harness().deps, "get_data", { page_id: "us-cpi-october" }));
    expect(b.result?.structuredContent.content).toBe(
      '{"points":[{"id":"dp:cpi-yoy","value":"2.4","unit":"percent","source":"src:bls"}]}',
    );
    const md = await body(await callTool(harness().deps, "get_page", { page_id: "us-cpi-october" }));
    expect(md.result?.content[0].text).toContain("0.2% (BLS)");
    expect(md.result?.structuredContent).toMatchObject({ url: `${ORIGIN}/2026/11/us-cpi-october`, truncated: false });
  });

  it("reports missing and withdrawn pages, unknown entities and charts as tool errors", async () => {
    for (const [name, args] of [
      ["get_page", { page_id: "nope" }],
      ["get_page", { page_id: "old-page" }],
      ["get_page", { path: "/2026/01/unknown" }],
      ["get_entity", { entity_id: "company:nobody" }],
      ["get_data", { page_id: "us-cpi-october", chart_id: "not-a-chart" }],
    ] as const) {
      const b = await body(await callTool(harness().deps, name, args));
      expect(b.result?.isError, `${name} ${JSON.stringify(args)}`).toBe(true);
    }
  });

  it("invalid arguments are a tool execution error; an unknown tool is a protocol error", async () => {
    const { deps } = harness();
    for (const [name, args] of [
      ["search_pages", {}],
      ["search_pages", { query: "x", limit: 500 }],
      ["search_pages", { query: "one two three four five six seven eight nine" }],
      ["get_entity", { entity_id: "country:US", limit: 21 }],
      ["get_page", {}],
      ["get_page", { page_id: "a", path: "/2026/11/a" }],
      ["get_page", { page_id: "../../etc/passwd" }],
      ["get_data", { page_id: "a", chart_id: "../x" }],
      ["list_categories", { extra: true }],
    ] as const) {
      const b = await body(await callTool(deps, name, args));
      expect(b.result?.isError, `${name} ${JSON.stringify(args)}`).toBe(true);
    }
    await expectRpcError(await callTool(deps, "delete_page", {}), 400, ERR.invalidParams);
  });

  it("no tool writes anything: DynamoDB and R2 untouched, D1 only SELECTs", async () => {
    const h = harness();
    for (const [name, args] of [
      ["search_pages", { query: "inflation" }],
      ["get_page", { page_id: "us-cpi-october" }],
      ["get_page", { page_id: "us-cpi-october", format: "json" }],
      ["list_categories", {}],
      ["get_entity", { entity_id: "country:US" }],
      ["get_data", { page_id: "us-cpi-october" }],
      ["get_data", { page_id: "us-cpi-october", chart_id: "cpi-trend" }],
    ] as const) {
      expect((await callTool(h.deps, name, args)).status).toBe(200);
    }
    expect(h.table.writes).toEqual([]);
    expect(h.bucket.writes).toEqual([]);
    expect(h.d1.statements.length).toBeGreaterThan(0);
    for (const sql of h.d1.statements) expect(sql.trim()).toMatch(/^SELECT\b/i);
  });

  it("never labels a file with the wrong version: mid-publish it re-reads the live record or says to retry", async () => {
    const h = harness();
    const v2 = { customMetadata: { version: "2", bundle_sha256: "b".repeat(64) } };
    // A publish of version 2 has written its files but not yet moved the live record (still version 1).
    await h.bucket.put("2026/11/us-cpi-october/index.md", "# Version two\n", v2);
    await h.bucket.put("2026/11/us-cpi-october/data.json", '{"v":2}', v2);
    for (const [name, args] of [
      ["get_page", { page_id: "us-cpi-october" }],
      ["get_data", { page_id: "us-cpi-october" }],
    ] as const) {
      const b = await body(await callTool(h.deps, name, args));
      expect(b.result?.isError, name).toBe(true);
      expect(b.result?.content[0].text).toMatch(/being updated/);
    }
    // Once the live record says version 2, the same files are served as version 2.
    const live = h.table.dump().find((i) => i.PK === "PAGE#us-cpi-october" && i.SK === "LATEST") as Record<string, never>;
    h.table.seed({ ...live, version: 2 as never });
    const b = await body(await callTool(h.deps, "get_page", { page_id: "us-cpi-october" }));
    expect(b.result?.structuredContent).toMatchObject({ version: 2, markdown: "# Version two\n" });
  });

  it("keeps responses within their size caps, and says when it truncated", async () => {
    const h = harness();
    await h.bucket.put("2026/11/us-cpi-october/index.md", `# Big\n${"é paragraph text. ".repeat(20_000)}`);
    await h.bucket.put(
      "2026/11/us-cpi-october/data.json",
      JSON.stringify({ rows: Array.from({ length: 50_000 }, (_, i) => ({ id: `dp:${i}`, v: "1" })) }),
    );
    for (const [name, args, cap] of [
      ["get_page", { page_id: "us-cpi-october" }, CAPS.markdownBytes],
      ["get_data", { page_id: "us-cpi-october" }, CAPS.dataBytes],
    ] as const) {
      const res = await callTool(h.deps, name, args);
      const text = await res.clone().text();
      expect(new TextEncoder().encode(text).length).toBeLessThanOrEqual(CAPS.resultBytes);
      const b = (await res.json()) as Rpc;
      const sc = b.result?.structuredContent;
      expect(sc.truncated).toBe(true);
      const content = (sc.markdown ?? sc.content) as string;
      expect(new TextEncoder().encode(content).length).toBeLessThanOrEqual(cap);
      expect(content).not.toContain("�");
      expect(b.result?.content[0].text).toContain("Truncated");
    }
  });
});

describe("2026-07-28 request validation", () => {
  const { deps } = harness();

  it("a missing MCP-Protocol-Version header → 400 HeaderMismatch", async () => {
    await expectRpcError(
      await rpc(deps, "tools/list", { headers: { "mcp-protocol-version": null } }),
      400,
      ERR.headerMismatch,
    );
  });

  it("header and _meta disagreeing → 400 HeaderMismatch", async () => {
    await expectRpcError(
      await rpc(deps, "tools/list", { headers: { "mcp-protocol-version": "2025-11-25" } }),
      400,
      ERR.headerMismatch,
    );
  });

  it("an unsupported version → 400 UnsupportedProtocolVersion listing the supported ones", async () => {
    const b = await expectRpcError(
      await rpc(deps, "tools/list", { noMeta: true, headers: { "mcp-protocol-version": "1900-01-01" } }),
      400,
      ERR.unsupportedVersion,
    );
    expect(b.error?.data).toEqual({ supported: [MODERN_VERSION, ...LEGACY_VERSIONS], requested: "1900-01-01" });
  });

  it("missing required _meta fields → 400 Invalid params", async () => {
    await expectRpcError(await rpc(deps, "tools/list", { noMeta: true }), 400, ERR.invalidParams);
    const res = await handle({ "io.modelcontextprotocol/protocolVersion": MODERN_VERSION });
    await expectRpcError(res, 400, ERR.invalidParams);
  });

  it("Mcp-Method missing or different → 400 HeaderMismatch", async () => {
    await expectRpcError(await rpc(deps, "tools/list", { headers: { "mcp-method": null } }), 400, ERR.headerMismatch);
    await expectRpcError(
      await rpc(deps, "tools/list", { headers: { "mcp-method": "tools/call" } }),
      400,
      ERR.headerMismatch,
    );
  });

  it("tools/call: Mcp-Name missing or different → 400 HeaderMismatch; Base64 sentinel accepted", async () => {
    await expectRpcError(
      await callTool(deps, "list_categories", {}, { headers: { "mcp-name": null } }),
      400,
      ERR.headerMismatch,
    );
    await expectRpcError(
      await callTool(deps, "list_categories", {}, { headers: { "mcp-name": "get_page" } }),
      400,
      ERR.headerMismatch,
    );
    const encoded = `=?base64?${btoa("list_categories")}?=`;
    expect((await callTool(deps, "list_categories", {}, { headers: { "mcp-name": encoded } })).status).toBe(200);
  });

  it("an unknown method → 404 Method not found", async () => {
    await expectRpcError(await rpc(deps, "resources/list"), 404, ERR.methodNotFound);
  });

  it("ping", async () => {
    expect((await body(await rpc(deps, "ping"))).result).toMatchObject({ resultType: "complete" });
  });

  async function handle(meta: Record<string, unknown>) {
    const { handleMcp } = await import("./server");
    return handleMcp(
      deps,
      new Request("http://mcp.test/mcp", {
        method: "POST",
        headers: {
          "content-type": "application/json",
          "mcp-protocol-version": MODERN_VERSION,
          "mcp-method": "tools/list",
        },
        body: JSON.stringify({ jsonrpc: "2.0", id: 9, method: "tools/list", params: { _meta: meta } }),
      }),
    );
  }
});

describe("transport", () => {
  it("GET and DELETE → 405 (no standalone stream, no sessions)", async () => {
    const { deps } = harness();
    for (const method of ["GET", "DELETE"]) {
      const res = await rpc(deps, "tools/list", { method });
      expect(res.status).toBe(405);
      expect(res.headers.get("allow")).toBe("POST");
    }
  });

  it("refuses foreign browser origins (DNS rebinding) and allows the site and non-browser clients", async () => {
    const { deps } = harness();
    expect((await rpc(deps, "tools/list", { headers: { origin: "https://evil.example" } })).status).toBe(403);
    expect((await rpc(deps, "tools/list", { headers: { origin: ORIGIN } })).status).toBe(200);
  });

  it("bad bodies: wrong type 415, oversize 413, parse error, batch, bad id", async () => {
    const { deps } = harness();
    expect((await rpc(deps, "tools/list", { headers: { "content-type": "text/plain" } })).status).toBe(415);
    await expectRpcError(
      await rpc(deps, "tools/list", { raw: `{"pad":"${"x".repeat(70_000)}"}` }),
      413,
      ERR.invalidRequest,
    );
    await expectRpcError(await rpc(deps, "tools/list", { raw: "{nope" }), 400, ERR.parse);
    await expectRpcError(await rpc(deps, "tools/list", { raw: "[]" }), 400, ERR.invalidRequest);
    await expectRpcError(
      await rpc(deps, "tools/list", { raw: '{"jsonrpc":"2.0","id":null,"method":"tools/list"}' }),
      400,
      ERR.invalidRequest,
    );
    await expectRpcError(
      await rpc(deps, "tools/list", { raw: '{"jsonrpc":"1.0","id":1,"method":"tools/list"}' }),
      400,
      ERR.invalidRequest,
    );
  });

  it("rate-limits per client with 429", async () => {
    let n = 0;
    const limiter = { limit: async () => ({ success: ++n <= 2 }) } as unknown as RateLimit;
    const { deps } = harness({ limiter });
    expect((await rpc(deps, "tools/list")).status).toBe(200);
    expect((await rpc(deps, "tools/list")).status).toBe(200);
    const limited = await rpc(deps, "tools/list");
    await expectRpcError(limited, 429, ERR.rateLimited);
    expect(limited.headers.get("retry-after")).toBe("60");
  });
});
