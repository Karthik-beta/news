// Percentages, points and other units, written the house way. Units are never converted: a unit the formatter
// does not know is written verbatim after the number, exactly as the source names it.
import type { Decimal } from "./decimal";
import { type CompactOptions, formatCompact, formatNumber, isNegative, isZeroValue, MINUS, withSign } from "./number";

export interface PercentOptions {
  /**
   * Digits after the point. Default: the source's own digits, never rounded ("6.50%" stays "6.50%"). Set it only
   * where a layout needs fewer (an axis tick); it never adds precision the source lacks.
   */
  decimals?: number;
  /** Digits after the point the source gives, when the value string has lost trailing zeros ("6.5" for 6.50). */
  sourceDecimals?: number;
}

/**
 * Rates, points and basis points keep the source's own digits (Karthik, 16 November 2026): every digit the value
 * is written with, and trailing zeros the source's decimals say it has. Nothing is rounded away.
 */
const sourcePlaces = (value: Decimal, options: PercentOptions) =>
  Math.max(0, value.split(".")[1]?.length ?? 0, options.sourceDecimals ?? 0);

function rateNumber(value: Decimal, options: PercentOptions): string {
  const available = sourcePlaces(value, options);
  return formatNumber(value, { decimals: options.decimals ?? available, sourceDecimals: available });
}

/** "2.4%", "6.50%", "−5.7%": the source's digits. No space before the sign. */
export function formatPercent(value: Decimal, options: PercentOptions = {}): string {
  return `${rateNumber(value, options)}%`;
}

/** Percentage points ("2.1 points") or basis points ("25 basis points"). */
export function formatPoints(value: Decimal, kind: "pp" | "bp", options: PercentOptions = {}): string {
  const number = rateNumber(value, options);
  const word = kind === "bp" ? "basis point" : "point";
  return `${number} ${word}${number === "1" || number === `${MINUS}1` ? "" : "s"}`;
}

/** The units the formatter writes in the house style. Everything else is written verbatim. */
export type UnitKind = "percent" | "pp" | "bp" | "other";

const UNIT_KINDS: Record<string, UnitKind> = {
  percent: "percent",
  "%": "percent",
  // Policy and market rates (ECB Data Portal): a rate is written as a percentage, its period stays in the label.
  "percent per annum": "percent",
  pp: "pp",
  "percentage points": "pp",
  "percentage point": "pp",
  bp: "bp",
  bps: "bp",
  "basis points": "bp",
  "basis point": "bp",
};

/** Classifies a source unit string, case-insensitively ("Percent" -> percent). The unit itself is unchanged. */
export function unitKind(unit: string): UnitKind {
  return UNIT_KINDS[unit.trim().toLowerCase()] ?? "other";
}

export interface QuantityOptions extends CompactOptions {
  /** Write the source's unit after the number for units the house style does not know. Default false. */
  showUnit?: boolean;
}

/**
 * A value in its unit: "2.4%", "2.1 points", "25 basis points", "420m", "1,204 participants" (with showUnit).
 * Money goes through formatMoney instead.
 */
export function formatQuantity(value: Decimal, unit: string, options: QuantityOptions = {}): string {
  const kind = unitKind(unit);
  if (kind === "percent") return formatPercent(value, options);
  if (kind === "pp" || kind === "bp") return formatPoints(value, kind, options);
  const number = formatCompact(value, options);
  return options.showUnit && unit ? `${number} ${unit}` : number;
}

/** A signed figure for compact change columns: "+12.9%", "−7.1%", "+2.1 points". */
export function formatSigned(value: Decimal, format: (value: Decimal) => string): string {
  return withSign(format(value), value);
}

export interface ChangeOptions {
  /** What the change is measured against, verbatim: "fiscal 2023". Gives "Up 2.0% on fiscal 2023". */
  against?: string;
}

/**
 * A change in words, never coloured (design/content-style.md): "Up 2.0% on fiscal 2023", "Down 3.4%",
 * "Unchanged". `format` writes the size of the change; the sign becomes the word.
 */
export function formatChange(value: Decimal, format: (value: Decimal) => string, options: ChangeOptions = {}): string {
  const tail = options.against ? ` on ${options.against}` : "";
  const absolute = value.startsWith("-") ? value.slice(1) : value;
  const size = format(absolute);
  if (isZeroValue(value) || !/[1-9]/.test(size)) return `Unchanged${tail}`;
  return `${isNegative(value) ? "Down" : "Up"} ${size}${tail}`;
}
