// axe-core on a rendered page bundle, for `news check` (Proof, C13). Usage: bun scripts/axe.ts <bundle dir>
//
// Opens <dir>/index.html in Chromium with the same WCAG 2.2 A and AA rules as tests/e2e/fixtures.ts and prints
// one JSON line: {"violations":[{"id","impact","help","nodes"}]}. The Rust side blocks on serious and critical.
// Site assets the page links (/_astro/…, fonts) are served from dist/client when the site is built, so colour
// contrast is measured with the real styles; the bundle's own files (charts/…) are served from the bundle.
import { existsSync, readFileSync } from "node:fs";
import { extname, join, normalize, resolve } from "node:path";
import AxeBuilder from "@axe-core/playwright";
import { chromium } from "@playwright/test";
// Same tags as WCAG_TAGS in tests/e2e/fixtures.ts (not imported: that file pulls in the test runner).
const WCAG_TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];

const dir = process.argv[2] ? resolve(process.argv[2]) : "";
if (!dir || !existsSync(join(dir, "index.html"))) {
  console.error("usage: bun scripts/axe.ts <bundle dir with index.html>");
  process.exit(2);
}
const assets = resolve(import.meta.dir, "..", "dist", "client");
const TYPES: Record<string, string> = {
  ".html": "text/html",
  ".css": "text/css",
  ".js": "text/javascript",
  ".json": "application/json",
  ".csv": "text/csv",
  ".svg": "image/svg+xml",
  ".woff2": "font/woff2",
};

const origin = "http://news.test";
const browser = await chromium.launch();
try {
  const page = await (await browser.newContext()).newPage();
  // Everything on the page's own origin comes from the bundle, then the built site; the network is not touched.
  await page.route("**/*", async (route) => {
    const url = new URL(route.request().url());
    if (url.origin !== origin) return route.abort();
    const rel = normalize(decodeURIComponent(url.pathname === "/" ? "/index.html" : url.pathname)).replace(/^[/\\]+/, "");
    for (const root of [dir, assets]) {
      const file = join(root, rel);
      if (!file.startsWith(root) || !existsSync(file)) continue;
      return route.fulfill({ body: readFileSync(file), contentType: TYPES[extname(file)] ?? "application/octet-stream" });
    }
    return route.fulfill({ status: 404, body: "" });
  });
  await page.goto(`${origin}/`);
  await page.evaluate(() => document.fonts.ready.then(() => undefined));
  const { violations } = await new AxeBuilder({ page }).withTags(WCAG_TAGS).analyze();
  console.log(
    JSON.stringify({
      violations: violations.map((v) => ({ id: v.id, impact: v.impact ?? null, help: v.help, nodes: v.nodes.length })),
    }),
  );
} finally {
  await browser.close();
}
