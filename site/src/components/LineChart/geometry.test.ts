// Builder's sanity checks for line and step-line geometry: positions by date, end labels kept apart, the path
// broken at gaps and held flat between decisions.
import { describe, expect, it } from "vitest";
import { spreadLabels, stopPositions } from "./geometry";
import { linePath } from "./LineChartPlot.client";

describe("stopPositions", () => {
  const slot = (date: string | null, i: number) => ({ ref: `pd_${i}`, label: `P${i}`, date });

  it("spaces periods evenly, or by date for step lines when every period has one", () => {
    const dated = [slot("2022-01-26", 0), slot("2022-03-16", 1), slot("2024-12-18", 2)];
    expect(stopPositions(dated, false)).toEqual([0, 50, 100]);
    const byDate = stopPositions(dated, true);
    expect(byDate[0]).toBe(0);
    expect(byDate[2]).toBe(100);
    expect(byDate[1]).toBeLessThan(10);
  });

  it("falls back to even spacing when a period has no date, and centres a single period", () => {
    expect(stopPositions([slot("2022-01-01", 0), slot(null, 1)], true)).toEqual([0, 100]);
    expect(stopPositions([slot(null, 0)], true)).toEqual([50]);
  });
});

describe("spreadLabels", () => {
  it("keeps labels at least a gap apart and inside the plot, in their order", () => {
    const out = spreadLabels([50, 52, 51, 99], 10);
    const sorted = [...out].sort((a, b) => a - b);
    for (let i = 1; i < sorted.length; i++) {
      expect((sorted[i] as number) - (sorted[i - 1] as number)).toBeGreaterThanOrEqual(10 - 1e-9);
    }
    expect(Math.max(...out)).toBeLessThanOrEqual(100);
    expect(out[0]).toBeLessThan(out[2] as number); // 50 stays above 51
  });

  it("leaves labels that do not collide where they are", () => {
    expect(spreadLabels([10, 60], 10)).toEqual([10, 60]);
  });
});

describe("linePath", () => {
  it("breaks the line at gaps", () => {
    expect(linePath([0, 50, 100], [10, null, 30], "linear")).toBe("M0.0,100.0M1000.0,300.0");
  });

  it("holds each value flat until the next stop for step lines", () => {
    expect(linePath([0, 50, 100], [80, 60, 60], "step")).toBe("M0.0,800.0H500.0V600.0H1000.0V600.0");
  });
});
