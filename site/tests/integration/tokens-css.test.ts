import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { cssFiles } from "../support/dist";
import { declarationsFor, normaliseValue, readTokens } from "../support/tokens";

// The CSS that actually ships (bundled and minified by Astro) must carry every token from tokens.json with the
// right value in each theme. The unit test checks the generated source file; this checks the build didn't lose,
// reorder or rewrite anything on the way to dist/.
const tokens = readTokens();
const css = cssFiles()
  .map((f) => readFileSync(f, "utf8"))
  .join("\n");

const normalised = (decls: Map<string, string>) =>
  new Map([...decls].map(([k, v]) => [k, normaliseValue(v)] as [string, string]));

describe("design tokens in the built CSS", () => {
  it("ship every light value at :root", () => {
    const root = normalised(declarationsFor(css, ":root"));
    const wrong = tokens.filter((t) => root.get(t.name) !== normaliseValue(t.light)).map((t) => t.name);
    expect(wrong).toEqual([]);
  });

  it('ship every dark value under :root[data-theme="dark"]', () => {
    const dark = normalised(declarationsFor(css, ':root[data-theme="dark"]'));
    const wrong = tokens.filter((t) => t.dark && dark.get(t.name) !== normaliseValue(t.dark)).map((t) => t.name);
    expect(wrong).toEqual([]);
  });

  it("ship every dark value for the system dark setting", () => {
    const system = normalised(
      declarationsFor(css, ':root:not([data-theme="light"])', "@media (prefers-color-scheme: dark)"),
    );
    const wrong = tokens.filter((t) => t.dark && system.get(t.name) !== normaliseValue(t.dark)).map((t) => t.name);
    expect(wrong).toEqual([]);
  });
});
