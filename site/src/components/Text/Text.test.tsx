// Tester's tests for Text (news-component-testing: rendering, design-system compliance, server rendering). The type
// scale in the browser (computed sizes per breakpoint, fonts, wrapping) is in tests/e2e/components/Text.spec.ts.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { hydrateAndCollectWarnings } from "../../../tests/support/hydrate";
import { renderComponent } from "../../../tests/support/render";
import { readTokens, siteRoot } from "../../../tests/support/tokens";
import { TEXT_VARIANTS, Text, textClass } from "./Text";

const css = readFileSync(join(siteRoot, "src/components/Text/Text.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
/** Declarations of the rule for `.text-<variant>`, as a map. */
function rule(variant: string): Map<string, string> {
  const m = new RegExp(`\\.text-${variant}\\s*\\{([^}]*)\\}`).exec(css);
  if (!m) return new Map();
  return new Map(
    (m[1] as string)
      .split(";")
      .map((d) => d.split(":").map((s) => s.trim()) as [string, string])
      .filter(([p, v]) => p && v),
  );
}
const tokenNames = new Set(readTokens().map((t) => t.name));
const varName = (v: string) => /^var\((--[\w-]+)\)$/.exec(v)?.[1];

describe("Text: rendering", () => {
  it.each(TEXT_VARIANTS)("variant %s renders a <p> with its class by default", (variant) => {
    const { container } = renderComponent(<Text variant={variant}>Words</Text>);
    const el = container.firstElementChild as HTMLElement;
    expect(el.tagName).toBe("P");
    expect(el.className).toBe(`text-${variant}`);
    expect(textClass(variant)).toBe(`text-${variant}`);
  });

  it("renders the element asked for, with id, extra classes and tabular figures", () => {
    renderComponent(
      <Text as="h1" variant="headline-xl" id="h" className="extra" tabular>
        Headline
      </Text>,
    );
    const h = screen.getByRole("heading", { level: 1, name: "Headline" });
    expect(h).toHaveAttribute("id", "h");
    expect(h.className).toBe("text-headline-xl text-tabular extra");
  });

  it("leaves out empty class names and the id when not given", () => {
    const { container } = renderComponent(<Text variant="body">x</Text>);
    expect(container.innerHTML).toBe('<p class="text-body">x</p>');
  });

  it("server-renders and hydrates with no warnings", async () => {
    const { warnings } = await hydrateAndCollectWarnings(
      <div>
        {TEXT_VARIANTS.map((v) => (
          <Text key={v} variant={v}>
            {v}
          </Text>
        ))}
      </div>,
    );
    expect(warnings).toEqual([]);
  });
});

describe("Text: design-system compliance (design/typography.md)", () => {
  it.each(TEXT_VARIANTS)("%s has a rule in Text.css", (variant) => {
    expect(rule(variant).size).toBeGreaterThan(0);
  });

  it.each(TEXT_VARIANTS)("%s uses one of the two typeface tokens and a size token", (variant) => {
    const r = rule(variant);
    expect(["var(--font-family-serif)", "var(--font-family-sans)"]).toContain(r.get("font-family"));
    const size = varName(r.get("font-size") ?? "");
    expect(size, "font-size is a token").toBeDefined();
    expect(tokenNames.has(size as string) || /-responsive$|^--font-fluid-/.test(size as string), `${size} exists`).toBe(
      true,
    );
  });

  it.each(TEXT_VARIANTS)("%s uses only defined tokens", (variant) => {
    for (const [prop, value] of rule(variant)) {
      for (const [, name] of value.matchAll(/var\((--[\w-]+)\)/g)) {
        const generated = /-responsive$|^--font-fluid-/.test(name as string);
        expect(tokenNames.has(name as string) || generated, `${prop}: ${name}`).toBe(true);
      }
    }
  });

  it("the serif (Literata) carries the wordmark, headlines and reading text; the sans the rest", () => {
    const serif = TEXT_VARIANTS.filter((v) => rule(v).get("font-family") === "var(--font-family-serif)");
    expect(serif).toEqual([
      "wordmark",
      "display",
      "headline-xl",
      "headline-l",
      "headline-m",
      "headline-s",
      "standfirst",
      "lead-summary",
      "body",
      "body-s",
    ]);
  });

  it("crimson (the brand colour) is used only by the wordmark and the fly title", () => {
    const branded = TEXT_VARIANTS.filter((v) => (rule(v).get("color") ?? "").includes("--color-brand"));
    expect(branded).toEqual(["wordmark", "fly"]);
  });

  it("line heights without a token are exactly the open design question (1.15, 1.3, 1.35, 1.4, and 1 for the wordmark and small figure)", () => {
    const literal = TEXT_VARIANTS.map((v) => [v, rule(v).get("line-height")] as const).filter(
      ([, lh]) => lh && !lh.startsWith("var("),
    );
    expect(Object.fromEntries(literal)).toEqual({
      wordmark: "1",
      "chart-title": "1.3",
      fly: "1.3",
      "chart-subtitle": "1.4",
      figure: "1.15",
      "figure-s": "1",
      label: "1.35",
      source: "1.4",
    });
  });

  it("no all-caps labels and no third face anywhere in the type styles", () => {
    expect(css).not.toMatch(/text-transform:\s*uppercase/);
    expect(css).not.toMatch(/monospace/);
  });
});
