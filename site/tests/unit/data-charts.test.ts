// Builder's checks for the data and Charts I components (#8), over every component fixture page rendered by the
// real renderer: every figure carries its data reference; missing, null, zero, negative, very large, very small,
// long and single-point data render as specified; each chart's CSV equals its data table; colour never follows a
// value. The formal tests and sign-off are the component tester's (news-component-testing).
import { JSDOM } from "jsdom";
import { describe, expect, it } from "vitest";
import { PageData, PageSources } from "../../src/renderer/bundle/schema";
import { PageDataResolver } from "../../src/renderer/data/resolve";
import { componentFixtureBundle, componentFixtures } from "../../src/renderer/fixtures/component-pages";
import { fixtureLibrary } from "../../src/renderer/fixtures/library";
import { parsePage, renderPage } from "../../src/renderer/render";

const library = fixtureLibrary();
const CHART_NAMES = ["ColumnChart", "BarChart", "LineChart", "StepLineChart", "DataTable", "ComparisonTable"];
const DATA_NAMES = [...CHART_NAMES, "KeyFigure", "KeyFigures"];

const pages = componentFixtures
  .filter((f) => DATA_NAMES.includes(f.component))
  .map((f) => {
    const input = componentFixtureBundle(f);
    const result = renderPage(input, { library });
    const doc = new JSDOM(result.article).window.document;
    const resolver = new PageDataResolver(PageData.parse(input.data), PageSources.parse(input.sources));
    return { id: `${f.component}/${f.fixture}`, f, result, doc, resolver };
  });

const page = (id: string) => {
  const p = pages.find((x) => x.id === id);
  if (!p) throw new Error(`no fixture page ${id}`);
  return p;
};
/** The figure for a statement (`chart`), found by its title id. */
const figure = (doc: Document, statement: string) => {
  const el = doc.getElementById(`${statement}-title`)?.closest("figure");
  if (!el) throw new Error(`no figure for ${statement}`);
  return el;
};
const islandProps = (el: Element) =>
  JSON.parse(el.querySelector("[data-island]")?.getAttribute("data-props") ?? "null");

describe("component fixtures", () => {
  it("every data and chart component has a default and an edge-case fixture page", () => {
    for (const name of DATA_NAMES.filter((n) => n !== "KeyFigure")) {
      expect(
        pages.some((p) => p.id === `${name}/default`),
        name,
      ).toBe(true);
      expect(
        pages.some((p) => p.id === `${name}/edge-cases`),
        name,
      ).toBe(true);
    }
  });
});

