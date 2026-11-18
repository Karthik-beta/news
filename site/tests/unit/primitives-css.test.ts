// Design-system compliance of the primitives' stylesheets (news-component-testing, "Design-system compliance";
// design/principles.md, layout.md, color.md). The token lint already rejects literal colours, font names and px
// font sizes; this adds the rules it does not cover: every var() is a real token, spacing is on the scale, no
// shadows, gradients or radius on content, motion within 200 ms, and crimson only where the design puts it.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { readTokens, siteRoot } from "../support/tokens";

const PRIMITIVES = ["FigureFrame", "Icon", "Page", "Popover", "Rule", "SourceRef", "Text", "VisuallyHidden"];
const read = (name: string) =>
  readFileSync(join(siteRoot, `src/components/${name}/${name}.css`), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");

/** Every declaration as [selector, property, value], media queries flattened. */
function declarations(css: string): [string, string, string][] {
  const out: [string, string, string][] = [];
  for (const m of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const selector = (m[1] as string).trim().replace(/\s+/g, " ");
    for (const d of (m[2] as string).split(";")) {
      const i = d.indexOf(":");
      if (i > 0)
        out.push([
          selector,
          d.slice(0, i).trim(),
          d
            .slice(i + 1)
            .trim()
            .replace(/\s*!important$/, ""),
        ]);
    }
  }
  return out;
}

const tokens = new Set(readTokens().map((t) => t.name));
// Aliases tokens.css defines on top of tokens.json (responsive sizes and the gutter).
const tokensCss = readFileSync(join(siteRoot, "src/styles/tokens.css"), "utf8");
const defined = new Set([...tokensCss.matchAll(/(--[\w-]+)\s*:/g)].map((m) => m[1] as string));

/**
 * Literal lengths the design documents give and that have no token, with where each is written. Anything else
 * must be a token. Reviewed by the tester against the design documents.
 */
const ALLOWED_LITERALS: Record<string, string> = {
  "Popover .popover max-width 320px": "components.md: popover and tooltip at most 320px wide",
  "FigureFrame .figure-frame__table > summary text-underline-offset 3px": "typography.md: links offset 3px",
  "SourceRef .source-ref text-underline-offset 4px": "typography.md: dotted cited/glossary underline offset 4px",
  "Icon .icon width 1em": "components.md: icons follow the text (1em)",
  "Icon .icon height 1em": "components.md: icons follow the text (1em)",
  "Icon .icon vertical-align -0.125em": "optical baseline alignment of a 1em icon",
  "VisuallyHidden .visually-hidden:not(.visually-hidden--focusable:focus-within) width 1px": "standard clip pattern",
  "VisuallyHidden .visually-hidden:not(.visually-hidden--focusable:focus-within) height 1px": "standard clip pattern",
  "VisuallyHidden .visually-hidden:not(.visually-hidden--focusable:focus-within) margin -1px": "standard clip pattern",
};

function literalLengths(name: string, decls: [string, string, string][]): string[] {
  return decls
    .filter(([, prop]) => !prop.startsWith("--"))
    .flatMap(([sel, prop, value]) =>
      [...value.replace(/var\([^)]*\)|calc\(100vw[^)]*\)\)?/g, "").matchAll(/(?<![\w.])-?\d*\.?\d+(px|em|rem)\b/g)].map(
        (m) => `${name} ${sel} ${prop} ${m[0]}`,
      ),
    );
}

const SPACING = /^(margin|padding|gap|row-gap|column-gap|inset|top|left|right|bottom)(-|$)/;

