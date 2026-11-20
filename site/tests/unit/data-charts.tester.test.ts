// Component tester's checks for #8 (data components and Charts I), beyond the builder's data-charts.test.ts:
// Karthik's decisions of 16 November 2026 (rates keep the source's digits in charts and key figures; crimson never
// marks a subject that fell), the KeyFigures 2-4 limit, units written by code from the data, and true minus signs
// through every chart surface (labels, tooltips, summary, CSV). Signed off in each component's SIGNOFF.md.
import { JSDOM } from "jsdom";
import { describe, expect, it } from "vitest";
import { colourRoles, displayValue, formatCell, modelSummary, unitLabel } from "../../src/components/ChartFigure/model";
import { MINUS } from "../../src/format/number";
import { PageData, PageSources } from "../../src/renderer/bundle/schema";
import { PageDataResolver } from "../../src/renderer/data/resolve";
import { componentFixtureBundle, componentFixtures } from "../../src/renderer/fixtures/component-pages";
import { fixtureLibrary } from "../../src/renderer/fixtures/library";
import { RenderError, renderPage } from "../../src/renderer/render";

const library = fixtureLibrary();
const base = componentFixtures.find((f) => f.component === "KeyFigures" && f.fixture === "default");
if (!base) throw new Error("no KeyFigures/default fixture");

const render = (oui: string) => renderPage({ ...componentFixtureBundle(base), oui }, { library });
const doc = (oui: string) => new JSDOM(render(oui).article).window.document;
const fixturePage = (name: string, fixture: string) => {
  const f = componentFixtures.find((x) => x.component === name && x.fixture === fixture);
  if (!f) throw new Error(`no fixture ${name}/${fixture}`);
  const input = componentFixtureBundle(f);
  return {
    doc: new JSDOM(renderPage(input, { library }).article).window.document,
    data: new PageDataResolver(PageData.parse(input.data), PageSources.parse(input.sources)),
  };
};
const figure = (d: Document, statement: string) => {
  const el = d.getElementById(`${statement}-title`)?.closest("figure");
  if (!el) throw new Error(`no figure for ${statement}`);
  return el;
};
const islandProps = (el: Element) => JSON.parse(el.querySelector("[data-island]")?.getAttribute("data-props") ?? "{}");

describe("formatting: rates and points keep the source's own digits (Karthik, 16 Nov 2026)", () => {
  const pct = { unit: "Percent", currency: null, scale: null };
  it.each([
    ["6.50", "6.50%"],
    ["0.25", "0.25%"],
    ["5.00", "5.00%"],
    ["3.0", "3.0%"],
    ["-0.40", `${MINUS}0.40%`],
  ])("chart labels and tooltips write %s as %s", (value, shown) => {
    expect(displayValue(value, pct)).toBe(shown);
  });

  it("points keep their digits too", () => {
    expect(displayValue("1.90", { unit: "percentage points", currency: null, scale: null })).toBe("1.90 points");
  });

  it("a key figure for a policy rate shows 6.50%-style digits, and so does its step-line chart", () => {
    const d = doc(`root = Page([figures, path])
figures = KeyFigures([peak, latest])
peak = KeyFigure("Peak rate", "dp_fed_upper_20230726")
latest = KeyFigure("Latest rate", "dp_fed_upper_20241218")
path = StepLineChart("Rates held for over a year at their peak", "Federal funds target range, upper limit, %", ["sr_fed_funds_upper"])`);
    const figures = [...d.querySelectorAll(".key-figure__figure")].map((e) =>
      e.querySelector(".popover-trigger")?.textContent?.trim(),
    );
    expect(figures).toEqual(["5.50%", "4.50%"]);
    const props = islandProps(figure(d, "path"));
    const shown = JSON.stringify(props);
    for (const v of ["0.25%", "0.50%", "1.00%", "5.50%", "4.50%"]) expect(shown).toContain(`"${v}"`);
    expect(shown).not.toMatch(/"(0\.5|1|5\.5|4\.5)%"/);
  });
});

