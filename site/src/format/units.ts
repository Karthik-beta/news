// Percentages, points and other units, written the house way. Units are never converted: a unit the formatter
// does not know is written verbatim after the number, exactly as the source names it.
import type { Decimal } from "./decimal";
import { type CompactOptions, formatCompact, formatNumber, isNegative, isZeroValue, MINUS, withSign } from "./number";

export interface PercentOptions {
  /** Digits after the point. Default: one, unless the source gives fewer. */
  decimals?: number;
  sourceDecimals?: number;
}

const defaultPlaces = (value: Decimal, options: PercentOptions) =>
  options.decimals ?? Math.min(1, options.sourceDecimals ?? value.split(".")[1]?.length ?? 0);

/** "2.4%", "−5.7%". No space before the sign. */
export function formatPercent(value: Decimal, options: PercentOptions = {}): string {
  return `${formatNumber(value, { decimals: defaultPlaces(value, options), sourceDecimals: options.sourceDecimals })}%`;
}

/** Percentage points ("2.1 points") or basis points ("25 basis points"). */
export function formatPoints(value: Decimal, kind: "pp" | "bp", options: PercentOptions = {}): string {
  const places = kind === "bp" ? (options.decimals ?? 0) : defaultPlaces(value, options);
  const number = formatNumber(value, { decimals: places, sourceDecimals: options.sourceDecimals });
  const word = kind === "bp" ? "basis point" : "point";
  return `${number} ${word}${number === "1" || number === `${MINUS}1` ? "" : "s"}`;
}

/** The units the formatter writes in the house style. Everything else is written verbatim. */
export type UnitKind = "percent" | "pp" | "bp" | "other";

const UNIT_KINDS: Record<string, UnitKind> = {
  percent: "percent",
  "%": "percent",
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
