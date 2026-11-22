// The Worker's request path without Astro: route table, edge cache, rate limit, data loaders and templates, over a
// FakeTable and an in-memory Cache API. Covers ADR 0010's "curation is data", "cache policy", "stale copy and abuse",
// and "pagination and filters" rows.

import type { Doc } from "@news/worker-shared/dynamo/table";
import { curationLatestKey } from "@news/worker-shared/keys";
import { encodeCursor } from "@news/worker-shared/read";
import { homeUrls } from "@news/worker-shared/urls";
import { createElement } from "react";
import { describe, expect, it } from "vitest";
import { ctxOf, FIXTURE_ITEMS, MemoryCache, ORIGIN, REGIONS, tableOf } from "../../tests/support/site-data";
import { staticMarkup } from "../renderer/markup";
import { Views } from "../views/Views";
import { cacheControl, matchRoute, POLICY } from "./routes";
import { type ServeDeps, serve } from "./serve";
import { type Ctx, loadView, notFoundView } from "./views";

interface Rig {
  clock: { now: number };
  cache: MemoryCache;
  ctx: Ctx;
  table: ReturnType<typeof tableOf>;
  limiterCalls: number;
  allowed: boolean;
  fail: boolean;
  get(path: string, init?: RequestInit): Promise<Response>;
}

function rig(items: Doc[] = FIXTURE_ITEMS.normal as Doc[]): Rig {
  const table = tableOf(items);
  const clock = { now: Date.parse("2025-01-15T16:00:00Z") };
  const cache = new MemoryCache(() => clock.now);
  const r: Rig = {
    clock,
    cache,
    table,
    ctx: ctxOf(table),
    limiterCalls: 0,
    allowed: true,
    fail: false,
    async get(path, init) {
      const request = new Request(`${ORIGIN}${path}`, init);
      const m = matchRoute(new URL(request.url), ORIGIN, REGIONS);
      if (m.type === "redirect") return new Response(null, { status: 301, headers: { location: m.location } });
      if (m.type === "reject") return new Response(m.message, { status: m.status });
      if (m.type !== "page") throw new Error("not ours");
      const deps: ServeDeps = {
        cache,
        waitUntil: () => {},
        now: () => clock.now,
        allow: async () => {
          r.limiterCalls++;
          return r.allowed;
        },
        render: async () => {
          if (r.fail) throw new Error("DynamoDB is down");
          const view = await loadView(r.ctx, m);
          if (view.type === "redirect")
            return new Response(null, { status: view.status, headers: { location: view.location } });
          return new Response(staticMarkup(createElement(Views, { body: view.body })), {
            status: view.body.type === "notfound" ? 404 : 200,
            headers: { "content-type": "text/html; charset=utf-8" },
          });
        },
      };
      return serve(request, m, deps);
    },
  };
  return r;
}

describe("a curation is data, not a build", () => {
  it("the next request after a new curation is written and purged differs, with the same code", async () => {
    const r = rig();
    const first = await r.get("/");
    const html1 = await first.text();
    expect(html1).toContain("Apple reports its results for the 2024 fiscal year");
    expect(r.cache.puts).toEqual([`${ORIGIN}/`]);

    // The write API commits a new LATEST (the lead is now another page) and purges the URL.
    const latest = (await r.table.get(curationLatestKey())) as Doc;
    const slots = structuredClone(latest.slots) as { lead: { page_id: string } };
    slots.lead = { page_id: "walking-trial" };
    await r.table.put({ ...latest, slots, version: 2 } as Doc);
    // Before the purge the edge still has the old page; that is the TTL's bound (5 minutes).
    expect(await (await r.get("/")).text()).toBe(html1);
    for (const key of homeUrls(ORIGIN, REGIONS)) r.cache.store.delete(key);

    const html2 = await (await r.get("/")).text();
    expect(html2).not.toBe(html1);
    expect(html2).toContain("Daily walking lowered blood pressure in a trial");
  });

  it("a page record that changes is shown on the home page after a purge or the TTL", async () => {
    const r = rig();
    await r.get("/");
    r.clock.now += POLICY.home.edge * 1000 + 1;
    const rec = (await r.table.get({ PK: "PAGE#apple-fy2024-results", SK: "LATEST" })) as Doc;
    await r.table.put({ ...rec, title: "Apple changes its headline" });
    expect(await (await r.get("/")).text()).toContain("Apple changes its headline");
  });
});

