// Chart colours, validated by script in both themes (design/color.md, "Chart colours"; the dataviz validator):
// the categorical palette passes every hard check, the first three slots pass on every pair, and the highlight-mode
// roles (and the sparkline's line and dot) are distinct for colour-blind readers and for everyone. Also checks
// every chart colour the component styles use is a real token, and that crimson is reached only through the
// highlight role.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { readTokens, siteRoot } from "../support/tokens";
import { validate } from "../support/validate-palette.js";

type Report = { report: [string, boolean | string, string][]; ok: boolean };
const run = validate as unknown as (
  palette: string[],
  opts: { mode: "light" | "dark"; surface: string; pairs?: "all" },
) => Report;

const tokens = new Map(readTokens().map((t) => [t.name, t]));
const value = (name: string, theme: "light" | "dark") => {
  const t = tokens.get(name);
  if (!t) throw new Error(`no token ${name}`);
  return theme === "dark" ? (t.dark ?? t.light) : t.light;
};
const check = (r: Report, name: string) => r.report.find(([n]) => n === name)?.[1];

describe.each(["light", "dark"] as const)("%s theme", (theme) => {
  const surface = value("--color-paper", theme);
  const cats = Array.from({ length: 8 }, (_, i) => value(`--chart-cat-${i + 1}`, theme));

  it("the categorical palette passes every hard check, slots in order", () => {
    const r = run(cats, { mode: theme, surface });
    expect(r.ok, JSON.stringify(r.report)).toBe(true);
  });

  it("the first three categorical slots pass on every pair", () => {
    const r = run(cats.slice(0, 3), { mode: theme, surface, pairs: "all" });
    expect(r.ok, JSON.stringify(r.report)).toBe(true);
  });

  it("highlight, comparison and context are distinct for colour-blind readers and for everyone", () => {
    const roles = ["--chart-highlight", "--chart-comparison", "--chart-context"].map((n) => value(n, theme));
    const r = run(roles, { mode: theme, surface, pairs: "all" });
    // Context is grey-blue and below 3:1 on purpose (it recedes; every chart has labels and a table), so the
    // lightness band and chroma floor do not apply to it; separation does.
    expect(check(r, "CVD separation"), JSON.stringify(r.report)).toBe("pass");
    expect(check(r, "Normal-vision floor"), JSON.stringify(r.report)).toBe("pass");
  });

  it("the neutral highlight (a subject that fell) is distinct from comparison and context, and from the hatch", () => {
    const roles = ["--chart-neutral-highlight", "--chart-comparison", "--chart-context"].map((n) => value(n, theme));
    const r = run(roles, { mode: theme, surface, pairs: "all" });
    // Achromatic on purpose, so the categorical lightness band and chroma floor do not apply; separation does.
    expect(check(r, "CVD separation"), JSON.stringify(r.report)).toBe("pass");
    expect(check(r, "Normal-vision floor"), JSON.stringify(r.report)).toBe("pass");
    const hatch = run([value("--chart-neutral-highlight", theme), value("--chart-neutral", theme)], {
      mode: theme,
      surface,
    });
    expect(check(hatch, "Normal-vision floor"), JSON.stringify(hatch.report)).toBe("pass");
  });

  it("the neutral highlight reads as bold label text on paper and wash (4.5:1)", () => {
    const lum = (hex: string) => {
      const [r, g, b] = [1, 3, 5].map((i) => {
        const c = Number.parseInt(hex.slice(i, i + 2), 16) / 255;
        return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
      }) as [number, number, number];
      return 0.2126 * r + 0.7152 * g + 0.0722 * b;
    };
    const ratio = (a: string, b: string) => {
      const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x) as [number, number];
      return (hi + 0.05) / (lo + 0.05);
    };
    const ink = value("--chart-neutral-highlight", theme);
    for (const bg of ["--color-paper", "--color-wash"]) {
      expect(ratio(ink, value(bg, theme)), bg).toBeGreaterThanOrEqual(4.5);
    }
  });

  it("the sparkline's line and its latest-point dot are distinct", () => {
    const r = run([value("--chart-comparison", theme), value("--chart-highlight", theme)], { mode: theme, surface });
    expect(check(r, "CVD separation")).toBe("pass");
    expect(check(r, "Normal-vision floor")).toBe("pass");
  });
});

describe("component styles", () => {
  const folders = ["ChartFigure", "ColumnChart", "BarChart", "LineChart", "Sparkline", "DataTable", "KeyFigure"];
  const css = folders.map((f) => {
    const path = join(siteRoot, "src/components", f, `${f}.css`);
    return { f, text: readFileSync(path, "utf8") };
  });

  it("use only chart colour tokens that exist", () => {
    for (const { f, text } of css) {
      // Properties the components define themselves (--chart-mark, --chart-plot-height …) are not tokens.
      const own = new Set(css.flatMap((c) => [...c.text.matchAll(/(--[a-z0-9-]+)\s*:/g)].map((d) => d[1])));
      // A var() with a fallback is geometry an island sets inline (--chart-axis-width), never a token.
      for (const m of text.matchAll(/var\((--(?:chart|color)-[a-z0-9-]+)\)/g)) {
        if (own.has(m[1])) continue;
        expect(tokens.has(m[1] as string), `${f}: ${m[1]}`).toBe(true);
      }
    }
  });

  it("reach crimson only through the highlight role, never through a value state", () => {
    for (const { f, text } of css) {
      const rules = text.replace(/\/\*[\s\S]*?\*\//g, "").split("}");
      for (const rule of rules) {
        if (!/--chart-highlight|--color-brand/.test(rule)) continue;
        const selector = rule.slice(0, rule.indexOf("{"));
        // The highlight role, or the sparkline's latest point (design/charts.md, "Sparklines").
        expect(selector, `${f}: ${selector.trim()}`).toMatch(/highlight|sparkline__dot/);
        expect(selector, `${f}: ${selector.trim()}`).not.toMatch(/negative|decrease|down|fall/);
      }
    }
  });
});
