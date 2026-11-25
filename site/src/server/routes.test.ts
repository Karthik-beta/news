// The route table: what a URL is, its canonical cache key, and that routes.ts matches ADR 0010's table.
import { readFileSync } from "node:fs";
import { encodeCursor } from "@news/worker-shared/read";
import { homeUrls, hubPath, regionViewPath } from "@news/worker-shared/urls";
import { describe, expect, it } from "vitest";
import { ORIGIN, REGIONS } from "../../tests/support/site-data";
import { cacheControl, cacheKeyOf, matchRoute, POLICY } from "./routes";

const url = (path: string) => new URL(`${ORIGIN}${path}`);
const match = (path: string) => matchRoute(url(path), ORIGIN, REGIONS);
const cursor = encodeCursor({ PK: "HUB#latest", SK: "2025-01-14T10:00:00.000Z#apple-fy2024-results" }) as string;

describe("what a URL is", () => {
  it.each([
    ["/", "home"],
    ["/region/india", "region"],
    ["/latest", "latest"],
    ["/economy", "category"],
    ["/economy/inflation", "category"],
    ["/entity/company%3Aapple", "entity"],
    ["/search", "search"],
    ["/search?q=inflation&region=india", "search"],
    ["/rss.xml", "feed"],
    ["/atom.xml", "feed"],
    ["/economy/inflation/rss.xml", "feed"],
    ["/sitemap.xml", "sitemap"],
    ["/sitemaps/2025-01.xml", "sitemap"],
    ["/llms.txt", "llms"],
  ])("%s is the %s route", (path, kind) => {
    expect(match(path)).toMatchObject({ type: "page", kind });
  });

  it("routes that belong to other parts of the site pass through", () => {
    for (const path of ["/methodology", "/report", "/feeds", "/fixtures/x", "/_astro/a.js"])
      expect(match(path)).toEqual({ type: "pass" });
  });

  it("a trailing slash answers 301 to the path without it, keeping the query", () => {
    expect(match("/economy/")).toEqual({ type: "redirect", location: "/economy" });
    expect(match("/latest/?region=india")).toEqual({ type: "redirect", location: "/latest?region=india" });
  });

  it("a path that starts with two slashes, or a slash and a backslash, is a 404, never an off-site 301", () => {
    for (const path of ["//evil.com/", "///evil.com/", "/\\evil.com/", "//evil.com"])
      expect(match(path), path).toMatchObject({ type: "reject", status: 404 });
  });

  it("an unknown region, a path that cannot be a category, or a long path is a 404", () => {
    for (const path of [
      "/region/mars",
      "/latest?region=mars",
      "/Economy",
      "/a/b/c/d/e",
      "/economy//x",
      "/region",
      "/entity",
      "/latest/x",
      "/%E0%A4%A",
    ])
      expect(match(path), path).toMatchObject({ type: "reject", status: 404 });
  });

  it("a malformed cursor is a 400, before anything is read", () => {
    for (const bad of ["", "!!", "x".repeat(701), "bm90LWEta2V5"]) {
      expect(match(`/economy?after=${bad}`), bad).toMatchObject({ type: "reject", status: 400 });
    }
    expect(match(`/economy?after=${cursor}`)).toMatchObject({ type: "page", after: cursor });
  });

  it("limits the requests that miss the cache on paths an attacker could multiply", () => {
    const limited = (path: string) => (match(path) as { limited: boolean }).limited;
    expect(limited("/")).toBe(false);
    expect(limited("/latest")).toBe(false);
    expect(limited("/region/india")).toBe(false);
    expect(limited("/economy")).toBe(true);
    expect(limited("/entity/company%3Aapple")).toBe(true);
    expect(limited("/latest?region=india")).toBe(true);
    expect(limited(`/region/india?after=${cursor}`)).toBe(true);
  });
});

