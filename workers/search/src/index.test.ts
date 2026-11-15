// Unit tests: run inside workerd (Cloudflare's vitest integration), no services, no network.
import { createExecutionContext, waitOnExecutionContext } from "cloudflare:test";
import { env, exports } from "cloudflare:workers";
import { describe, expect, it } from "vitest";
import worker from "./index";

const NAME = "news-search";
const at = (path: string, init?: RequestInit) => exports.default.fetch(new Request(`http://localhost${path}`, init));

describe(NAME, () => {
  it("answers /health with the worker name and status", async () => {
    const res = await at("/health");
    expect(res.status).toBe(200);
    expect(res.headers.get("content-type")).toContain("application/json");
    expect(await res.json()).toEqual({ worker: NAME, status: "ok" });
  });

  it("works as a plain module handler with an execution context", async () => {
    const handler: ExportedHandler<Cloudflare.Env> = worker;
    const ctx = createExecutionContext();
    const res = await handler.fetch?.(new Request("http://localhost/health") as never, env, ctx);
    await waitOnExecutionContext(ctx);
    expect(res?.status).toBe(200);
    expect(await res?.json()).toEqual({ worker: NAME, status: "ok" });
  });

  it("ignores the query string on /health", async () => {
    const res = await at("/health?probe=1&x=%20");
    expect(res.status).toBe(200);
  });

  it("returns 501 with a JSON error for everything else until Phase 6", async () => {
    const res = await at("/anything");
    expect(res.status).toBe(501);
    expect(res.headers.get("content-type")).toContain("application/json");
    expect(await res.json()).toEqual({ error: "Not implemented yet (Phase 6)" });
  });

  // Routing is an exact match on the path: near-misses must not be treated as /health.
  it.each(["/", "/health/", "/health/extra", "/Health", "/HEALTH", "/healthz", "//health", "/%68ealth", "/health%2F"])(
    "does not treat %s as /health",
    async (path) => {
      expect((await at(path)).status).toBe(501);
    },
  );

  it("survives malformed percent-encoding and very long paths without a 5xx other than 501", async () => {
    for (const path of ["/%E0%A4%A", "/%", `/${"a".repeat(8000)}`]) {
      expect((await at(path)).status).toBe(501);
    }
  });

  it.each(["POST", "PUT", "PATCH", "DELETE"])("returns 501 for %s on an unknown route", async (method) => {
    const res = await at("/anything", { method, body: "{}" });
    expect(res.status).toBe(501);
  });

  it("is stateless: many concurrent requests all get the same answer", async () => {
    const results = await Promise.all(Array.from({ length: 50 }, () => at("/health")));
    expect(results.map((r) => r.status)).toEqual(Array(50).fill(200));
    expect(new Set(await Promise.all(results.map((r) => r.text()))).size).toBe(1);
  });
});
