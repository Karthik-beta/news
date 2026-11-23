// Feeds, sitemaps and llms.txt: structural checks of the XML a reader or crawler would parse (well-formed, the required
// elements, unique IDs, absolute links, date formats), over the fixture data.
import { JSDOM } from "jsdom";
import { describe, expect, it } from "vitest";
import { ctxOf, FIXTURE_ITEMS, ORIGIN, REGIONS, tableOf } from "../../tests/support/site-data";
import { esc, renderMachine, sitemapIndexXml, urlsetXml } from "./machine";
import { matchRoute } from "./routes";

const RFC822 = /^[A-Z][a-z]{2}, \d{2} [A-Z][a-z]{2} \d{4} \d{2}:\d{2}:\d{2} GMT$/;
const RFC3339 = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?Z$/;

const run = async (path: string, set = "normal") => {
  const m = matchRoute(new URL(`${ORIGIN}${path}`), ORIGIN, REGIONS);
  if (m.type !== "page") throw new Error(`${path} is not a page route`);
  const res = await renderMachine({ ...ctxOf(tableOf(FIXTURE_ITEMS[set])), origin: ORIGIN }, m);
  return { res, text: await res.text() };
};

/** Parses as XML (a parse error makes JSDOM throw), so a feed that is not well-formed fails here. */
const parse = (text: string) => new JSDOM(text, { contentType: "text/xml" }).window.document;
const textOf = (el: Element, tag: string) => el.getElementsByTagName(tag)[0]?.textContent ?? "";

describe("RSS 2.0", () => {
  it("is well-formed with the required channel and item elements, unique absolute guids and RFC 822 dates", async () => {
    const { res, text } = await run("/rss.xml");
    expect(res.headers.get("content-type")).toContain("application/rss+xml");
    const doc = parse(text);
    expect(doc.documentElement.getAttribute("version")).toBe("2.0");
    const channel = doc.getElementsByTagName("channel")[0] as Element;
    for (const tag of ["title", "link", "description"]) expect(textOf(channel, tag), tag).not.toBe("");
    expect(RFC822.test(textOf(channel, "lastBuildDate"))).toBe(true);
    const items = [...doc.getElementsByTagName("item")];
    expect(items).toHaveLength(20);
    const guids = items.map((i) => textOf(i, "guid"));
    expect(new Set(guids).size).toBe(items.length);
    for (const item of items) {
      expect(textOf(item, "title")).not.toBe("");
      expect(textOf(item, "link")).toMatch(new RegExp(`^${ORIGIN}/\\d{4}/\\d{2}/`));
      expect(textOf(item, "guid")).toBe(textOf(item, "link"));
      expect(textOf(item, "pubDate")).toMatch(RFC822);
    }
  });

  it("lists the newest page first, whatever its index status", async () => {
    const items = (FIXTURE_ITEMS.normal ?? []).map((i) =>
      i.PK === "HUB#latest" && String(i.SK).includes("us-cpi-december-2024") ? { ...i, index_status: "noindex" } : i,
    );
    const m = matchRoute(new URL(`${ORIGIN}/rss.xml`), ORIGIN, REGIONS);
    if (m.type !== "page") throw new Error("not a page");
    const doc = parse(await (await renderMachine({ ...ctxOf(tableOf(items)), origin: ORIGIN }, m)).text());
    expect(textOf(doc.getElementsByTagName("item")[0] as Element, "link")).toBe(
      `${ORIGIN}/2025/01/us-cpi-december-2024`,
    );
  });
});

describe("Atom 1.0", () => {
  it("is well-formed with id, title and updated, and entries with unique absolute ids and RFC 3339 dates", async () => {
    const { res, text } = await run("/atom.xml");
    expect(res.headers.get("content-type")).toContain("application/atom+xml");
    const doc = parse(text);
    expect(doc.documentElement.namespaceURI).toBe("http://www.w3.org/2005/Atom");
    const feed = doc.documentElement;
    const own = (tag: string) => [...feed.children].find((c) => c.localName === tag)?.textContent ?? "";
    expect(own("id")).toBe(`${ORIGIN}/atom.xml`);
    expect(own("title")).not.toBe("");
    expect(own("updated")).toMatch(RFC3339);
    const entries = [...doc.getElementsByTagName("entry")];
    expect(entries).toHaveLength(20);
    expect(new Set(entries.map((e) => textOf(e, "id"))).size).toBe(entries.length);
    for (const e of entries) {
      expect(textOf(e, "id")).toMatch(new RegExp(`^${ORIGIN}/`));
      expect(textOf(e, "title")).not.toBe("");
      expect(textOf(e, "published")).toMatch(RFC3339);
      expect(textOf(e, "updated")).toMatch(RFC3339);
      expect(e.getElementsByTagName("link")[0]?.getAttribute("href")).toBe(textOf(e, "id"));
    }
  });
});

describe("category feeds", () => {
  it("hold only that category's pages, and name it", async () => {
    const { text } = await run("/economy/inflation/rss.xml");
    const doc = parse(text);
    expect(textOf(doc.getElementsByTagName("channel")[0] as Element, "title")).toBe("News: Inflation");
    const links = [...doc.getElementsByTagName("item")].map((i) => textOf(i, "link"));
    expect(links.length).toBeGreaterThan(0);
    expect(links).toContain(`${ORIGIN}/2025/01/us-cpi-december-2024`);
    expect(links).not.toContain(`${ORIGIN}/2025/01/apple-fy2024-results`);
  });

  it("an unknown category is a plain 404", async () => {
    const { res } = await run("/nothing-here/atom.xml");
    expect(res.status).toBe(404);
  });
});

describe("sitemaps and llms.txt", () => {
  it("the index names a child per month, the child a url and lastmod per page, and both escape what they print", () => {
    const index = parse(sitemapIndexXml(ORIGIN, ["2025-01", "2025-02"]));
    expect([...index.getElementsByTagName("loc")].map((l) => l.textContent)).toEqual([
      `${ORIGIN}/sitemaps/2025-01.xml`,
      `${ORIGIN}/sitemaps/2025-02.xml`,
    ]);
    const set = parse(urlsetXml(ORIGIN, [{ path: "/2025/01/a&b", updated_at: "2025-01-14T10:00:00.000Z" }]));
    expect(set.getElementsByTagName("loc")[0]?.textContent).toBe(`${ORIGIN}/2025/01/a&b`);
    expect(set.getElementsByTagName("lastmod")[0]?.textContent).toMatch(RFC3339);
  });

  it("with no shard bound the index is empty, not an error", async () => {
    const { res, text } = await run("/sitemap.xml");
    expect(res.status).toBe(200);
    expect(parse(text).getElementsByTagName("sitemap")).toHaveLength(0);
  });

  it("llms.txt names the sections from the taxonomy and the machine routes", async () => {
    const { res, text } = await run("/llms.txt");
    expect(res.headers.get("content-type")).toContain("text/plain");
    for (const line of [
      `- [Economy](${ORIGIN}/economy)`,
      `${ORIGIN}/atom.xml`,
      `${ORIGIN}/sitemap.xml`,
      `${ORIGIN}/mcp`,
      `${ORIGIN}/data/`,
    ])
      expect(text).toContain(line);
  });

  it("esc drops control characters XML forbids and escapes the five that matter", () => {
    expect(esc('a<b>&"c\u0001d')).toBe("a&lt;b&gt;&amp;&quot;cd");
  });
});
