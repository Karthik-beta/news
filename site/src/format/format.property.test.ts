// Property test: formatting never changes a value, a unit or a currency. Thousands of seeded random values go
// through the formatters and are parsed back by an independent reader written here (not the formatters' own
// helpers). As-reported output must round-trip exactly; compact output must equal the source value rounded to
// the digits it shows, in the same currency, with the same unit.
import { describe, expect, it } from "vitest";
import { formatMoney, formatNumber, formatPercent, formatQuantity, MINUS, SCALE_EXPONENT, type Scale } from ".";
import { CURRENCY_PREFIX, CURRENCY_WORD } from "./currency";

const RUNS = 3000;

/** mulberry32: a small seeded PRNG, so a failure always reproduces with the same seed. */
function rng(seed: number) {
  let a = seed;
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function randomDecimal(r: () => number): string {
  const intDigits = Math.floor(r() * 16);
  const fracDigits = Math.floor(r() * 7);
  let int = intDigits === 0 ? "0" : String(1 + Math.floor(r() * 9));
  for (let i = 1; i < intDigits; i++) int += String(Math.floor(r() * 10));
  let frac = "";
  for (let i = 0; i < fracDigits; i++) frac += String(Math.floor(r() * 10));
  const negative = r() < 0.3;
  const s = `${int}${frac ? `.${frac}` : ""}`;
  return negative && /[1-9]/.test(s) ? `-${s}` : s;
}

const pick = <T>(r: () => number, xs: readonly T[]): T => xs[Math.floor(r() * xs.length)] as T;

/** Exact value of a decimal string as (BigInt mantissa, exponent): "-12.5" -> (-125n, -1). */
function exact(text: string): { m: bigint; e: number } {
  const negative = text.startsWith("-");
  const [int = "0", frac = ""] = (negative ? text.slice(1) : text).split(".");
  const m = BigInt(int + frac);
  return { m: negative ? -m : m, e: -frac.length };
}

/** a * 10^ea - b * 10^eb, as a mantissa at the smaller exponent. */
function diff(a: { m: bigint; e: number }, b: { m: bigint; e: number }) {
  const e = Math.min(a.e, b.e);
  return { m: a.m * 10n ** BigInt(a.e - e) - b.m * 10n ** BigInt(b.e - e), e };
}

const SUFFIX_EXP: Record<string, number> = { "": 0, m: 6, bn: 9, trn: 12 };
const WORD_SCALE: Record<string, Scale> = {
  "": "units",
  thousand: "thousands",
  million: "millions",
  billion: "billions",
  trillion: "trillions",
};
const PREFIX_TO_CODE = new Map(Object.entries(CURRENCY_PREFIX).map(([code, p]) => [p, code]));
const WORD_TO_CODE = new Map(Object.entries(CURRENCY_WORD).map(([code, w]) => [w, code]));

/**
 * Reads a formatted money string back into value text, currency and suffix or scale word. A bare ¥ is yen or yuan,
 * so the reader takes the currency the subtitle names, as a reader of the page would.
 */
function readMoney(text: string, subtitleNames?: string) {
  const m =
    /^(−?)([A-Z]{3} |[A-Z]{0,2}\$|€|£|₹|₩|¥)?([0-9,]+(?:\.[0-9]+)?)(m|bn|trn)?(?: (thousand|million|billion|trillion))?(?: (yen|yuan))?$/u.exec(
      text,
    );
  if (!m) throw new Error(`unreadable money: ${text}`);
  const [, minus, prefix = "", digits = "", suffix = "", word = "", currencyWord = ""] = m;
  const currency = currencyWord
    ? WORD_TO_CODE.get(currencyWord)
    : prefix.endsWith(" ")
      ? prefix.trim()
      : prefix === "¥" && (subtitleNames === "JPY" || subtitleNames === "CNY")
        ? subtitleNames
        : PREFIX_TO_CODE.get(prefix);
  return { value: `${minus ? "-" : ""}${digits.replaceAll(",", "")}`, currency, suffix, word };
}

const CURRENCIES = [...Object.keys(CURRENCY_PREFIX), "JPY", "CNY", "CHF", "SEK", "BRL"] as const;
const SCALES = Object.keys(SCALE_EXPONENT) as Scale[];

describe("formatting never changes a value, unit or currency (property test)", () => {
  it("as-reported money round-trips exactly: same digits, same scale, same currency", () => {
    const r = rng(20261115);
    for (let i = 0; i < RUNS; i++) {
      const value = randomDecimal(r);
      const currency = pick(r, CURRENCIES);
      const scale = pick(r, SCALES);
      const text = formatMoney(value, currency, { scale, style: "as-reported" });
      const back = readMoney(text);
      const where = `${value} ${currency} ${scale} -> ${text}`;
      expect(exact(back.value), where).toEqual(exact(value));
      expect(back.currency, where).toBe(currency);
      expect(WORD_SCALE[back.word], where).toBe(scale);
      if (value.startsWith("-") && /[1-9]/.test(value)) expect(text.startsWith(MINUS), where).toBe(true);
      expect(text, where).not.toMatch(/-/);
    }
  });

  it("compact money equals the source value rounded to the digits shown, in the same currency", () => {
    const r = rng(42);
    for (let i = 0; i < RUNS; i++) {
      const value = randomDecimal(r);
      const currency = pick(r, CURRENCIES);
      const scale = pick(r, SCALES);
      const currencyNamed = r() < 0.5;
      const text = formatMoney(value, currency, { scale, currencyNamed });
      const back = readMoney(text, currencyNamed ? currency : undefined);
      const where = `${value} ${currency} ${scale} -> ${text}`;
      expect(back.currency, where).toBe(currency);
      // Shown value, in units of the currency, against the source value in the same units.
      const shown = exact(back.value);
      shown.e += SUFFIX_EXP[back.suffix] ?? Number.NaN;
      const source = exact(value);
      source.e += SCALE_EXPONENT[scale];
      const d = diff(shown, source);
      const lastDigit = (SUFFIX_EXP[back.suffix] as number) - (back.value.split(".")[1]?.length ?? 0);
      // |shown - source| <= half a unit in the last digit shown.
      const half = { m: 5n, e: lastDigit - 1 };
      const abs = { m: d.m < 0n ? -d.m : d.m, e: d.e };
      expect(diff(abs, half).m <= 0n, where).toBe(true);
    }
  });

  it("plain numbers keep every digit when no rounding is asked for", () => {
    const r = rng(7);
    for (let i = 0; i < RUNS; i++) {
      const value = randomDecimal(r);
      const text = formatNumber(value);
      expect(exact(text.replace(MINUS, "-").replaceAll(",", "")), `${value} -> ${text}`).toEqual(exact(value));
    }
  });

  it("percentages keep the % unit and the value to the digits shown", () => {
    const r = rng(99);
    for (let i = 0; i < RUNS; i++) {
      const value = randomDecimal(r);
      const text = formatPercent(value, { decimals: 6 });
      expect(text.endsWith("%"), text).toBe(true);
      expect(exact(text.slice(0, -1).replace(MINUS, "-").replaceAll(",", "")), `${value} -> ${text}`).toEqual(
        exact(value),
      );
    }
  });

  it("units the house style does not know are written verbatim", () => {
    const r = rng(5);
    const units = ["Index 1982-84=100", "Thousands of Persons", "tonnes", "°F", "miles", "kWh", "participants"];
    for (let i = 0; i < RUNS / 10; i++) {
      const unit = pick(r, units);
      const text = formatQuantity(randomDecimal(r), unit, { showUnit: true });
      expect(text.endsWith(` ${unit}`), text).toBe(true);
    }
  });
});
