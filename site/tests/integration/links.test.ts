import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { htmlFiles, resolveToDistFile, urlPathOf } from "../support/dist";
import { parseHtml, type Ref, sameSiteRefs } from "../support/links";

// Internal link check of dist/: every same-site URL in a page (links, assets, srcset candidates, SVG <use>)
// must resolve to a built file, and every #fragment must name an element id on the target page.
// External URLs are not fetched (unit and integration tests make no network calls).
// The extraction itself is unit-tested in tests/unit/links.test.ts.

const domCache = new Map<string, Document>();
function load(file: string): Document {
  let doc = domCache.get(file);
  if (!doc) {
    doc = parseHtml(readFileSync(file, "utf8"));
    domCache.set(file, doc);
  }
  return doc;
}

const refsOf = (file: string): Ref[] => sameSiteRefs(load(file), urlPathOf(file).replace(/index\.html$/, ""));

describe("internal links in the built site", () => {
  const files = htmlFiles();
  const pages = files.map((f) => [urlPathOf(f), f]);

  it("finds the page's own assets (sanity check that the crawler sees links)", () => {
    expect(files.flatMap(refsOf).some((r) => r.url.pathname.endsWith(".css"))).toBe(true);
  });

  it.each(pages)("every same-site URL in %s resolves to a built file", (_p, file) => {
    const broken = refsOf(file)
      .filter((r) => !resolveToDistFile(r.url.pathname))
      .map((r) => r.raw);
    expect(broken, "links to files that are not in dist/").toEqual([]);
  });

  it.each(pages)("every #fragment in %s names an element on its target", (_p, file) => {
    const broken = refsOf(file)
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
