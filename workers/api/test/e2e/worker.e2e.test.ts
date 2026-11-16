// End to end: the real wrangler.jsonc, started by Wrangler (workerd under Miniflare), reached over real HTTP.
// No secrets and no data bindings are configured here, which is exactly what this suite checks: the Worker fails
// closed, and the public report intake refuses bad input before touching any storage.
import { type RunningWorker, startWorker } from "@news/worker-test-support/e2e";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

const NAME = "news-api";
const SITE = "https://news.karthik.nz";
let worker: RunningWorker;

beforeAll(async () => {
  worker = await startWorker();
});
afterAll(async () => {
  await worker?.close();
});

const code = async (res: Response) => ((await res.json()) as { error: { code: string } }).error.code;
const report = (body: unknown, headers: Record<string, string> = {}) =>
  worker.get("/v1/reports", {
    method: "POST",
    headers: { "content-type": "application/json", origin: SITE, ...headers },
    body: typeof body === "string" ? body : JSON.stringify(body),
  });

describe(`${NAME} over HTTP`, () => {
  it("GET /health returns 200 JSON, also with HEAD", async () => {
    const res = await worker.get("/health");
    expect(res.status).toBe(200);
    expect(res.headers.get("content-type")).toContain("application/json");
    expect(res.headers.get("x-content-type-options")).toBe("nosniff");
    expect(await res.json()).toEqual({ worker: NAME, status: "ok" });
    const head = await worker.get("/health", { method: "HEAD" });
    expect(head.status).toBe(200);
    expect(await head.text()).toBe("");
  });

  it("every write is refused with 401 when no write key is configured", async () => {
    const attempts: [string, string][] = [
      ["PUT", "/v1/pages/abc"],
      ["POST", "/v1/pages/abc/withdraw"],
      ["POST", "/v1/pages/abc/index"],
      ["PUT", "/v1/home/curation"],
      ["POST", "/v1/home/curation/rollback"],
      ["PUT", "/v1/taxonomy"],
      ["PUT", "/v1/redirects"],
      ["GET", "/v1/reports"],
    ];
    for (const [method, path] of attempts) {
      const res = await worker.get(path, {
        method,
        headers: {
          authorization: "Bearer anything-at-all-0123456789abcdef",
          "content-type": "application/json",
          "idempotency-key": "e2e-key-123",
        },
        body: method === "GET" ? undefined : "{}",
      });
      expect(res.status, `${method} ${path}`).toBe(401);
      expect(await code(res)).toBe("unauthorized");
    }
  });

  it("unknown routes are 404 JSON; under /v1/ a caller without the key gets 401 first", async () => {
    for (const path of ["/", "/v1", "/%E0%A4%A"]) {
      const res = await worker.get(path);
      expect(res.status, path).toBe(404);
      expect(await code(res)).toBe("not_found");
    }
    for (const path of ["/v1/pages", "/v1/nope"]) {
      const res = await worker.get(path);
      expect(res.status, path).toBe(401);
      expect(await code(res)).toBe("unauthorized");
    }
  });

  it("report intake: CORS preflight for the site only", async () => {
    const ok = await worker.get("/v1/reports", { method: "OPTIONS", headers: { origin: SITE } });
    expect(ok.status).toBe(204);
    expect(ok.headers.get("access-control-allow-origin")).toBe(SITE);
    const other = await worker.get("/v1/reports", { method: "OPTIONS", headers: { origin: "https://evil.example" } });
    expect(other.headers.get("access-control-allow-origin")).toBeNull();
  });

  it("report intake: refuses other origins, wrong types, oversize bodies; a honeypot gets 202", async () => {
    expect((await report({ page_id: "x", text: "hello there" }, { origin: "https://evil.example" })).status).toBe(403);
    const form = await report("page_id=x", { "content-type": "application/x-www-form-urlencoded" });
    expect([form.status, await code(form)]).toEqual([400, "unsupported_content_type"]);
    const big = await report({ page_id: "x", text: "x".repeat(10_000) });
    expect([big.status, await code(big)]).toEqual([413, "payload_too_large"]);
    const bot = await report({ page_id: "x", text: "buy cheap things now", website: "http://spam.example" });
    expect(bot.status).toBe(202);
  });

  it("report intake is rate-limited per client (5 a minute)", async () => {
    // One burst, so that it cannot straddle two limiter windows.
    const statuses = await Promise.all(
      Array.from({ length: 20 }, () =>
        report({ page_id: "x", text: "spam spam spam", website: "bot" }, { "cf-connecting-ip": "192.0.2.200" }).then(
          (r) => r.status,
        ),
      ),
    );
    expect(statuses).toContain(429);
    expect(statuses.every((s) => s === 202 || s === 429)).toBe(true);
  });
});
