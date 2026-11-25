import { describe, expect, it } from "vitest";
import {
  formatAbout,
  formatAxisMonth,
  formatChange,
  formatCompact,
  formatDate,
  formatDateWithWeekday,
  formatMoney,
  formatMonthYear,
  formatNumber,
  formatPercent,
  formatPeriodLabel,
  formatPoints,
  formatQuantity,
  formatRange,
  formatShortDate,
  formatSigned,
  MINUS,
  parseIsoDate,
  unitKind,
  useTrueMinus,
} from ".";
import { parseDecimal, roundDecimal, shiftDecimal } from "./decimal";

// Examples come from design/content-style.md wherever it gives one.
const M = MINUS;

describe("decimal helpers", () => {
  it("round half away from zero, exactly", () => {
    const r = (v: string, p: number) => roundDecimal(parseDecimal(v), p);
    expect(r("2.45", 1)).toEqual({ negative: false, int: "2", frac: "5" });
    expect(r("-2.45", 1)).toEqual({ negative: true, int: "2", frac: "5" });
    expect(r("9.96", 1)).toEqual({ negative: false, int: "10", frac: "0" });
    expect(r("0.05", 1)).toEqual({ negative: false, int: "0", frac: "1" });
    expect(r("1.2", 3)).toEqual({ negative: false, int: "1", frac: "200" });
  });

  it("move the point without losing digits (no floats: 0.1 + 0.2 style errors cannot happen)", () => {
    expect(shiftDecimal(parseDecimal("391035"), -3)).toEqual({ negative: false, int: "391", frac: "035" });
    expect(shiftDecimal(parseDecimal("0.000001"), 6)).toEqual({ negative: false, int: "0000001", frac: "" });
    expect(shiftDecimal(parseDecimal("12.5"), -4)).toEqual({ negative: false, int: "0", frac: "00125" });
  });

  it("reject anything that is not a plain decimal string", () => {
    for (const bad of ["1e6", "1,234", " 1", "01", "+1", "1.", ".5", "NaN", "", "−1"]) {
      expect(() => parseDecimal(bad), bad).toThrow(TypeError);
    }
  });
});

describe("numbers", () => {
  it("use comma thousands separators and a point for decimals", () => {
    expect(formatNumber("167045")).toBe("167,045");
    expect(formatNumber("46.2")).toBe("46.2");
    expect(formatNumber("1234567.891")).toBe("1,234,567.891");
    expect(formatNumber("999")).toBe("999");
    expect(formatNumber("1000")).toBe("1,000");
  });

  it("write negatives with the true minus sign U+2212, never a hyphen", () => {
    expect(formatNumber("-5.7")).toBe(`${M}5.7`);
    expect(formatNumber("-1234")).toBe(`${M}1,234`);
    expect(formatNumber("-5.7")).not.toContain("-");
    expect(M).toBe("−");
  });

  it("never sign a zero, including one that rounds to zero", () => {
    expect(formatNumber("0")).toBe("0");
    expect(formatNumber("-0.04", { decimals: 1 })).toBe("0.0");
  });

  it("never add precision the source does not have", () => {
    expect(formatNumber("2", { decimals: 1 })).toBe("2");
    expect(formatNumber("2.0", { decimals: 1 })).toBe("2.0");
    expect(formatNumber("2.46", { decimals: 1 })).toBe("2.5");
  });

  it("write m, bn and trn with no space, one decimal place by default", () => {
    expect(formatCompact("391035", { scale: "millions" })).toBe("391.0bn");
    expect(formatCompact("65600000000")).toBe("65.6bn");
    expect(formatCompact("1200000000000")).toBe("1.2trn");
    expect(formatCompact("420000000")).toBe("420.0m");
    expect(formatCompact("420", { scale: "millions", decimals: 0 })).toBe("420m");
    expect(formatCompact("-5700", { scale: "millions" })).toBe(`${M}5.7bn`);
  });

  it("keep the source's precision when it is coarser than one decimal place", () => {
    expect(formatCompact("2", { scale: "billions" })).toBe("2bn");
    expect(formatCompact("391035000000", { sourceDecimals: -6 })).toBe("391.0bn");
    expect(formatCompact("12", { scale: "millions" })).toBe("12m");
    expect(formatCompact("12.0", { scale: "millions" })).toBe("12.0m");
  });

  it("move to the next suffix when rounding reaches a thousand", () => {
    expect(formatCompact("999960", { scale: "millions" })).toBe("1.0trn");
    expect(formatCompact("999949", { scale: "millions" })).toBe("999.9bn");
  });

  it("keep every digit under a million", () => {
    expect(formatCompact("167045")).toBe("167,045");
    expect(formatCompact("167.045", { scale: "thousands" })).toBe("167,045");
    expect(formatCompact("0.000012")).toBe("0.000012");
  });

  it("handle very large and very small values exactly", () => {
    expect(formatCompact("123456789012345678901234567890")).toBe("123,456,789,012,345,678.9trn");
    expect(formatNumber("0.000000000000000001")).toBe("0.000000000000000001");
  });

  it("write ranges with an en dash and no spaces, and approximations with About", () => {
    expect(formatRange("2020", "2024")).toBe("2020–2024");
    expect(formatRange("4.50", formatPercent("4.75", { decimals: 2 }))).toBe("4.50–4.75%");
    expect(formatAbout(formatNumber("3000"))).toBe("About 3,000");
  });

  it("can fix hyphen minus signs in text that arrives formatted", () => {
    expect(useTrueMinus("fell to -5.7% from -3")).toBe(`fell to ${M}5.7% from ${M}3`);
    expect(useTrueMinus("2020-2024 and COVID-19")).toBe("2020-2024 and COVID-19");
  });
});

