// Production smoke test, run by the deploy workflow after a deploy:  bun infra/cloud/smoke.ts [origin]
// GET and one POST (the MCP `tools/list`), read-only, no credentials. Checks what the issue's "Done when" lists: an
// article from R2, home, a hub, search, MCP tools/list, robots.txt, a sitemap and a feed. The article is the first
// one in the feed, so nothing is hard-coded. A failed check is tried again (a deploy takes a moment to reach the edge).
type Fetch = (url: string, init?: RequestInit) => Promise<Response>;

const MCP_VERSION = "2026-07-28";

export async function smoke(
  origin: string,
  log: (line: string) => void,
  fetchFn: Fetch = fetch,
  sleep: (ms: number) => Promise<void> = (ms) => new Promise((r) => setTimeout(r, ms)),
  attempts = 3,
): Promise<number> {
  if (!/^https:\/\/[a-z0-9.-]+$/.test(origin)) throw new Error("The origin must be https://<host> with no path.");
  const get = (path: string, init?: RequestInit) => fetchFn(origin + path, { redirect: "manual", ...init });
  const text = async (path: string, mustContain: string[]) => {
    const res = await get(path);
    if (res.status !== 200) throw new Error(`${path} answered ${res.status}`);
    const body = await res.text();
    for (const s of mustContain) if (!body.includes(s)) throw new Error(`${path} does not contain ${s}`);
    return body;
  };
  const checks: [string, () => Promise<void>][] = [
    ["robots.txt", async () => void (await text("/robots.txt", ["Sitemap:"]))],
    ["home", async () => void (await text("/", ["<html"]))],
    ["hub (latest)", async () => void (await text("/latest", ["<html"]))],
    ["search", async () => void (await text("/search?q=inflation", ["<html"]))],
    ["sitemap", async () => void (await text("/sitemap.xml", ["<sitemapindex"]))],
    [
      "MCP tools/list",
      async () => {
        const res = await get("/mcp", {
          method: "POST",
          headers: {
            "content-type": "application/json",
            accept: "application/json, text/event-stream",
            "mcp-protocol-version": MCP_VERSION,
            "mcp-method": "tools/list",
          },
          body: JSON.stringify({
            jsonrpc: "2.0",
            id: 1,
            method: "tools/list",
            params: {
              _meta: {
                "io.modelcontextprotocol/protocolVersion": MCP_VERSION,
                "io.modelcontextprotocol/clientInfo": { name: "news-smoke", version: "1.0.0" },
                "io.modelcontextprotocol/clientCapabilities": {},
              },
            },
          }),
        });
        const json = (await res.json().catch(() => ({}))) as { result?: { tools?: unknown[] } };
        if (res.status !== 200 || !json.result?.tools?.length)
          throw new Error(`/mcp answered ${res.status} without tools`);
      },
    ],
    [
      "feed and an article from R2",
      async () => {
        const feed = await text("/rss.xml", ["<rss"]);
        const link = feed.match(/<item>[\s\S]*?<link>([^<]+)<\/link>/)?.[1]?.trim();
        if (!link) throw new Error("/rss.xml lists no page, so there is no article to fetch");
        if (!link.startsWith(`${origin}/`)) throw new Error("The first feed entry is not on this origin");
        await text(link.slice(origin.length), ["<html"]);
      },
    ],
  ];

  let failed = 0;
  for (const [name, check] of checks) {
    let error: unknown;
    for (let i = 1; i <= attempts; i++) {
      try {
        await check();
        error = undefined;
        break;
      } catch (e) {
        error = e;
        if (i < attempts) await sleep(10_000);
      }
    }
    log(error ? `FAIL ${name}: ${error instanceof Error ? error.message : String(error)}` : `ok   ${name}`);
    if (error) failed++;
  }
  return failed;
}

if (import.meta.main) {
  const failed = await smoke(process.argv[2] ?? process.env.NEWS_SITE_ORIGIN ?? "https://news.karthik.nz", console.log);
  process.exit(failed ? 1 : 0);
}
