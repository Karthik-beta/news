import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { isWorkerRoute, resolveToDistFile } from "../support/dist";
import { parseHtml, sameSiteRefs } from "../support/links";

// The reader-facing static pages in the build: prerendered files (ADR 0010), each with one h1, and every link in
// them resolving to a built page, a Worker route or an element id. HTML validity is html-validity.test.ts's.
const PAGES = [
  "methodology",
  "about",
  "glossary",
  "privacy",
  "legal",
  "contact",
  "report",
  "data",
  "corrections",
  "feeds",
];

describe.each(PAGES)("/%s/", (name) => {
  const file = resolveToDistFile(`/${name}/`);
  const doc = file ? parseHtml(readFileSync(file, "utf8")) : undefined;

  it("is a prerendered file with one h1 and no script (the report form is the one island)", () => {
    expect(file, `dist/${name}/index.html`).toBeDefined();
    expect(doc?.querySelectorAll("h1")).toHaveLength(1);
    expect(doc?.querySelectorAll("script")).toHaveLength(name === "report" ? 1 : 0);
  });

  it("links only to built pages, Worker routes and elements that exist", () => {
    const broken = sameSiteRefs(doc as Document, `/${name}/`).filter((r) => {
      const target = resolveToDistFile(r.url.pathname);
      if (!target) return !isWorkerRoute(r.url.pathname);
      if (r.url.hash.length > 1 && target.endsWith(".html")) {
        return !parseHtml(readFileSync(target, "utf8")).getElementById(decodeURIComponent(r.url.hash.slice(1)));
      }
      return false;
    });
    expect(broken.map((r) => r.raw)).toEqual([]);
  });
});
