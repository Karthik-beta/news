// Component tester's re-test of #8's fix round (DC-1): crimson never marks a highlighted subject that fell
// (Karthik, 16 Nov 2026), with the rules in design/charts.md, "When the subject fell":
//   bar and column: the value is below zero (no chart takes a comparison value yet);
//   line and step line: the last value in the shown range is below the first, gaps skipped;
//   data table: a highlighted period whose every value is below zero, or a highlighted series by the line rule.
// Every case renders a real page through the renderer, with the fixture data plus a few synthetic series for
// the edges the fixtures lack (gaps at both ends, an equal first and last value, all values negative and falling).
// The browser half (computed colours, legend keys, forced colours) is in tests/e2e/components/data-charts-r1.tester.spec.ts.
import { JSDOM } from "jsdom";
import { describe, expect, it } from "vitest";
import { seriesFell, valueFell } from "../../src/components/ChartFigure/model";
import { componentFixtureBundle, componentFixtures } from "../../src/renderer/fixtures/components.vite";
import { fixtureLibrary } from "../../src/renderer/fixtures/library";
import { renderPage } from "../../src/renderer/render";

const library = fixtureLibrary();
const base = componentFixtures.find((f) => f.name === "LineChart" && f.fixture === "edge-cases");
if (!base) throw new Error("no LineChart/edge-cases fixture");

const YEARS = ["pd_fx_2019", "pd_fx_2020", "pd_fx_2021", "pd_fx_2022", "pd_fx_2023", "pd_fx_2024"];

/** Synthetic series (values by year from 2019; null is a gap) and bar points, added to the fixture data. */
const SERIES: Record<string, (string | null)[]> = {
  sr_t_ends_fell: [null, "3.0", "3.4", "2.5", null, null],
  sr_t_ends_rose: [null, null, "2.5", "2.0", "3.0", null],
  sr_t_equal: ["2.0", "1.0", "3.0", "2.0"],
  sr_t_neg_fell: ["-1.0", "-2.0", "-0.5", "-3.0"],
  sr_t_flat: ["1.5", "1.5", "1.5"],
  sr_t_one_of_many: [null, null, "4.0", null, null, null],
};
const BARS: Record<string, string> = {
  dp_t_bar_neg_a: "-1.2",
  dp_t_bar_neg_b: "-4.0",
  dp_t_bar_neg_c: "-0.3",
  dp_t_bar_negzero: "-0.0",
};

type Json = Record<string, unknown>;
function input(oui: string) {
  const b = componentFixtureBundle(base as NonNullable<typeof base>);
  const data = structuredClone(b.data) as Json & { points: Json; series: Json };
  const point = (id: string, value: string, period: string) => {
    data.points[id] = {
      label: "Synthetic",
      value,
      value_text: value,
      unit: "Percent",
      currency: null,
      scale: null,
      period,
      source: "src_fixture_synthetic",
      locator: { kind: "json", series: id },
    };
  };
  for (const [ref, values] of Object.entries(SERIES)) {
    const name = ref.slice(5); // "sr_t_ends_fell" -> "ends_fell"
    data.series[ref] = {
      label: `Synthetic ${name}`,
      unit: "Percent",
      currency: null,
      scale: null,
      points: values.map((v, i) => {
        const period = YEARS[i] as string;
        if (v === null) return { period, point: null };
        const id = `dp_${name}_${i}`;
        point(id, v, period);
        return { period, point: id };
      }),
    };
  }
  for (const [id, v] of Object.entries(BARS)) point(id, v, "pd_fx_2024");
  return { ...b, data, oui };
}

const render = (statements: string) => {
  const oui = `root = Page([c])\nc = ${statements}`;
  const result = renderPage(input(oui), { library });
  const doc = new JSDOM(result.article).window.document;
  const fig = doc.getElementById("c-title")?.closest("figure");
  if (!fig) throw new Error(`no figure rendered for: ${statements}`);
  const props = JSON.parse(fig.querySelector("[data-island]")?.getAttribute("data-props") ?? "null");
  return { fig, props, html: fig.outerHTML };
};