describe("primitive stylesheets follow the design system", () => {
  for (const name of PRIMITIVES) {
    const decls = declarations(read(name));

    it(`${name}: every var() is a defined token`, () => {
      const unknown = decls.flatMap(([sel, prop, value]) =>
        [...value.matchAll(/var\((--[\w-]+)/g)]
          .map((m) => m[1] as string)
          .filter((t) => !tokens.has(t) && !defined.has(t))
          .map((t) => `${sel} ${prop}: ${t}`),
      );
      expect(unknown).toEqual([]);
    });

    it(`${name}: literal lengths only where the design gives them`, () => {
      const literals = literalLengths(name, decls);
      expect(literals.filter((k) => !(k in ALLOWED_LITERALS))).toEqual([]);
    });

    it(`${name}: spacing comes from the space scale (or is zero)`, () => {
      const off = decls
        .filter(([sel, prop]) => SPACING.test(prop) && !sel.includes("visually-hidden") && !sel.includes("::before"))
        .filter(
          ([, , value]) => !/^(0|auto|var\(--space-\d+\)|calc\(.*\))$/.test(value) && !/var\(--space-/.test(value),
        )
        .filter(([, prop, value]) => !(prop.startsWith("margin") && value === "0"))
        .map(([sel, prop, value]) => `${sel} ${prop}: ${value}`);
      expect(off).toEqual([]);
      // calc() spacing only adds space tokens together (charts.md 10px = 8 + 2, 6px = 4 + 2).
      for (const [, prop, value] of decls) {
        if (SPACING.test(prop) && value.startsWith("calc(") && !value.includes("100%")) {
          expect(value.replace(/var\(--(space|border)-[\w-]+\)/g, "T"), `${prop}: ${value}`).toMatch(
            /^calc\((-1 \* )?T( [+-] T)*\)$/,
          );
        }
      }
    });

    it(`${name}: no shadows, gradients, rounded corners on content, or motion over 200 ms`, () => {
      for (const [sel, prop, value] of decls) {
        if (prop === "box-shadow") expect(value, sel).toBe("none");
        expect(value, `${sel} ${prop}`).not.toMatch(/gradient\(/);
        if (prop === "border-radius") expect(value, sel).toBe("var(--radius-none)");
        expect(prop, `${sel} animates`).not.toMatch(/^animation/);
        if (prop.startsWith("transition")) {
          for (const [, n, unit] of value.matchAll(/(\d*\.?\d+)(ms|s)\b/g)) {
            expect(Number(n) * (unit === "s" ? 1000 : 1), `${sel} ${prop}`).toBeLessThanOrEqual(200);
          }
        }
      }
    });
  }

  it("the allow-list of literal lengths is exactly what the stylesheets use (none stale)", () => {
    const found = PRIMITIVES.flatMap((name) => literalLengths(name, declarations(read(name))));
    expect(found.sort()).toEqual(Object.keys(ALLOWED_LITERALS).sort());
  });

  it("crimson (--color-brand, --chart-highlight) appears only on the brand rule and tag, the wordmark and the fly title", () => {
    const uses = PRIMITIVES.flatMap((name) =>
      declarations(read(name))
        .filter(([, , value]) => /--color-brand|--chart-highlight/.test(value))
        .map(([sel]) => `${name} ${sel}`),
    );
    expect(uses.sort()).toEqual(
      [
        "Rule .rule--brand, .rule-top--brand",
        "Rule .rule--brand::before, .rule-top--brand::before",
        "Text .text-wordmark",
        "Text .text-fly",
      ].sort(),
    );
  });

  it("the popover and tooltip: paper, a 1px rule border, 12px padding, no shadow, no radius (components.md)", () => {
    const popover = new Map(
      declarations(read("Popover"))
        .filter(([sel]) => sel === ".popover")
        .map(([, p, v]) => [p, v]),
    );
    expect(popover.get("background")).toBe("var(--color-paper)");
    expect(popover.get("border")).toBe("var(--border-hairline) solid var(--color-rule)");
    expect(popover.get("padding")).toBe("var(--space-4)");
    expect(tokensCss).toMatch(/--space-4:\s*12px/);
    expect(popover.get("box-shadow")).toBe("none");
    expect(popover.get("border-radius")).toBe("var(--radius-none)");
  });

  it("FigureFrame spacing matches charts.md: rule→title 16, title→subtitle 4, →plot 12, plot→source 10, source→actions 6", () => {
    const d = declarations(read("FigureFrame"));
    const get = (sel: string, prop: string) => d.find(([s, p]) => s === sel && p === prop)?.[2];
    expect(get(".figure-frame", "padding-top")).toBe("var(--space-5)");
    expect(get(".figure-frame__caption", "gap")).toBe("var(--space-2)");
    expect(get(".figure-frame__legend, .figure-frame__plot", "margin-top")).toBe("var(--space-4)");
    expect(get(".figure-frame__source", "margin-top")).toBe("calc(var(--space-3) + var(--space-1))");
    expect(get(".figure-frame__actions", "margin-top")).toBe("calc(var(--space-2) + var(--space-1))");
    expect(tokensCss).toMatch(/--space-1:\s*2px[\s\S]*--space-2:\s*4px[\s\S]*--space-3:\s*8px[\s\S]*--space-5:\s*16px/);
  });

  it("Page blocks are 28px apart (layout.md, Article page)", () => {
    const gap = declarations(read("Page")).find(([s, p]) => s === ".page-body" && p === "gap")?.[2];
    expect(gap).toBe("var(--space-8)");
    expect(tokensCss).toMatch(/--space-8:\s*28px/);
  });
});