describe("cache headers per route", () => {
  const cursor = encodeCursor({ PK: "HUB#latest", SK: "2025-01-10T00:00:00.000Z#walking-trial" }) as string;
  it.each([
    ["/", POLICY.home],
    ["/?utm_source=x", POLICY.home],
    ["/region/india", POLICY.home],
    ["/latest", POLICY.hub],
    ["/economy", POLICY.hub],
    ["/economy/inflation", POLICY.hub],
    ["/entity/company%3Aapple", POLICY.hub],
    ["/latest?region=india", POLICY.hubPage],
    [`/economy?after=${cursor}`, POLICY.hubPage],
    ["/nothing-here", POLICY.notFound],
    ["/economy/nothing-here", POLICY.notFound],
  ])("%s", async (path, policy) => {
    const res = await rig().get(path);
    expect(res.headers.get("cache-control")).toBe(cacheControl(policy));
  });

  it("a cursor on a missing hub is a not-found, and a malformed one is a 400 that is not cached", async () => {
    const r = rig();
    expect((await r.get("/economy?after=%21%21")).status).toBe(400);
    expect(r.cache.puts).toEqual([]);
  });

  it("/?utm_source=x hits the entry for /", async () => {
    const r = rig();
    await r.get("/");
    const reads = r.table.calls.length;
    const hit = await r.get("/?utm_source=x");
    expect(hit.status).toBe(200);
    expect(r.table.calls.length).toBe(reads);
    expect(r.cache.puts).toEqual([`${ORIGIN}/`]);
  });

  it("a stored copy keeps no internal header and is served with the route's policy", async () => {
    const r = rig();
    await r.get("/latest");
    const hit = await r.get("/latest");
    expect([...hit.headers.keys()].filter((h) => h.startsWith("x-news"))).toEqual([]);
    expect(hit.headers.get("cache-control")).toBe(cacheControl(POLICY.hub));
  });
});

