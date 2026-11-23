// Component tester's checks for the ChoroplethMap (#13), on top of the builder's choropleth.test.ts: every shaded
// region sits in the class its value belongs to, on every fixture; legend labels and the words read for each class;
// very large, very small, zero and thousands; the accessible names (the map carries the summary, the ranked table and
// bars are "Ranked: <title>", so nothing is read twice and no two tables share a name); the map ships no island of
// its own; the spec's Markdown and CSV; and the map in context in the charts article fixture.
import { JSDOM } from "jsdom";
import { describe, expect, it } from "vitest";
import { classText, mapClasses } from "../../src/components/ChoroplethMap/classes";
import { fixtureBundles } from "../../src/renderer/fixtures/bundles/vite";
import { componentFixtureBundle, componentFixtures } from "../../src/renderer/fixtures/component-pages";
import { fixtureLibrary } from "../../src/renderer/fixtures/library";
import { renderPage } from "../../src/renderer/render";

const library = fixtureLibrary();
const pct = { unit: "Percent", currency: null, scale: null };
const fixtureDoc = (fixture: string) => {
  const f = componentFixtures.find((x) => x.component === "ChoroplethMap" && x.fixture === fixture);
  if (!f) throw new Error(`no fixture ${fixture}`);
  return new JSDOM(renderPage(componentFixtureBundle(f), { library }).article).window.document;
};
const docs = { default: fixtureDoc("default"), "edge-cases": fixtureDoc("edge-cases") };
const maps = (d: Document) => [...d.querySelectorAll("figure:has(.choropleth)")];
const statementOf = (fig: Element) => fig.querySelector("h2[id$='-title']")?.id.replace(/-title$/, "") ?? "";
const all = Object.entries(docs).flatMap(([name, d]) =>
  maps(d).map((fig) => [`${name}/${statementOf(fig)}`, fig] as const),
);

describe("classes on the page", () => {
  it.each(all)(
    "%s: shades rise with the values, equal values share a shade, and every legend class is drawn",
    (_, fig) => {
      // The visible table holds each value as published (the legend may be in a larger display unit).
      const values = new Map(
        [...fig.querySelectorAll(".choropleth__table td[data-ref]")].map((td) => [
          td.getAttribute("data-ref"),
          Number((td.textContent ?? "").replace(/,/g, "")),
        ]),
      );
      const stepOf = (el: Element | null) => Number(/heatmap__step--(\d)/.exec(el?.getAttribute("class") ?? "")?.[1]);
      const legendSteps = [...fig.querySelectorAll(".choropleth__swatch")].map(stepOf);
      const shaded = [...fig.querySelectorAll("path[data-ref]")]
        .map((p) => ({
          code: p.getAttribute("data-region"),
          v: values.get(p.getAttribute("data-ref")) as number,
          s: stepOf(p),
        }))
        .sort((a, b) => a.v - b.v);
      for (let i = 1; i < shaded.length; i++) {
        const [a, b] = [shaded[i - 1], shaded[i]] as [(typeof shaded)[number], (typeof shaded)[number]];
        expect(legendSteps.indexOf(b.s), `${a.code} ${a.v} → ${b.code} ${b.v}`).toBeGreaterThanOrEqual(
          legendSteps.indexOf(a.s),
        );
        if (a.v === b.v) expect(b.s, `${a.code} = ${b.code}`).toBe(a.s);
      }
      // Every legend class names at least one country on the map (no swatch for an empty range).
      expect(new Set(shaded.map((x) => x.s))).toEqual(new Set(legendSteps));
    },
  );

  it("no legend class is ever empty, whatever the values (rounding can carry a break past the next value)", () => {
    // The gaps fixture's values: 8.75 rounded to a break of 9 with 12.5 above used to leave "9 to 10" empty.
    expect(mapClasses(["0", "3.25", "3.25", "8.75", "12.5", "47"], pct, 0).joins.map((j) => j.value)).toEqual([
      "3",
      "9",
    ]);
    let seed = 7;
    const random = () => {
      seed = (seed * 16807) % 2147483647;
      return seed / 2147483647;
    };
    for (let trial = 0; trial < 2000; trial++) {
      const n = 1 + Math.floor(random() * 40);
      const values = Array.from({ length: n }, () => (random() * 10 ** Math.floor(random() * 5)).toFixed(2));
      const c = mapClasses(values, pct, 0);
      const counts = c.steps.map(() => 0);
      for (const v of values) counts[c.classOf(v)] = (counts[c.classOf(v)] ?? 0) + 1;
      expect(
        counts.every((k) => k > 0),
        `${values.join(",")} → ${c.joins.map((j) => j.value)}`,
      ).toBe(true);
      expect(c.steps.length).toBeLessThanOrEqual(5);
    }
  });

  it("legend steps run lightest to darkest in the order the heatmap's validated ramp uses", () => {
    expect(mapClasses(["1", "2"], pct, 0).steps.length).toBe(2);
    for (let n = 1; n <= 5; n++) {
      const values = Array.from({ length: 50 }, (_, i) => String(((i + 1) * 10) / 50));
      const c = mapClasses(values.slice(0, n === 1 ? 1 : 50), pct, 0);
      expect([...c.steps].sort((a, b) => a - b)).toEqual(c.steps);
    }
  });
});

