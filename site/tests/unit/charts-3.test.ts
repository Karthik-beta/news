// Builder's checks for Charts III (#13, part 2): SmallMultiples, Heatmap and Histogram on their fixture pages.
// Small multiples share one x axis and one value axis across panels; histogram bins sit on round numbers with
// exact edges and counts; heatmap classes take sequential blue steps validated in both themes, with cell text that
// reads on every step. The shared checks (every number cited, CSV equals the table, Markdown, colour never follows
// a value) run for these charts in data-charts.test.ts. The formal tests and sign-off are the component tester's.

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { JSDOM } from "jsdom";
import { describe, expect, it } from "vitest";
import { heatmapClasses, rampSteps } from "../../src/components/Heatmap/Heatmap";
import type { HeatmapPlotProps } from "../../src/components/Heatmap/HeatmapPlot.client";
import { binTarget, histogramBins } from "../../src/components/Histogram/Histogram";
import { panelColumns } from "../../src/components/SmallMultiples/SmallMultiples";
import { componentFixtureBundle, componentFixtures } from "../../src/renderer/fixtures/component-pages";
import { fixtureLibrary } from "../../src/renderer/fixtures/library";
import { renderPage } from "../../src/renderer/render";
import { readTokens, siteRoot } from "../support/tokens";
import { validateOrdinal } from "../support/validate-palette.js";

const library = fixtureLibrary();
const NAMES = ["SmallMultiples", "Heatmap", "Histogram"];
const pages = componentFixtures
  .filter((f) => NAMES.includes(f.component))
  .map((f) => {
    const result = renderPage(componentFixtureBundle(f), { library });
    return { id: `${f.component}/${f.fixture}`, result, doc: new JSDOM(result.article).window.document };
  });
const page = (id: string) => {
  const p = pages.find((x) => x.id === id);
  if (!p) throw new Error(`no fixture page ${id}`);
  return p;
};
const figure = (doc: Document, statement: string) => {
  const el = doc.getElementById(`${statement}-title`)?.closest("figure");
  if (!el) throw new Error(`no figure for ${statement}`);
  return el;
};
const islands = (el: Element, name: string) =>
  [...el.querySelectorAll(`[data-island="${name}"]`)].map((i) => JSON.parse(i.getAttribute("data-props") ?? "null"));

describe("fixtures", () => {
  it("each chart has a default and an edge-case fixture, and the defaults resolve cleanly", () => {
    for (const name of NAMES) {
      expect(page(`${name}/default`).result.problems, name).toEqual([]);
      expect(page(`${name}/edge-cases`), name).toBeTruthy();
    }
  });
});

describe("SmallMultiples shares its scales", () => {
  it("every panel has the same value axis and the same periods, from all panels' data", () => {
    for (const [id, statement, island] of [
      ["SmallMultiples/default", "margins", "LineChartPlot"],
      ["SmallMultiples/default", "columns", "ColumnChartPlot"],
      ["SmallMultiples/edge-cases", "many", "LineChartPlot"],
      ["SmallMultiples/edge-cases", "fellColumns", "ColumnChartPlot"],
    ] as const) {
      const panels = islands(figure(page(id).doc, statement), island);
      expect(panels.length, statement).toBeGreaterThan(1);
      const first = panels[0];
      for (const p of panels) {
        expect(p.ticks, statement).toEqual(first.ticks);
        expect(p.xLabels, statement).toEqual(first.xLabels);
        if (island === "LineChartPlot") expect(p.endRoom, statement).toEqual(first.endRoom);
      }
    }
  });

  it("the shared axis covers every panel: products' 31.5% and services' 73.9% sit on one 0-80 scale", () => {
    const [services, products] = islands(figure(page("SmallMultiples/default").doc, "margins"), "LineChartPlot");
    expect(services.ticks.map((t: { label: string }) => t.label)).toEqual(["0", "20", "40", "60", "80%"]);
    // Same value, same height in every panel: products' line sits lower than services' on the same axis.
    const yOf = (p: { series: { ys: (number | null)[] }[] }) => p.series[0]?.ys[4] as number;
    expect(yOf(products)).toBeGreaterThan(yOf(services));
  });

  it("panels with different periods share one x axis; a period a panel lacks is a gap, never a zero", () => {
    const panels = islands(figure(page("SmallMultiples/edge-cases").doc, "periods"), "LineChartPlot");
    const stops = panels.map((p) => p.stops.map((s: { key: string }) => s.key));
    expect(new Set(stops.map((s) => s.join()))).toHaveProperty("size", 1);
    const gaps = panels[1].series[0].ys as (number | null)[];
    expect(gaps.filter((y) => y === null).length).toBeGreaterThan(0);
  });

  it("a series in another unit is left out; unknown series show No data", () => {
    expect(islands(figure(page("SmallMultiples/edge-cases").doc, "units"), "LineChartPlot")).toHaveLength(2);
    const unknown = figure(page("SmallMultiples/edge-cases").doc, "unknown");
    expect(unknown.querySelector("[data-island]")).toBeNull();
    expect(unknown.textContent).toContain("No data");
  });

  it("the highlighted panel is crimson, the rest context; a panel that fell takes the neutral highlight", () => {
    const [services, products] = islands(figure(page("SmallMultiples/default").doc, "margins"), "LineChartPlot");
    expect(services.series[0].role).toBe("highlight");
    expect(products.series[0].role).toBe("context");
    const fellLine = islands(figure(page("SmallMultiples/edge-cases").doc, "fellLine"), "LineChartPlot");
    expect(fellLine[0].series[0].role).toBe("highlight-neutral");
    const fellColumns = islands(figure(page("SmallMultiples/edge-cases").doc, "fellColumns"), "ColumnChartPlot");
    for (const b of fellColumns[0].bands) for (const m of b.marks) expect(m.role).toBe("highlight-neutral");
    const plain = islands(figure(page("SmallMultiples/default").doc, "columns"), "ColumnChartPlot");
    for (const p of plain) for (const b of p.bands) for (const m of b.marks) expect(m.role).toBe("cat-1");
  });

  it("2 panels across on phones, then 3 or 4 so the last row is as full as it can be", () => {
    expect([2, 3, 4, 5, 6, 8, 9, 12].map((n) => [n, panelColumns(n)])).toEqual([
      [2, { mid: 2, wide: 2 }],
      [3, { mid: 3, wide: 3 }],
      [4, { mid: 2, wide: 4 }],
      [5, { mid: 3, wide: 3 }],
      [6, { mid: 3, wide: 3 }],
      [8, { mid: 3, wide: 4 }],
      [9, { mid: 3, wide: 3 }],
      [12, { mid: 3, wide: 4 }],
    ]);
  });
});

