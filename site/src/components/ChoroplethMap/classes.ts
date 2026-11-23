// The classes of a choropleth map (design/charts.md, "Maps"): up to five, one per step of the validated sequential
// ramp, broken at round numbers near the quintiles so each class holds about as many countries. Equal ranges (the
// heatmap's classes) would put most countries of a skewed measure, such as unemployment, in one shade.
import { formatNumber, SCALE_EXPONENT, unitKind } from "../../format";
import { decimalToString, parseDecimal, shiftDecimal } from "../../format/decimal";
import type { Measure } from "../ChartFigure/model";
import { rampSteps } from "../Heatmap/Heatmap";

/** The validated sequential ramp has five steps (Heatmap.css), so a map has at most five classes. */
const MAX_CLASSES = 5;

export interface MapClasses {
  /** The breaks between classes, lowest first (one fewer than the classes), as exact decimals and as labelled. */
  joins: { value: string; label: string }[];
  /** The ramp step (1-5) of each class. */
  steps: number[];
  /** The class of a value: the number of breaks at or below it, so a value on a break goes up a class. */
  classOf: (value: string) => number;
  /** The one value's label when there is a single class and the values are all equal; otherwise null. */
  only: string | null;
  /** "min to max" when there is a single class but the values differ (no break survived); otherwise null. */
  range: string | null;
}

/** A number rounded to one significant figure, written as an exact decimal ("0.0004", "1000000"). */
export function roundOne(n: number): string {
  const [digit, exp] = n.toPrecision(1).split("e") as [string, string | undefined];
  return exp === undefined ? digit : decimalToString(shiftDecimal(parseDecimal(digit), Number(exp)));
}

/**
 * Breaks near the quintiles, rounded to one significant figure. Breaks that round to the same number, fall outside
 * the data, or would leave a class empty are dropped, so there are 1 to 5 classes and every one holds a value.
 */
export function mapClasses(values: readonly string[], measure: Measure, exponent: number): MapClasses {
  const sorted = values.map(Number).sort((a, b) => a - b);
  const hi = sorted.at(-1) as number;
  const quantile = (q: number) => sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))] as number;
  const breaks: string[] = [];
  for (let i = 1; i < MAX_CLASSES; i++) {
    const b = roundOne(quantile(i / MAX_CLASSES));
    // Rounding can carry a break past the values above the last one (8.75 → 9 with 12.5 next): keep a break only
    // when some value lies between the last break and it, so no legend class is empty.
    const below = Number(breaks.at(-1) ?? Number.NEGATIVE_INFINITY);
    if (Number(b) <= hi && sorted.some((v) => v >= below && v < Number(b))) breaks.push(b);
  }
  // Labels in the display unit the ranked values use (bn, trn); the top break carries "%", as an axis's top tick does.
  const pct = unitKind(measure.unit) === "percent" ? "%" : "";
  const shift = SCALE_EXPONENT[measure.scale ?? "units"] - exponent;
  const shifted = (v: string) => decimalToString(shiftDecimal(parseDecimal(v), shift));
  // A break is ours, so it drops the zeros a shift adds ("0.02", not "0.020000"); a value keeps the source's digits.
  const label = (v: string) =>
    formatNumber(
      shifted(v)
        .replace(/(\.\d*?)0+$/, "$1")
        .replace(/\.$/, ""),
    );
  const joins = breaks.map((value, i) => ({ value, label: `${label(value)}${i === breaks.length - 1 ? pct : ""}` }));
  // Source digits, compared as numbers: 10.2 and 10.4 differ, so a lone class must not wear one of them as its label.
  const lo = values.reduce((a, v) => (Number(v) < Number(a) ? v : a));
  const single = joins.length === 0;
  const equal = single && Number(lo) === Number(hi);
  const show = (v: string) => formatNumber(shifted(v));
  return {
    joins,
    steps: rampSteps(joins.length + 1),
    classOf: (value) => breaks.filter((b) => Number(value) >= Number(b)).length,
    only: equal ? `${show(lo)}${pct}` : null,
    range: single && !equal ? `${show(lo)} to ${show(values.find((v) => Number(v) === hi) as string)}${pct}` : null,
  };
}

/** What a class covers, in words, for screen readers: "under 3", "3 to 5", "10% or more". */
export function classText({ joins, only, range }: MapClasses, i: number): string {
  if (only !== null) return only;
  if (range !== null) return range;
  const lower = joins[i - 1]?.label;
  const upper = joins[i]?.label;
  if (lower === undefined) return `under ${upper}`;
  if (upper === undefined) return `${lower} or more`;
  return `${lower.replace(/%$/, "")} to ${upper}`;
}
