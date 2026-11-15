import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { declarationsFor, readTokens, repoRoot, siteRoot } from "../support/tokens";

// tokens.json is the source of truth; design/tokens/build.mjs generates tokens.css and
// site/scripts/sync-tokens.ts copies it into the site (run by `pretest`, `predev` and `prebuild`).
const tokens = readTokens();
const designCss = readFileSync(join(repoRoot, "design/tokens/tokens.css"), "utf8");
const siteCss = readFileSync(join(siteRoot, "src/styles/tokens.css"), "utf8");
const DARK_MEDIA = "@media (prefers-color-scheme: dark)";

describe("design tokens", () => {
  it("tokens.json has tokens, and dark values for the mode-dependent colours", () => {
    expect(tokens.length).toBeGreaterThan(50);
    for (const name of ["--color-paper", "--color-ink", "--color-brand", "--chart-highlight"]) {
      expect(tokens.find((t) => t.name === name)?.dark, name).toBeDefined();
    }
  });

  it("the site copy is the generated file with only the provenance line changed", () => {
    const strip = (css: string) => css.split("\n").slice(1).join("\n");
    expect(strip(siteCss)).toBe(strip(designCss));
    expect(siteCss.split("\n")[0]).toContain("COPIED from design/tokens/tokens.css");
  });

  it("declares every token at :root with its light value", () => {
    const root = declarationsFor(siteCss, ":root");
    for (const t of tokens) expect(root.get(t.name), t.name).toBe(t.light);
  });

  it('declares every dark value under :root[data-theme="dark"]', () => {
    const explicit = declarationsFor(siteCss, ':root[data-theme="dark"]');
    for (const t of tokens.filter((t) => t.dark)) expect(explicit.get(t.name), t.name).toBe(t.dark);
  });

  it("declares every dark value for the system setting, unless the reader chose light", () => {
    const system = declarationsFor(siteCss, ':root:not([data-theme="light"])', DARK_MEDIA);
    for (const t of tokens.filter((t) => t.dark)) expect(system.get(t.name), t.name).toBe(t.dark);
  });

  it("does not let theme-independent tokens change in dark mode", () => {
    const explicit = declarationsFor(siteCss, ':root[data-theme="dark"]');
    for (const t of tokens.filter((t) => !t.dark)) expect(explicit.has(t.name), t.name).toBe(false);
  });

  it("defines every categorical chart slot, in order, with no gaps", () => {
    const slots = tokens.filter((t) => /^--chart-cat-\d+$/.test(t.name)).map((t) => Number(t.name.split("-").pop()));
    expect(slots).toEqual(Array.from({ length: slots.length }, (_, i) => i + 1));
    expect(slots.length).toBeGreaterThanOrEqual(8);
  });

  it("turns motion off under prefers-reduced-motion", () => {
    const reduced = declarationsFor(siteCss, ":root", "@media (prefers-reduced-motion: reduce)");
    expect(reduced.get("--motion-fast")).toBe("0ms");
    expect(reduced.get("--motion-base")).toBe("0ms");
  });
});
