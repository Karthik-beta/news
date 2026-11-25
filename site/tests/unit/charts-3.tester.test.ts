// Component tester's checks for Charts III part 1 (#13): SmallMultiples, Histogram and Heatmap, on top of the
// builder's charts-3.test.ts. Histogram bins and counts are recounted from the source rows with an independent exact
// decimal comparison (fixtures and seeded random sets: negatives, duplicates, values on edges, ≥ 1e12, 1e-9, 18
// significant digits, 200 items); tooltips list at most 8 values with their citations. Small multiples share one x
// and value axis in line and column form, with gaps, zeros, negatives and dropped units; a panel with no values says
// No data. Heatmap classes partition every value exactly; one value, signed data, 60 periods; true minus in every
// label; the schema limits.
import { JSDOM } from "jsdom";
import { describe, expect, it } from "vitest";
import { heatmapClasses } from "../../src/components/Heatmap/Heatmap";
import HeatmapSpec from "../../src/components/Heatmap/spec";
import { binTarget, histogramBins } from "../../src/components/Histogram/Histogram";
import HistogramSpec from "../../src/components/Histogram/spec";
import SmallMultiplesSpec from "../../src/components/SmallMultiples/spec";
import { parseDecimal } from "../../src/format/decimal";
import {
  componentFixtureBundle,
  componentFixtureData,
  componentFixtures,
} from "../../src/renderer/fixtures/component-pages";
import { fixtureLibrary } from "../../src/renderer/fixtures/library";
import { renderPage } from "../../src/renderer/render";

const library = fixtureLibrary();

// ---------------------------------------------------------------------------------------------------------------
// Exact decimals, written independently of the component's own helpers.

const fracLen = (v: string) => parseDecimal(v).frac.length;
const big = (v: string, places: number) => {
  const { negative, int, frac } = parseDecimal(v);
  const n = BigInt(int + frac.padEnd(places, "0"));
  return negative ? -n : n;
};
/** −1, 0 or 1, exactly. */
const cmp = (a: string, b: string) => {
  const p = Math.max(fracLen(a), fracLen(b));
  const x = big(a, p);
  const y = big(b, p);
  return x < y ? -1 : x > y ? 1 : 0;
};
const sub = (a: string, b: string) => {
  const p = Math.max(fracLen(a), fracLen(b));
  return { n: big(a, p) - big(b, p), p };
};
/** The bin of a value among half-open bins [e_i, e_i+1), counted exactly; −1 when outside. */
const binOf = (v: string, edges: readonly string[]) =>
  edges.slice(0, -1).findIndex((e, i) => cmp(v, e) >= 0 && cmp(v, edges[i + 1] as string) < 0);
/** Is the step 1, 2 or 5 × 10^n? */
const roundStep = (step: string) => /^0\.0*[125]$|^[125]0*$/.test(step.replace(/^0+(?=\d)/, ""));

/** Seeded random numbers, so a failure reproduces. */
function rng(seed: number) {
  let s = seed >>> 0;
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return s / 2 ** 32;
  };
}

// ---------------------------------------------------------------------------------------------------------------
// Pages: the fixtures, and pages made for one test from the fixture data plus synthetic points and series.

interface Point {
  value: string | null;
  unit?: string;
  currency?: string | null;
  scale?: string | null;
  period?: string;
}
interface Series {
  label: string;
  unit?: string;
  currency?: string | null;
  scale?: string | null;
  points: [period: string, point: string | null][];
}

function render(oui: string, points: Record<string, Point> = {}, series: Record<string, Series> = {}) {
  const base = structuredClone(componentFixtureData()) as ReturnType<typeof componentFixtureData> & {
    data: { points: Record<string, unknown>; series: Record<string, unknown> };
  };
  for (const [ref, p] of Object.entries(points)) {
    base.data.points[ref] = {
      label: ref,
      value: p.value,
      value_text: p.value,
      unit: p.unit ?? "Percent",
      currency: p.currency ?? null,
      scale: p.scale ?? null,
      period: p.period ?? "pd_fx_2024",
      source: "src_fixture_synthetic",
      locator: { kind: "json", series: ref },
    };
  }
  for (const [ref, s] of Object.entries(series)) {
    base.data.series[ref] = {
      label: s.label,
      unit: s.unit ?? "Percent",
      currency: s.currency ?? null,
      scale: s.scale ?? null,
      points: s.points.map(([period, point]) => ({ period, point })),
    };
  }
  const result = renderPage(
    { oui, ...base, head: { flyTitle: "Test", headline: "Test", summary: "Test" } },
    { library },
  );
  return { result, doc: new JSDOM(result.article).window.document, data: base.data };
}

