// Builder's sanity checks for the shared table model: units written by code, colour by entity (never by value),
// the CSV of exact decimals, and the accessible summary.
import { describe, expect, it } from "vitest";
import {
  colourRoles,
  modelSummary,
  modelToChartTable,
  seriesFell,
  seriesTable,
  type TableModel,
  unitLabel,
  valueFell,
} from "./model";

const usd = { unit: "USD", currency: "USD", scale: "millions" as const };
const pct = { unit: "Percent", currency: null, scale: null };

const model = (rows: number): TableModel => ({
  title: "Title",
  subtitle: "Subtitle",
  head: "Period",
  columns: [{ key: "sr_a", label: "Net sales", ...usd }],
  rows: Array.from({ length: rows }, (_, i) => ({
    key: `pd_${i}`,
    label: `P${i}`,
    highlight: false,
    cells: [{ ref: i === 1 ? null : `dp_${i}`, value: i === 1 ? null : String(1000 * (i + 1)) }],
  })),
  sources: [],
});

describe("unitLabel", () => {
  it.each([
    [usd, "US$ millions"],
    [{ unit: "EUR", currency: "EUR", scale: "billions" as const }, "€ billions"],
    [{ unit: "JPY", currency: "JPY", scale: "millions" as const }, "millions of yen"],
    [{ unit: "CHF", currency: "CHF", scale: "millions" as const }, "CHF millions"],
    [{ unit: "USD", currency: "USD", scale: "units" as const }, "US$"],
    [pct, "%"],
    [{ unit: "pp", currency: null, scale: null }, "percentage points"],
    [{ unit: "Index 2020=100", currency: null, scale: null }, "Index 2020=100"],
    [{ unit: "Persons", currency: null, scale: "thousands" as const }, "Persons, thousands"],
  ])("%o -> %s", (m, expected) => {
    expect(unitLabel(m)).toBe(expected);
  });
});

describe("colourRoles", () => {
  it("gives the named subject crimson, one comparison blue and the rest context", () => {
    expect(colourRoles(["a", "b", "c"], { highlight: "b", comparison: "c" })).toEqual([
      "context",
      "highlight",
      "comparison",
    ]);
  });

  it("never uses crimson without a named subject, whatever the values", () => {
    for (const n of [1, 2, 4, 8, 9]) {
      const keys = Array.from({ length: n }, (_, i) => `s${i}`);
      expect(colourRoles(keys)).not.toContain("highlight");
    }
    expect(colourRoles(["a"], { highlight: "not-here" })).toEqual(["cat-1"]);
  });

  it("assigns categorical slots in the order given, never cycled", () => {
    expect(colourRoles(["a", "b", "c"])).toEqual(["cat-1", "cat-2", "cat-3"]);
    expect(colourRoles(["b", "a"])).toEqual(["cat-1", "cat-2"]);
  });

  it("gives a subject that fell the neutral highlight, never crimson; the others keep their roles", () => {
    expect(colourRoles(["a", "b", "c"], { highlight: "b", comparison: "c", fell: true })).toEqual([
      "context",
      "highlight-neutral",
      "comparison",
    ]);
    expect(colourRoles(["a", "b"], { highlight: "z", fell: true })).toEqual(["cat-1", "cat-2"]);
  });
});

describe("what 'fell' means (design/charts.md, 'When the subject fell')", () => {
  it("bars and columns: below zero, or below the comparison value when there is one", () => {
    expect(valueFell("-0.1")).toBe(true);
    expect(valueFell("-8.1")).toBe(true);
    expect(valueFell("0")).toBe(false);
    expect(valueFell("0.00")).toBe(false);
    expect(valueFell("12.5")).toBe(false);
    expect(valueFell(null)).toBe(false);
    expect(valueFell(undefined)).toBe(false);
    expect(valueFell("90", "100")).toBe(true);
    expect(valueFell("100", "100")).toBe(false);
    expect(valueFell("-5", "-10")).toBe(false);
  });

  it("lines: the last value in the shown range below the first, gaps skipped", () => {
    expect(seriesFell(["3.1", "2.9", "2.7"])).toBe(true);
    expect(seriesFell([null, "3.1", "3.5", "2.9", null])).toBe(true);
    expect(seriesFell(["2.7", "3.4", "2.9"])).toBe(false);
    expect(seriesFell(["2.0", "1.0", "2.0"])).toBe(false);
    expect(seriesFell(["2.0"])).toBe(false);
    expect(seriesFell([null, null])).toBe(false);
    expect(seriesFell(["-1", "-3"])).toBe(true);
  });

  it("a highlighted period that fell marks its table row; a rise does not", () => {
    const s = (values: (string | null)[]) => ({
      ok: true as const,
      ref: "sr_a",
      label: "Output",
      unit: "Percent",
      currency: null,
      scale: null,
      sources: [],
      points: values.map((v, i) => ({
        ref: `dp_${i}`,
        value: v,
        period: { ref: `pd_${i}`, label: `P${i}`, period: { start: null, end: null } },
      })),
    });
    const fell = seriesTable([s(["1.0", "-2.0"]) as never], { title: "", subtitle: "", highlight: "pd_1" });
    expect(fell.rows.map((r) => [r.highlight, r.fell])).toEqual([
      [false, false],
      [true, true],
    ]);
    const rose = seriesTable([s(["1.0", "2.0"]) as never], { title: "", subtitle: "", highlight: "pd_1" });
    expect(rose.rows[1]?.fell).toBe(false);
  });
});

describe("modelToChartTable", () => {
  it("keeps exact decimals and nulls, with the unit in each column's heading", () => {
    const t = modelToChartTable(model(3));
    expect(t.columns).toEqual([
      { key: "label", label: "Period" },
      { key: "sr_a", label: "Net sales (US$ millions)" },
    ]);
    expect(t.rows).toEqual([
      { label: "P0", sr_a: "1000" },
      { label: "P1", sr_a: null },
      { label: "P2", sr_a: "3000" },
    ]);
    expect(t.currency).toBe("USD");
    expect(t.scale).toBe("millions");
  });
});

describe("modelSummary", () => {
  it("states every value when there are 12 or fewer, and says No data for a gap", () => {
    const s = modelSummary(model(3));
    expect(s).toBe("Title. Net sales: P0, $1.0bn; P1, no data; P2, $3.0bn.");
  });

  it("gives the count, the endpoints and the extremes for a long series", () => {
    const s = modelSummary(model(40));
    expect(s).toContain("40 periods");
    expect(s).toContain("from P0, $1.0bn to P39, $40.0bn");
    expect(s).toContain("highest $40.0bn in P39");
    expect(s).toContain("lowest $1.0bn in P0");
  });
});