describe("currencies", () => {
  const cases: [string, string, string, string][] = [
    // value, currency, scale, expected compact
    ["391035", "USD", "millions", "$391.0bn"],
    ["1200", "EUR", "billions", "€1.2trn"],
    ["45.6", "GBP", "billions", "£45.6bn"],
    ["2100000", "INR", "millions", "₹2.1trn"],
    ["302231", "KRW", "billions", "₩302.2trn"],
    ["2880", "TWD", "billions", "NT$2.9trn"],
    ["82.3", "HKD", "billions", "HK$82.3bn"],
    ["14.7", "AUD", "billions", "A$14.7bn"],
    ["9.1", "CAD", "billions", "C$9.1bn"],
    ["3.2", "SGD", "billions", "S$3.2bn"],
    ["4.5", "CHF", "billions", "CHF 4.5bn"],
  ];
  it.each(cases)("%s %s (%s) is %s", (value, currency, scale, expected) => {
    expect(formatMoney(value, currency, { scale: scale as "millions" })).toBe(expected);
  });

  it("write yen and yuan as words unless the subtitle names the currency", () => {
    expect(formatMoney("1200", "JPY", { scale: "billions" })).toBe("1.2trn yen");
    expect(formatMoney("1200", "JPY", { scale: "billions", currencyNamed: true })).toBe("¥1.2trn");
    expect(formatMoney("85.4", "CNY", { scale: "billions" })).toBe("85.4bn yuan");
    expect(formatMoney("85.4", "CNY", { scale: "billions", currencyNamed: true })).toBe("¥85.4bn");
  });

  it("put the minus sign before the currency", () => {
    expect(formatMoney("-5700", "USD", { scale: "millions" })).toBe(`${M}$5.7bn`);
    expect(formatMoney("-4.5", "CHF", { scale: "billions" })).toBe(`${M}CHF 4.5bn`);
    expect(formatMoney("-12", "JPY", { scale: "billions" })).toBe(`${M}12bn yen`);
  });

  it("write the exact value as reported, with the source's scale word", () => {
    expect(formatMoney("391035", "USD", { scale: "millions", style: "as-reported" })).toBe("$391,035 million");
    expect(formatMoney("391035000000", "USD", { style: "as-reported" })).toBe("$391,035,000,000");
    expect(formatMoney("1.5", "EUR", { scale: "billions", style: "as-reported" })).toBe("€1.5 billion");
  });

  it("refuse a currency that is not an ISO 4217 code rather than guess", () => {
    expect(() => formatMoney("1", "US$")).toThrow(TypeError);
    expect(() => formatMoney("1", "usd")).toThrow(TypeError);
  });
});