function fixture(component: string, name: string) {
  const f = componentFixtures.find((x) => x.component === component && x.fixture === name);
  if (!f) throw new Error(`no fixture ${component}/${name}`);
  const bundle = componentFixtureBundle(f);
  const result = renderPage(bundle, { library });
  return {
    result,
    doc: new JSDOM(result.article).window.document,
    data: bundle.data as { points: Record<string, { value: string | null; unit: string }> },
  };
}

const figure = (doc: Document, statement: string) => {
  const el = doc.getElementById(`${statement}-title`)?.closest("figure");
  if (!el) throw new Error(`no figure for ${statement}`);
  return el;
};
// biome-ignore lint/suspicious/noExplicitAny: island props are each island's own JSON
const islands = (el: Element, name?: string): any[] =>
  [...el.querySelectorAll(name ? `[data-island="${name}"]` : "[data-island]")].map((i) =>
    JSON.parse(i.getAttribute("data-props") ?? "null"),
  );
const statements = (oui: string) =>
  [...oui.matchAll(/^(\w+) = (Histogram|SmallMultiples|Heatmap)\(/gm)].map((m) => m[1] as string);

// ---------------------------------------------------------------------------------------------------------------

describe("Histogram: bins and counts equal the source rows", () => {
  /** Checks one set of values against histogramBins, independently. */
  function expectExactBins(values: string[]) {
    const target = binTarget(values.length);
    const bins = histogramBins(values, target);
    const n = bins.counts.length;
    expect(n, "bins").toBeGreaterThanOrEqual(1);
    expect(n, "at most Sturges' target").toBeLessThanOrEqual(target);
    expect(n, "at most 12").toBeLessThanOrEqual(12);
    expect(bins.edges).toHaveLength(n + 1);
    expect(roundStep(bins.step), `step ${bins.step}`).toBe(true);
    for (let i = 0; i < n; i++) {
      const d = sub(bins.edges[i + 1] as string, bins.edges[i] as string);
      const s = big(bins.step, d.p);
      expect(d.n, `edge ${i} to ${i + 1} is one step`).toBe(s);
      const k = big(bins.edges[i] as string, d.p);
      expect(((k % s) + s) % s, `edge ${bins.edges[i]} is a multiple of the step`).toBe(0n);
    }
    const counts = new Array(n).fill(0);
    values.forEach((v, j) => {
      const b = binOf(v, bins.edges);
      expect(b, `${v} lies inside the edges`).toBeGreaterThanOrEqual(0);
      expect(bins.bin[j], `${v}'s bin`).toBe(b);
      counts[b] += 1;
    });
    expect(bins.counts).toEqual(counts);
    // The ends are tight: no empty bin before the lowest value or after the highest.
    expect(bins.counts[0]).toBeGreaterThan(0);
    expect(bins.counts[n - 1]).toBeGreaterThan(0);
  }

  it("every fixture histogram: bands recount from data.json, tooltips list that range's values with citations", () => {
    for (const name of ["default", "edge-cases"]) {
      const f = fixture("Histogram", name);
      const oui = componentFixtures.find((x) => x.component === "Histogram" && x.fixture === name)?.oui ?? "";
      for (const statement of statements(oui)) {
        const fig = figure(f.doc, statement);
        const [plot] = islands(fig, "ColumnChartPlot");
        if (!plot) continue; // every value unknown: no plot
        // The items' values, from the source rows, in the chart's unit (the first value's).
        const refs = [
          ...(oui
            .split("\n")
            .find((l) => l.startsWith(`${statement} =`))
            ?.matchAll(/value: "(d[pv]_\w+)"/g) ?? []),
        ].map((m) => m[1] as string);
        const rows = refs
          .map((r) => ({ ref: r, point: f.data.points[r] }))
          .filter((r) => r.point && r.point.value !== null);
        const unit = rows[0]?.point?.unit;
        const plotted = rows.filter((r) => r.point?.unit === unit);
        const values = plotted.map((r) => r.point?.value as string);
        expectExactBins(values);
        const bins = histogramBins(values, binTarget(values.length));
        expect(plot.bands).toHaveLength(bins.counts.length);
        for (const [i, band] of plot.bands.entries()) {
          const inBin = plotted.filter((r) => binOf(r.point?.value as string, bins.edges) === i).map((r) => r.ref);
          expect(band.label, `${statement} band ${i}`).toMatch(new RegExp(`: ${inBin.length} values?(, 8 listed)?$`));
          const listed = band.tooltip.map((t: { ref: string }) => t.ref);
          expect(listed.length).toBe(Math.min(8, inBin.length));
          for (const ref of listed) expect(inBin, `${statement}: ${ref} listed in its own range`).toContain(ref);
          for (const t of band.tooltip) {
            expect(t.ref).toMatch(/^d[pv]_/);
            expect(t.source, `${t.ref} cites a source`).toBeTruthy();
          }
          expect(band.marks[0].label, "no count written on the plot").toBeNull();
        }
      }
    }
  });

  it("seeded random sets: negatives, duplicates, edges, ≥ 1e12, 1e-9 and up to 200 values recount exactly", () => {
    const rand = rng(13);
    for (let t = 0; t < 300; t++) {
      const n = 1 + Math.floor(rand() * 200);
      const places = Math.floor(rand() * 4);
      const mag = 10 ** Math.floor(rand() * 16 - 9);
      const values = Array.from({ length: n }, () => {
        const x = (rand() - 0.3) * mag * 100;
        // A quarter of the values sit on a round number, so they land on an edge.
        const v = rand() < 0.25 ? Math.round(x / mag) * mag : x;
        const s = v.toFixed(Math.max(places, Math.max(0, -Math.floor(Math.log10(mag))) + places));
        return s.replace(/^-0(\.0+)?$/, "0").replace(/^-(0\.0+)$/, "$1");
      });
      expectExactBins(values);
    }
  }, 60_000); // about 2 s alone; slower when the whole unit project shares the CPU

  it("all-equal values, a single zero and very large integers make one tight range", () => {
    expectExactBins(["5", "5", "5", "5"]);
    expectExactBins(["0", "0"]);
    expectExactBins(["123456789012345678", "98765432109876543", "150000000000000000"]);
  });

  it("values with 18 significant digits next to an edge are binned exactly, not after rounding to a float", () => {
    // Number("1.99999999999999999") is 2: a float comparison puts it in [2, 3) or [2, 4).
    expectExactBins(["0", "1.99999999999999999"]);
    expectExactBins(["99999999999999999", "199999999999999999", "100000000000000000"]);
  });

  it("the tooltip lists at most 8 values and says so; the summary and table carry them all", () => {
    const points: Record<string, Point> = {};
    for (let i = 0; i < 12; i++) points[`dp_t_${i}`] = { value: `1.${i % 10}` };
    for (let i = 12; i < 14; i++) points[`dp_t_${i}`] = { value: "9.5" };
    const items = Object.keys(points)
      .map((r, i) => `{label: "Item ${i}", value: "${r}"}`)
      .join(", ");
    const { doc } = render(
      `root = Page([h])\nh = Histogram("Most values sit low", "Synthetic, %", [${items}])`,
      points,
    );
    const fig = figure(doc, "h");
    const [plot] = islands(fig, "ColumnChartPlot");
    const crowded = plot.bands.find((b: { tooltip: unknown[] }) => b.tooltip.length === 8);
    expect(crowded?.label).toMatch(/: 12 values, 8 listed$/);
    expect(fig.querySelectorAll("tbody tr")).toHaveLength(14);
    expect(plot.summary).toContain("14 values in");
  });

  it("formats with a true minus and the source's currency; the step and edges never convert units", () => {
    const { doc } = fixture("Histogram", "edge-cases");
    const [signs] = islands(figure(doc, "signs"), "ColumnChartPlot");
    expect(signs.summary).toContain("−10% to under 0%");
    expect(signs.summary).not.toMatch(/-\d/);
    expect(signs.xLabels[0].label).toBe("−10");
    const [money] = islands(figure(doc, "money"), "ColumnChartPlot");
    expect(money.summary).toMatch(/ranges of \$50(\.0)?bn/);
    expect(money.bands[0].tooltip[0].value).toMatch(/^\$\d+\.\dbn$/);
  });

  it("edge labels are bare round numbers like any value axis: no decimals the edges do not have", () => {
    const { doc } = fixture("Histogram", "edge-cases");
    const labels = (s: string) =>
      islands(figure(doc, s), "ColumnChartPlot")[0].xLabels.map((l: { label: string }) => l.label);
    expect(labels("money")).toEqual(["0", "50", "100", "150", "200"]);
    expect(labels("large")).toEqual(["80,000", "100,000", "120,000", "140,000", "160,000"]);
    // A half step keeps its decimal on every edge.
    const cpi = islands(figure(fixture("Histogram", "default").doc, "cpi"), "ColumnChartPlot")[0];
    expect(cpi.xLabels.map((l: { label: string }) => l.label)).toEqual(["2.0", "2.5", "3.0", "3.5", "4.0"]);
  });

  it("a highlight on an item with no data, or in another unit, highlights nothing", () => {
    const { doc } = render(
      `root = Page([h])\nh = Histogram("t", "s", [{label: "A", value: "dp_t_a"}, {label: "B", value: "dp_t_b"}, {label: "C", value: "dp_t_c", highlight: true}])`,
      {
        dp_t_a: { value: "1" },
        dp_t_b: { value: "2" },
        dp_t_c: { value: "3", unit: "USD", currency: "USD", scale: "millions" },
      },
    );
    const [plot] = islands(figure(doc, "h"), "ColumnChartPlot");
    for (const b of plot.bands) expect(b.marks[0].role).toBe("cat-1");
  });

  it("the schema takes 2–200 items", () => {
    const item = { label: "x", value: "dp_x" };
    const parse = (n: number) =>
      HistogramSpec.props.safeParse({ title: "t", subtitle: "s", items: Array.from({ length: n }, () => item) })
        .success;
    expect([1, 2, 200, 201].map(parse)).toEqual([false, true, true, false]);
  });
});

describe("SmallMultiples: shared scales", () => {
  const year = (y: number) => `pd_fx_${y}`;
  const points: Record<string, Point> = {};
  const series: Record<string, Series> = {};
  // a: rises from −40 to 10; b: flat zeros; c: only 2021-2022, large; d: no values at all; e: dollars (dropped).
  const make = (ref: string, label: string, values: (string | null)[], start = 2019, extra: Partial<Point> = {}) => {
    series[ref] = { label, points: [], unit: extra.unit, currency: extra.currency, scale: extra.scale };
    values.forEach((v, i) => {
      const p = `dp_${ref.slice(3)}_${start + i}`;
      points[p] = { value: v, period: year(start + i), ...extra };
      (series[ref] as Series).points.push([year(start + i), p]);
    });
  };
  make("sr_t_a", "Rising from below zero", ["-40", "-20", "0", "10"]);
  make("sr_t_b", "Flat at zero", ["0", "0", "0", "0"]);
  make("sr_t_c", "Short and large", ["250", "180"], 2021);
  make("sr_t_d", "Not yet published", [null, null, null, null]);
  make("sr_t_e", "In dollars", ["1", "2", "3", "4"], 2019, { unit: "USD", currency: "USD", scale: "billions" });
  const refs = `["sr_t_a", "sr_t_b", "sr_t_c", "sr_t_d", "sr_t_e"]`;

  for (const form of ["line", "column"] as const) {
    it(`${form}: one value axis from all panels' values and one x axis from all their periods`, () => {
      const { doc } = render(`root = Page([m])\nm = SmallMultiples("t", "s, %", ${refs}, "${form}")`, points, series);
      const panels = islands(figure(doc, "m"), form === "line" ? "LineChartPlot" : "ColumnChartPlot");
      expect(panels.length, "the dollar series is left out; the empty one draws no plot").toBe(3);
      const labels = panels[0].ticks.map((t: { label: string }) => t.label);
      expect(labels[0]).toMatch(/^−/); // covers a's −40
      expect(Number(labels.at(-1)?.replace("%", ""))).toBeGreaterThanOrEqual(250); // and c's 250
      for (const p of panels) {
        expect(p.ticks).toEqual(panels[0].ticks);
        expect(p.xLabels).toEqual(panels[0].xLabels);
        const stops = form === "line" ? p.stops : p.bands;
        expect(stops.map((s: { key: string }) => s.key)).toEqual([
          "pd_fx_2019",
          "pd_fx_2020",
          "pd_fx_2021",
          "pd_fx_2022",
        ]);
      }
      // The short panel has nothing in 2019 and 2020: a gap or a hatched column, never a zero.
      const short = panels[2];
      if (form === "line") expect(short.series[0].ys.slice(0, 2)).toEqual([null, null]);
      else
        for (const b of short.bands.slice(0, 2))
          expect(b.marks.every((m: { height: number; ref: string | null }) => m.ref === null)).toBe(true);
    });
  }

  it("a panel with no values names itself and says No data, with no empty plot and no tab stop", () => {
    for (const form of ["line", "column"]) {
      const { doc } = render(`root = Page([m])\nm = SmallMultiples("t", "s, %", ${refs}, "${form}")`, points, series);
      const fig = figure(doc, "m");
      const panel = [...fig.querySelectorAll(".small-multiples__panel")].find((li) =>
        li.querySelector(".small-multiples__name")?.textContent?.includes("Not yet published"),
      );
      expect(panel, form).toBeTruthy();
      expect(panel?.querySelector("[data-island]"), `${form}: no empty plot`).toBeNull();
      expect(panel?.textContent, form).toContain("No data");
    }
    // The fixture's eight-panel page: the last panel is the all-null series.
    const many = figure(fixture("SmallMultiples", "edge-cases").doc, "many");
    const last = [...many.querySelectorAll(".small-multiples__panel")].at(-1);
    expect(last?.querySelector("[data-island]")).toBeNull();
    expect(last?.textContent).toBe("Not yet publishedNo data");
  });

  it("the highlight rule: crimson when it rose, neutral when it fell, nothing when the highlight was left out", () => {
    const roles = (form: string, highlight: string) => {
      const { doc } = render(
        `root = Page([m])\nm = SmallMultiples("t", "s, %", ${refs}, "${form}", "${highlight}")`,
        points,
        series,
      );
      return islands(figure(doc, "m")).map((p) =>
        form === "line"
          ? p.series[0].role
          : p.bands.flatMap((b: { marks: { role: string }[] }) => b.marks.map((m) => m.role)),
      );
    };
    // a rose by the line rule: crimson as lines; as columns it has columns below zero, so it is neutral.
    expect(roles("line", "sr_t_a")).toEqual(["highlight", "context", "context"]);
    expect(roles("column", "sr_t_a")[0]).toEqual([
      "highlight-neutral",
      "highlight-neutral",
      "highlight-neutral",
      "highlight-neutral",
    ]);
    // c fell (250 to 180).
    expect(roles("line", "sr_t_c")[2]).toBe("highlight-neutral");
    // The dollar series is not drawn, so naming it highlights nothing.
    expect(roles("line", "sr_t_e")).toEqual(["cat-1", "cat-1", "cat-1"]);
  });

  it("a panel is never wider than a phone's plot, so its x labels are thinned as on a phone at every width", () => {
    for (const [p, s] of [
      ["default", "margins"],
      ["default", "columns"],
      ["edge-cases", "many"],
    ] as const) {
      for (const panel of islands(figure(fixture("SmallMultiples", p).doc, s))) {
        for (const l of panel.xLabels) expect(l.wide, `${s} ${l.label}`).toBe(l.phone);
      }
    }
  });

  it("the schema takes 2–12 series", () => {
    const parse = (n: number) =>
      SmallMultiplesSpec.props.safeParse({
        title: "t",
        subtitle: "s",
        series: Array.from({ length: n }, (_, i) => `sr_x_${i}`),
      }).success;
    expect([1, 2, 12, 13].map(parse)).toEqual([false, true, true, false]);
  });
});

describe("Heatmap: classes, labels and limits", () => {
  const pct = { unit: "Percent", currency: null, scale: null };

  it("every cell's class is the legend range that holds its value, exactly (the fixture pages)", () => {
    for (const name of ["default", "edge-cases"]) {
      const { doc } = fixture("Heatmap", name);
      for (const fig of doc.querySelectorAll("figure")) {
        const [plot] = islands(fig, "HeatmapPlot");
        if (!plot) continue;
        const keys = [...fig.querySelectorAll(".heatmap__legend li")]
          .map((li) => ({ step: /heatmap__step--(\d)/.exec(li.innerHTML)?.[1], text: li.textContent ?? "" }))
          .filter((k) => k.step);
        const ranges = keys.map((k) => {
          const [lo, hi] = k.text
            .replace(/%$/, "")
            .split(" to ")
            .map((x) => x.replace("−", "-").replaceAll(",", ""));
          return { step: Number(k.step), lo: lo as string, hi: hi as string };
        });
        for (const row of plot.rows) {
          for (const c of row.cells) {
            if (c.step === null) continue;
            const v = c.tooltip.value.replace(/[^\d.−-]/g, "").replace("−", "-");
            const i = ranges.findIndex(
              (r, j) => cmp(v, r.lo) >= 0 && (cmp(v, r.hi) < 0 || (j === ranges.length - 1 && cmp(v, r.hi) <= 0)),
            );
            expect(i, `${c.key} ${v}`).toBeGreaterThanOrEqual(0);
            expect(c.step, `${c.key} ${v}`).toBe(ranges[i]?.step);
          }
        }
      }
    }
  });

  it("labels use the true minus: legend, cells and tooltips", () => {
    const fig = figure(fixture("Heatmap", "edge-cases").doc, "matrix");
    expect(fig.querySelector(".heatmap__legend")?.textContent).toContain("−10 to −5");
    const [plot] = islands(fig, "HeatmapPlot");
    const negative = plot.rows[1].cells[0];
    expect(negative.label).toBe("−5.7");
    expect(negative.tooltip.value).toBe("−5.7%");
    expect(fig.textContent).not.toMatch(/(^|[^\w])-\d/);
  });

  it("one value, or all equal: one class and one legend key holding it", () => {
    for (const values of [["42.5"], ["7", "7", "7"]]) {
      const c = heatmapClasses(values, pct, 0);
      expect(c.edges, values.join()).toHaveLength(2);
      expect(c.steps).toEqual([3]);
      expect(cmp(values[0] as string, c.edges[0]?.value as string)).toBeGreaterThanOrEqual(0);
      expect(cmp(values[0] as string, c.edges[1]?.value as string)).toBeLessThan(0);
    }
    const fig = figure(fixture("Heatmap", "edge-cases").doc, "single");
    const keys = [...fig.querySelectorAll(".heatmap__legend li")].map((li) => li.textContent);
    expect(keys).toEqual(["40 to 50%"]);
  });

  it("60 periods: one column each, labels thinned, and values never written (no column has room)", () => {
    const { doc } = render(`root = Page([w])\nw = Heatmap("t", "Synthetic index, 2020=100", ["sr_fx_long"])`);
    const [plot] = islands(figure(doc, "w"), "HeatmapPlot");
    expect(plot.columns).toBe(60);
    expect(plot.fit).toBeNull();
    const phone = plot.xLabels.filter((l: { phone: boolean }) => l.phone).length;
    expect(phone).toBeGreaterThan(1);
    expect(phone).toBeLessThan(13);
  });

  it("x labels are planned for the room left after the row names (the axis's 40 and 72 characters)", () => {
    for (const name of ["default", "edge-cases"]) {
      for (const fig of fixture("Heatmap", name).doc.querySelectorAll("figure")) {
        const [plot] = islands(fig, "HeatmapPlot");
        if (!plot) continue;
        const names = Number.parseFloat(plot.nameWidth) + 2;
        const longest = Math.max(...plot.xLabels.map((l: { label: string }) => l.label.length));
        for (const [key, room] of [
          ["phone", 40],
          ["wide", 72],
        ] as const) {
          const shown = plot.xLabels.filter((l: Record<string, boolean>) => l[key]).length;
          expect(shown, `${key}: ${plot.periods.length} periods`).toBeLessThanOrEqual(
            Math.max(2, Math.floor((room - names) / (longest + 2))),
          );
        }
      }
    }
  });

  it("very large and very small values keep their digits in cells and tooltips", () => {
    const { doc } = render(
      `root = Page([l, s])\nl = Heatmap("t", "s", ["sr_fx_large"])\ns = Heatmap("t", "s", ["sr_fx_small"])`,
    );
    const [large] = islands(figure(doc, "l"), "HeatmapPlot");
    const [small] = islands(figure(doc, "s"), "HeatmapPlot");
    for (const c of [...large.rows[0].cells, ...small.rows[0].cells]) {
      if (c.label === null) continue;
      expect(c.label).not.toMatch(/e[+-]?\d/i);
      expect(c.label).not.toBe("0");
    }
    expect(small.rows[0].cells[0].tooltip.value).toContain("0.000000012");
  });

  it("the schema takes 1–12 series", () => {
    const parse = (n: number) =>
      HeatmapSpec.props.safeParse({
        title: "t",
        subtitle: "s",
        series: Array.from({ length: n }, (_, i) => `sr_x_${i}`),
      }).success;
    expect([0, 1, 12, 13].map(parse)).toEqual([false, true, true, false]);
  });

  it("says it is for magnitudes: signed data reads as least to most, so zero is a middle shade", () => {
    expect(HeatmapSpec.dataShape).toMatch(/non-negative/i);
  });
});
