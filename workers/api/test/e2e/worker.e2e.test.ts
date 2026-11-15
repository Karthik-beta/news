// End to end: the real wrangler.jsonc, started by Wrangler (workerd under Miniflare), reached over real HTTP.
import { type RunningWorker, startWorker } from "@news/worker-test-support/e2e";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

const NAME = "news-api";
let worker: RunningWorker;

beforeAll(async () => {
  worker = await startWorker();
});
afterAll(async () => {
  await worker?.close();
});

describe(`${NAME} over HTTP`, () => {
  it("listens on a real loopback port", () => {
    expect(worker.url.protocol).toBe("http:");
    expect(["127.0.0.1", "localhost", "[::1]"]).toContain(worker.url.hostname);
    expect(Number(worker.url.port)).toBeGreaterThan(0);
  });

  it("GET /health returns 200 JSON", async () => {
    const res = await worker.get("/health");
    expect(res.status).toBe(200);
    expect(res.headers.get("content-type")).toContain("application/json");
    expect(await res.json()).toEqual({ worker: NAME, status: "ok" });
  });

  it("GET /health works with a query string and through HEAD", async () => {
    expect((await worker.get("/health?probe=1")).status).toBe(200);
    const head = await worker.get("/health", { method: "HEAD" });
    expect(head.status).toBe(200);
    expect(await head.text()).toBe("");
  });

  it("answers 501 with a JSON error on a route that is not built yet", async () => {
    const res = await worker.get("/v1/pages");
    expect(res.status).toBe(501);
    expect(res.headers.get("content-type")).toContain("application/json");
    expect(await res.json()).toEqual({ error: "Not implemented yet (Phase 6)" });
  });

  it("answers 501 for write methods and near-miss health paths", async () => {
    expect((await worker.get("/v1/pages", { method: "POST", body: JSON.stringify({ a: 1 }) })).status).toBe(501);
    expect((await worker.get("/v1/pages/1", { method: "DELETE" })).status).toBe(501);
    for (const path of ["/", "/health/", "/Health", "/health/x", "/%E0%A4%A"]) {
      expect((await worker.get(path)).status).toBe(501);
    }
  });

  it("serves many concurrent requests", async () => {
    const results = await Promise.all(Array.from({ length: 40 }, (_, i) => worker.get(i % 2 ? "/health" : "/nope")));
    expect(results.map((r) => r.status)).toEqual(Array.from({ length: 40 }, (_, i) => (i % 2 ? 200 : 501)));
  });
});
