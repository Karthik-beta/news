// Unit tests (inside workerd): public report intake (rate limit, honeypot, no IP stored) and the Errata pull.
import { makePageRecord } from "@news/worker-shared/testing/fixtures";
import { schemas } from "@news/worker-shared/validate";
import { describe, expect, it } from "vitest";
import { countingLimiter, expectItemsValid, harness, ORIGIN } from "./testing";

const IP = "198.51.100.23";
const REPORT = { page_id: "test-page", kind: "figure", text: "The headline CPI figure differs from the BLS table." };

function withPage(limits: { ip?: number; global?: number } = {}) {
  const h = harness({
    limiters: { reportIp: countingLimiter(limits.ip ?? 5), reportGlobal: countingLimiter(limits.global ?? 60) },
  });
  h.table.seed(makePageRecord() as never);
  const send = (body: unknown, headers: Record<string, string> = {}) =>
    h.call("POST", "/v1/reports", {
      body,
      auth: null,
      key: null,
      headers: { "cf-connecting-ip": IP, origin: ORIGIN, "user-agent": "Mozilla/5.0 test", ...headers },
    });
  return { h, send };
}

const reports = (h: ReturnType<typeof harness>) => h.table.dump().filter((i) => i.type === "report");

describe("POST /v1/reports (public)", () => {
  it("stores the text, the page and the optional e-mail, and nothing about the request", async () => {
    const { h, send } = withPage();
    const res = await send({ ...REPORT, email: "reader@example.org" });
    expect(res.status).toBe(202);
    expect(await res.json()).toEqual({ status: "received" });
    expect(res.headers.get("access-control-allow-origin")).toBe(ORIGIN);
    const [stored, ...rest] = reports(h);
    expect(rest).toEqual([]);
    expectItemsValid([stored as Record<string, unknown>]);
    expect(Object.keys(stored ?? {}).sort()).toEqual(
      [
        "PK",
        "SK",
        "type",
        "id",
        "page_id",
        "page_path",
        "kind",
        "text",
        "email",
        "lang",
        "created_at",
        "expires_at",
      ].sort(),
    );
    const text = JSON.stringify(h.table.dump()) + JSON.stringify(h.logs);
    expect(text).not.toContain(IP);
    expect(text).not.toContain("Mozilla");
    expect(stored).toMatchObject({ page_id: "test-page", page_path: "/2026/11/test-page", kind: "figure", lang: "en" });
  });

  it("works without an e-mail and without an Origin header", async () => {
    const { h, send } = withPage();
    const res = await h.call("POST", "/v1/reports", { body: REPORT, auth: null, key: null });
    expect(res.status).toBe(202);
    expect(reports(h)[0]?.email).toBeUndefined();
    expect((await send({ page_id: "test-page", text: "Typo in the second paragraph." })).status).toBe(202);
  });

  it("a filled honeypot gets the same 202 and stores nothing", async () => {
    const { h, send } = withPage();
    const res = await send({ ...REPORT, website: "http://spam.example" });
    expect(res.status).toBe(202);
    expect(await res.json()).toEqual({ status: "received" });
    expect(h.table.writes).toEqual([]);
    expect(h.table.calls).toEqual([]);
  });

  it("rate-limits per client and globally", async () => {
    const { h, send } = withPage({ ip: 2, global: 3 });
    expect((await send(REPORT)).status).toBe(202);
    expect((await send(REPORT)).status).toBe(202);
    const limited = await send(REPORT);
    expect(limited.status).toBe(429);
    expect(limited.headers.get("retry-after")).toBe("60");
    expect(((await limited.json()) as { error: { code: string } }).error.code).toBe("rate_limited");
    expect((await send(REPORT, { "cf-connecting-ip": "192.0.2.1" })).status).toBe(202);
    expect((await send(REPORT, { "cf-connecting-ip": "192.0.2.2" })).status).toBe(429);
    expect(reports(h)).toHaveLength(3);
  });

  it.each([
    ["an oversize body", { ...REPORT, text: "x".repeat(9000) }, {}, 413, "payload_too_large"],
    ["a too-long text", { ...REPORT, text: "x".repeat(2001) }, {}, 400, "invalid_request"],
    ["a too-short text", { ...REPORT, text: "bad" }, {}, 400, "invalid_request"],
    ["an invalid e-mail", { ...REPORT, email: "nope" }, {}, 400, "invalid_request"],
    ["an extra field", { ...REPORT, ip: "1.2.3.4" }, {}, 400, "invalid_request"],
    ["an unknown page", { ...REPORT, page_id: "no-such-page" }, {}, 400, "unknown_page"],
    ["another site's origin", REPORT, { origin: "https://evil.example" }, 403, "forbidden"],
    ["form encoding", REPORT, { "content-type": "application/x-www-form-urlencoded" }, 400, "unsupported_content_type"],
  ])("refuses %s", async (_label, body, headers, status, code) => {
    const { h, send } = withPage();
    const res = await send(body, headers as Record<string, string>);
    expect(res.status).toBe(status);
    const json = await res.json();
    expect(schemas.validateErrorBody(json)).toBe(true);
    expect((json as { error: { code: string } }).error.code).toBe(code);
    expect(reports(h)).toEqual([]);
  });

  it("refuses reports on a withdrawn page", async () => {
    const { h, send } = withPage();
    h.table.seed(makePageRecord({}, { status: "withdrawn" }) as never);
    expect((await send(REPORT)).status).toBe(400);
  });

  it("answers the CORS preflight for the site only", async () => {
    const { h } = withPage();
    const ok = await h.call("OPTIONS", "/v1/reports", { auth: null, key: null, headers: { origin: ORIGIN } });
    expect(ok.status).toBe(204);
    expect(ok.headers.get("access-control-allow-methods")).toBe("POST, OPTIONS");
    const other = await h.call("OPTIONS", "/v1/reports", {
      auth: null,
      key: null,
      headers: { origin: "https://evil.example" },
    });
    expect(other.headers.get("access-control-allow-origin")).toBeNull();
  });
});

