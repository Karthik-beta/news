// Chart geometry: the value axis (4 to 6 gridlines on round numbers, bars from zero) and the bare numbers that
// label ticks and marks (design/charts.md, "Axes and scales"). Geometry uses Number() of the exact decimal, never
// for text: every label is written from the decimal string through the shared formatters, so no float noise
// ever reaches the page. Nothing here converts a value: a scale change (millions shown as bn) moves the point.
import { compactNumber, formatNumber, SCALE_EXPONENT, type Scale, unitKind } from "../../format";
import { decimalToString, magnitude, parseDecimal, shiftDecimal } from "../../format/decimal";

export interface Measure {
  unit: string;
  currency: string | null;
  scale: Scale | null;
}

export interface Tick {
  /** The tick's value in the source's own scale, as an exact decimal. */
  value: string;
  /** Position from the top of the plot, 0-100. */
  y: number;
  /** The bare number shown on the axis ("400", "2.5", "−1,000"); the top percentage tick carries "%". */
  label: string;
  /** True for the zero line (bars) or the bottom of the axis (lines): drawn as the baseline. */
  baseline: boolean;
}

export interface ValueScale {
  min: number;
  max: number;
  ticks: Tick[];
  /** Position from the top of the plot (0-100) of a value in the source's scale. */
  y: (value: number) => number;
  /** Position of zero, clamped to the plot; the bars' baseline. */
  zero: number;
  /** The power of ten the axis and bare labels are written in (9 for bn); 0 for percentages. */
  displayExponent: number;
}

const MANTISSAS = [1, 2, 5] as const;

/** The step decimal: mantissa × 10^exp, exactly. */
function stepDecimal(k: number, mantissa: number, exp: number): string {
  return decimalToString(shiftDecimal(parseDecimal(String(k * mantissa)), exp));
}

/**
 * A round-number axis over the values: steps of 1, 2 or 5 × 10^n, 4 to 6 gridlines where the data allows.
 * `includeZero` (bars, columns, and lines by default) stretches the axis to zero.
 */
export function niceScale(
  values: readonly number[],
  measure: Measure,
  { includeZero = true, displayExponent }: { includeZero?: boolean; displayExponent?: number } = {},
): ValueScale {
  const finite = values.filter((v) => Number.isFinite(v));
  let lo = finite.length ? Math.min(...finite) : 0;
  let hi = finite.length ? Math.max(...finite) : 1;
  if (includeZero) {
    lo = Math.min(lo, 0);
    hi = Math.max(hi, 0);
  }
  if (lo === hi) {
    // One value (or all equal): give it room above and, unless it sits on zero, below.
    const pad = Math.abs(hi) || 1;
    if (hi > 0 || !includeZero) hi += pad / 2;
    if (lo < 0 || !includeZero) lo -= pad / 2;
    if (lo === hi) hi = lo + 1;
  }
  const span = hi - lo;
  const base = Math.floor(Math.log10(span)) - 1;
  let chosen: { k0: number; k1: number; mantissa: number; exp: number } | null = null;
  outer: for (let exp = base; exp <= base + 2; exp++) {
    for (const mantissa of MANTISSAS) {
      const step = mantissa * 10 ** exp;
      const k0 = Math.floor(lo / step + 1e-9);
      const k1 = Math.ceil(hi / step - 1e-9);
      if (k1 - k0 + 1 <= 6) {
        chosen = { k0, k1, mantissa, exp };
        break outer;
      }
    }
  }
  const { k0, k1, mantissa, exp } = chosen ?? { k0: 0, k1: 1, mantissa: 1, exp: Math.ceil(Math.log10(span)) };
  const step = mantissa * 10 ** exp;
  const min = k0 * step;
  const max = k1 * step;
  const y = (v: number) => ((max - v) / (max - min)) * 100;
  const kind = unitKind(measure.unit);
  const display = displayExponent ?? 0;
  const sourceExp = SCALE_EXPONENT[measure.scale ?? "units"];
  // Digits after the point the step needs once written in the display unit.
  const places = Math.max(0, -(exp + sourceExp - display));
  const ticks: Tick[] = [];
  for (let k = k0; k <= k1; k++) {
    const value = stepDecimal(k, mantissa, exp);
    const shown = decimalToString(shiftDecimal(parseDecimal(value), sourceExp - display));
    let label = formatNumber(shown, { decimals: places, sourceDecimals: places });
    if (k === k1 && kind === "percent") label = `${label}%`;
    // Bars and columns: the zero line (always on the axis, since it includes zero). Lines: the axis bottom.
    ticks.push({ value, y: y(k * step), label, baseline: includeZero ? k === 0 : k === k0 });
  }
  return { min, max, ticks, y, zero: Math.min(100, Math.max(0, y(0))), displayExponent: display };
}

