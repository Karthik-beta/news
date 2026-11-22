// Helpers for the integration project: read the built site in dist/.
import { existsSync, readdirSync, statSync } from "node:fs";
import { join, relative, sep } from "node:path";
import { matchRoute } from "../../src/server/routes";
import { REGIONS } from "./site-data";
import { siteRoot } from "./tokens";

// The Cloudflare adapter writes the static assets to dist/client and the Worker to dist/server. The home page is
// server-rendered, so it is no file here; its tests run against the Worker's code (tests/unit/site-*.test.ts).
export const distRoot = join(siteRoot, "dist", "client");

/** Every file under dist/client/, as absolute paths. Throws a clear error if the site has not been built. */
export function distFiles(): string[] {
  if (!existsSync(join(distRoot, "component-index.json"))) {
    throw new Error(
      "dist/client/component-index.json not found: run `bun run build` first (`bun run test:integration` does this).",
    );
  }
  const out: string[] = [];
  const walk = (dir: string) => {
    for (const name of readdirSync(dir)) {
      const path = join(dir, name);
      if (statSync(path).isDirectory()) walk(path);
      else out.push(path);
    }
  };
  walk(distRoot);
  return out.sort();
}

export const htmlFiles = () => distFiles().filter((f) => f.endsWith(".html"));
export const cssFiles = () => distFiles().filter((f) => f.endsWith(".css"));

/** Site path (`/a/b.html`) of a file in dist/. */
export const urlPathOf = (file: string) => `/${relative(distRoot, file).split(sep).join("/")}`;

/**
 * Resolves a site-relative URL path to the file the static host would serve, following Astro's
 * `build.format: "directory"` output: `/a` and `/a/` serve `/a/index.html`; `/a.html` serves itself.
 */
export function resolveToDistFile(pathname: string, root = distRoot): string | undefined {
  const clean = decodeURIComponent(pathname).replace(/^\/+/, "");
  const candidates = [clean, join(clean, "index.html"), `${clean.replace(/\/$/, "")}.html`];
  for (const c of candidates) {
    const file = join(root, c);
    if (existsSync(file) && statSync(file).isFile()) return file;
  }
  return undefined;
}

/**
 * Paths the SSR Worker serves (ADR 0010: home, region views, `/latest`, hubs, entity pages), so they are not files in dist/.
 * A trailing slash counts: the Worker answers 301 to the page. `/search` is the search Worker's page, not this one's.
 */
export function isWorkerRoute(pathname: string): boolean {
  const origin = "https://example.test";
  const m = matchRoute(new URL(pathname, origin), origin, REGIONS);
  if (m.type === "redirect") return isWorkerRoute(m.location);
  return m.type === "page" || /^\/search\/?$/.test(pathname);
}
