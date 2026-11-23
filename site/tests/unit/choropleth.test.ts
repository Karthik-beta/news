// Builder's checks for the ChoroplethMap (#13): the ranked table's rows are the map's data (same references, same
// order as the bars), regions without data are hatched and listed as "No data", and the committed geometry stays
// within its size budget. The shared chart checks (every number cited, CSV equals the table) run for the map in
// data-charts.test.ts. The formal tests and sign-off are the component tester's.
import { readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { gzipSync } from "node:zlib";
import { JSDOM } from "jsdom";
import { describe, expect, it } from "vitest";
import { equalEarth } from "../../scripts/gen-map-geometry";
import { mapClasses, roundOne } from "../../src/components/ChoroplethMap/classes";
import { world } from "../../src/components/ChoroplethMap/geo/world";
import { componentFixtureBundle, componentFixtures } from "../../src/renderer/fixtures/component-pages";
import { fixtureLibrary } from "../../src/renderer/fixtures/library";
import { renderPage } from "../../src/renderer/render";
import { siteRoot } from "../support/tokens";

const library = fixtureLibrary();
const doc = (fixture: string) => {
  const f = componentFixtures.find((x) => x.component === "ChoroplethMap" && x.fixture === fixture);
  if (!f) throw new Error(`no fixture ${fixture}`);
  return new JSDOM(renderPage(componentFixtureBundle(f), { library }).article).window.document;
};
const figure = (d: Document, statement: string) => {
  const el = d.getElementById(`${statement}-title`)?.closest("figure");
  if (!el) throw new Error(`no figure for ${statement}`);
  return el;
};
const maps = [
  ["default", "jobless"],
  ["default", "g7"],
  ["edge-cases", "gaps"],
  ["edge-cases", "one"],
  ["edge-cases", "same"],
  ["edge-cases", "sizes"],
  ["edge-cases", "tiny"],
] as const;

describe("the ranked table matches the map", () => {
  it.each(maps)("%s/%s: table rows, bars and shaded regions are the same data", (fixture, statement) => {
    const fig = figure(doc(fixture), statement);
    const table = fig.querySelector(".choropleth__table table") as Element;
    const rows = [...table.querySelectorAll("tbody tr")].map((tr) => ({
      label: tr.querySelector("th")?.textContent,
      ref: tr.querySelector("td")?.getAttribute("data-ref"),
      missing: tr.querySelector("td")?.classList.contains("is-missing"),
    }));
    const shaded = [...fig.querySelectorAll("path[data-ref]")].map((p) => p.getAttribute("data-ref"));
    // Every shaded region is a row with a value, and every row with a value is shaded (unless its code has no shape).
    const valued = rows.filter((r) => !r.missing).map((r) => r.ref);
    for (const ref of shaded) expect(valued).toContain(ref);
    const codes = new Set([...fig.querySelectorAll("path")].map((p) => p.getAttribute("data-region")));
    const offMap = statement === "gaps" ? 1 : 0; // "XKX" has no shape
    expect(shaded.length).toBe(valued.length - offMap);
    expect(codes.has("XKX")).toBe(false);
    // The table is ranked, largest first, and lists the same rows in the same order as the phone's bars.
    const bars = JSON.parse(fig.querySelector(".choropleth__bars [data-island]")?.getAttribute("data-props") ?? "null");
    expect(bars.rows.map((r: { label: string }) => r.label)).toEqual(rows.map((r) => r.label));
    const csvRows = fig.querySelectorAll(".figure-frame__table tbody tr").length;
    expect(csvRows).toBe(rows.length);
  });

  it("rows run from the largest value to the smallest, missing last", () => {
    const fig = figure(doc("default"), "jobless");
    const cells = [...fig.querySelectorAll(".choropleth__table tbody td")].map((td) => td.textContent ?? "");
    const numbers = cells.filter((c) => c !== "No data").map((c) => Number(c.replace(/,/g, "")));
    expect(numbers).toEqual([...numbers].sort((a, b) => b - a));
    expect(numbers.length).toBeGreaterThan(100);
  });
});

describe("no data", () => {
  it("a named country with no value, or an unknown reference, is hatched and listed as No data", () => {
    const fig = figure(doc("edge-cases"), "gaps");
    for (const [code, name] of [
      ["CHL", "Chile"],
      ["PER", "Peru"],
    ]) {
      expect(fig.querySelector(`path[data-region="${code}"]`)?.classList.contains("is-missing"), code).toBe(true);
      expect(fig.querySelector(`path[data-region="${code}"]`)?.getAttribute("fill")).toMatch(/^url\(#hatch-/);
      const row = [...fig.querySelectorAll(".choropleth__table tbody tr")].find(
        (r) => r.querySelector("th")?.textContent === name,
      );
      expect(row?.querySelector("td")?.textContent, name).toBe("No data");
    }
    // A value in another unit is left out, never converted: Colombia is hatched and not in the table.
    expect(fig.querySelector('path[data-region="COL"]')?.classList.contains("is-missing")).toBe(true);
    expect(fig.querySelector(".choropleth__table")?.textContent).not.toContain("Colombia");
    // The legend says so, with the hatched key.
    const legend = fig.querySelector(".choropleth__legend") as Element;
    expect(legend.textContent).toContain("No data");
    expect(legend.querySelector(".chart-hatch")).not.toBeNull();
    // The hatch pattern the paths point at is in the same SVG.
    const id = fig.querySelector("path.is-missing")?.getAttribute("fill")?.slice(5, -1);
    expect(fig.querySelector(`svg pattern#${id}`)).not.toBeNull();
  });

  it("nothing published, or every reference unknown: the frame says No data and draws no map", () => {
    const d = doc("edge-cases");
    for (const s of ["allnull", "unknown"]) {
      const fig = figure(d, s);
      expect(fig.querySelector("svg"), s).toBeNull();
      expect(fig.querySelector(".chart-figure__missing")?.textContent, s).toBe("No data");
    }
  });
});

describe("legend and classes", () => {
  const pct = { unit: "Percent", currency: null, scale: null };

  it("five classes on the real data, each used, the breaks under the joins and read out in words", () => {
    const fig = figure(doc("default"), "jobless");
    const classes = [...fig.querySelectorAll(".choropleth__class")];
    expect(classes.length).toBe(5);
    const steps = new Set([...fig.querySelectorAll("path[data-ref]")].map((p) => p.getAttribute("class")));
    expect(steps.size).toBe(5);
    expect(classes.map((c) => c.querySelector(".visually-hidden")?.textContent)).toEqual([
      "under 3",
      "3 to 5",
      "5 to 6",
      "6 to 8%",
      "8% or more",
    ]);
    expect(fig.querySelectorAll(".choropleth__edge--join").length).toBe(4);
  });

  it("breaks are round numbers near the quintiles; a value on a break goes up a class", () => {
    const c = mapClasses(["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"], pct, 0);
    expect(c.joins.map((j) => j.value)).toEqual(["3", "5", "7", "9"]);
    expect(c.steps).toEqual([1, 2, 3, 4, 5]);
    expect(["2.9", "3", "9", "10"].map(c.classOf)).toEqual([0, 1, 4, 4]);
    expect(roundOne(0.00043)).toBe("0.0004");
    expect(roundOne(1_250_000)).toBe("1000000");
    expect(roundOne(15)).toBe("20");
  });

  it("every value equal, or one value: one class, labelled with the value as published", () => {
    const fig = figure(doc("edge-cases"), "same");
    expect(fig.querySelectorAll(".choropleth__class").length).toBe(1);
    expect(fig.querySelector(".choropleth__edge")?.textContent).toBe("5.0%");
    const c = mapClasses(["12.5"], pct, 0);
    expect([c.joins, c.steps, c.only]).toEqual([[], [3], "12.5%"]);
  });

  it("breaks in a larger display unit drop the zeros a shift adds", () => {
    const usd = { unit: "USD", currency: "USD", scale: "millions" as const };
    const c = mapClasses(["1250000", "980000", "15000"], usd, 12);
    expect(c.joins.map((j) => j.label)).toEqual(["0.02", "1"]);
  });
});

describe("geometry", () => {
  const file = join(siteRoot, "src/components/ChoroplethMap/geo/world.ts");

  it("stays within its size budget: 40 KB, 15 KB gzip", () => {
    expect(statSync(file).size).toBeLessThanOrEqual(40 * 1024);
    expect(gzipSync(readFileSync(file), { level: 9 }).length).toBeLessThanOrEqual(15 * 1024);
  });

  it("names its source and licence, and holds one closed path per country, Antarctica left out", () => {
    expect(world.source).toMatch(/Natural Earth/);
    expect(world.licence).toMatch(/Public domain/);
    const codes = Object.keys(world.regions);
    expect(codes.length).toBeGreaterThan(200);
    expect(codes).not.toContain("ATA");
    for (const c of ["USA", "FRA", "NOR", "SGP", "LUX", "KOS"]) expect(codes, c).toContain(c);
    for (const d of Object.values(world.regions) as string[]) {
      expect(d).toMatch(/^M[\d-][\s\S]*z$/);
      // A relative line command with no coordinates is invalid SVG (the browser drops the whole path).
      expect(d).not.toMatch(/l(z|M)/);
    }
  });

  it("Equal Earth: the equator and the central meridian at the origin, symmetric, poles at the published extent", () => {
    expect(equalEarth(0, 0)).toEqual([0, 0]);
    const [x, y] = equalEarth(180, 0);
    expect(equalEarth(-180, 0)[0]).toBeCloseTo(-x, 12);
    expect(y).toBe(0);
    // Šavrič et al. (2018): the map is about 2.05 times as wide as it is tall.
    expect((2 * x) / (2 * equalEarth(0, 90)[1])).toBeCloseTo(2.05, 2);
  });
});