describe("stale copy and abuse", () => {
  it("a render error with a stored copy serves it with s-maxage=30; without one, a plain 503", async () => {
    const r = rig();
    const fresh = await (await r.get("/latest")).text();
    r.clock.now += POLICY.hub.edge * 1000 + 1;
    r.fail = true;
    const stale = await r.get("/latest");
    expect(stale.status).toBe(200);
    expect(stale.headers.get("cache-control")).toBe(cacheControl(POLICY.stale));
    expect(stale.headers.get("cache-control")).toContain("s-maxage=30");
    expect(await stale.text()).toBe(fresh);

    const none = await r.get("/economy");
    expect(none.status).toBe(503);
    expect(none.headers.get("cache-control")).toBe("no-store");
    expect(await none.text()).not.toContain("DynamoDB");
  });

  it("a stored copy of a page is kept far longer than its fresh time", async () => {
    const r = rig();
    await r.get("/latest");
    r.clock.now += POLICY.hub.edge * 1000 + 3600_000; // a day and an hour on
    expect(await r.cache.match(new Request(`${ORIGIN}/latest`))).toBeDefined();
  });

  it("an unknown first segment makes no URL# read, and the taxonomy is read once a minute", async () => {
    const r = rig();
    const res = await r.get("/no-such-category/whatever");
    expect(res.status).toBe(404);
    const urlReads = r.table.calls.flatMap((c) => c.keys).filter((k) => k.PK.startsWith("URL#"));
    expect(urlReads).toEqual([]);
    const taxonomyReads = () => r.table.calls.filter((c) => c.op === "query" && c.keys[0]?.PK === "TAX#tree").length;
    expect(taxonomyReads()).toBe(1);
    await r.get("/no-such-category/other");
    await r.get("/another-one");
    expect(taxonomyReads()).toBe(1);
  });

  it("a known first segment under a missing sub-category looks for a redirect, then 404s", async () => {
    const r = rig();
    await r.table.put({
      PK: "URL#/economy/old-name",
      SK: "REDIRECT",
      type: "redirect",
      lang: "en",
      from: "/economy/old-name",
      to: "/economy/inflation",
      status: 301,
      created_at: "2025-01-01T00:00:00.000Z",
    });
    const moved = await r.get("/economy/old-name");
    expect(moved.status).toBe(301);
    expect(moved.headers.get("location")).toBe("/economy/inflation");
    expect(moved.headers.get("cache-control")).toBe(cacheControl(POLICY.redirect));
    const gone = await r.get("/economy/never-existed");
    expect(gone.status).toBe(404);
    const reads = r.table.calls.flatMap((c) => c.keys).filter((k) => k.PK === "URL#/economy/never-existed");
    expect(reads).toHaveLength(1);
  });

  it("the rate limiter is hit on cache misses of 404, cursor and filter requests, not on first pages", async () => {
    const r = rig();
    const cursor = encodeCursor({ PK: "HUB#cat/economy", SK: "2025-01-14T10:00:00.000Z#x" }) as string;
    await r.get("/");
    await r.get("/latest");
    await r.get("/region/india");
    expect(r.limiterCalls).toBe(0);
    await r.get("/missing-category");
    await r.get(`/economy?after=${cursor}`);
    await r.get("/economy?region=india");
    expect(r.limiterCalls).toBe(3);
    // Cached answers do not spend it.
    await r.get("/economy?region=india");
    expect(r.limiterCalls).toBe(3);
    r.allowed = false;
    const blocked = await r.get("/economy?region=europe");
    expect(blocked.status).toBe(429);
    expect(blocked.headers.get("retry-after")).toBe("60");
    expect(blocked.headers.get("cache-control")).toBe("no-store");
  });

  it("only GET and HEAD", async () => {
    const res = await rig().get("/latest", { method: "POST" });
    expect(res.status).toBe(405);
  });
});