/** The decimal with the largest magnitude, as written ("−3132.5" beats "1695.2"). */
export function largestMagnitude(values: readonly string[]): string | null {
  let best: string | null = null;
  let bestMag = Number.NEGATIVE_INFINITY;
  for (const v of values) {
    const p = parseDecimal(v);
    const m = magnitude(p);
    if (best === null || m > bestMag || (m === bestMag && Math.abs(Number(v)) > Math.abs(Number(best)))) {
      best = v;
      bestMag = m;
    }
  }
  return best;
}

/**
 * The power of ten bare labels are written in: what the compact form of the largest value uses (9 for "$391.0bn",
 * so the axis reads 100, 200 … 400). Percentages, points and basis points are written as they are.
 */
export function displayExponent(values: readonly string[], measure: Measure): number {
  const kind = unitKind(measure.unit);
  if (kind !== "other" && !measure.currency) return 0;
  const big = largestMagnitude(values);
  if (big === null) return 0;
  return compactNumber(big, { scale: measure.scale ?? "units" }).exponent;
}

/**
 * A value as a bare number in the axis's display unit, for value labels next to marks: "391.0" (bn), "2.4" (%),
 * "4.50" (a rate). One decimal place for m/bn/trn; everything else keeps the digits the source published.
 */
export function bareNumber(value: string, measure: Measure, exponent: number): string {
  const sourceExp = SCALE_EXPONENT[measure.scale ?? "units"];
  const shifted = shiftDecimal(parseDecimal(value), sourceExp - exponent);
  const available = shifted.frac.length;
  const places = exponent > 0 ? Math.min(1, available) : available;
  return formatNumber(decimalToString(shifted), { decimals: places, sourceDecimals: available });
}

/** Roughly how many characters of x labels fit across the plot: a phone (about 290px) and a tablet or wider. */
const X_CAPACITY = { phone: 40, wide: 72 } as const;

/** Which of `count` stops to label for a given room: first, last and every step between, never crowding the last. */
function labelledStops(count: number, maxLabels: number): Set<number> {
  const shown = new Set<number>();
  if (count === 0) return shown;
  const step = Math.max(1, Math.ceil((count - 1) / Math.max(1, maxLabels - 1)));
  for (let i = 0; i < count; i += step) shown.add(i);
  const last = count - 1;
  for (const i of [...shown]) if (i !== 0 && i !== last && last - i < step / 2 + 0.5) shown.delete(i);
  shown.add(last);
  return shown;
}

/**
 * X labels for a plot: every label that fits, thinned evenly on phones rather than rotated (design/charts.md,
 * "Axes and scales"). `xs` are the stops' positions, 0-100.
 */
export function xLabelPlan(
  labels: readonly string[],
  xs: readonly number[],
): { x: number; label: string; phone: boolean; wide: boolean }[] {
  const longest = Math.max(1, ...labels.map((l) => l.length));
  const labelsFor = (room: number) => Math.max(2, Math.floor(room / (longest + 2)));
  const phone = labelledStops(labels.length, labelsFor(X_CAPACITY.phone));
  const wide = labelledStops(labels.length, labelsFor(X_CAPACITY.wide));
  return labels.map((label, i) => ({ x: xs[i] ?? 0, label, phone: phone.has(i), wide: wide.has(i) }));
}

/** The width of the axis labels, in ch, so marks start after them. */
export function axisWidth(ticks: readonly { label: string }[]): string {
  return `${Math.max(2, ...ticks.map((t) => t.label.length))}ch`;
}