describe("Histogram bins", () => {
  it("Sturges' rule picks the bin count, at most 12", () => {
    expect([1, 2, 12, 16, 60, 200, 10_000].map(binTarget)).toEqual([1, 2, 5, 5, 7, 9, 12]);
  });

  it("monthly inflation in 2024 falls in four half-point ranges; a value on an edge goes up a range", () => {
    const cpi = ["3.1", "3.2", "3.5", "3.4", "3.3", "3.0", "2.9", "2.5", "2.4", "2.6", "2.7", "2.9"];
    const bins = histogramBins(cpi, binTarget(cpi.length));
    expect(bins.edges).toEqual(["2.0", "2.5", "3.0", "3.5", "4.0"]);
    expect(bins.step).toBe("0.5");
    expect(bins.counts).toEqual([1, 5, 5, 1]);
    // 3.0 opens [3, 3.5) and 3.5 opens [3.5, 4): edges are compared as exact decimals.
    expect(bins.bin[cpi.indexOf("3.0")]).toBe(2);
    expect(bins.bin[cpi.indexOf("3.5")]).toBe(3);
  });

  it("edges are exact: 0.3 starts [0.3, 0.4), never the end of [0.2, 0.3)", () => {
    const bins = histogramBins(["0.1", "0.2", "0.3"], 3);
    expect(bins.edges).toEqual(["0.1", "0.2", "0.3", "0.4"]);
    expect(bins.counts).toEqual([1, 1, 1]);
  });

  it("the counts add up to the values plotted, with negatives, zero and one value", () => {
    const signs = ["-5.7", "0", "3.4", "12.6", "0.02"];
    const a = histogramBins(signs, binTarget(signs.length));
    expect(a.counts.reduce((x, y) => x + y, 0)).toBe(signs.length);
    expect(Number(a.edges[0])).toBeLessThanOrEqual(-5.7);
    expect(Number(a.edges.at(-1))).toBeGreaterThan(12.6);
    const one = histogramBins(["42.5"], 1);
    expect(one).toMatchObject({ edges: ["42", "43"], counts: [1] });
    expect(histogramBins([], 3).counts).toEqual([]);
  });

  it("the fixture's sixteen items make five ranges of 5 and the highlighted item's range is crimson", () => {
    const [plot] = islands(figure(page("Histogram/default").doc, "items"), "ColumnChartPlot");
    expect(plot.xLabels.map((l: { label: string }) => l.label)).toEqual(["0", "5", "10", "15", "20", "25"]);
    expect(plot.bands.map((b: { marks: { role: string }[] }) => b.marks[0]?.role)).toEqual([
      "context",
      "context",
      "context",
      "context",
      "highlight",
    ]);
    // The axis counts in whole numbers, and no count is written on the plot (a count is not a source figure).
    for (const t of plot.ticks) expect(Number(t.label)).toSatisfy(Number.isInteger);
    expect(plot.bands.every((b: { marks: { label: string | null }[] }) => b.marks[0]?.label === null)).toBe(true);
  });

  it("a highlighted value below zero is never crimson; no data and other units are left out and named", () => {
    const fig = figure(page("Histogram/edge-cases").doc, "signs");
    const [plot] = islands(fig, "ColumnChartPlot");
    const roles = plot.bands.map((b: { marks: { role: string }[] }) => b.marks[0]?.role);
    expect(roles).toContain("highlight-neutral");
    expect(roles).not.toContain("highlight");
    expect(fig.querySelector(".histogram__missing")?.textContent).toBe(
      "No data, not counted: Not published; Different unit.",
    );
  });
});