describe("formatting: units are written by code from the data, never typed", () => {
  const m = (currency: string | null, scale: string | null, unit = "USD") =>
    ({ unit, currency, scale }) as Parameters<typeof unitLabel>[0];
  it.each([
    [m("USD", "millions"), "US$ millions"],
    [m("EUR", "billions", "EUR"), "€ billions"],
    [m("GBP", "millions", "GBP"), "£ millions"],
    [m("INR", "trillions", "INR"), "₹ trillions"],
    [m("KRW", "billions", "KRW"), "₩ billions"],
    [m("TWD", "millions", "TWD"), "NT$ millions"],
    [m("HKD", "millions", "HKD"), "HK$ millions"],
    [m("AUD", "millions", "AUD"), "A$ millions"],
    [m("JPY", "millions", "JPY"), "millions of yen"],
    [m("CHF", "millions", "CHF"), "CHF millions"],
    [m(null, null, "Percent"), "%"],
    [m(null, null, "percentage points"), "percentage points"],
    [m(null, null, "Index 2020=100"), "Index 2020=100"],
  ])("%o heads its column as %s", (measure, label) => {
    expect(unitLabel(measure)).toBe(label);
  });

  it.each([
    ["391035", "USD", "millions", "$391.0bn"],
    ["2100", "INR", "billions", "₹2.1trn"],
    ["-5700", "USD", "millions", `${MINUS}$5.7bn`],
    ["1200", "EUR", "millions", "€1.2bn"],
    ["450", "HKD", "millions", "HK$450m"],
  ])("money %s %s %s is %s in a chart label", (value, currency, scale, shown) => {
    expect(displayValue(value, { unit: currency, currency, scale: scale as "millions" })).toBe(shown);
  });

  it("a gap is 'No data' in every table style, never a zero", () => {
    const pct = { unit: "Percent", currency: null, scale: null };
    expect(formatCell(null, pct, "as-reported")).toBe("No data");
    expect(formatCell(null, pct, "compact")).toBe("No data");
  });

  it("the chart summary uses true minus signs", () => {
    const summary = modelSummary({
      title: "Output fell",
      subtitle: "",
      head: "Year",
      columns: [{ key: "a", label: "Change", unit: "Percent", currency: null, scale: null }],
      rows: [{ key: "r", label: "2024", highlight: false, cells: [{ ref: "dp_x", value: "-1.8" }] }],
      sources: [],
    });
    expect(summary).toContain(`${MINUS}1.8%`);
    expect(summary).not.toContain("-1.8");
  });
});

describe("KeyFigures holds two to four figures (design/components.md)", () => {
  const figures = (n: number) =>
    Array.from({ length: n }, (_, i) => `f${i} = KeyFigure("Figure ${i}", "dp_aapl_sales_iphone_fy2024")`).join("\n");
  const page = (n: number) =>
    `root = Page([row])\nrow = KeyFigures([${Array.from({ length: n }, (_, i) => `f${i}`).join(", ")}])\n${figures(n)}`;

  it.each([2, 3, 4])("%i figures render as one row", (n) => {
    expect(doc(page(n)).querySelectorAll(".key-figure")).toHaveLength(n);
  });

  it.each([1, 5])("%i figures is refused as an invalid page, never rendered as a broken row", (n) => {
    expect(() => render(page(n))).toThrow(RenderError);
  });
});

describe("colour roles: one highlight at most, never chosen by value", () => {
  it("a highlight that names no series highlights nothing", () => {
    expect(colourRoles(["a", "b"], { highlight: "z" })).toEqual(["cat-1", "cat-2"]);
  });
  it("the comparison never takes the highlight's place", () => {
    expect(colourRoles(["a", "b", "c"], { highlight: "a", comparison: "a" })).toEqual([
      "highlight",
      "context",
      "context",
    ]);
  });
  it("exactly one highlight however many series", () => {
    const roles = colourRoles(["a", "b", "c", "d"], { highlight: "c", comparison: "a" });
    expect(roles.filter((r) => r === "highlight")).toHaveLength(1);
  });
});

/**
 * Karthik, 16 Nov 2026: "Crimson never marks a subject that fell. A highlighted subject that fell uses the neutral
 * highlight." No neutral-highlight token exists yet, so the test asks only that the fallen subject is not drawn in
 * the highlight (crimson) role. Fixtures: the `fell` statements in the BarChart, ColumnChart and LineChart edge cases.
 */
describe("crimson never marks a highlighted subject that fell (DC-1)", () => {
  it("a highlighted bar for a fall is not crimson, nor its label", () => {
    const { doc: d } = fixturePage("BarChart", "edge-cases");
    const props = islandProps(figure(d, "fell"));
    const china = props.rows.find((r: { label: string }) => r.label === "Greater China");
    expect(china.negative).toBe(true);
    expect(china.highlight).toBe(true);
    expect(china.role, "the fallen subject's bar").not.toBe("highlight");
    const row = figure(d, "fell").querySelector(`.bar-chart__row:has(.is-negative) .bar-chart__value`);
    expect(row?.classList.contains("is-highlight"), "the fallen subject's label").toBe(false);
  });

  it("a highlighted column for a period that fell is not crimson", () => {
    const { doc: d } = fixturePage("ColumnChart", "edge-cases");
    const props = islandProps(figure(d, "fell"));
    const band = props.bands.find((b: { highlight: boolean }) => b.highlight);
    expect(band, "the fixture highlights a period").toBeTruthy();
    for (const mark of band.marks) expect(mark.role).not.toBe("highlight");
  });

  it("a highlighted line that ended below where it started is not crimson", () => {
    const { doc: d } = fixturePage("LineChart", "edge-cases");
    const el = figure(d, "fell");
    expect(el.querySelectorAll(".chart-colour--highlight")).toHaveLength(0);
    expect(JSON.stringify(islandProps(el))).not.toContain('"role":"highlight"');
  });
});