describe("search, feeds and sitemaps", () => {
  it("search keys on the normalised words and the filters, and drops what the page does not take", () => {
    const key = (path: string) => cacheKeyOf(url(path), ORIGIN, REGIONS);
    expect(key("/search")).toBe(`${ORIGIN}/search`);
    expect(key("/search?q=Inflation++US&region=india&utm_source=x&limit=50")).toBe(
      key("/search?region=india&q=inflation us"),
    );
    expect(key("/search?q=inflation")).not.toBe(key("/search?q=inflation&type=macro"));
  });

  it("a search with a bad filter or a long query is a 400; every search miss is rate limited", () => {
    expect(match("/search?q=a&region=North%20America")).toMatchObject({ type: "reject", status: 400 });
    expect(match(`/search?q=${"a".repeat(201)}`)).toMatchObject({ type: "reject", status: 400 });
    expect(match("/search/x")).toMatchObject({ type: "reject", status: 404 });
    expect(match("/search?q=inflation")).toMatchObject({ kind: "search", limited: true, purged: false });
  });

  it("feeds are keyed by format and category; a category feed is rate limited, the site-wide one is not", () => {
    expect(match("/rss.xml")).toMatchObject({ kind: "feed", feed: { format: "rss" }, limited: false });
    expect(match("/economy/inflation/atom.xml")).toMatchObject({
      kind: "feed",
      feed: { format: "atom", category: "economy/inflation" },
      key: `${ORIGIN}/economy/inflation/atom.xml`,
      limited: true,
    });
    expect(match("/Economy/rss.xml")).toMatchObject({ type: "reject", status: 404 });
    expect(match("/a/b/c/d/e/rss.xml")).toMatchObject({ type: "reject", status: 404 });
  });

  it("a child sitemap names a real month; nothing else lives under /sitemaps", () => {
    expect(match("/sitemaps/2025-01.xml")).toMatchObject({ kind: "sitemap", month: "2025-01" });
    for (const path of ["/sitemaps/2025-13.xml", "/sitemaps/x.xml", "/sitemaps", "/sitemaps/2025-01.xml/x"])
      expect(match(path), path).toMatchObject({ type: "reject", status: 404 });
  });
});

describe("the canonical cache key", () => {
  it("ignores parameters the route does not know, so /?utm_source=x is the purged /", () => {
    expect(cacheKeyOf(url("/?utm_source=x&fbclid=1"), ORIGIN, REGIONS)).toBe(`${ORIGIN}/`);
    expect(cacheKeyOf(url("/economy?utm_medium=y"), ORIGIN, REGIONS)).toBe(`${ORIGIN}/economy`);
    expect(cacheKeyOf(url("/?after=x&region=india"), ORIGIN, REGIONS)).toBe(`${ORIGIN}/`);
  });

  it("puts the known parameters in a fixed order", () => {
    const a = cacheKeyOf(url(`/economy?after=${cursor}&region=india`), ORIGIN, REGIONS);
    const b = cacheKeyOf(url(`/economy?region=india&after=${cursor}`), ORIGIN, REGIONS);
    expect(a).toBe(b);
    expect(a).toBe(`${ORIGIN}/economy?region=india&after=${cursor}`);
  });

  it("is the exact URL the api purges: home, every region view and every hub path", () => {
    const hubs = [
      "latest",
      "cat/economy",
      "cat/economy/inflation",
      "cat/g7",
      "ent/company:apple",
      "ent/country:IN",
      "region/india",
    ];
    const purged = [
      ...homeUrls(ORIGIN, REGIONS),
      ...hubs.map((h) => `${ORIGIN}${hubPath(h)}`),
      `${ORIGIN}${regionViewPath("europe")}`,
    ];
    for (const purgedUrl of purged) {
      expect(cacheKeyOf(new URL(purgedUrl), ORIGIN, REGIONS), purgedUrl).toBe(purgedUrl);
    }
    // Typed by a reader with the ID unencoded.
    expect(cacheKeyOf(url("/entity/company:apple"), ORIGIN, REGIONS)).toBe(`${ORIGIN}${hubPath("ent/company:apple")}`);
  });
});

/** The ADR's route table, row by row: [first cell starts with, edge, browser] in seconds. */
const SECONDS = { s: 1, min: 60, h: 3600, day: 86_400 } as const;
const seconds = (cell: string) => {
  const m = /(\d+)\s*(s|min|h|day)\b/.exec(cell);
  return Number(m?.[1]) * SECONDS[m?.[2] as keyof typeof SECONDS];
};

describe("routes.ts matches ADR 0010", () => {
  const adr = readFileSync(new URL("../../../docs/adr/0010-ssr-routing-and-caching.md", import.meta.url), "utf8");
  const rows = adr
    .split("\n")
    .filter((l) => l.startsWith("| ") && l.split("|").length > 6)
    .map((l) => l.split("|").map((c) => c.trim()));
  const row = (start: string) => {
    const r = rows.find((c) => c[1]?.startsWith(start));
    expect(r, start).toBeDefined();
    return { edge: seconds(r?.[4] ?? ""), browser: seconds(r?.[5] ?? "") };
  };

  it.each([
    ["`/`", "home"],
    ["`/latest`, `/<category>", "hub"],
    ["The same with `?after", "hubPage"],
    ["Redirect:", "redirect"],
    ["`/search", "search"],
    ["`/rss.xml`", "feed"],
    ["`/sitemap.xml`", "daily"],
    ["`/llms.txt`", "daily"],
    ["404", "notFound"],
  ] as const)("%s", (start, name) => {
    expect(POLICY[name]).toEqual(row(start));
  });

  it("writes Cache-Control from the policy", () => {
    expect(cacheControl(POLICY.home)).toBe("public, max-age=60, s-maxage=300");
  });
});