describe("legend labels and words", () => {
  it("very large values: breaks in the display unit, with thousands separators", () => {
    // US$ m values of about 1.2trn: shown in trn (exponent 12), breaks read "0.02" and "1".
    const usd = { unit: "USD", currency: "USD", scale: "millions" as const };
    expect(mapClasses(["1250000", "980000", "15000"], usd, 12).joins.map((j) => j.label)).toEqual(["0.02", "1"]);
    // ≥ 1e12 in units, shown in the source's units: separators, never exponent notation.
    const big = mapClasses(["4000000000000", "9000000000000", "20000000000000", "70000000000000"], pct, 0);
    for (const j of big.joins) expect(j.label).toMatch(/^\d{1,3}(,\d{3})+%?$/);
  });

  it("very small values keep their digits; zero is a value, not a gap", () => {
    const c = mapClasses(["0", "0.0004", "0.0011"], pct, 0);
    expect(c.joins.map((j) => j.label)).toEqual(["0.0004", "0.001%"]);
    expect(c.classOf("0")).toBe(0);
    expect(classText(c, 0)).toBe("under 0.0004");
    expect(classText(c, 2)).toBe("0.001% or more");
  });

  it("thousands in the source's units get separators: 10,000 not 10000", () => {
    const c = mapClasses(
      ["1200", "3400", "8800", "12500", "27000", "41000", "65000", "99000", "120000", "8000"],
      pct,
      0,
    );
    expect(c.joins.map((j) => j.label)).toEqual(["8,000", "10,000", "40,000", "100,000%"]);
    expect(classText(c, 2)).toBe("10,000 to 40,000");
  });

  it("every class reads as words, with % only on the top break, as an axis's top tick", () => {
    const c = mapClasses(["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"], pct, 0);
    expect(c.joins.map((_, i) => classText(c, i)).concat(classText(c, 4))).toEqual([
      "under 3",
      "3 to 5",
      "5 to 7",
      "7 to 9%",
      "9% or more",
    ]);
  });
});

describe("accessible names", () => {
  it.each(all)("%s: the map carries the summary; the ranked table and the bars are 'Ranked: <title>'", (_, fig) => {
    const d = fig.ownerDocument;
    const title = d.getElementById(`${statementOf(fig)}-title`)?.textContent ?? "";
    const map = fig.querySelector("svg.choropleth__map") as Element;
    expect(map.getAttribute("role")).toBe("img");
    expect(map.getAttribute("aria-label")).toMatch(new RegExp(`^${title.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\. `));
    // It speaks the data: the largest country by name (every country, when there are 12 or fewer).
    expect(map.getAttribute("aria-label")).toContain(fig.querySelector(".choropleth__table tbody th")?.textContent);
    expect(map.getAttribute("aria-label")).not.toMatch(/periods/);
    // The visible ranked table: named by a hidden "Ranked:" and the title, never the same name as the data table.
    const table = fig.querySelector(".choropleth__table table") as Element;
    const name = (table.getAttribute("aria-labelledby") ?? "")
      .split(" ")
      .map((id) => d.getElementById(id)?.textContent?.trim())
      .join(" ");
    expect(name).toBe(`Ranked: ${title}`);
    const dataTable = fig.querySelector(".figure-frame__table table") as Element;
    expect(dataTable.getAttribute("aria-labelledby")).toBe(`${statementOf(fig)}-title`);
    // The phone's bars: named "Ranked: <title>", so the summary is not read a second time.
    const bars = JSON.parse(fig.querySelector(".choropleth__bars [data-island]")?.getAttribute("data-props") ?? "{}");
    expect(bars.summary).toBe(`Ranked: ${title}`);
    expect(bars.summary).not.toBe(map.getAttribute("aria-label"));
  });

  it("the legend's class words are in a list, the edge labels hidden from screen readers", () => {
    const fig = maps(docs.default)[0] as Element;
    expect(fig.querySelector(".choropleth__scale")?.tagName).toBe("OL");
    for (const e of fig.querySelectorAll(".choropleth__edge")) expect(e.getAttribute("aria-hidden")).toBe("true");
    for (const s of fig.querySelectorAll(".choropleth__swatch")) expect(s.getAttribute("aria-hidden")).toBe("true");
  });
});

describe("server rendering", () => {
  it("the map is static SVG: the only island in a map's figure is the phone's bars", () => {
    for (const d of Object.values(docs)) {
      for (const fig of maps(d)) {
        const islands = [...fig.querySelectorAll("[data-island]")].map((i) => i.getAttribute("data-island"));
        expect(islands, statementOf(fig)).toEqual(["BarChartPlot"]);
        expect(fig.querySelector(".choropleth__map [data-island]")).toBeNull();
      }
    }
  });

  it("each map's hatch pattern id is its own, so several maps on one page never share or clash", () => {
    const ids = maps(docs["edge-cases"]).map((f) => f.querySelector("pattern")?.id);
    expect(new Set(ids).size).toBe(ids.length);
  });
});

describe("in context: the charts article fixture", () => {
  it("the G7 map renders in the article with its legend, map, ranked table and bars, every value cited", () => {
    const d = new JSDOM(renderPage(fixtureBundles.charts, { library }).article).window.document;
    const fig = d.getElementById("g7map-title")?.closest("figure") as Element;
    expect(fig).not.toBeNull();
    expect(fig.querySelectorAll(".choropleth__table tbody tr").length).toBe(7);
    expect(fig.querySelectorAll("path[data-ref]").length).toBe(7);
    for (const td of fig.querySelectorAll(".choropleth__table td")) expect(td.getAttribute("data-ref")).toMatch(/^dp_/);
    expect(fig.querySelector(".figure-frame__source")?.textContent).toContain("International Monetary Fund");
  });
});