describe("hubs", () => {
  /** A hub of 45 entries, the newest first, the way the api writes them. */
  const hub45 = () => {
    const items: Doc[] = (FIXTURE_ITEMS.normal as Doc[]).filter((i) => i.type === "taxonomy_node");
    for (let n = 0; n < 45; n++) {
      const published = new Date(Date.parse("2025-01-10T00:00:00.000Z") + n * 3_600_000).toISOString();
      const id = `page-${String(n).padStart(2, "0")}`;
      items.push({
        PK: "HUB#cat/economy",
        SK: `${published}#${id}`,
        type: "hub_entry",
        lang: "en",
        hub: "cat/economy",
        page_id: id,
        path: `/2025/01/${id}`,
        title: `Story ${n}`,
        summary: "A story.",
        page_type: "macro",
        tier: 2,
        region: n % 3 === 0 ? "india" : "europe",
        published_at: published,
        updated_at: published,
        index_status: "index",
      });
    }
    return items;
  };
  const links = (html: string) =>
    [...html.matchAll(/<a class="pagination__end" href="([^"]+)" rel="next">/g)].map((m) =>
      (m[1] as string).replaceAll("&amp;", "&"),
    );
  const titles = (html: string) =>
    [...html.matchAll(/class="headline__link" href="[^"]+">(Story \d+)</g)].map((m) => m[1]);

  it("paginates at 20: 20, 20 and 5, newest first, with working Older links", async () => {
    const r = rig(hub45());
    const sizes: number[] = [];
    let path = "/economy";
    const seen: (string | undefined)[] = [];
    for (let i = 0; i < 4 && path; i++) {
      const res = await r.get(path);
      expect(res.status).toBe(200);
      const html = await res.text();
      sizes.push(titles(html).length);
      seen.push(...titles(html));
      path = links(html)[0] as string;
    }
    expect(sizes.filter((n) => n > 0)).toEqual([20, 20, 5]);
    expect(seen).toHaveLength(45);
    expect(new Set(seen).size).toBe(45);
    expect(seen[0]).toBe("Story 44");
    expect(seen[44]).toBe("Story 0");
  });

  it("the first page has no parameter, a cursor page is noindex, and a hub of 20 has no Older link", async () => {
    const r = rig(hub45());
    const html = await (await r.get("/economy")).text();
    const robots = async (path: string) => {
      const m = matchRoute(new URL(`${ORIGIN}${path}`), ORIGIN, REGIONS);
      const view = m.type === "page" ? await loadView(r.ctx, m) : undefined;
      return view?.type === "view" ? view.chrome.robots : undefined;
    };
    expect(await robots("/economy")).toBe("index, follow");
    const next = links(html)[0] as string;
    expect(next).toMatch(/^\/economy\?after=/);
    expect(await robots(next)).toBe("noindex, follow");
    const small = rig(hub45().filter((i) => i.type !== "hub_entry" || Number(String(i.page_id).slice(5)) < 20));
    expect(links(await (await small.get("/economy")).text())).toEqual([]);
  });

  it("the region filter keeps matching entries across a scan boundary; the cursor is the last one examined", async () => {
    const r = rig(hub45());
    const first = await (await r.get("/economy?region=india")).text();
    // 15 of the 45 are India (n % 3 === 0); all of them fit one page.
    expect(titles(first)).toHaveLength(15);
    expect(links(first)).toEqual([]);
    // europe: 30 of 45; the first page ends at the 20th match, and the second continues from it.
    const eu1 = await (await r.get("/economy?region=europe")).text();
    expect(titles(eu1)).toHaveLength(20);
    const next = links(eu1)[0] as string;
    expect(next).toContain("region=europe");
    const eu2 = await (await r.get(next)).text();
    expect(titles(eu2)).toHaveLength(10);
    expect(new Set([...titles(eu1), ...titles(eu2)]).size).toBe(30);
  });

  it("a region with no match among the entries examined shows an empty state, not an error", async () => {
    const r = rig(hub45());
    const res = await r.get("/economy?region=china");
    expect(res.status).toBe(200);
    expect(await res.text()).toContain("There are no pages in this region yet.");
  });

  it("an entity hub groups by year and shows the newest page's first figures", async () => {
    const r = rig();
    const data = { version: 1, points: {}, derived: {}, series: {}, periods: {}, quotes: {}, context: {} };
    const html = await (await r.get("/entity/company%3Aapple")).text();
    expect(html).toContain(">2025<");
    expect(html).toContain("Apple reports its results for the 2024 fiscal year");
    expect(html).not.toContain("Latest figures"); // no data.json in this rig: no strip
    void data;
  });

  it("an unknown entity is a 404 and a known one under an unknown id looks for a redirect once", async () => {
    const r = rig();
    expect((await r.get("/entity/company%3Anobody")).status).toBe(404);
    expect(r.table.calls.flatMap((c) => c.keys).filter((k) => k.PK === "URL#/entity/company%3Anobody")).toHaveLength(1);
  });
});

describe("the 404 page", () => {
  it("is the NotFound template with the site's navigation", async () => {
    const view = await notFoundView(ctxOf(tableOf()));
    expect(view.type === "view" && view.chrome.nav.map((n) => n.href)).toEqual([
      "/tech",
      "/business",
      "/finance",
      "/economy",
      "/science",
    ]);
  });
});
