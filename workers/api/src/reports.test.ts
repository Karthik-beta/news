// Unit tests (inside workerd): public report intake (rate limits, daily cap, honeypot, no IP stored, never the
// publishing table) and the Errata pull (order, cursor, retention, settling window).
import { makePageRecord } from "@news/worker-shared/testing/fixtures";
import { schemas } from "@news/worker-shared/validate";
import { describe, expect, it } from "vitest";
import { countingLimiter, harness, ORIGIN } from "./testing";

const IP = "198.51.100.23";
const REPORT = { page_id: "test-page", kind: "figure", text: "The headline CPI figure differs from the BLS table." };

function withPage(limits: { ip?: number; global?: number; daily?: number } = {}) {
  const h = harness({
    limiters: { reportIp: countingLimiter(limits.ip ?? 5), reportGlobal: countingLimiter(limits.global ?? 60) },
    ...(limits.daily ? { reportDailyCap: limits.daily } : {}),
  });
  // A published page has a search row; the intake checks the page there, not in DynamoDB.
  h.search.rows.set("test-page", makePageRecord());
  const send = (body: unknown, headers: Record<string, string> = {}) =>
    h.call("POST", "/v1/reports", {
      body,
      auth: null,
      key: null,
      headers: { "cf-connecting-ip": IP, origin: ORIGIN, "user-agent": "Mozilla/5.0 test", ...headers },
    });
  return { h, send };
}

const code = async (r: Response) => ((await r.json()) as { error: { code: string } }).error.code;

