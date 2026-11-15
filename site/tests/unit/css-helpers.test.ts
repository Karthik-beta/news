import { describe, expect, it } from "vitest";
import { declarationsFor, normaliseValue } from "../support/tokens";

// The token tests use these helpers as their oracle, so they are tested against readable and minified CSS.
const readable = `
/* comment { not a rule } */
:root {
  color-scheme: light;
  --color-paper: #F4F4F1;
  --motion-fast: 120ms;
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) { --color-paper: #17181A; }
}
:root[data-theme="dark"] { --color-paper: #17181A; }
@media (prefers-reduced-motion: reduce) {
  :root { --motion-fast: 0ms; }
}`;
const minified =
  ":root{color-scheme:light;--color-paper:#f4f4f1;--motion-fast:.12s}@media (prefers-color-scheme:dark){:root:not([data-theme=light]){--color-paper:#17181a}}:root[data-theme=dark]{--color-paper:#17181a}@media (prefers-reduced-motion:reduce){:root{--motion-fast:0s}}";

describe("declarationsFor", () => {
  it.each([
    ["readable", readable],
    ["minified", minified],
  ])("separates top-level rules from rules inside @media (%s)", (_kind, css) => {
    expect(normaliseValue(declarationsFor(css, ":root").get("--motion-fast") ?? "")).toBe("120ms");
    expect(
      normaliseValue(
        declarationsFor(css, ":root", "@media (prefers-reduced-motion: reduce)").get("--motion-fast") ?? "",
      ),
    ).toBe("0");
    expect(normaliseValue(declarationsFor(css, ':root[data-theme="dark"]').get("--color-paper") ?? "")).toBe("#17181a");
    expect(
      declarationsFor(css, ':root:not([data-theme="light"])', "@media (prefers-color-scheme: dark)").has(
        "--color-paper",
      ),
    ).toBe(true);
  });

  it("ignores declarations inside comments", () => {
    expect(declarationsFor(readable, "comment").size).toBe(0);
  });
});

describe("normaliseValue", () => {
  it.each([
    ["#FFF", "#ffffff"],
    ["#C8213A", "#c8213a"],
    ["-0.018em", "-.018em"],
    ["120ms", ".12s"],
    ["0px", "0"],
    ['"Fira Sans", system-ui, sans-serif', "Fira Sans,system-ui,sans-serif"],
  ])("treats %s and %s as the same value", (a, b) => {
    expect(normaliseValue(a)).toBe(normaliseValue(b));
  });

  it("keeps different values different", () => {
    expect(normaliseValue("#065da0")).not.toBe(normaliseValue("#065da1"));
    expect(normaliseValue("120ms")).not.toBe(normaliseValue("200ms"));
    expect(normaliseValue("1rem")).not.toBe(normaliseValue("1em"));
  });
});
