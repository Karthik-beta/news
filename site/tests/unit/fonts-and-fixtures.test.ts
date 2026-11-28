import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import type { AstroIntegration } from "astro";
import { describe, expect, it } from "vitest";
import config from "../../astro.config.mjs";
import { fontFiles } from "../../scripts/sync-fonts";
import { fixturesEnabled, rendererFixturePaths } from "../../src/renderer/fixtures/routes";
import { repoRoot, siteRoot, tokenValue } from "../support/tokens";

const fontsCss = readFileSync(join(siteRoot, "src/styles/fonts.css"), "utf8");

describe("self-hosted fonts", () => {
  it("fonts.css declares exactly the files sync-fonts copies, all from the site's own origin", () => {
    const urls = [...fontsCss.matchAll(/url\("([^"]+)"\)/g)].map((m) => m[1]);
    expect(urls.sort()).toEqual(
      fontFiles()
        .map(([, f]) => `/fonts/${f}`)
        .sort(),
    );
    for (const url of urls) expect(url).toMatch(/^\/fonts\/[a-z0-9-]+\.woff2$/);
    expect(fontsCss).not.toMatch(/https?:\/\//);
  });

  it("every file exists in the pinned Fontsource packages", () => {
    for (const [pkg, file] of fontFiles()) {
      expect(existsSync(join(repoRoot, "node_modules", pkg, "files", file)), file).toBe(true);
    }
  });

  it("ships Literata roman and italic (variable) and Fira Sans 400-700 roman plus 400 italic", () => {
    const faces = [...fontsCss.matchAll(/@font-face\s*{([^}]*)}/g)].map((m) => m[1] as string);
    const get = (face: string, prop: string) => new RegExp(`${prop}:\\s*([^;]+);`).exec(face)?.[1]?.trim();
    const set = new Set(faces.map((f) => `${get(f, "font-family")}|${get(f, "font-style")}|${get(f, "font-weight")}`));
    expect([...set].sort()).toEqual(
      [
        "Literata|normal|400 800",
        "Literata|italic|400 800",
        '"Fira Sans"|normal|400',
        '"Fira Sans"|normal|500',
        '"Fira Sans"|normal|600',
        '"Fira Sans"|normal|700',
        '"Fira Sans"|italic|400',
      ].sort(),
    );
    for (const f of faces) expect(get(f, "font-display")).toBe("swap");
  });

  it("names the same families as the font tokens", () => {
    expect(tokenValue("--font-family-serif").startsWith("Literata,")).toBe(true);
    expect(tokenValue("--font-family-sans").startsWith('"Fira Sans",')).toBe(true);
  });

  it("covers the currency signs and minus with the Latin and Latin Extended subsets", () => {
    const ranges = [...fontsCss.matchAll(/unicode-range:\s*([^;]+);/g)].map((m) => m[1] as string);
    const covers = (cp: number) =>
      ranges.some((r) =>
        r.split(",").some((part) => {
          const [a, b] = part.trim().replace("U+", "").split("-");
          const lo = Number.parseInt(a as string, 16);
          const hi = b ? Number.parseInt(b, 16) : lo;
          return cp >= lo && cp <= hi;
        }),
      );
    for (const ch of ["$", "€", "£", "¥", "₹", "₩", "−", "–", "µ", "σ", "Δ"]) {
      expect(covers(ch.codePointAt(0) as number), ch).toBe(true);
    }
  });
});

describe("fixture pages", () => {
  /** The routes the news-fixture-pages integration (astro.config.mjs) adds under the given environment. */
  const injected = (env: Record<string, string>) => {
    const saved = { ...process.env };
    Object.assign(process.env, { NEWS_FIXTURES: "", OPENUI_SPIKE: "", ...env });
    const patterns: string[] = [];
    try {
      const integration = config.integrations?.find(
        (i): i is AstroIntegration =>
          typeof i === "object" && i !== null && "name" in i && i.name === "news-fixture-pages",
      );
      const setup = integration?.hooks["astro:config:setup"] as unknown as (o: object) => void;
      setup({ injectRoute: (r: { pattern: string }) => patterns.push(r.pattern) });
    } finally {
      process.env = saved;
    }
    return patterns;
  };

  it("a production build adds no fixture routes, so no fixture code or styles reach dist/", () => {
    expect(injected({})).toEqual([]);
  });

  it("NEWS_FIXTURES=1 adds the renderer, primitive, component and fixture-page routes; OPENUI_SPIKE=1 the spike's", () => {
    expect(injected({ NEWS_FIXTURES: "1" })).toEqual([
      "/fixtures/renderer/[bundle]",
      "/fixtures/renderer/[bundle]/[...file]",
      "/fixtures/primitives/[name]",
      "/fixtures/components/[name]/[fixture]",
      "/fixtures/components/[name]/[fixture]/[...file]",
      "/fixtures/images/[file]",
      "/fixtures/pages/[page]",
      "/fixtures/pages/[page]/[...file]",
    ]);
    expect(injected({ OPENUI_SPIKE: "1" })).toEqual(["/fixtures/openui-ssr/[approach]"]);
  });

  it("are built only with NEWS_FIXTURES=1", () => {
    expect(fixturesEnabled({})).toBe(false);
    expect(fixturesEnabled({ NEWS_FIXTURES: "0" })).toBe(false);
    expect(fixturesEnabled({ NEWS_FIXTURES: "1" })).toBe(true);
    expect(rendererFixturePaths({})).toEqual([]);
    expect(rendererFixturePaths({ NEWS_FIXTURES: "1" }).map((p) => p.params.bundle)).toEqual([
      "apple-fy2024",
      "edge-cases",
      "static",
      "charts",
    ]);
  });
});