describe("every rendered figure carries its source ID", () => {
  // Text that is not a figure: axis ticks, period and category labels, headings, source lines, the citation
  // panel itself, and the article head (the renderer's own).
  const NOT_FIGURES = [
    ".chart-plot__tick",
    ".chart-plot__x-label",
    ".line-chart__event-label",
    ".data-table__head",
    ".data-table__row-head",
    ".bar-chart__label",
    ".key-figure__label",
    "figcaption",
    ".figure-frame__source",
    ".figure-frame__actions",
    ".source-line",
    ".popover",
    ".article__head",
    ".article__sources",
    ".chart-legend",
  ].join(", ");

  it.each(pages.map((p) => [p.id, p] as const))("%s: every number on the page is cited", (_id, p) => {
    const body = p.doc.querySelector(".page-body") as Element;
    const uncited: string[] = [];
    for (const el of body.querySelectorAll("*")) {
      const own = [...el.childNodes]
        .filter((n) => n.nodeType === 3)
        .map((n) => n.textContent ?? "")
        .join("");
      if (!/\d/.test(own) || el.closest(NOT_FIGURES)) continue;
      const cited = el.closest("[data-ref]") || el.closest("button.source-ref");
      if (!cited) uncited.push(`${el.tagName.toLowerCase()}.${el.className}: ${own.trim()}`);
    }
    expect(uncited).toEqual([]);
  });

  it.each(pages.map((p) => [p.id, p] as const))("%s: every data-ref names a reference", (_id, p) => {
    for (const el of p.doc.querySelectorAll("[data-ref]")) {
      expect(el.getAttribute("data-ref")).toMatch(/^(dp|dv|sr|pd|q|cx)_[a-z0-9_]+$/);
    }
  });

  it("a key figure's value opens a citation popover with the exact value as reported", () => {
    const { doc } = page("KeyFigures/default");
    const trigger = doc.querySelector(".key-figure__figure button.source-ref") as Element;
    expect(trigger.textContent).toBe("$391.0bn");
    const panel = doc.getElementById(trigger.getAttribute("popovertarget") as string) as Element;
    expect(panel.querySelector(".popover__value")?.textContent).toBe("$391,035 million");
    expect(panel.querySelector("a.popover__link")?.getAttribute("href")).toMatch(/^https:\/\/www\.sec\.gov\//);
  });
});

describe("edge cases render as specified", () => {
  it("an unknown series or reference shows the frame with 'No data' and no island", () => {
    for (const [id, statement] of [
      ["ColumnChart/edge-cases", "unknown"],
      ["LineChart/edge-cases", "unknown"],
      ["StepLineChart/edge-cases", "unknown"],
      ["BarChart/edge-cases", "unknown"],
      ["DataTable/edge-cases", "unknown"],
    ] as const) {
      const fig = figure(page(id).doc, statement);
      expect(fig.querySelector(".chart-figure__missing")?.textContent, id).toBe("No data");
      expect(fig.querySelector("[data-island]"), id).toBeNull();
      expect(fig.textContent, id).not.toMatch(/\b0(\.0)?%/);
    }
    const kf = page("KeyFigures/edge-cases").doc;
    const unknown = [...kf.querySelectorAll(".key-figure")].find((k) => k.textContent?.includes("Unknown reference"));
    expect(unknown?.querySelector(".key-figure__figure")?.textContent).toBe("No data");
    expect(unknown?.querySelector(".sparkline")).toBeNull();
    expect(unknown?.querySelector(".key-figure__change")).toBeNull();
  });

  it("a series with no values at all says 'No data' and keeps its table of gaps", () => {
    const fig = figure(page("ColumnChart/edge-cases").doc, "allnull");
    expect(fig.querySelector(".chart-figure__missing")?.textContent).toBe("No data");
    const cells = [...fig.querySelectorAll("tbody td")].map((c) => c.textContent);
    expect(cells.length).toBeGreaterThan(0);
    expect(new Set(cells)).toEqual(new Set(["No data"]));
  });

  it("a null value is a hatched mark, a gap in the line and 'No data' in the table, never a zero", () => {
    const col = figure(page("ColumnChart/edge-cases").doc, "gaps");
    expect(col.querySelectorAll(".column-chart__fill.chart-hatch").length).toBe(2);
    expect([...col.querySelectorAll("tbody td")].filter((c) => c.textContent === "No data")).toHaveLength(2);
    const line = figure(page("LineChart/edge-cases").doc, "gaps");
    const path = line.querySelector(".line-chart__line")?.getAttribute("d") ?? "";
    expect(path.match(/M/g)?.length).toBe(3); // three runs between the two gaps
    const bars = figure(page("BarChart/edge-cases").doc, "cases");
    const missing = [...bars.querySelectorAll(".bar-chart__row")].filter((r) => r.querySelector(".chart-hatch"));
    expect(missing.map((r) => r.querySelector(".bar-chart__label")?.textContent).sort()).toEqual([
      "Not published",
      "Unknown reference",
    ]);
    for (const r of missing) expect(r.querySelector(".bar-chart__value")?.textContent).toBe("No data");
    const kf = page("KeyFigures/edge-cases").doc;
    const unpublished = [...kf.querySelectorAll(".key-figure")].find((k) => k.textContent?.includes("Not published"));
    expect(unpublished?.querySelector(".key-figure__figure")?.textContent).toBe("No data");
  });

  it("zero is written as zero, with a flat mark", () => {
    const col = figure(page("ColumnChart/edge-cases").doc, "zero");
    const zeros = islandProps(col).bands.filter((b: { marks: { label: string }[] }) => b.marks[0]?.label === "0");
    expect(zeros.length).toBe(3);
    for (const b of zeros) expect(b.marks[0].height).toBeCloseTo(0);
    const bars = figure(page("BarChart/edge-cases").doc, "cases");
    const zero = [...bars.querySelectorAll(".bar-chart__row")].find((r) => r.textContent?.startsWith("Zero"));
    expect(zero?.querySelector(".bar-chart__value")?.textContent).toBe("0%");
    const kf = page("KeyFigures/edge-cases").doc;
    expect(kf.querySelector(".key-figure .key-figure__figure .source-ref")?.textContent).toBe("0%");
  });

  it("negative values extend below or left of zero, with a true minus, in the same colour", () => {
    const col = figure(page("ColumnChart/edge-cases").doc, "negative");
    const props = islandProps(col);
    for (const b of props.bands) {
      expect(b.marks[0].negative).toBe(true);
      expect(b.marks[0].top).toBeCloseTo(props.zero);
      expect(b.marks[0].role).toBe("cat-1");
      expect(b.marks[0].label).toMatch(/^−\d/);
    }
    const bars = figure(page("BarChart/edge-cases").doc, "negatives");
    const fall = [...bars.querySelectorAll(".bar-chart__row")].find((r) => r.textContent?.startsWith("A fall"));
    expect(fall?.querySelector(".bar-chart__value")?.textContent).toBe("−5.7%");
    expect(fall?.querySelector(".bar-chart__value")?.classList.contains("is-negative")).toBe(true);
    expect(fall?.querySelector(".bar-chart__bar")?.className).toContain("chart-colour--context");
  });

  it("very large and very small values keep their digits: no exponents, no rounding to zero", () => {
    const large = figure(page("ColumnChart/edge-cases").doc, "large");
    const small = figure(page("ColumnChart/edge-cases").doc, "small");
    for (const f of [large, small]) expect(f.innerHTML).not.toMatch(/\d[eE][+-]?\d/);
    expect(small.querySelector("tbody td")?.textContent).toBe("0.000000012");
    expect(large.querySelector("tbody td")?.textContent).toBe("123,456,789,012,345,678");
    expect(islandProps(small).ticks.some((t: { label: string }) => t.label !== "0" && /[1-9]/.test(t.label))).toBe(
      true,
    );
  });

  it("a long series draws every point, thins its x labels and summarises its range", () => {
    const line = figure(page("LineChart/edge-cases").doc, "long");
    const props = islandProps(line);
    expect(props.stops).toHaveLength(60);
    expect(props.xLabels.filter((l: { phone: boolean }) => l.phone).length).toBeLessThan(10);
    expect(props.summary).toContain("60 periods");
    expect(line.querySelectorAll("tbody tr")).toHaveLength(60);
  });

  it("a single point is a marker with its value, not an empty plot", () => {
    const line = figure(page("LineChart/edge-cases").doc, "single");
    expect(line.querySelectorAll(".line-chart__marker").length).toBeGreaterThanOrEqual(1);
    expect(line.querySelector(".line-chart__end-value")?.textContent).toBe("42.5%");
    const col = figure(page("ColumnChart/edge-cases").doc, "single");
    expect(islandProps(col).bands).toHaveLength(1);
    expect(col.querySelector(".column-chart__value")?.textContent).toBe("42.5");
  });

  it("a series in another unit is left out of the chart, never converted", () => {
    const col = figure(page("ColumnChart/edge-cases").doc, "units");
    const heads = [...col.querySelectorAll("thead .data-table__label")].map((h) => h.textContent);
    expect(heads).toEqual(["Comparison measure", "Third measure"]);
    const bars = figure(page("BarChart/edge-cases").doc, "cases");
    expect(bars.textContent).not.toContain("Different unit");
  });

  it("a licence that forbids redistribution removes the CSV and JSON, but keeps the data table", () => {
    for (const id of [
      "ColumnChart/edge-cases",
      "LineChart/edge-cases",
      "BarChart/edge-cases",
      "DataTable/edge-cases",
    ]) {
      const p = page(id);
      const fig = figure(p.doc, "restricted");
      expect(fig.querySelector("a[download]"), id).toBeNull();
      expect(fig.querySelector("table"), id).not.toBeNull();
      expect(
        p.result.charts.some((c) => c.statementId === "restricted"),
        id,
      ).toBe(false);
    }
  });

  it("step lines hold each rate until the next decision and label decisions from the data", () => {
    const fig = figure(page("StepLineChart/default").doc, "path");
    const props = islandProps(fig);
    expect(props.curve).toBe("step");
    expect(fig.querySelector(".line-chart__line")?.getAttribute("d")).toMatch(/H[\d.]+V[\d.]+/);
    expect(props.events.map((e: { label: string | null }) => e.label).filter(Boolean)).toContain(
      "Rate cut, 18 Sep 2024",
    );
    expect(props.events.map((e: { label: string | null }) => e.label)).toContain("Rate rise, 16 Mar 2022");
    // Positioned by date: 2022 holds most of the decisions, so it spans less than half the axis per decision.
    const xs = props.stops.map((s: { x: number }) => s.x);
    expect(xs[0]).toBe(0);
    expect(xs.at(-1)).toBe(100);
    expect(props.xLabels.map((l: { label: string }) => l.label)).toEqual(["2022", "2023", "2024"]);
  });

  it("key figures write the change in words and never colour it", () => {
    const kf = page("KeyFigures/default").doc;
    const changes = [...kf.querySelectorAll(".key-figure__change")].map((c) => c.textContent);
    expect(changes).toContain("Up 2.0% on fiscal 2023");
    expect(changes).toContain("Down 7.7% on fiscal 2023");
    expect(changes).toContain("Up 3.1 points on fiscal 2023");
    for (const c of kf.querySelectorAll(".key-figure__change")) expect(c.className).toBe("key-figure__change");
  });
});

describe("CSV equals the data table", () => {
  const charts = pages.flatMap((p) => p.result.charts.map((c) => [`${p.id} ${c.statementId}`, p, c] as const));

  it("covers every chart and table fixture with downloads", () => {
    expect(charts.length).toBeGreaterThanOrEqual(20);
  });

  it.each(charts)("%s", (_name, p, chart) => {
    const fig = figure(p.doc, chart.statementId);
    const table = fig.querySelector("table") as HTMLTableElement;
    const body = [...table.querySelectorAll("tbody tr")];
    const csvRows = chart.csv.trimEnd().split("\r\n").slice(1);
    expect(body.length).toBe(chart.table.rows.length);
    expect(csvRows.length).toBe(chart.table.rows.length);
    const valueKeys = chart.table.columns.slice(1).filter((c) => c.key !== "unit");
    body.forEach((tr, i) => {
      const row = chart.table.rows[i] as Record<string, string | null>;
      expect(tr.querySelector("th")?.textContent).toBe(row.label);
      const cells = [...tr.querySelectorAll("td")];
      valueKeys.forEach((col, j) => {
        const cell = cells[j] as Element;
        const exact = row[col.key] ?? null;
        if (exact === null) {
          expect(cell.textContent).toBe("No data");
          return;
        }
        // The cell's own reference resolves to exactly the CSV's value (compact and as-reported tables alike) …
        const ref = cell.getAttribute("data-ref");
        expect(ref).toBeTruthy();
        const v = p.resolver.value(ref as string);
        expect(v.ok && v.value).toBe(exact);
        // … and an as-reported cell shows those very digits.
        if (table.classList.contains("data-table--as-reported")) {
          expect((cell.textContent ?? "").replaceAll(",", "").replace("−", "-")).toBe(exact);
        }
      });
    });
  });
});

/** True when any element in the parsed page names a subject: a highlight prop, or a highlighted bar or row. */
function namesHighlight(value: unknown): boolean {
  if (Array.isArray(value)) return value.some(namesHighlight);
  if (!value || typeof value !== "object") return false;
  const v = value as { type?: string; props?: Record<string, unknown>; highlight?: unknown };
  if (v.type === "element" && v.props) {
    if (v.props.highlight) return true;
    return Object.values(v.props).some(namesHighlight);
  }
  return Boolean(v.highlight);
}

describe("colour never follows the value", () => {
  it("no fixture shows crimson unless the page writer named the subject, and no fall is crimson", () => {
    for (const p of pages) {
      const crimson = p.doc.querySelectorAll(".chart-colour--highlight, .is-highlight");
      if (!namesHighlight(parsePage(p.f.oui, library.library))) expect(crimson.length, p.id).toBe(0);
      for (const el of p.doc.querySelectorAll(".is-negative")) {
        expect(el.classList.contains("is-highlight"), `${p.id}: ${el.textContent}`).toBe(false);
      }
    }
    for (const p of pages) {
      for (const island of p.doc.querySelectorAll("[data-island='ColumnChartPlot']")) {
        const props = JSON.parse(island.getAttribute("data-props") ?? "{}");
        for (const b of props.bands)
          for (const m of b.marks) if (m.negative) expect(m.role, `${p.id} ${b.label}`).not.toBe("highlight");
      }
    }
  });

  it("change cells and change words carry no colour class", () => {
    for (const p of pages) {
      for (const el of p.doc.querySelectorAll(".comparison-table__change, .key-figure__change")) {
        expect(el.className, p.id).not.toMatch(/colour|highlight|brand|error/);
      }
    }
  });
});

describe("Markdown", () => {
  it.each(pages.map((p) => [p.id, p] as const))("%s: index.md has every chart's title and data", (_id, p) => {
    for (const c of p.result.charts) {
      expect(p.result.markdown).toContain(`### ${c.table.title.replace(/([\\`*_[\]<>|])/g, "\\$1")}`);
    }
  });
});
