import { readFileSync } from "node:fs";
import { JSDOM } from "jsdom";
import { describe, expect, it } from "vitest";
import { htmlFiles, resolveToDistFile, urlPathOf } from "../support/dist";

// Internal link check of dist/: every same-site URL in a page (links, assets, srcset candidates, SVG <use>)
// must resolve to a built file, and every #fragment must name an element id on the target page.
// External URLs are not fetched (unit and integration tests make no network calls).
const ORIGIN = "https://news.karthik.nz"; // astro.config.mjs `site`

const URL_ATTRIBUTES: [selector: string, attribute: string][] = [
  ["a[href]", "href"],
  ["area[href]", "href"],
  ["link[href]", "href"],
  ["script[src]", "src"],
  ["img[src]", "src"],
  ["iframe[src]", "src"],
  ["source[src]", "src"],
  ["video[src]", "src"],
  ["video[poster]", "poster"],
  ["audio[src]", "src"],
  ["track[src]", "src"],
  ["use[href]", "href"],
  ["form[action]", "action"],
];

interface Ref {
  page: string;
  raw: string;
  url: URL;
}

const domCache = new Map<string, Document>();
function load(file: string): Document {
  let doc = domCache.get(file);
  if (!doc) {
    doc = new JSDOM(readFileSync(file, "utf8")).window.document;
    domCache.set(file, doc);
  }
  return doc;
}

function collectRefs(file: string): Ref[] {
  const doc = load(file);
  const page = urlPathOf(file).replace(/index\.html$/, "");
  const base = new URL(page, ORIGIN);
  const raws: string[] = [];
  for (const [selector, attribute] of URL_ATTRIBUTES) {
    for (const el of doc.querySelectorAll(selector)) raws.push(el.getAttribute(attribute) ?? "");
  }
  for (const el of doc.querySelectorAll("[srcset]")) {
    for (const candidate of (el.getAttribute("srcset") ?? "").split(","))
      raws.push(candidate.trim().split(/\s+/)[0] ?? "");
  }
  return raws
    .filter((raw) => raw !== "" && !/^(mailto|tel|data|javascript):/i.test(raw))
    .map((raw) => ({ page, raw, url: new URL(raw, base) }))
    .filter((ref) => ref.url.origin === ORIGIN);
}

describe("internal links in the built site", () => {
  const files = htmlFiles();
  const refs = files.flatMap(collectRefs);

  it("finds the page's own assets (sanity check that the crawler sees links)", () => {
    expect(refs.some((r) => r.url.pathname.endsWith(".css"))).toBe(true);
  });

  it.each(files.map((f) => [urlPathOf(f), f]))("every same-site URL in %s resolves to a built file", (_p, file) => {
    const broken = collectRefs(file)
      .filter((r) => !resolveToDistFile(r.url.pathname))
      .map((r) => r.raw);
    expect(broken, "links to files that are not in dist/").toEqual([]);
  });

  it.each(files.map((f) => [urlPathOf(f), f]))("every #fragment in %s names an element on its target", (_p, file) => {
    const broken = collectRefs(file)
      .filter((r) => r.url.hash.length > 1)
      .filter((r) => {
        const target = resolveToDistFile(r.url.pathname);
        if (!target?.endsWith(".html")) return false; // reported by the previous test, or not a page
        return !load(target).getElementById(decodeURIComponent(r.url.hash.slice(1)));
      })
      .map((r) => r.raw);
    expect(broken, "fragments with no matching id").toEqual([]);
  });
});
