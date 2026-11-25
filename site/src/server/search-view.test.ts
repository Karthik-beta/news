// The search page: the form alone before a search, results, the empty state, and the daily-limit answer when D1 fails.
// The index itself (filters, FTS5) is tested against real D1 in workers/search; here the shard is a recording fake.
import type { SearchShard } from "@news/worker-shared/search";
import { createElement } from "react";
import { describe, expect, it } from "vitest";
import { ctxOf, ORIGIN, REGIONS, tableOf } from "../../tests/support/site-data";
import { staticMarkup } from "../renderer/markup";
import { Views } from "../views/Views";
import { matchRoute } from "./routes";
import { loadView } from "./views";

const ROW = {
  page_id: "us-cpi-december-2024",
  path: "/2025/01/us-cpi-december-2024",
  lang: "en",
  title: "US consumer prices rise 2.9% in the year to December",
  summary: "Housing keeps the rate up.",
  categories: " economy economy/inflation ",
  region: "north-america",
  page_type: "macro",
  published_at: "2025-01-15T13:00:00.000Z",
};

/** A shard whose D1 returns `rows` (or throws) and records the SQL and bound values it was asked. */
function shardOf(rows: unknown[] | Error) {
  const calls: { sql: string; binds: unknown[] }[] = [];
  return {
    calls,
    shard: {
      year: 2025,
      db: {
        prepare: (sql: string) => ({
          bind: (...binds: unknown[]) => ({
            all: async () => {
              calls.push({ sql, binds });
              if (rows instanceof Error) throw rows;
              return { results: rows };
            },
          }),
        }),
      } as unknown as SearchShard["db"],
    },
  };
}

const search = async (qs: string, rows: unknown[] | Error = [ROW]) => {
  const { shard, calls } = shardOf(rows);
  const ctx = ctxOf(tableOf());
  ctx.data = { ...ctx.data, shards: [shard] };
  const m = matchRoute(new URL(`${ORIGIN}/search${qs}`), ORIGIN, REGIONS);
  if (m.type !== "page") throw new Error("not a page");
  const v = await loadView(ctx, m);
  if (v.type !== "view") throw new Error("redirect");
  return { v, calls, html: staticMarkup(createElement(Views, { body: v.body })) };
};

describe("/search", () => {
  it("before a search shows the form with its filters and reads nothing", async () => {
    const { v, calls, html } = await search("");
    expect(calls).toHaveLength(0);
    expect(v.type === "view" && v.chrome.robots).toBe("noindex, follow");
    expect(html).toContain('role="search"');
    for (const name of ["category", "region", "type", "from", "to"]) expect(html).toContain(`name="${name}"`);
    expect(html).not.toContain("state-message");
  });

  it("lists the hits as stored, with the filters passed to the index and kept in the form", async () => {
    const { calls, html } = await search("?q=inflation&region=north-america&type=macro&from=2025-01-01");
    expect(calls).toHaveLength(1);
    expect(calls[0]?.binds).toEqual(expect.arrayContaining(["north-america", "macro", "2025-01-01"]));
    expect(html).toContain("US consumer prices rise 2.9% in the year to December");
    expect(html).toContain('href="/2025/01/us-cpi-december-2024"');
    expect(html).toContain("1 page");
    expect(html).toMatch(/<option value="north-america" selected/);
    expect(html).toContain('value="inflation"');
  });

  it("an empty result is the empty state, with a way on", async () => {
    const { html } = await search("?q=zzzz", []);
    expect(html).toContain("No pages match &quot;zzzz&quot;");
    expect(html).toContain('href="/latest"');
  });

  it("renders only what the cache key keeps: different raw text with the same key gives identical HTML", async () => {
    const key = (qs: string) => {
      const m = matchRoute(new URL(`${ORIGIN}/search${qs}`), ORIGIN, REGIONS);
      return m.type === "page" ? m.key : m.type;
    };
    for (const [a, b] of [
      ["?q=!!!&region=india", "?q=???&region=india"],
      ["?q=Inflation++US", "?q=inflation%20us!!"],
    ] as const) {
      expect(key(a)).toBe(key(b));
      const [x, y] = [await search(a, []), await search(b, [])];
      expect(x.html).toBe(y.html);
      expect(x.html).not.toContain("!!");
      expect(x.html).not.toContain("???");
    }
    expect((await search("?q=Inflation++US", [])).html).toContain("No pages match &quot;inflation us&quot;");
  });

  it("filters with an empty query get their own key, not /search", () => {
    const key = (qs: string) => {
      const m = matchRoute(new URL(`${ORIGIN}/search${qs}`), ORIGIN, REGIONS);
      return m.type === "page" ? m.key : m.type;
    };
    expect(key("")).toBe(`${ORIGIN}/search`);
    expect(key("?q=!!!")).toBe(`${ORIGIN}/search`);
    expect(key("?region=india")).not.toBe(key(""));
    expect(key("?region=india")).not.toBe(key("?region=europe"));
    expect(key("?q=%20&type=macro")).not.toBe(key(""));
  });

  it("a query of more than 8 words says so and reads nothing", async () => {
    const { calls, html } = await search("?q=one+two+three+four+five+six+seven+eight+nine");
    expect(calls).toHaveLength(0);
    expect(html).toContain("at most 8 words");
  });

  it("D1 failing is the daily-limit message and a Retry-After until 00:00 UTC", async () => {
    const { v, html } = await search("?q=inflation", new Error("D1 daily limit"));
    expect(v.type === "view" && v.body.type === "search" && v.body.unavailable?.retryAfter).toBe(
      // The fixture clock is 2025-01-15T16:00Z: eight hours to midnight.
      8 * 3600,
    );
    expect(html).toContain("00:00 UTC");
    expect(html).toContain('href="/latest"');
    expect(html).not.toContain("US consumer prices");
  });
});
