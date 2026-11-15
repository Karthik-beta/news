import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { resolveToDistFile } from "../support/dist";
import { parseHtml, SITE_ORIGIN, sameSiteRefs } from "../support/links";

// The internal link check (tests/integration/links.test.ts) relies on these two functions; a checker that
// silently finds nothing would pass every build, so they are tested on known input here.

const page = `<!doctype html><html><head>
  <link rel="stylesheet" href="/_astro/index.abc.css">
  <link rel="icon" href="favicon.svg">
  <script type="module" src="./app.js"></script>
</head><body>
  <a href="/economy/">Economy</a>
  <a href="../markets/fed-rates#chart-1">Rates</a>
  <a href="#sources">Sources</a>
  <a href="${SITE_ORIGIN}/about/">About</a>
  <a href="https://www.sec.gov/edgar">EDGAR</a>
  <a href="//example.org/x">Protocol-relative</a>
  <a href="mailto:corrections@example.org">Email</a>
  <a href="tel:+640000">Phone</a>
  <a href="javascript:void(0)">Script</a>
  <img src="data:image/gif;base64,R0lGOD" alt="">
  <img src="/img/a.png" srcset="/img/a-2x.png 2x, /img/a-3x.png 3x" alt="A">
  <picture><source srcset="/img/b.avif 1x"><img src="/img/b.png" alt="B"></picture>
  <svg><use href="/icons.svg#arrow"></use></svg>
  <form action="/search/"></form>
  <a href="">empty</a>
</body></html>`;

describe("sameSiteRefs", () => {
  const refs = sameSiteRefs(parseHtml(page), "/economy/us-cpi/");
  const paths = refs.map((r) => r.url.pathname + r.url.hash);

  it("finds links, assets, srcset candidates, <use> and form actions on the same site", () => {
    expect(paths).toEqual(
      expect.arrayContaining([
        "/_astro/index.abc.css",
        "/economy/us-cpi/favicon.svg",
        "/economy/us-cpi/app.js",
        "/economy/",
        "/economy/markets/fed-rates#chart-1",
        "/economy/us-cpi/#sources",
        "/about/",
        "/img/a.png",
        "/img/a-2x.png",
        "/img/a-3x.png",
        "/img/b.avif",
        "/img/b.png",
        "/icons.svg#arrow",
        "/search/",
      ]),
    );
  });

  it("leaves out other sites and non-fetchable schemes", () => {
    expect(refs.every((r) => r.url.origin === SITE_ORIGIN)).toBe(true);
    expect(refs.map((r) => r.raw).join(" ")).not.toMatch(/sec\.gov|example\.org|mailto:|tel:|javascript:|data:/);
  });

  it("ignores empty attributes", () => {
    expect(refs.some((r) => r.raw === "")).toBe(false);
  });

  it("honours <base href>", () => {
    const doc = parseHtml('<html><head><base href="/base/"></head><body><a href="x/">x</a></body></html>');
    expect(sameSiteRefs(doc, "/a/b/").map((r) => r.url.pathname)).toEqual(["/base/x/"]);
  });
});

describe("resolveToDistFile", () => {
  let root: string;
  beforeAll(() => {
    root = mkdtempSync(join(tmpdir(), "news-dist-"));
    mkdirSync(join(root, "economy", "us-cpi"), { recursive: true });
    writeFileSync(join(root, "index.html"), "");
    writeFileSync(join(root, "economy", "us-cpi", "index.html"), "");
    writeFileSync(join(root, "404.html"), "");
    writeFileSync(join(root, "a b.css"), "");
  });
  afterAll(() => rmSync(root, { recursive: true, force: true }));

  it.each([
    ["/", "index.html"],
    ["/economy/us-cpi/", join("economy", "us-cpi", "index.html")],
    ["/economy/us-cpi", join("economy", "us-cpi", "index.html")],
    ["/404", "404.html"],
    ["/404.html", "404.html"],
    ["/a%20b.css", "a b.css"],
  ])("serves %s from %s, as a static host does", (path, file) => {
    expect(resolveToDistFile(path, root)).toBe(join(root, file));
  });

  it.each(["/economy/", "/missing.css", "/economy/us-cpi/chart.png"])("finds nothing for %s", (path) => {
    expect(resolveToDistFile(path, root)).toBeUndefined();
  });
});
