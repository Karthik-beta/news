import { describe, expect, it } from "vitest";
import { lintSource, lintTree } from "../../scripts/lint-tokens";

// The token-only lint (scripts/lint-tokens.ts). Planted violations must fail it; the real source tree must pass.
describe("token lint", () => {
  it("passes on every file in site/src", () => {
    expect(lintTree().map((f) => `${f.file}:${f.line} ${f.rule} ${f.text}`)).toEqual([]);
  });

  it("fails on a planted hex colour, in CSS and in TSX", () => {
    expect(lintSource(".x { color: #C8213A; }", "x.css").map((f) => f.rule)).toEqual(["hex-colour"]);
    expect(lintSource('const a = <path fill="#fff" />;', "x.tsx").map((f) => f.rule)).toEqual(["hex-colour"]);
    expect(lintSource("const style = { color: '#18181880' };", "x.ts").map((f) => f.rule)).toEqual(["hex-colour"]);
    expect(lintSource(".x { background: rgb(200 33 58); }", "x.css").map((f) => f.rule)).toEqual(["colour-function"]);
  });

  it("fails on a planted font name", () => {
    const css = lintSource('.x { font-family: "Fira Sans", Arial, sans-serif; }', "x.css").map((f) => f.rule);
    expect(css).toContain("font-family");
    expect(css).toContain("font-name");
    expect(lintSource("const s = { fontFamily: 'Georgia' };", "x.tsx").map((f) => f.rule)).toEqual([
      "font-name",
      "font-family",
    ]);
    expect(lintSource(".x { font: 600 15px/1.3 monospace; }", "x.css").map((f) => f.rule)).toContain("font-name");
  });

  it("fails on a planted px font size", () => {
    expect(lintSource(".x { font-size: 13.5px; }", "x.css").map((f) => f.rule)).toEqual(["px-font-size"]);
    expect(lintSource(".x { font: 600 15px/1.3 var(--font-family-sans); }", "x.css").map((f) => f.rule)).toEqual([
      "px-font-size",
    ]);
    expect(lintSource("const s = { fontSize: 14 };", "x.tsx").map((f) => f.rule)).toEqual(["px-font-size"]);
    expect(lintSource('<text font-size="12">1</text>', "x.astro").map((f) => f.rule)).toEqual(["px-font-size"]);
  });

  it("allows tokens, fragments, entities and comments", () => {
    const ok = [
      [
        ".x { color: var(--color-ink); font-family: var(--font-family-sans); font-size: var(--font-size-ui); }",
        "x.css",
      ],
      ['<a href="#source-src_aapl">Source</a>', "x.tsx"],
      ["<p>Apple&#x27;s</p>", "x.html.ts"],
      ["/* not #fff and not 12px font-size: 12px */ .x { margin: 12px; }", "x.css"],
      ["// font-size: 12px and #abc in a comment\nconst url = 'https://example.com/#top';", "x.ts"],
      [".x { font: inherit; max-width: 320px; }", "x.css"],
    ] as const;
    for (const [text, file] of ok) expect(lintSource(text, file), text).toEqual([]);
  });

  it("reports the line of each finding", () => {
    expect(lintSource(".a {}\n.b {\n  color: #123456;\n}", "x.css")[0]?.line).toBe(3);
  });
});
