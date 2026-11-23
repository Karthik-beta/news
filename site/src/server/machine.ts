// What machines read (ADR 0010, section 4): RSS and Atom feeds, the XML sitemaps and llms.txt. Rendered on request from
// the same data layer as the pages and cached at the edge by serve.ts. Every function that builds text is pure.

import type { HubEntry } from "@news/worker-shared/records";
import { indexedMonths, indexedPages } from "@news/worker-shared/search";
import { absolute } from "@news/worker-shared/urls";
import { ACCURACY_NOTE, LICENCE_LINE, SITE_DESCRIPTION } from "../components/Footer/Footer";
import type { PageMatch } from "./serve";
import type { Ctx } from "./views";

export const FEED_ENTRIES = 20;
const SITE_TITLE = "News";

const CONTENT_TYPE = {
  rss: "application/rss+xml; charset=utf-8",
  atom: "application/atom+xml; charset=utf-8",
  xml: "application/xml; charset=utf-8",
  text: "text/plain; charset=utf-8",
} as const;

/** Escapes text for XML and drops the control characters XML 1.0 forbids. */
export const esc = (s: string) =>
  [...s]
    .filter((c) => (c.codePointAt(0) as number) >= 0x20 || c === "\t" || c === "\n" || c === "\r")
    .join("")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");

const xml = (body: string) => `<?xml version="1.0" encoding="UTF-8"?>\n${body}\n`;
/** RFC 822 date, as RSS wants it. */
const rfc822 = (iso: string) => new Date(iso).toUTCString();

export interface FeedInput {
  origin: string;
  /** The feed's own path: "/rss.xml", "/economy/atom.xml". */
  selfPath: string;
  /** The page the feed is about: "/" or "/economy". */
  homePath: string;
  title: string;
  description: string;
  entries: HubEntry[];
  /** `updated` when there are no entries. */
  now: Date;
}

export function rssXml(f: FeedInput): string {
  const items = f.entries
    .map((e) => {
      const link = esc(absolute(f.origin, e.path));
      return `    <item>
      <title>${esc(e.title)}</title>
      <link>${link}</link>
      <guid isPermaLink="true">${link}</guid>
      <pubDate>${rfc822(e.published_at)}</pubDate>
      <description>${esc(e.summary)}</description>
    </item>`;
    })
    .join("\n");
  const newest =
    f.entries
      .map((e) => e.updated_at)
      .sort()
      .pop() ?? "";
  return xml(`<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom">
  <channel>
    <title>${esc(f.title)}</title>
    <link>${esc(absolute(f.origin, f.homePath))}</link>
    <description>${esc(f.description)}</description>
    <language>en-gb</language>
    <lastBuildDate>${rfc822(newest || f.now.toISOString())}</lastBuildDate>
    <atom:link href="${esc(absolute(f.origin, f.selfPath))}" rel="self" type="application/rss+xml"/>
${items}
  </channel>
</rss>`);
}

export function atomXml(f: FeedInput): string {
  const entries = f.entries
    .map((e) => {
      const link = esc(absolute(f.origin, e.path));
      return `  <entry>
    <id>${link}</id>
    <title>${esc(e.title)}</title>
    <link rel="alternate" href="${link}"/>
    <published>${e.published_at}</published>
    <updated>${e.updated_at}</updated>
    <summary>${esc(e.summary)}</summary>
  </entry>`;
    })
    .join("\n");
  const newest =
    f.entries
      .map((e) => e.updated_at)
      .sort()
      .pop() ?? "";
  return xml(`<feed xmlns="http://www.w3.org/2005/Atom" xml:lang="en-GB">
  <id>${esc(absolute(f.origin, f.selfPath))}</id>
  <title>${esc(f.title)}</title>
  <subtitle>${esc(f.description)}</subtitle>
  <updated>${newest || f.now.toISOString()}</updated>
  <link rel="self" href="${esc(absolute(f.origin, f.selfPath))}"/>
  <link rel="alternate" href="${esc(absolute(f.origin, f.homePath))}"/>
${entries}
</feed>`);
}

export const sitemapIndexXml = (origin: string, months: string[]) =>
  xml(`<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${months.map((m) => `  <sitemap><loc>${esc(absolute(origin, `/sitemaps/${m}.xml`))}</loc></sitemap>`).join("\n")}
</sitemapindex>`);

export const urlsetXml = (origin: string, pages: { path: string; updated_at: string }[]) =>
  xml(`<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${pages.map((p) => `  <url><loc>${esc(absolute(origin, p.path))}</loc><lastmod>${p.updated_at}</lastmod></url>`).join("\n")}
</urlset>`);

export function llmsTxt(origin: string, sections: { name: string; slug: string }[]): string {
  return `# ${SITE_TITLE}

> ${SITE_DESCRIPTION}

${ACCURACY_NOTE} ${LICENCE_LINE}

## Sections

${sections.map((s) => `- [${s.name}](${absolute(origin, `/${s.slug}`)})`).join("\n")}
- [Latest](${absolute(origin, "/latest")})

## For machines

- [How pages are made](${absolute(origin, "/methodology/")})
- [Data access and MCP](${absolute(origin, "/data/")})
- [MCP server](${absolute(origin, "/mcp")}): read-only tools for pages, data and search
- [RSS feed](${absolute(origin, "/rss.xml")}) and [Atom feed](${absolute(origin, "/atom.xml")}); each category has its own at /<category>/rss.xml and /<category>/atom.xml
- [Sitemap](${absolute(origin, "/sitemap.xml")})
- Every page has a Markdown version at <page address>/index.md and its data at <page address>/data.json
`;
}

const reply = (body: string, type: keyof typeof CONTENT_TYPE) =>
  new Response(body, { headers: { "content-type": CONTENT_TYPE[type], "x-content-type-options": "nosniff" } });
const missing = () => new Response("Not found\n", { status: 404, headers: { "content-type": CONTENT_TYPE.text } });

/** The response for a feed, sitemap or llms.txt request. A D1 failure throws: serve() answers with the stored copy. */
export async function renderMachine(ctx: Ctx & { origin: string }, m: PageMatch): Promise<Response> {
  const { origin } = ctx;
  if (m.kind === "feed" && m.feed) {
    const { format, category } = m.feed;
    const tax = await ctx.taxonomy();
    const node = category === undefined ? undefined : tax.find((n) => n.id === category);
    if (category !== undefined && !node) return missing();
    const { items } = await ctx.data.reader.listHub(category === undefined ? "latest" : `cat/${category}`, {
      limit: FEED_ENTRIES,
    });
    const input: FeedInput = {
      origin,
      selfPath: new URL(m.key).pathname,
      homePath: category === undefined ? "/" : `/${category}`,
      title: node ? `${SITE_TITLE}: ${node.name}` : SITE_TITLE,
      description: SITE_DESCRIPTION,
      entries: items,
      now: ctx.now,
    };
    return format === "rss" ? reply(rssXml(input), "rss") : reply(atomXml(input), "atom");
  }
  if (m.kind === "sitemap") {
    return m.month
      ? reply(urlsetXml(origin, await indexedPages(ctx.data.shards, m.month)), "xml")
      : reply(sitemapIndexXml(origin, await indexedMonths(ctx.data.shards)), "xml");
  }
  if (m.kind === "llms") {
    const tax = await ctx.taxonomy();
    return reply(
      llmsTxt(
        origin,
        tax.filter((n) => n.parent === null).map((n) => ({ name: n.name, slug: n.id })),
      ),
      "text",
    );
  }
  return missing();
}