describe("GET /v1/reports (Errata pull)", () => {
  it("needs the key", async () => {
    const { h } = withPage();
    expect((await h.call("GET", "/v1/reports", { auth: null })).status).toBe(401);
  });

  it("returns reports in order of receipt, paged with next_after, across months", async () => {
    const { h, send } = withPage({ ip: 100 });
    const times = ["2026-10-30T10:00:00.000Z", "2026-11-01T09:00:00.000Z", "2026-11-02T09:00:00.000Z"];
    for (const t of times) {
      h.setNow(t);
      expect((await send(REPORT)).status).toBe(202);
    }
    h.setNow("2026-11-12T10:00:00Z");
    const first = await h.call("GET", "/v1/reports?after=2026-10-01T00:00:00Z&limit=2");
    expect(first.status).toBe(200);
    const page1 = (await first.json()) as { reports: { created_at: string }[]; next_after: string | null };
    expect(schemas.validateReportsResponse(page1)).toBe(true);
    expect(page1.reports.map((r) => r.created_at)).toEqual(times.slice(0, 2));
    expect(page1.next_after).toMatch(/^2026-11-01T09:00:00.000Z#r_[0-9a-f]{32}$/);
    const second = await h.call("GET", `/v1/reports?after=${encodeURIComponent(page1.next_after ?? "")}&limit=2`);
    const page2 = (await second.json()) as { reports: { created_at: string }[]; next_after: string | null };
    expect(page2.reports.map((r) => r.created_at)).toEqual(times.slice(2));
    expect(page2.next_after).toBeNull();
  });

  it("refuses a malformed cursor or limit", async () => {
    const { h } = withPage();
    expect((await h.call("GET", "/v1/reports?after=yesterday")).status).toBe(400);
    expect((await h.call("GET", "/v1/reports?limit=0")).status).toBe(400);
    expect((await h.call("GET", "/v1/reports?limit=201")).status).toBe(400);
  });
});