/** Every colour role the figure uses, from its classes and its island props. */
const rolesIn = (r: ReturnType<typeof render>) => {
  const fromClasses = [...r.html.matchAll(/chart-colour--([a-z0-9-]+)/g)].map((m) => m[1]);
  const fromProps = [...JSON.stringify(r.props ?? {}).matchAll(/"role":"([a-z0-9-]+)"/g)].map((m) => m[1]);
  return new Set([...fromClasses, ...fromProps]);
};
const highlightedRow = (fig: Element) => fig.querySelector(".data-table__row.is-highlight");

/** The subject is drawn in crimson: the highlight role and no neutral highlight anywhere. */
function expectCrimson(r: ReturnType<typeof render>) {
  const roles = rolesIn(r);
  expect(roles.has("highlight"), "crimson role present").toBe(true);
  expect(roles.has("highlight-neutral"), "no neutral highlight").toBe(false);
  expect(r.fig.querySelector(".is-highlight-neutral"), "no neutral-highlight label").toBeNull();
  expect(r.fig.querySelector(".data-table__row.is-fell"), "no fallen table row").toBeNull();
}
/** The subject is drawn in the neutral highlight: never the crimson role, nor a crimson label. */
function expectNeutral(r: ReturnType<typeof render>) {
  const roles = rolesIn(r);
  expect(roles.has("highlight-neutral"), "neutral highlight present").toBe(true);
  expect(roles.has("highlight"), "no crimson role").toBe(false);
  expect(
    r.fig.querySelector(".is-highlight:not(.data-table__row):not(.bar-chart__row):not(.column-chart__band)"),
  ).toBeNull();
}

describe("DC-1 rules, read straight from the functions", () => {
  it.each([
    ["0", false],
    ["0.0", false],
    ["-0", false],
    ["-0.0", false],
    ["-0.01", true],
    ["-1e-9", true],
    ["0.000000012", false],
    ["-123456789012345678", true],
    ["not a number", false],
    ["", false],
  ])("valueFell(%j) is %s", (v, fell) => {
    expect(valueFell(v)).toBe(fell);
  });

  it.each([
    [[null, "3", "2", null], true],
    [[null, null, "2", "3"], false],
    [["2", "1", "3", "2"], false],
    [["5"], false],
    [[null, "5", null], false],
    [[], false],
    [["-1", "-3"], true],
    [["-3", "-1"], false],
    [["0", "-0"], false],
    [["1.10", "1.1"], false],
  ] as [(string | null)[], boolean][])("seriesFell(%j) is %s", (values, fell) => {
    expect(seriesFell(values)).toBe(fell);
  });
});

describe("bar chart: the highlighted bar fell when its value is below zero", () => {
  const bars = (highlighted: string, others: string[] = ["dp_fx_bar_positive"]) =>
    render(
      `BarChart("t", "Synthetic values, %", [${[highlighted, ...others]
        .map((v, i) => `{label: "B${i}", value: "${v}"${i === 0 ? ", highlight: true" : ""}}`)
        .join(", ")}])`,
    );

  it("a highlighted value of exactly 0 is crimson, and so is -0.0", () => {
    expectCrimson(bars("dp_fx_bar_zero"));
    expectCrimson(bars("dp_t_bar_negzero"));
  });

  it("a highlighted negative bar is the neutral highlight, its label bold in it, its table row marked", () => {
    const r = bars("dp_fx_bar_negative");
    expectNeutral(r);
    const row = r.props.rows.find((x: { highlight: boolean }) => x.highlight);
    expect(row.role).toBe("highlight-neutral");
    // The island is rendered on the server too: the label is bold in the neutral highlight, never crimson.
    expect(r.fig.querySelectorAll(".bar-chart__value.is-highlight-neutral")).toHaveLength(1);
    expect(r.fig.querySelectorAll(".bar-chart__value.is-highlight")).toHaveLength(0);
    expect(highlightedRow(r.fig)?.classList.contains("is-fell")).toBe(true);
  });

  it("all bars negative: only the highlighted one is neutral, the rest context", () => {
    const r = bars("dp_t_bar_neg_a", ["dp_t_bar_neg_b", "dp_t_bar_neg_c"]);
    expect(r.props.rows.map((x: { role: string }) => x.role).sort()).toEqual([
      "context",
      "context",
      "highlight-neutral",
    ]);
  });

  it("a highlighted bar with no value never fell", () => {
    const r = bars("dp_fx_bar_null");
    expect(rolesIn(r).has("highlight-neutral")).toBe(false);
    expect(r.fig.querySelector(".data-table__row.is-fell")).toBeNull();
  });
});

