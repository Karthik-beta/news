// Number formats from design/content-style.md ("Numbers"). Formatters only format: they never convert a value,
// a unit, a currency or a period. A scale change (391,035 million -> 391.0bn) is notation, and rounding is to
// the precision the style asks for, never beyond what the source gives.
import {
  type Decimal,
  type DecimalParts,
  decimalToString,
  groupThousands,
  isZero,
  magnitude,
  parseDecimal,
  roundDecimal,
  shiftDecimal,
} from "./decimal";

/** The true minus sign (U+2212). Never a hyphen. */
export const MINUS = "−";
/** En dash for ranges, no spaces: "2020–2024". */
export const EN_DASH = "–";

/** The scale a source states its value in. The value is not multiplied; the scale says what it counts. */
export type Scale = "units" | "thousands" | "millions" | "billions" | "trillions";

export const SCALE_EXPONENT: Record<Scale, number> = {
  units: 0,
  thousands: 3,
  millions: 6,
  billions: 9,
  trillions: 12,
};

/** Scale words for "as reported" figures: "$391,035 million". */
export const SCALE_WORD: Record<Scale, string> = {
  units: "",
  thousands: "thousand",
  millions: "million",
  billions: "billion",
  trillions: "trillion",
};

const COMPACT_SUFFIXES = [
  { exp: 12, suffix: "trn" },
  { exp: 9, suffix: "bn" },
  { exp: 6, suffix: "m" },
] as const;

export interface NumberOptions {
  /**
   * Digits after the point. Omit to keep the source's own digits ("as reported"). Never adds precision beyond
   * `sourceDecimals`.
   */
  decimals?: number;
  /** Digits after the point the source is precise to, in the value's own scale. Default: digits as written. */
  sourceDecimals?: number;
}

/** Joins sign and digits with grouping: "-1234.5" -> "−1,234.5". Zero after rounding is never signed. */
export function writeDecimal(p: DecimalParts): string {
  const sign = p.negative && !isZero(p) ? MINUS : "";
  const int = groupThousands(p.int.replace(/^0+(?=\d)/, "") || "0");
  return `${sign}${int}${p.frac ? `.${p.frac}` : ""}`;
}

function sourcePrecision(value: Decimal, sourceDecimals?: number): number {
  return sourceDecimals ?? parseDecimal(value).frac.length;
}

function placesFor(requested: number | undefined, available: number, asWritten: number): number {
  if (requested === undefined) return Math.max(0, Math.min(asWritten, available));
  return Math.max(0, Math.min(requested, available));
}

/**
 * A plain number with thousands separators and a true minus: "167,045", "46.2", "−5.7". The value's scale is
 * not applied (use `formatCompact` or the money and unit formatters for that).
 */
export function formatNumber(value: Decimal, options: NumberOptions = {}): string {
  const p = parseDecimal(value);
  const available = sourcePrecision(value, options.sourceDecimals);
  return writeDecimal(roundDecimal(p, placesFor(options.decimals, available, p.frac.length)));
}

export interface CompactOptions extends NumberOptions {
  /** The scale the value is stated in. Default "units". */
  scale?: Scale;
}

export interface CompactNumber {
  /** The signed number with separators: "391.0", "−5.7", "167,045". */
  number: string;
  /** "m", "bn", "trn" or "" when the value is under a million. */
  suffix: "" | "m" | "bn" | "trn";
  /** The exponent the number is shown in (9 for bn), for tests and the round trip. */
  exponent: number;
}

/**
 * Compact form for key figures, prose and labels: one decimal place for m / bn / trn values, unless the source is
 * less precise ("2bn" stays "2bn"). Values under a million keep all their digits: "167,045".
 */
export function compactNumber(value: Decimal, options: CompactOptions = {}): CompactNumber {
  const scaleExp = SCALE_EXPONENT[options.scale ?? "units"];
  const source = sourcePrecision(value, options.sourceDecimals);
  const inUnits = shiftDecimal(parseDecimal(value), scaleExp);
  const requested = options.decimals ?? 1;

  for (let i = 0; i < COMPACT_SUFFIXES.length; i++) {
    const { exp, suffix } = COMPACT_SUFFIXES[i] as (typeof COMPACT_SUFFIXES)[number];
    if (magnitude(inUnits) < exp) continue;
    const places = placesFor(requested, source + exp - scaleExp, requested);
    const rounded = roundDecimal(shiftDecimal(inUnits, -exp), places);
    // 999.96bn rounds to 1,000.0bn: show it as 1.0trn instead.
    const bigger = COMPACT_SUFFIXES[i - 1];
    if (bigger && magnitude(rounded) >= bigger.exp - exp) {
      const upPlaces = placesFor(requested, source + bigger.exp - scaleExp, requested);
      const up = roundDecimal(shiftDecimal(inUnits, -bigger.exp), upPlaces);
      return { number: writeDecimal(up), suffix: bigger.suffix, exponent: bigger.exp };
    }
    return { number: writeDecimal(rounded), suffix, exponent: exp };
  }
  // Under a million: all the digits the source gives, in units.
  const places = Math.max(0, source - scaleExp);
  const kept = options.decimals === undefined ? places : Math.min(options.decimals, places);
  return { number: writeDecimal(roundDecimal(inUnits, kept)), suffix: "", exponent: 0 };
}

/** "391.0bn", "420m", "−5.7bn", "167,045". */
export function formatCompact(value: Decimal, options: CompactOptions = {}): string {
  const c = compactNumber(value, options);
  return `${c.number}${c.suffix}`;
}

/**
 * Adds an explicit plus for compact change columns: "+12.9%", "−7.1%". A value that is zero, or that rounds to
 * zero in `formatted` ("0.0%"), stays unsigned. `formatted` must be the output of a formatter for `value`.
 */
export function withSign(formatted: string, value: Decimal): string {
  if (isNegative(value) || !/[1-9]/.test(formatted)) return formatted;
  return `+${formatted}`;
}

/** True when the value is below zero (and not a rounded-away zero): for change words. */
export function isNegative(value: Decimal): boolean {
  const p = parseDecimal(value);
  return p.negative && !isZero(p);
}

export function isZeroValue(value: Decimal): boolean {
  return isZero(parseDecimal(value));
}

/** Absolute value, exactly. */
export function absDecimal(value: Decimal): Decimal {
  return decimalToString({ ...parseDecimal(value), negative: false });
}

/** Replaces hyphen-minus signs in front of digits with U+2212, for text that arrives already formatted. */
export function useTrueMinus(text: string): string {
  return text.replace(/(^|[^\w])-(?=\d)/g, `$1${MINUS}`);
}

/** "2020–2024", "4.50–4.75%": joins two already formatted ends with an en dash and no spaces. */
export function formatRange(from: string, to: string): string {
  return `${from}${EN_DASH}${to}`;
}

/** "About 3,000". Never "~". */
export function formatAbout(formatted: string): string {
  return `About ${formatted}`;
}