describe("units", () => {
  it("write percentages with no space and the source's own digits, never rounded", () => {
    expect(formatPercent("2.4")).toBe("2.4%");
    expect(formatPercent("2")).toBe("2%");
    expect(formatPercent("2.45")).toBe("2.45%");
    expect(formatPercent("6.50")).toBe("6.50%");
    expect(formatPercent("6.5", { sourceDecimals: 2 })).toBe("6.50%");
    expect(formatPercent("-5.7")).toBe(`${M}5.7%`);
    // An explicit precision (an axis tick) may round, but never adds digits the source lacks.
    expect(formatPercent("2.45", { decimals: 1 })).toBe("2.5%");
    expect(formatPercent("2.4", { decimals: 3 })).toBe("2.4%");
  });

  it("write percentage points and basis points as words", () => {
    expect(formatPoints("2.1", "pp")).toBe("2.1 points");
    expect(formatPoints("1", "pp")).toBe("1 point");
    expect(formatPoints("25", "bp")).toBe("25 basis points");
    expect(formatPoints("12.5", "bp")).toBe("12.5 basis points");
    expect(formatPoints("0.25", "pp")).toBe("0.25 points");
    expect(formatPoints("-25", "bp")).toBe(`${M}25 basis points`);
  });

  it("classify the source's unit without changing it", () => {
    expect(unitKind("Percent")).toBe("percent");
    expect(unitKind("Percent per annum")).toBe("percent");
    expect(unitKind("pp")).toBe("pp");
    expect(unitKind("Index 1982-84=100")).toBe("other");
    expect(formatQuantity("314.54", "Index 1982-84=100")).toBe("314.54");
    expect(formatQuantity("1204", "participants", { showUnit: true })).toBe("1,204 participants");
    expect(formatQuantity("158000", "Thousands of Persons", { scale: "thousands" })).toBe("158.0m");
    expect(formatQuantity("2.4", "Percent")).toBe("2.4%");
  });

  it("write signed changes for compact columns", () => {
    expect(formatSigned("12.9", formatPercent)).toBe("+12.9%");
    expect(formatSigned("-7.1", formatPercent)).toBe(`${M}7.1%`);
    expect(formatSigned("0.0", formatPercent)).toBe("0.0%");
    expect(formatSigned("2.1", (v) => formatPoints(v, "pp"))).toBe("+2.1 points");
  });

  it("write changes in words, never as colours", () => {
    expect(formatChange("2.0", formatPercent, { against: "fiscal 2023" })).toBe("Up 2.0% on fiscal 2023");
    expect(formatChange("-3.4", formatPercent)).toBe("Down 3.4%");
    expect(formatChange("0", formatPercent)).toBe("Unchanged");
    expect(formatChange("-0.04", (v) => formatPercent(v, { decimals: 2 }))).toBe("Down 0.04%");
    expect(formatChange("-0.04", formatPercent)).toBe("Down 0.04%");
    expect(formatChange("-0.04", (v) => formatPercent(v, { decimals: 1 }))).toBe("Unchanged");
  });
});

describe("dates and periods", () => {
  it("write full dates as day month year, no comma", () => {
    expect(formatDate("2024-11-01")).toBe("1 November 2024");
    expect(formatDate("2024-12-31")).toBe("31 December 2024");
  });

  it("add the weekday only when asked (the home page date line)", () => {
    expect(formatDateWithWeekday("2024-11-06")).toBe("Wednesday 6 November 2024");
  });

  it("write short dates, months and month ticks", () => {
    expect(formatShortDate("2024-11-01")).toBe("1 Nov 2024");
    expect(formatShortDate("2024-09-15")).toBe("15 Sep 2024");
    expect(formatMonthYear("2026-10")).toBe("October 2026");
    expect(formatAxisMonth("2024-01")).toBe("Jan");
    expect(formatAxisMonth("2024-02-29")).toBe("Feb");
  });

  it("show a date-time as its UTC calendar date, whatever the machine's time zone", () => {
    expect(formatDate("2026-11-13T23:30:00-05:00")).toBe("14 November 2026");
    expect(formatDate("2026-11-13T00:30:00+01:00")).toBe("12 November 2026");
    expect(formatDate("2026-11-13T10:00:00Z")).toBe("13 November 2026");
  });

  it("refuse dates that do not exist", () => {
    expect(() => parseIsoDate("2023-02-29")).toThrow(RangeError);
    expect(() => parseIsoDate("1 Nov 2024")).toThrow(TypeError);
  });

  it("keep fiscal period labels verbatim, as the company writes them", () => {
    for (const label of [
      "fiscal 2024",
      "Q4 FY26",
      "fourth quarter of fiscal 2024",
      "FY2024/25",
      "H1 2025",
      "1H FY3/2025",
    ]) {
      expect(formatPeriodLabel(label)).toBe(label);
    }
  });
});
