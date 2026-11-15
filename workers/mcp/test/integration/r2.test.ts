// The MCP Worker only reads page bundles from R2. This proves it gets a bucket binding and can read what the
// write API stored: full reads, metadata, ranges, listings and misses. The full R2 suite lives in the api tests.
import { env } from "cloudflare:workers";
import { afterAll, describe, expect, it } from "vitest";

const prefix = `test-${crypto.randomUUID()}/`;
const keys = ["pages/p1/index.md", "pages/p1/data.json", "pages/p2/index.md"].map((k) => prefix + k);

afterAll(async () => {
  await env.PAGES.delete(keys);
});

describe("R2 reads from the mcp Worker", () => {
  it("reads a stored page bundle, its metadata, and a byte range", async () => {
    await env.PAGES.put(keys[0] as string, "# Title\nBody text", { httpMetadata: { contentType: "text/markdown" } });
    const obj = await env.PAGES.get(keys[0] as string);
    expect(await obj?.text()).toBe("# Title\nBody text");
    expect(obj?.httpMetadata?.contentType).toBe("text/markdown");
    const range = await env.PAGES.get(keys[0] as string, { range: { offset: 2, length: 5 } });
    expect(await range?.text()).toBe("Title");
  });

  it("lists one page's files by prefix and nothing for an unknown page", async () => {
    // Seed everything this test lists, so it also passes when run on its own (`-t`).
    await env.PAGES.put(keys[0] as string, "# Title\nBody text");
    await env.PAGES.put(keys[1] as string, "{}");
    await env.PAGES.put(keys[2] as string, "# Two");
    const p1 = await env.PAGES.list({ prefix: `${prefix}pages/p1/` });
    expect(p1.objects.map((o) => o.key)).toEqual([keys[1], keys[0]]);
    expect((await env.PAGES.list({ prefix: `${prefix}pages/zzz/` })).objects).toEqual([]);
  });

  it("returns null for a page that does not exist", async () => {
    expect(await env.PAGES.get(`${prefix}pages/missing/index.md`)).toBeNull();
  });
});
