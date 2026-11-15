// End to end: the real wrangler.jsonc, started by Wrangler (workerd under Miniflare), spoken to over real HTTP the way
// an MCP client would (spec revision 2026-07-28). No data bindings are configured here: discovery and the tool list
// need none, and a tool that needs data fails with a clean JSON-RPC error.
import { type RunningWorker, startWorker } from "@news/worker-test-support/e2e";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

const VERSION = "2026-07-28";
const META = {
  "io.modelcontextprotocol/protocolVersion": VERSION,
  "io.modelcontextprotocol/clientInfo": { name: "e2e", version: "1.0.0" },
  "io.modelcontextprotocol/clientCapabilities": {},
};
let worker: RunningWorker;

beforeAll(async () => {
  worker = await startWorker();
});
afterAll(async () => {
  await worker?.close();
});

function mcp(method: string, params: Record<string, unknown> = {}, headers: Record<string, string> = {}) {
  return worker.get("/mcp", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      accept: "application/json, text/event-stream",
      "mcp-protocol-version": VERSION,
      "mcp-method": method,
      ...(typeof params.name === "string" ? { "mcp-name": params.name } : {}),
      ...headers,
    },
    body: JSON.stringify({ jsonrpc: "2.0", id: 7, method, params: { ...params, _meta: META } }),
  });
}

type Rpc = { id?: number; result?: Record<string, unknown>; error?: { code: number } };

describe("news-mcp over HTTP", () => {
  it("GET /health", async () => {
    const res = await worker.get("/health");
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({ worker: "news-mcp", status: "ok" });
  });

  it("server/discover and tools/list answer statelessly, with no session header", async () => {
    const discover = await mcp("server/discover");
    expect(discover.status).toBe(200);
    expect(discover.headers.get("mcp-session-id")).toBeNull();
    const d = (await discover.json()) as Rpc;
    expect(d.id).toBe(7);
    expect(d.result).toMatchObject({ resultType: "complete", supportedVersions: expect.arrayContaining([VERSION]) });

    const list = (await (await mcp("tools/list")).json()) as Rpc;
    expect(((list.result?.tools ?? []) as { name: string }[]).map((t) => t.name)).toEqual([
      "search_pages",
      "get_page",
      "list_categories",
      "get_entity",
      "get_data",
    ]);
  });

  it("a legacy initialize is answered without a session", async () => {
    const res = await worker.get("/mcp", {
      method: "POST",
      headers: { "content-type": "application/json", accept: "application/json, text/event-stream" },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "old", version: "1" } },
      }),
    });
    expect(res.status).toBe(200);
    expect(res.headers.get("mcp-session-id")).toBeNull();
    expect(((await res.json()) as Rpc).result).toMatchObject({ protocolVersion: "2025-11-25" });
  });

  it("a tool that needs data fails cleanly when storage is not configured", async () => {
    const res = await mcp("tools/call", { name: "get_page", arguments: { page_id: "abc" } });
    expect(res.status).toBe(500);
    expect(((await res.json()) as Rpc).error?.code).toBe(-32603);
  });

  it("transport rules: 405 for GET and DELETE, 403 for a foreign origin, 400 for a missing version header", async () => {
    expect((await worker.get("/mcp")).status).toBe(405);
    expect((await worker.get("/mcp", { method: "DELETE" })).status).toBe(405);
    expect((await mcp("tools/list", {}, { origin: "https://evil.example" })).status).toBe(403);
    const res = await worker.get("/mcp", {
      method: "POST",
      headers: { "content-type": "application/json", "mcp-method": "tools/list" },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "tools/list", params: { _meta: META } }),
    });
    expect(res.status).toBe(400);
    expect(((await res.json()) as Rpc).error?.code).toBe(-32020);
  });

  it("is rate-limited (60 requests a minute per client)", async () => {
    // One burst, so that it cannot straddle two limiter windows (a window allows 60).
    const burst = await Promise.all(
      Array.from({ length: 130 }, () => mcp("ping", {}, { "cf-connecting-ip": "192.0.2.77" }).then((r) => r.status)),
    );
    expect(burst).toContain(200);
    expect(burst).toContain(429);
    expect(burst.every((s) => s === 200 || s === 429)).toBe(true);
  });
});
