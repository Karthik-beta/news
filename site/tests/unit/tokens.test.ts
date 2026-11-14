import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const root = join(__dirname, "..", "..", "..");
const tokens = JSON.parse(readFileSync(join(root, "design/tokens/tokens.json"), "utf8"));
const css = readFileSync(join(root, "site/src/styles/tokens.css"), "utf8");

describe("design tokens in the site", () => {
  it("match tokens.json for the brand colour in both themes", () => {
    expect(css).toContain(`--color-brand: ${tokens.color.brand.$value};`);
    expect(css).toContain(`--color-brand: ${tokens.color.brand.$extensions["news.dark"]};`);
  });

  it("define every categorical chart slot", () => {
    for (const slot of ["1", "2", "3", "4", "5", "6", "7", "8"]) {
      expect(css).toContain(`--chart-cat-${slot}:`);
    }
  });
});
