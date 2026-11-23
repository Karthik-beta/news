// The search Worker and the shared query module against a real D1 shard (Miniflare) with the production schema.
import { applyD1Migrations, createExecutionContext, waitOnExecutionContext } from "cloudflare:test";
import { env, exports } from "cloudflare:workers";
import {
  indexedMonths,
  indexedPages,
  indexPage,
  listShards,
  SearchUnavailableError,
  searchPages,
} from "@news/worker-shared/search";
import { makePageRecord } from "@news/worker-shared/testing/fixtures";
import { beforeAll, describe, expect, it } from "vitest";
import worker from "../../src/index";

const page = (id: string, over: Parameters<typeof makePageRecord>[0] = {}) =>
  makePageRecord({
    id,
    published_at: "2026-11-10T12:00:00.000Z",
    updated_at: "2026-11-11T12:00:00.000Z",
    ...over,
  });

beforeAll(async () => {
  await applyD1Migrations(env.SEARCH_2026, env.SEARCH_MIGRATIONS);
  const db = env.SEARCH_2026;
  await indexPage(
    db,
    page("cpi-us", { title: "US inflation slows", categories: ["economy/inflation"], index_status: "index" }),
  );
  await indexPage(
    db,
    page("cpi-india", { title: "India inflation edges up", region: "india", page_type: "news", index_status: "index" }),
  );
  await indexPage(
    db,
    page("cpi-old", {
      title: "Inflation in the spring",
      published_at: "2026-03-02T00:00:00.000Z",
      index_status: "index",
    }),
  );
  await indexPage(db, page("cpi-hidden", { title: "Inflation note", index_status: "noindex" }));
});

const get = async (query: string) => {
  const res = await exports.default.fetch(new Request(`https://search.test/v1/search?${query}`));
  return {
    res,
    body: (await res.json()) as { results?: { page_id: string; url: string }[]; error?: { code: string } },
  };
};

describe("searchPages", () => {
  it("filters by category, region, type and date over the real schema", async () => {
    const shards = listShards(env);
    const ids = async (p: Parameters<typeof searchPages>[1]) =>
      (await searchPages(shards, p)).map((h) => h.page_id).sort();
    expect(await ids({ query: "inflation" })).toEqual(["cpi-hidden", "cpi-india", "cpi-old", "cpi-us"]);
    expect(await ids({ query: "inflation", category: "economy" })).toContain("cpi-us");
    expect(await ids({ query: "inflation", region: "india" })).toEqual(["cpi-india"]);
    expect(await ids({ query: "inflation", page_type: "news" })).toEqual(["cpi-india"]);
    expect(await ids({ query: "inflation", to: "2026-06-30" })).toEqual(["cpi-old"]);
    expect(await ids({ query: "inflation", from: "2026-11-01" })).not.toContain("cpi-old");
  });

  it("a D1 failure is a SearchUnavailableError", async () => {
    const broken = {
      year: 2026,
      db: { prepare: () => ({ bind: () => ({ all: () => Promise.reject(new Error("D1 is over its limit")) }) }) },
    };
    await expect(searchPages([broken as never], { query: "inflation" })).rejects.toBeInstanceOf(SearchUnavailableError);
  });
});

describe("the sitemap queries", () => {
  it("list only pages with index_status = index, by month", async () => {
    const shards = listShards(env);
    expect(await indexedMonths(shards)).toEqual(["2026-03", "2026-11"]);
    const paths = (await indexedPages(shards, "2026-11")).map((p) => p.path);
    expect(paths.sort()).toEqual(["/2026/11/cpi-india", "/2026/11/cpi-us"]);
    expect(await indexedPages(shards, "2026-03")).toEqual([
      { path: "/2026/11/cpi-old", updated_at: "2026-11-11T12:00:00.000Z" },
    ]);
    expect(await indexedPages(shards, "2025-01")).toEqual([]);
  });
});

describe("GET /v1/search", () => {
  it("returns the contract's results with absolute URLs, and caches the answer under the normalised key", async () => {
    const first = await get("q=Inflation++US&region=north-america");
    expect(first.res.status).toBe(200);
    expect(first.res.headers.get("cache-control")).toContain("s-maxage=600");
    expect(first.body.results?.map((r) => r.page_id)).toEqual(["cpi-us"]);
    expect(first.body.results?.[0]?.url).toBe("https://news.karthik.nz/2026/11/cpi-us");
    expect(first.body.results?.[0]).not.toHaveProperty("path");
  });

  it("answers 400 for a bad request and 501 elsewhere", async () => {
    expect((await get("")).body.error?.code).toBe("invalid_request");
    expect((await get("q=a&region=Not A Region")).res.status).toBe(400);
    expect((await get("q=one+two+three+four+five+six+seven+eight+nine")).res.status).toBe(400);
    expect((await exports.default.fetch(new Request("https://search.test/search"))).status).toBe(501);
  });

  it("answers 503 search_unavailable with Retry-After when D1 fails", async () => {
    const broken = {
      prepare: () => ({ bind: () => ({ all: () => Promise.reject(new Error("over the daily limit")) }) }),
    };
    const ctx = createExecutionContext();
    const res = await worker.fetch(
      new Request("https://search.test/v1/search?q=unavailable") as never,
      { SEARCH_2026: broken } as never,
      ctx,
    );
    await waitOnExecutionContext(ctx);
    expect(res.status).toBe(503);
    expect(Number(res.headers.get("retry-after"))).toBeGreaterThan(0);
    expect(((await res.json()) as { error: { code: string } }).error.code).toBe("search_unavailable");
  });
});
