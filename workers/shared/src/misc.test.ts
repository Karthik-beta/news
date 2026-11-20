// Unit tests for the small modules: keys, URLs, crypto, HTTP helpers, search query building, purge, read layer.
import { describe, expect, it } from "vitest";
import { secretsEqual, sha256Hex } from "./crypto";
import { FakeTable } from "./dynamo/fake";
import { toDoc } from "./dynamo/table";
import { BodyTooLargeError, isJsonContentType, readBodyCapped } from "./http";
import { categoryLineage, hubsForPage, versionFromSk, versionSk } from "./keys";
import { CloudflarePurger, PurgeError } from "./purge";
import { decodeCursor, encodeCursor, Reader } from "./read";
import { listShards, QueryTooLongError, shardFor, toFtsQuery } from "./search";
import { makeMeta, makePageRecord } from "./testing/fixtures";
import { normalizeTimestamp } from "./time";
import { homeUrls, hubPath, pageFileUrls, regionViews } from "./urls";

describe("keys", () => {
  it("pads versions and parses them back", () => {
    expect(versionSk(7)).toBe("V#000007");
    expect(versionFromSk("V#000123")).toBe(123);
    expect(() => versionSk(0)).toThrow();
    expect(() => versionSk(1_000_000)).toThrow();
    expect(() => versionSk(1.5)).toThrow();
  });

  it("lists a page in latest, its categories with parents, its entities and its region", () => {
    expect(hubsForPage(makeMeta())).toEqual([
      "latest",
      "cat/economy",
      "cat/economy/inflation",
      "ent/country:US",
      "region/north-america",
    ]);
    expect(categoryLineage("a/b/c")).toEqual(["a", "a/b", "a/b/c"]);
  });
});

describe("timestamps", () => {
  it.each([
    ["2026-11-12T10:00:00Z", "2026-11-12T10:00:00.000Z"],
    ["2026-11-12T10:00:00.5Z", "2026-11-12T10:00:00.500Z"],
    ["2026-11-12T10:00:00.123Z", "2026-11-12T10:00:00.123Z"],
    ["2028-02-29T00:00:00Z", "2028-02-29T00:00:00.000Z"],
    ["2026-02-29T00:00:00Z", null],
    ["2026-02-31T12:00:00Z", null],
    ["2026-04-31T12:00:00Z", null],
    ["2026-13-01T00:00:00Z", null],
    ["2026-11-12T24:00:00Z", null],
    ["2026-11-12 10:00:00Z", null],
    ["2026-11-12T10:00:00+01:00", null],
  ])("%s → %s", (input, expected) => {
    expect(normalizeTimestamp(input)).toBe(expected);
  });

  it("normalised timestamps sort as time", () => {
    const a = normalizeTimestamp("2026-11-12T10:00:00Z") as string;
    const b = normalizeTimestamp("2026-11-12T10:00:00.5Z") as string;
    expect(a < b).toBe(true);
  });
});

describe("urls", () => {
  it("maps hubs to site paths", () => {
    expect(["latest", "cat/economy/inflation", "ent/company:apple", "region/europe"].map(hubPath)).toEqual([
      "/latest",
      "/economy/inflation",
      "/entity/company%3Aapple",
      "/region/europe",
    ]);
  });

  it("a page's purge list has both aliases and every file, index.html included", () => {
    expect(pageFileUrls("https://n.test", "/2026/11/p", ["index.html", "data.json"])).toEqual([
      "https://n.test/2026/11/p",
      "https://n.test/2026/11/p/",
      "https://n.test/2026/11/p/index.html",
      "https://n.test/2026/11/p/data.json",
    ]);
  });

  it("region views: default list, override, invalid codes dropped", () => {
    expect(regionViews()).toContain("india");
    expect(regionViews("europe, india,Bad Code")).toEqual(["europe", "india"]);
    expect(homeUrls("https://x.test", ["europe"])).toEqual(["https://x.test/", "https://x.test/region/europe"]);
  });
});

