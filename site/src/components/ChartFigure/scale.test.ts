// Builder's sanity checks for the chart geometry (the formal tests are the component tester's): round-number
// axes with 4 to 6 gridlines that include zero for bars, bare labels written from exact decimals, and x labels
// thinned rather than rotated.
import { describe, expect, it } from "vitest";
import { axisWidth, bareNumber, displayExponent, largestMagnitude, niceScale, xLabelPlan } from "./scale";

const usdMillions = { unit: "USD", currency: "USD", scale: "millions" as const };
const percent = { unit: "Percent", currency: null, scale: null };

describe("niceScale", () => {
  const ranges: [string, number[]][] = [
    ["Apple sales, US$ m", [274515, 365817, 394328, 383285, 391035]],
    ["inflation, %", [3.1, 3.2, 3.5, 2.4, 2.9]],
    ["all negative", [-5.7, -3.2, -1.1, -0.4]],
    ["mixed sign", [-3132.5, 1695.2, -984.4]],
    ["very small", [0.000000012, 0.000000018, 0.000000009]],
    ["very large", [1.2e17, 1.5e17]],
    ["one value", [42.5]],
    ["all zero", [0, 0, 0]],
  ];

  it.each(ranges)("%s: 3 to 6 round gridlines that contain the data and zero", (_name, values) => {
    const s = niceScale(values, percent);
    expect(s.ticks.length).toBeGreaterThanOrEqual(3);
    expect(s.ticks.length).toBeLessThanOrEqual(6);
    expect(s.min).toBeLessThanOrEqual(Math.min(0, ...values));
    expect(s.max).toBeGreaterThanOrEqual(Math.max(0, ...values));
    // Exactly one baseline, at zero.
    const base = s.ticks.filter((t) => t.baseline);
    expect(base).toHaveLength(1);
    expect(Number(base[0]?.value)).toBe(0);
    // Ticks are exact decimals of 1, 2 or 5 × 10^n steps: no float noise in any label.
    for (const t of s.ticks) {
      expect(t.value).toMatch(/^-?\d+(\.\d+)?$/);
      expect(t.label).not.toMatch(/\d[eE][+-]?\d/);
      expect(Number(t.label.replaceAll(",", "").replace("−", "-").replace("%", ""))).toBe(Number(t.value));
    }
    // Positions run from the top (max) to the bottom (min).
    expect(s.y(s.max)).toBeCloseTo(0);
    expect(s.y(s.min)).toBeCloseTo(100);
  });

  it("starts near the data when zero is not wanted, with the bottom line as the baseline", () => {
    const s = niceScale([2.4, 3.5], percent, { includeZero: false });
    expect(s.min).toBeGreaterThan(0);
    expect(s.ticks[0]?.baseline).toBe(true);
    expect(s.ticks.slice(1).some((t) => t.baseline)).toBe(false);
  });

  it("labels a money axis as bare numbers in the display unit, and puts % on the top percentage tick only", () => {
    const money = niceScale([274515, 391035], usdMillions, { displayExponent: 9 });
    expect(money.ticks.map((t) => t.label)).toEqual(["0", "100", "200", "300", "400"]);
    const pct = niceScale([3.1, 3.5], percent);
    const labels = pct.ticks.map((t) => t.label);
    expect(labels.at(-1)).toMatch(/%$/);
    expect(labels.slice(0, -1).every((l) => !l.endsWith("%"))).toBe(true);
  });

  it("writes negative ticks with a true minus", () => {
    const s = niceScale([-5.7, -0.4], percent);
    expect(s.ticks.some((t) => t.label.startsWith("−"))).toBe(true);
    expect(s.ticks.some((t) => t.label.startsWith("-"))).toBe(false);
  });
});

describe("bare numbers for value labels", () => {
  it("writes money in the compact unit with one decimal, never converting", () => {
    expect(displayExponent(["274515", "391035"], usdMillions)).toBe(9);
    expect(bareNumber("391035", usdMillions, 9)).toBe("391.0");
    expect(bareNumber("53768", usdMillions, 9)).toBe("53.8");
  });

  it("keeps percentages as published, one decimal at most", () => {
    expect(displayExponent(["2.4"], percent)).toBe(0);
    expect(bareNumber("2.4", percent, 0)).toBe("2.4");
    expect(bareNumber("66.0", percent, 0)).toBe("66.0");
    expect(bareNumber("-5.7", percent, 0)).toBe("−5.7");
    expect(bareNumber("0", percent, 0)).toBe("0");
  });

  it("finds the value with the largest magnitude as written", () => {
    expect(largestMagnitude(["-3132.5", "1695.2"])).toBe("-3132.5");
    expect(largestMagnitude([])).toBeNull();
  });
});

describe("x labels", () => {
  it("always labels the first and last period and thins the rest on phones", () => {
    const labels = Array.from({ length: 60 }, (_, i) => `M${i}`);
    const xs = labels.map((_, i) => (i / 59) * 100);
    const plan = xLabelPlan(labels, xs);
    expect(plan[0]?.phone && plan[0]?.wide).toBe(true);
    expect(plan[59]?.phone && plan[59]?.wide).toBe(true);
    const phone = plan.filter((p) => p.phone).length;
    const wide = plan.filter((p) => p.wide).length;
    expect(phone).toBeLessThanOrEqual(wide);
    expect(phone).toBeLessThanOrEqual(10);
  });

  it("labels every period when they fit", () => {
    const plan = xLabelPlan(["2020", "2021", "2022", "2023", "2024"], [0, 25, 50, 75, 100]);
    expect(plan.every((p) => p.wide && p.phone)).toBe(true);
  });

  it("measures the axis labels in ch", () => {
    expect(axisWidth([{ label: "400" }, { label: "−1,000" }])).toBe("6ch");
  });
});