describe("Heatmap colour steps", () => {
  const tokens = new Map(readTokens().map((t) => [t.name, t]));
  const value = (name: string, theme: "light" | "dark") => {
    const t = tokens.get(name);
    if (!t) throw new Error(`no token ${name}`);
    return theme === "dark" ? (t.dark ?? t.light) : t.light;
  };
  // The steps Heatmap.css uses, least to most, with the text colour written on each.
  const css = readFileSync(join(siteRoot, "src/components/Heatmap/Heatmap.css"), "utf8");
  const stepsIn = (block: string) =>
    [1, 2, 3, 4, 5].map((n) => {
      const m = new RegExp(
        `\\.heatmap__step--${n} \\{\\s*--chart-mark: var\\((--chart-seq-blue-\\d+)\\);\\s*--heatmap-text: var\\((--color-\\w+)\\);`,
      ).exec(block);
      if (!m) throw new Error(`step ${n} not found`);
      return { fill: m[1] as string, text: m[2] as string };
    });
  const themes = {
    light: stepsIn(css.slice(0, css.indexOf("@media (prefers-color-scheme: dark)"))),
    dark: stepsIn(css.slice(css.indexOf(':root[data-theme="dark"]'))),
  };

  it.each(["light", "dark"] as const)("%s: one hue, monotone, visible steps (the dataviz validator)", (theme) => {
    const ramp = themes[theme].map((s) => value(s.fill, theme));
    const r = (validateOrdinal as (p: string[], o: object) => { ok: boolean; report: unknown })(ramp, {
      mode: theme,
      surface: value("--color-paper", theme),
    });
    expect(r.ok, JSON.stringify(r.report)).toBe(true);
  });

  it.each(["light", "dark"] as const)("%s: the value on every step reads at 4.5:1", (theme) => {
    const lum = (hex: string) => {
      const [r, g, b] = [1, 3, 5].map((i) => {
        const c = Number.parseInt(hex.slice(i, i + 2), 16) / 255;
        return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
      }) as [number, number, number];
      return 0.2126 * r + 0.7152 * g + 0.0722 * b;
    };
    for (const s of themes[theme]) {
      const [hi, lo] = [lum(value(s.fill, theme)), lum(value(s.text, theme))].sort((x, y) => y - x) as [number, number];
      expect((hi + 0.05) / (lo + 0.05), `${s.fill} with ${s.text}`).toBeGreaterThanOrEqual(4.5);
    }
  });

  it("the dark block repeats the media-query block exactly", () => {
    const media = css.slice(
      css.indexOf("@media (prefers-color-scheme: dark)"),
      css.indexOf(':root[data-theme="dark"]'),
    );
    expect(stepsIn(media.replaceAll(':root:not([data-theme="light"]) ', ""))).toEqual(themes.dark);
  });

  it("fewer classes take steps spread across the ramp, ends included", () => {
    expect([1, 2, 3, 4, 5].map(rampSteps)).toEqual([[3], [1, 5], [1, 3, 5], [1, 2, 4, 5], [1, 2, 3, 4, 5]]);
  });

  it("classes sit on the value axis's round numbers; a value on an edge goes up a class", () => {
    const pct = { unit: "Percent", currency: null, scale: null };
    const c = heatmapClasses(["66.0", "73.9", "31.5", "37.2"], pct, 0);
    expect(c.edges.map((e) => e.value)).toEqual(["30", "40", "50", "60", "70", "80"]);
    expect(c.steps).toEqual([1, 2, 3, 4, 5]);
    expect(["31.5", "40", "66.0", "70", "73.9"].map(c.classOf)).toEqual([0, 1, 3, 4, 4]);
  });

  it("each cell carries its step, its value's reference and, where it has one, its value", () => {
    const fig = figure(page("Heatmap/edge-cases").doc, "matrix");
    const [plot] = islands(fig, "HeatmapPlot") as HeatmapPlotProps[];
    const cells = plot?.rows.flatMap((r) => r.cells) ?? [];
    expect(cells.some((c) => c.step === null && c.label === null)).toBe(true); // a gap: hatched, no value
    for (const c of cells) if (c.step !== null) expect(c.ref).toMatch(/^dp_/);
    expect(fig.querySelectorAll(".heatmap__cell.chart-hatch").length).toBeGreaterThan(0);
    expect(fig.querySelector(".heatmap__legend")?.textContent).toContain("No data");
    // Nothing in a heatmap is crimson: colour is magnitude.
    expect(fig.querySelector(".chart-colour--highlight, .is-highlight")).toBeNull();
  });
});