describe("crypto", () => {
  it("sha256", async () => {
    expect(await sha256Hex("abc")).toBe("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
  });

  it("compares secrets exactly, whatever their lengths", async () => {
    expect(await secretsEqual("s3cret-value", "s3cret-value")).toBe(true);
    expect(await secretsEqual("s3cret-valuf", "s3cret-value")).toBe(false);
    expect(await secretsEqual("", "s3cret-value")).toBe(false);
    expect(await secretsEqual("s3cret-value-and-more", "s3cret-value")).toBe(false);
  });
});

describe("http", () => {
  const req = (body: BodyInit, headers: Record<string, string> = {}) =>
    new Request("http://x/", { method: "POST", body, headers, duplex: "half" } as RequestInit);

  it("reads a body under the cap", async () => {
    expect(await readBodyCapped(req("hello"), 10)).toBe("hello");
  });

  it("refuses by Content-Length before reading, and while streaming", async () => {
    await expect(readBodyCapped(req("x", { "content-length": "999" }), 10)).rejects.toBeInstanceOf(BodyTooLargeError);
    const stream = new ReadableStream({
      start(c) {
        for (let i = 0; i < 5; i++) c.enqueue(new TextEncoder().encode("12345"));
        c.close();
      },
    });
    await expect(readBodyCapped(req(stream), 10)).rejects.toBeInstanceOf(BodyTooLargeError);
  });

  it("refuses invalid UTF-8", async () => {
    await expect(readBodyCapped(req(new Uint8Array([0xff, 0xfe])), 10)).rejects.toThrow();
  });

  it("accepts only JSON content types", () => {
    expect(isJsonContentType("application/json")).toBe(true);
    expect(isJsonContentType("application/json; charset=UTF-8")).toBe(true);
    expect(isJsonContentType("text/plain")).toBe(false);
    expect(isJsonContentType("application/json; charset=latin1")).toBe(false);
    expect(isJsonContentType(null)).toBe(false);
  });
});

describe("search query building", () => {
  it.each([
    ["inflation", '"inflation"*'],
    ["US CPI, October!", '"US" "CPI" "October"*'],
    ['title:x OR "a" NEAR(b) *', '"title" "x" "OR" "a" "NEAR" "b"'],
    ["café Ünïcode 2026", '"café" "Ünïcode" "2026"*'],
    ["   ", null],
    ["***", null],
    // A prefix needs 3 characters: a 1- or 2-letter prefix would match a large share of the index.
    ["US", '"US"'],
    ["inflation in", '"inflation" "in"'],
    ["fed rat", '"fed" "rat"*'],
    // No normalisation the index does not apply too: indexed text is stored as written (FTS5 unicode61 folds case
    // and diacritics on both sides), so a full-width word is searched as written.
    ["ＡＢＣ results", '"ＡＢＣ" "results"*'],
  ])("%j → %j", (input, expected) => {
    expect(toFtsQuery(input)).toBe(expected);
  });

  it("keeps combining marks inside their word, as FTS5's unicode61 tokenizer does", () => {
    expect(toFtsQuery("naïve")).toBe('"naïve"*');
    expect(toFtsQuery("café naïve")).toBe('"café" "naïve"*');
    expect(toFtsQuery("महंगाई दर")).toBe('"महंगाई" "दर"');
    expect(toFtsQuery("́")).toBeNull();
  });

  it("accepts 8 words, all required, and refuses more instead of dropping some", () => {
    expect(toFtsQuery("a b c d e f g h")?.split(" ")).toHaveLength(8);
    expect(() => toFtsQuery("alpha beta gamma delta epsilon zeta eta theta iota")).toThrow(QueryTooLongError);
  });

  it("finds SEARCH_<year> shards, newest first, and routes by publication year", () => {
    const db = { prepare: () => ({}) } as unknown as D1Database;
    const env = { SEARCH_2025: db, SEARCH_2026: db, SEARCH_DB: db, OTHER: "x" };
    expect(listShards(env).map((s) => s.year)).toEqual([2026, 2025]);
    expect(shardFor(env, "2025-03-01T00:00:00Z")).toBe(db);
    expect(() => shardFor(env, "2024-03-01T00:00:00Z")).toThrow(/SEARCH_2024/);
  });
});

describe("CloudflarePurger", () => {
  const zoneId = "0123456789abcdef0123456789abcdef";

  it("deduplicates, batches by 30 and pauses between calls", async () => {
    const bodies: string[][] = [];
    const sleeps: number[] = [];
    const purger = new CloudflarePurger({
      zoneId,
      token: "t",
      fetch: async (r) => {
        expect(r.headers.get("authorization")).toBe("Bearer t");
        expect(r.url).toBe(`https://api.cloudflare.com/client/v4/zones/${zoneId}/purge_cache`);
        bodies.push(((await r.json()) as { files: string[] }).files);
        return Response.json({ success: true });
      },
      sleep: async (ms) => {
        sleeps.push(ms);
      },
    });
    const urls = Array.from({ length: 65 }, (_, i) => `https://news.test/p${i}`);
    expect(await purger.purge([...urls, ...urls])).toHaveLength(65);
    expect(bodies.map((b) => b.length)).toEqual([30, 30, 5]);
    expect(sleeps).toEqual([250, 250]);
  });

  it("waits once on 429, then fails with PurgeError", async () => {
    let calls = 0;
    const purger = new CloudflarePurger({
      zoneId,
      token: "t",
      fetch: async () => {
        calls++;
        return new Response("slow down", { status: 429, headers: { "retry-after": "1" } });
      },
      sleep: async () => {},
    });
    await expect(purger.purge(["https://news.test/"])).rejects.toBeInstanceOf(PurgeError);
    expect(calls).toBe(2);
  });

  it("refuses a malformed zone ID", () => {
    expect(() => new CloudflarePurger({ zoneId: "../x", token: "t" })).toThrow();
  });
});

describe("Reader", () => {
  it("reads pages, hubs newest first with cursors, and the newest curation version", async () => {
    const t = new FakeTable();
    const r = new Reader(t);
    // A PageRecord is an interface, which TypeScript will not widen to the index-signature Doc: convert, do not cast.
    t.seed(toDoc(makePageRecord({ id: "a" })));
    for (const [i, day] of ["01", "02", "03"].entries())
      t.seed({ PK: "HUB#latest", SK: `2026-11-${day}T00:00:00Z#p${i}`, page_id: `p${i}` });
    t.seed(
      { PK: "HOME#CURATION", SK: "V#000002" },
      { PK: "HOME#CURATION", SK: "V#000010" },
      { PK: "HOME#CURATION", SK: "LATEST" },
    );

    expect((await r.getPage("a"))?.title).toBe("Test page a");
    expect(await r.getPage("b")).toBeUndefined();
    const first = await r.listHub("latest", { limit: 2 });
    expect(first.items.map((i) => i.page_id)).toEqual(["p2", "p1"]);
    const second = await r.listHub("latest", { limit: 2, cursor: first.cursor });
    expect(second.items.map((i) => i.page_id)).toEqual(["p0"]);
    expect(await r.newestCurationVersion()).toBe(10);
    expect([...(await r.getPages(["a", "a", "zz"])).keys()]).toEqual(["a"]);
  });

  it("cursors round-trip and refuse junk", () => {
    const c = encodeCursor({ PK: "HUB#x", SK: "2026-11-01T00:00:00Z#a" });
    expect(decodeCursor("HUB#x", c)).toEqual({ PK: "HUB#x", SK: "2026-11-01T00:00:00Z#a" });
    expect(() => decodeCursor("HUB#x", "not valid!")).toThrow(RangeError);
  });
});
