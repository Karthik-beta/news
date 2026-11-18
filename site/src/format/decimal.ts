// Exact decimal arithmetic on strings, for formatting only. Values in data.json are decimal strings exactly as
// the source published them ("391035", "-0.3"); they are never turned into floats on the way to the page, so a
// figure can't pick up a binary rounding error (0.1 + 0.2) between the source and the reader.
//
// Only the operations formatting needs exist here: move the decimal point (a scale change such as millions to
// billions, which is notation, not conversion), round, and measure magnitude. Nothing multiplies by a rate.

/** A plain decimal string: optional minus, no exponent, no grouping, no leading zeros ("0.5", "-12", "391035"). */
export type Decimal = string;

export const DECIMAL_PATTERN = /^-?(0|[1-9][0-9]*)(\.[0-9]+)?$/;

/** A parsed decimal: sign, integer digits and fraction digits, all as strings. */
export interface DecimalParts {
  negative: boolean;
  int: string;
  frac: string;
}

export function isDecimal(value: unknown): value is Decimal {
  return typeof value === "string" && DECIMAL_PATTERN.test(value);
}

export function parseDecimal(value: Decimal): DecimalParts {
  if (!isDecimal(value)) throw new TypeError(`Not a plain decimal string: ${JSON.stringify(value)}`);
  const negative = value.startsWith("-");
  const [int = "0", frac = ""] = (negative ? value.slice(1) : value).split(".");
  return { negative, int, frac };
}

/** Back to a canonical string: no leading zeros, "-0" becomes "0", fraction digits kept as they are. */
export function decimalToString({ negative, int, frac }: DecimalParts): Decimal {
  const i = int.replace(/^0+(?=\d)/, "") || "0";
  const zero = /^0*$/.test(i) && /^0*$/.test(frac);
  return `${negative && !zero ? "-" : ""}${i}${frac ? `.${frac}` : ""}`;
}

export function isZero(p: DecimalParts): boolean {
  return /^0*$/.test(p.int) && /^0*$/.test(p.frac);
}

/** Number of digits after the point, as written ("391.0" has 1, "2" has 0). */
export function fractionDigits(value: Decimal): number {
  return parseDecimal(value).frac.length;
}

/** Multiplies by 10^exp by moving the point. Exact; keeps every digit (trailing fraction zeros included). */
export function shiftDecimal(p: DecimalParts, exp: number): DecimalParts {
  if (exp === 0) return { ...p };
  const digits = p.int + p.frac;
  const point = p.int.length + exp;
  if (point <= 0) return { negative: p.negative, int: "0", frac: "0".repeat(-point) + digits };
  if (point >= digits.length)
    return { negative: p.negative, int: digits + "0".repeat(point - digits.length), frac: "" };
  return { negative: p.negative, int: digits.slice(0, point), frac: digits.slice(point) };
}

/**
 * Rounds to `places` digits after the point, half away from zero (2.45 -> 2.5, -2.45 -> -2.5), padding with zeros
 * when the value has fewer digits. `places` is 0 or more.
 */
export function roundDecimal(p: DecimalParts, places: number): DecimalParts {
  if (places < 0 || !Number.isInteger(places)) throw new RangeError(`places must be a whole number >= 0: ${places}`);
  if (p.frac.length <= places) return { ...p, frac: p.frac.padEnd(places, "0") };
  const kept = p.int + p.frac.slice(0, places);
  const roundUp = (p.frac[places] ?? "0") >= "5";
  const digits = roundUp ? incrementDigits(kept) : kept;
  const intLength = digits.length - places;
  return { negative: p.negative, int: digits.slice(0, intLength) || "0", frac: digits.slice(intLength) };
}

function incrementDigits(digits: string): string {
  const out = digits.split("");
  for (let i = out.length - 1; i >= 0; i--) {
    if (out[i] === "9") {
      out[i] = "0";
    } else {
      out[i] = String(Number(out[i]) + 1);
      return out.join("");
    }
  }
  return `1${out.join("")}`;
}

/**
 * Power of ten of the leading digit: 391035 -> 5, 0.05 -> -2, 0 -> -Infinity. Used to pick m / bn / trn.
 */
export function magnitude(p: DecimalParts): number {
  const int = p.int.replace(/^0+/, "");
  if (int) return int.length - 1;
  const firstNonZero = p.frac.search(/[1-9]/);
  return firstNonZero === -1 ? Number.NEGATIVE_INFINITY : -(firstNonZero + 1);
}

/** "1234567" -> "1,234,567". Groups the integer digits only. */
export function groupThousands(int: string): string {
  return int.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}
