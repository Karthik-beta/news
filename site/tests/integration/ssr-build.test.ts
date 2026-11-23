// ADR 0010 "Production guards": a production build of the Worker (no NEWS_PREVIEW, no fixtures) has no preview route and
// none of the preview code, and the machine routes are in it. It builds one into dist/_production-check, so the
// build the other integration tests read is left alone.
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, rmSync, statSync } from "node:fs";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { siteRoot } from "../support/tokens";

const OUT = join(siteRoot, "dist", "_production-check");

const filesUnder = (dir: string): string[] =>
  readdirSync(dir).flatMap((n) => {
    const p = join(dir, n);
    return statSync(p).isDirectory() ? filesUnder(p) : [p];
  });

beforeAll(() => {
  rmSync(OUT, { recursive: true, force: true });
  execFileSync("bun", ["--bun", "astro", "build"], {
    cwd: siteRoot,
    // VITEST is cleared: astro.config.mjs drops the adapter when it is set, and this is a real build.
    env: {
      ...process.env,
      NEWS_OUT_DIR: OUT,
      NEWS_PREVIEW: "",
      NEWS_FIXTURES: "",
      CLOUDFLARE_ENV: "local",
      VITEST: "",
    },
    stdio: "pipe",
    timeout: 300_000,
  });
}, 330_000);

afterAll(() => rmSync(OUT, { recursive: true, force: true }));

describe("a production build", () => {
  it("has no preview route, no preview code and no preview header", () => {
    expect(existsSync(join(OUT, "server", "entry.mjs"))).toBe(true);
    const code = filesUnder(join(OUT, "server")).filter((f) => /\.(m?js|json)$/.test(f));
    expect(code.length).toBeGreaterThan(3);
    for (const file of code) {
      const text = readFileSync(file, "utf8");
      // "__preview" alone is in the reserved slugs of workers/shared, so the route itself is the marker.
      for (const marker of ["/__preview/curation", "x-news-preview", "the preview build is refused"])
        expect(text.includes(marker), `${file} contains ${marker}`).toBe(false);
    }
  });

  it("holds the machine routes, which are not prerendered", () => {
    const text = filesUnder(join(OUT, "server"))
      .filter((f) => f.endsWith(".mjs"))
      .map((f) => readFileSync(f, "utf8"))
      .join("\n");
    for (const marker of ["sitemapindex", "application/atom+xml", "daily limit"]) expect(text).toContain(marker);
    expect(existsSync(join(OUT, "client", "robots.txt"))).toBe(true);
  });
});