describe("column chart: the highlighted column fell when its value is below zero", () => {
  const col = (series: string, period: string) => render(`ColumnChart("t", "Change, %", ["${series}"], "${period}")`);

  it("a highlighted zero is crimson", () => expectCrimson(col("sr_fx_zero", "pd_fx_2019")));
  it("a highlighted surplus among deficits is crimson", () => expectCrimson(col("sr_fx_mixed", "pd_fx_2023")));
  it("all values negative: the highlighted column is neutral, its label too, its table row marked", () => {
    const r = col("sr_fx_negative", "pd_fx_2022");
    expectNeutral(r);
    const band = r.props.bands.find((b: { highlight: boolean }) => b.highlight);
    expect(band.marks.map((m: { role: string }) => m.role)).toEqual(["highlight-neutral"]);
    const others = r.props.bands.filter((b: { highlight: boolean }) => !b.highlight);
    for (const b of others) expect(b.marks.map((m: { role: string }) => m.role)).toEqual(["context"]);
    expect(r.html).toMatch(/column-chart__value[^"]*is-highlight-neutral/);
    expect(highlightedRow(r.fig)?.classList.contains("is-fell")).toBe(true);
  });
  it("a highlighted gap never fell", () => {
    const r = col("sr_fx_gaps", "pd_fx_2020");
    expect(rolesIn(r).has("highlight-neutral")).toBe(false);
    expect(r.fig.querySelector(".data-table__row.is-fell")).toBeNull();
  });
});

describe.each(["LineChart", "StepLineChart"] as const)("%s: fell when the last value is below the first", (kind) => {
  const line = (series: string[], highlight: string) =>
    kind === "LineChart"
      ? render(`LineChart("t", "Synthetic measures, %", [${series.map((s) => `"${s}"`).join(", ")}], "${highlight}")`)
      : render(
          `StepLineChart("t", "Synthetic measures, %", [${series.map((s) => `"${s}"`).join(", ")}], [], "${highlight}")`,
        );

  it("gaps at both ends are skipped: 3.0 to 2.5 fell", () => expectNeutral(line(["sr_t_ends_fell"], "sr_t_ends_fell")));
  it("gaps at both ends are skipped: 2.5 to 3.0 rose", () => expectCrimson(line(["sr_t_ends_rose"], "sr_t_ends_rose")));
  it("an equal first and last value did not fall", () => expectCrimson(line(["sr_t_equal"], "sr_t_equal")));
  it("a flat line did not fall", () => expectCrimson(line(["sr_t_flat"], "sr_t_flat")));
  it("a single point did not fall", () => expectCrimson(line(["sr_fx_single"], "sr_fx_single")));
  it("one value among gaps did not fall", () => expectCrimson(line(["sr_t_one_of_many"], "sr_t_one_of_many")));
  it("all values negative and rising is crimson", () => expectCrimson(line(["sr_fx_negative"], "sr_fx_negative")));
  it("all values negative and falling is neutral", () => expectNeutral(line(["sr_t_neg_fell"], "sr_t_neg_fell")));
  it("a real fall: consumer prices 3.1% to 2.9% over 2024", () =>
    expectNeutral(line(["sr_us_cpi_2024"], "sr_us_cpi_2024")));

  it("only the subject changes: context and comparison keep their roles, the legend keys follow", () => {
    // Step lines take one or two series; line charts up to four.
    const series = ["sr_fx_other", "sr_t_ends_fell", "sr_fx_third"].slice(0, kind === "LineChart" ? 3 : 2);
    const r = line(series, "sr_t_ends_fell");
    expectNeutral(r);
    const expected = ["context", "highlight-neutral", "context"].slice(0, series.length);
    expect(r.props.series.map((s: { role: string }) => s.role)).toEqual(expected);
    const keys = [...r.fig.querySelectorAll(".chart-legend .chart-key")].map((k) =>
      [...k.classList].find((c) => c.startsWith("chart-colour--")),
    );
    expect(keys).toEqual(expected.map((role) => `chart-colour--${role}`));
    const legend = [...r.fig.querySelectorAll(".chart-legend__item")].map((i) => i.textContent?.trim());
    expect(legend).toEqual(["Comparison measure", "Synthetic ends_fell", "Third measure"].slice(0, series.length));
    const subject = r.props.series.find((s: { role: string }) => s.role === "highlight-neutral");
    expect(subject.end.highlight, "its end label is bold in the subject's colour").toBe(true);
    expect(subject.end.name).toBe("Synthetic ends_fell");
    expect(subject.end.value).toBe("2.5%");
    expect(subject.thin, "the subject keeps the highlight's weight").toBe(false);
  });
});

describe("line chart with a comparison: the comparison stays blue when the subject fell", () => {
  it("comparison and neutral highlight together", () => {
    const r = render(
      `LineChart("t", "Synthetic measures, %", ["sr_t_ends_fell", "sr_fx_other", "sr_fx_third"], "sr_t_ends_fell", "sr_fx_other")`,
    );
    expect(r.props.series.map((s: { role: string }) => s.role)).toEqual(["highlight-neutral", "comparison", "context"]);
  });
  it("a comparison that fell is not affected (only the subject is judged)", () => {
    const r = render(
      `LineChart("t", "Synthetic measures, %", ["sr_t_ends_rose", "sr_us_cpi_2024"], "sr_t_ends_rose", "sr_us_cpi_2024")`,
    );
    expect(r.props.series.map((s: { role: string }) => s.role)).toEqual(["highlight", "comparison"]);
  });
});

describe("data table: the highlighted row's bar", () => {
  const table = (series: string[], highlight: string, rows?: "series") =>
    render(
      `DataTable("t", "Synthetic measures, %", [${series.map((s) => `"${s}"`).join(", ")}], "${highlight}"${rows ? `, "${rows}"` : ""})`,
    );
  const fell = (r: ReturnType<typeof render>) => {
    const row = highlightedRow(r.fig);
    expect(row, "a highlighted row").not.toBeNull();
    return row?.classList.contains("is-fell");
  };

  it("a period with every value below zero is marked", () => {
    expect(fell(table(["sr_fx_negative", "sr_fx_gaps"], "pd_fx_2023"))).toBe(true);
  });
  it("a period with mixed signs is not", () => {
    expect(fell(table(["sr_fx_negative", "sr_fx_gaps"], "pd_fx_2024"))).toBe(false);
  });
  it("a period with a zero and a negative is not (zero is not below zero)", () => {
    expect(fell(table(["sr_fx_zero", "sr_fx_negative"], "pd_fx_2021"))).toBe(false);
  });
  it("a gap is skipped: the remaining negatives mark it", () => {
    expect(fell(table(["sr_fx_gaps", "sr_fx_negative"], "pd_fx_2020"))).toBe(true);
  });
  it("a period with no values is not", () => {
    expect(fell(table(["sr_t_ends_fell"], "pd_fx_2019"))).toBe(false);
  });
  it("rows as series: the line rule, gaps at the ends skipped", () => {
    expect(fell(table(["sr_t_ends_fell", "sr_fx_other"], "sr_t_ends_fell", "series"))).toBe(true);
    expect(fell(table(["sr_t_ends_rose", "sr_fx_other"], "sr_t_ends_rose", "series"))).toBe(false);
    expect(fell(table(["sr_t_equal", "sr_fx_other"], "sr_t_equal", "series"))).toBe(false);
    expect(fell(table(["sr_fx_negative", "sr_fx_other"], "sr_fx_negative", "series"))).toBe(false);
  });
  it("only the highlighted row is ever marked", () => {
    const r = table(["sr_fx_negative"], "pd_fx_2022");
    expect(r.fig.querySelectorAll(".data-table__row.is-fell")).toHaveLength(1);
    expect(r.fig.querySelectorAll(".data-table__row.is-fell:not(.is-highlight)")).toHaveLength(0);
  });
});
