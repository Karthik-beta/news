// Builder's sanity checks for the shared table model: units written by code, colour by entity (never by value),
// the CSV of exact decimals, and the accessible summary.
import { describe, expect, it } from "vitest";
import { colourRoles, modelSummary, modelToChartTable, type TableModel, unitLabel } from "./model";

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
