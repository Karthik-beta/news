import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { gzipSync } from "node:zlib";
import { describe, expect, it } from "vitest";
import { estimateTokens, INDEX_BUDGET } from "../../src/components/contract";
import { cssFiles, distRoot, htmlFiles, urlPathOf } from "../support/dist";
import { parseHtml } from "../support/links";

// Budgets on the built site (needs `NEWS_FIXTURES=1 bun run build`, which `bun run test:integration` does):
// the component index fits ADR 0001's token budget, static pages ship 0 KB of JavaScript, a page with an island
// stays within the JS budget, and nothing is loaded from another origin (fonts included).

/** gzip level 9, the way the ADR measured. */
const gz = (buf: Buffer) => gzipSync(buf, { level: 9 }).length;
const KB = 1024;
/** ADR 0001 measured ~67 KB gzip for the React DOM floor plus a chart island. Leave a little room, no more. */
const ISLAND_PAGE_JS_BUDGET_KB = 80;

/** Every JS file a script loads, following static and dynamic imports between built chunks. */
function scriptGraph(entry: string): string[] {
  const seen = new Set<string>();
  const visit = (urlPath: string) => {
    if (seen.has(urlPath)) return;
    seen.add(urlPath);
    const code = readFileSync(join(distRoot, urlPath), "utf8");
    for (const m of code.matchAll(/(?:from|import)\s*\(?\s*["'](\.{1,2}\/[^"']+\.js)["']/g)) {
      visit(new URL(m[1] as string, `https://x${urlPath}`).pathname);
    }
  };
  visit(entry);
  return [...seen];
}

const pages = htmlFiles().map((file) => {
  const doc = parseHtml(readFileSync(file, "utf8"));
  const scripts = [...doc.querySelectorAll("script[src]")].map((s) => s.getAttribute("src") as string);
  const inline = [...doc.querySelectorAll("script:not([src])")].filter(
    (s) => s.getAttribute("type") !== "application/ld+json",
  );
  return { path: urlPathOf(file), doc, scripts, inline };
});

describe("component index in the build", () => {
  it(`emits component-index.json within ${INDEX_BUDGET.total} tokens, no entry over ${INDEX_BUDGET.perEntry}`, () => {
    const index = JSON.parse(readFileSync(join(distRoot, "component-index.json"), "utf8"));
    const lines: string[] = index.components.map((c: { line: string }) => c.line);
    expect(lines.length).toBeGreaterThan(0);
    expect(estimateTokens(lines.join("\n"))).toBeLessThanOrEqual(INDEX_BUDGET.total);
    for (const line of lines) expect(estimateTokens(line), line).toBeLessThanOrEqual(INDEX_BUDGET.perEntry);
    expect(index.components.map((c: { name: string }) => c.name)).not.toContain("FixtureLineChart");
  });
});

describe("JavaScript budget", () => {
  it("built the fixture pages (NEWS_FIXTURES=1)", () => {
    expect(pages.map((p) => p.path)).toContain("/fixtures/renderer/apple-fy2024/index.html");
  });

  it("ships no inline scripts on any page", () => {
    for (const p of pages) expect(p.inline.length, p.path).toBe(0);
  });

  it.each(pages.filter((p) => !p.doc.querySelector("[data-island]")).map((p) => [p.path, p.scripts] as const))(
    "%s has no islands and ships 0 KB of JavaScript",
    (_path, scripts) => {
      expect(scripts).toEqual([]);
    },
  );

  it.each(pages.filter((p) => p.doc.querySelector("[data-island]")).map((p) => [p.path, p.scripts] as const))(
    `%s ships at most ${ISLAND_PAGE_JS_BUDGET_KB} KB gzip of JavaScript, all from /_astro/`,
    (_path, scripts) => {
      expect(scripts.length).toBeGreaterThan(0);
      const files = new Set(scripts.flatMap((s) => scriptGraph(s)));
      for (const f of files) expect(f).toMatch(/^\/_astro\/[^/]+\.js$/);
      const total = [...files].reduce((sum, f) => sum + gz(readFileSync(join(distRoot, f))), 0);
      expect(total / KB).toBeLessThanOrEqual(ISLAND_PAGE_JS_BUDGET_KB);
    },
  );
});

describe("per-chart JavaScript budget", () => {
  // ADR 0001: the React DOM floor is shared and cached across pages; a chart island itself measured about 1 KB.
  // Each island's own code (its chunk plus anything it loads that the page's entry does not already load) stays
  // within this budget, so adding chart types never grows what one chart costs.
  const ISLAND_BUDGET_KB = 8;
  /** Chunks a script loads before running: static imports only. */
  const staticGraph = (entry: string): Set<string> => {
    const seen = new Set<string>();
    const visit = (urlPath: string) => {
      if (seen.has(urlPath)) return;
      seen.add(urlPath);
      const code = readFileSync(join(distRoot, urlPath), "utf8");
      for (const m of code.matchAll(/(?:from|import)\s*["'](\.{1,2}\/[^"']+\.js)["']/g)) {
        visit(new URL(m[1] as string, `https://x${urlPath}`).pathname);
      }
    };
    visit(entry);
    return seen;
  };
  const islandPages = pages.filter((p) => p.doc.querySelector("[data-island]"));
  const names = [
    ...new Set(
      islandPages.flatMap((p) =>
        [...p.doc.querySelectorAll("[data-island]")].map((e) => e.getAttribute("data-island") as string),
      ),
    ),
  ];

  it.each(names)(`the %s island costs at most ${ISLAND_BUDGET_KB} KB gzip of its own`, (name) => {
    const page = islandPages.find((p) => p.doc.querySelector(`[data-island="${name}"]`));
    const entry = page?.scripts[0] as string;
    const preloaded = staticGraph(entry);
    const chunk = readdirSync(join(distRoot, "_astro")).find(
      (f) => f.startsWith(`${name}.client.`) && f.endsWith(".js"),
    );
    expect(chunk, `${name}: its own chunk in dist/_astro`).toBeDefined();
    const own = scriptGraph(`/_astro/${chunk}`).filter((f) => !preloaded.has(f));
    const total = own.reduce((sum, f) => sum + gz(readFileSync(join(distRoot, f))), 0);
    expect(total / KB, own.join(", ")).toBeLessThanOrEqual(ISLAND_BUDGET_KB);
  });
});

describe("no third-party requests", () => {
  const ORIGIN = "https://news.karthik.nz";
  const fetched = (doc: Document) => [
    ...[...doc.querySelectorAll("link[href]:not([rel~='canonical']):not([rel~='alternate'])")].map((l) =>
      l.getAttribute("href"),
    ),
    ...[...doc.querySelectorAll("[src]")].map((e) => e.getAttribute("src")),
    ...[...doc.querySelectorAll("[srcset]")].flatMap((e) =>
      (e.getAttribute("srcset") ?? "").split(",").map((c) => c.trim().split(/\s+/)[0]),
    ),
  ];

  it.each(pages.map((p) => [p.path, p.doc] as const))("%s loads only same-origin resources", (_path, doc) => {
    for (const url of fetched(doc)) {
      expect(new URL(url as string, ORIGIN).origin, url as string).toBe(ORIGIN);
    }
  });

  it("the stylesheets load fonts and imports only from the site itself", () => {
    const css = cssFiles().map((f) => readFileSync(f, "utf8"));
    const urls = css.flatMap((c) => [...c.matchAll(/url\(\s*["']?([^"')]+)["']?\s*\)/g)].map((m) => m[1] as string));
    expect(urls.some((u) => u.includes("/fonts/"))).toBe(true);
    for (const u of urls.filter((x) => !x.startsWith("data:"))) expect(new URL(u, ORIGIN).origin, u).toBe(ORIGIN);
    for (const c of css) expect(c).not.toMatch(/@import\s+url\(\s*["']?https?:/);
    for (const c of css) expect(c).not.toMatch(/fonts\.(googleapis|gstatic)\.com/);
  });
});