describe("POST /v1/reports (public)", () => {
  it("stores the text, the page, where and the optional e-mail, and nothing about the request", async () => {
    const { h, send } = withPage();
    const res = await send({ ...REPORT, where: "Chart 2, the October bar", email: "reader@example.org" });
    expect(res.status).toBe(202);
    expect(await res.json()).toEqual({ status: "received" });
    expect(res.headers.get("access-control-allow-origin")).toBe(ORIGIN);
    const [stored, ...rest] = h.reports.rows;
    expect(rest).toEqual([]);
    expect(Object.keys(stored ?? {}).sort()).toEqual(
      ["id", "page_id", "page_path", "kind", "text", "where", "email", "lang", "created_at", "expires_at"].sort(),
    );
    const text = JSON.stringify(h.reports.rows) + JSON.stringify(h.logs);
    expect(text).not.toContain(IP);
    expect(text).not.toContain("Mozilla");
    expect(stored).toMatchObject({ page_id: "test-page", page_path: "/2026/11/test-page", kind: "figure", lang: "en" });
  });

  it("never touches the DynamoDB table that publishing writes to", async () => {
    const { h, send } = withPage();
    expect((await send(REPORT)).status).toBe(202);
    expect((await send({ ...REPORT, page_id: "no-such-page" })).status).toBe(400);
    expect(h.table.calls).toEqual([]);
  });

  it("works without an e-mail and without an Origin header", async () => {
    const { h, send } = withPage();
    const res = await h.call("POST", "/v1/reports", { body: REPORT, auth: null, key: null });
    expect(res.status).toBe(202);
    expect(h.reports.rows[0]?.email).toBeUndefined();
    expect((await send({ page_id: "test-page", text: "Typo in the second paragraph." })).status).toBe(202);
  });

  it("a filled honeypot gets the same 202 and stores nothing", async () => {
    const { h, send } = withPage();
    const res = await send({ ...REPORT, website: "http://spam.example" });
    expect(res.status).toBe(202);
    expect(await res.json()).toEqual({ status: "received" });
    expect(h.reports.rows).toEqual([]);
    expect(h.table.calls).toEqual([]);
  });

  it("rate-limits per client and per location", async () => {
    const { h, send } = withPage({ ip: 2, global: 3 });
    expect((await send(REPORT)).status).toBe(202);
    expect((await send(REPORT)).status).toBe(202);
    const limited = await send(REPORT);
    expect(limited.status).toBe(429);
    expect(limited.headers.get("retry-after")).toBe("60");
    expect(await code(limited)).toBe("rate_limited");
    expect((await send(REPORT, { "cf-connecting-ip": "192.0.2.1" })).status).toBe(202);
    expect((await send(REPORT, { "cf-connecting-ip": "192.0.2.2" })).status).toBe(429);
    expect(h.reports.rows).toHaveLength(3);
  });

  it("caps reports per UTC day across the whole site, then stops asking the database until tomorrow", async () => {
    const { h, send } = withPage({ ip: 100, global: 100, daily: 2 });
    expect((await send(REPORT)).status).toBe(202);
    expect((await send(REPORT, { "cf-connecting-ip": "192.0.2.9" })).status).toBe(202);
    const full = await send(REPORT, { "cf-connecting-ip": "192.0.2.10" });
    expect([full.status, await code(full)]).toEqual([429, "rate_limited"]);
    expect(Number(full.headers.get("retry-after"))).toBe(14 * 3600); // 10:00 → midnight UTC
    const add = h.reports.add.bind(h.reports);
    let asked = 0;
    h.reports.add = (r, cap) => {
      asked++;
      return add(r, cap);
    };
    expect((await send(REPORT, { "cf-connecting-ip": "192.0.2.11" })).status).toBe(429);
    expect(asked).toBe(0);
    h.setNow("2026-11-13T00:00:01Z");
    expect((await send(REPORT, { "cf-connecting-ip": "192.0.2.12" })).status).toBe(202);
    expect(h.reports.rows).toHaveLength(3);
  });

  it.each([
    ["an oversize body", { ...REPORT, text: "x".repeat(9000) }, {}, 413, "payload_too_large"],
    ["a too-long text", { ...REPORT, text: "x".repeat(2001) }, {}, 400, "invalid_request"],
    ["a text over 4,000 bytes", { ...REPORT, text: "€".repeat(1500) }, {}, 400, "invalid_request"],
    ["a too-short text", { ...REPORT, text: "bad" }, {}, 400, "invalid_request"],
    ["a too-long where", { ...REPORT, where: "x".repeat(201) }, {}, 400, "invalid_request"],
    ["an invalid e-mail", { ...REPORT, email: "nope" }, {}, 400, "invalid_request"],
    ["an extra field", { ...REPORT, ip: "1.2.3.4" }, {}, 400, "invalid_request"],
    ["an unknown page", { ...REPORT, page_id: "no-such-page" }, {}, 400, "unknown_page"],
    ["another site's origin", REPORT, { origin: "https://evil.example" }, 403, "forbidden"],
    ["form encoding", REPORT, { "content-type": "application/x-www-form-urlencoded" }, 400, "unsupported_content_type"],
  ])("refuses %s", async (_label, body, headers, status, errorCode) => {
    const { h, send } = withPage();
    const res = await send(body, headers as Record<string, string>);
    expect(res.status).toBe(status);
    const json = await res.json();
    expect(schemas.validateErrorBody(json)).toBe(true);
    expect((json as { error: { code: string } }).error.code).toBe(errorCode);
    expect(h.reports.rows).toEqual([]);
  });

  it("refuses reports on a withdrawn page (its search row is gone)", async () => {
    const { h, send } = withPage();
    h.search.rows.delete("test-page");
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
  type Page = { reports: { created_at: string; id: string }[]; next_after: string | null };
  const pull = async (h: ReturnType<typeof harness>, query: string) => {
    const res = await h.call("GET", `/v1/reports${query}`);
    expect(res.status).toBe(200);
    const page = (await res.json()) as Page;
    expect(schemas.validateReportsResponse(page)).toBe(true);
    return page;
  };

  it("needs the key", async () => {
    const { h } = withPage();
    expect((await h.call("GET", "/v1/reports", { auth: null })).status).toBe(401);
  });

  it("returns reports in order of receipt, paged with next_after", async () => {
    const { h, send } = withPage({ ip: 100 });
    const times = ["2026-10-30T10:00:00.000Z", "2026-11-01T09:00:00.000Z", "2026-11-02T09:00:00.000Z"];
    for (const t of times) {
      h.setNow(t);
      expect((await send(REPORT)).status).toBe(202);
    }
    h.setNow("2026-11-12T10:00:00Z");
    const page1 = await pull(h, "?after=2026-10-01T00:00:00Z&limit=2");
    expect(page1.reports.map((r) => r.created_at)).toEqual(times.slice(0, 2));
    expect(page1.next_after).toMatch(/^2026-11-01T09:00:00.000Z#r_[0-9a-f]{32}$/);
    const page2 = await pull(h, `?after=${encodeURIComponent(page1.next_after ?? "")}&limit=2`);
    expect(page2.reports.map((r) => r.created_at)).toEqual(times.slice(2));
    expect(page2.next_after).toBeNull();
  });

  it("a whole-second or short-fraction `after` still includes reports later in that second", async () => {
    const { h, send } = withPage();
    h.setNow("2026-11-12T10:00:00.500Z");
    expect((await send(REPORT)).status).toBe(202);
    h.setNow("2026-11-12T11:00:00Z");
    expect((await pull(h, "?after=2026-11-12T10:00:00Z")).reports).toHaveLength(1);
    expect((await pull(h, "?after=2026-11-12T10:00:00.5Z")).reports).toHaveLength(1);
    expect((await pull(h, "?after=2026-11-12T10:00:00.501Z")).reports).toHaveLength(0);
  });

  it("leaves reports of the last minute for a later pull, so none can land behind next_after", async () => {
    const { h, send } = withPage();
    h.setNow("2026-11-12T10:00:00Z");
    expect((await send(REPORT)).status).toBe(202);
    h.setNow("2026-11-12T10:00:30Z");
    expect((await pull(h, "?after=2026-11-12T09:00:00Z")).reports).toHaveLength(0);
    h.setNow("2026-11-12T10:01:01Z");
    expect((await pull(h, "?after=2026-11-12T09:00:00Z")).reports).toHaveLength(1);
  });

  it("never returns expired reports or looks back more than 180 days, and deletes the expired ones", async () => {
    const { h, send } = withPage();
    h.setNow("2026-01-10T10:00:00Z");
    expect((await send(REPORT)).status).toBe(202);
    h.setNow("2026-05-10T10:00:00Z");
    expect((await send(REPORT)).status).toBe(202);
    h.setNow("2026-07-10T10:00:00Z"); // the January report is 181 days old
    const page = await pull(h, "?after=2025-12-01T00:00:00Z");
    expect(page.reports.map((r) => r.created_at)).toEqual(["2026-05-10T10:00:00.000Z"]);
    expect(h.reports.rows).toHaveLength(1);
  });

  it("refuses a malformed cursor, an impossible date or a bad limit", async () => {
    const { h } = withPage();
    expect((await h.call("GET", "/v1/reports?after=yesterday")).status).toBe(400);
    expect((await h.call("GET", "/v1/reports?after=2026-02-31T00:00:00Z")).status).toBe(400);
    expect((await h.call("GET", "/v1/reports?limit=0")).status).toBe(400);
    expect((await h.call("GET", "/v1/reports?limit=201")).status).toBe(400);
  });
});
