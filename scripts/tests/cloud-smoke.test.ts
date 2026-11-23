// infra/cloud/smoke.ts against a fake origin: no network.
import { describe, expect, test } from "bun:test";
import { smoke } from "../../infra/cloud/smoke";

const O = "https://news.example.test";
const FEED = `<rss><channel><item><title>t</title><link>${O}/2026/11/a-page/</link></item></channel></rss>`;

function site(overrides: Record<string, () => Response> = {}) {
  const calls: string[] = [];
  const routes: Record<string, () => Response> = {
    "/robots.txt": () => new Response(`Sitemap: ${O}/sitemap.xml\n`),
    "/": () => new Response("<html>home"),
    "/latest": () => new Response("<html>hub"),
    "/search?q=inflation": () => new Response("<html>search"),
    "/sitemap.xml": () => new Response("<sitemapindex>"),
    "/mcp": () => Response.json({ jsonrpc: "2.0", id: 1, result: { tools: [{ name: "search_pages" }] } }),
    "/rss.xml": () => new Response(FEED),
    "/2026/11/a-page/": () => new Response("<html>article"),
    ...overrides,
  };
  const fetchFn = async (url: string) => {
    const path = url.slice(O.length);
    calls.push(path);
    return routes[path]?.() ?? new Response("nope", { status: 404 });
  };
  return { fetchFn, calls };
}
const noWait = async () => {};

describe("production smoke test", () => {
  test("passes when every check answers, and fetches the article named by the feed", async () => {
    const { fetchFn, calls } = site();
    const out: string[] = [];
    expect(await smoke(O, (l) => out.push(l), fetchFn, noWait)).toBe(0);
    expect(out.every((l) => l.startsWith("ok"))).toBe(true);
    expect(out.length).toBe(7);
    expect(calls).toContain("/2026/11/a-page/");
  });

  test("fails on a down page, retrying first", async () => {
    let n = 0;
    const { fetchFn } = site({
      "/latest": () => {
        n++;
        return new Response("err", { status: 503 });
      },
    });
    const out: string[] = [];
    expect(await smoke(O, (l) => out.push(l), fetchFn, noWait)).toBe(1);
    expect(n).toBe(3);
    expect(out.join("\n")).toContain("FAIL hub (latest): /latest answered 503");
  });

  test("fails when MCP lists no tools or the feed has no page", async () => {
    const { fetchFn } = site({
      "/mcp": () => Response.json({ result: { tools: [] } }),
      "/rss.xml": () => new Response("<rss><channel></channel></rss>"),
    });
    const out: string[] = [];
    expect(await smoke(O, (l) => out.push(l), fetchFn, noWait, 1)).toBe(2);
    expect(out.join("\n")).toContain("no article to fetch");
  });

  test("refuses an origin that is not a bare https host", async () => {
    await expect(smoke("http://news.example.test", () => {}, site().fetchFn, noWait)).rejects.toThrow("https");
    await expect(smoke(`${O}/path`, () => {}, site().fetchFn, noWait)).rejects.toThrow("https");
  });
});
