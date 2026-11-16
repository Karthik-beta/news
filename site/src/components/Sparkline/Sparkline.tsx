// Sparkline (design/charts.md, "Sparklines"): a trend inside a key figure, list or card. 120×36px, a 2.2px line in
// the comparison blue, the latest point as a 4.5px crimson dot; no axes, no labels (the value is written beside
// it). Static SVG at its true size, so the line never scales: it ships no JavaScript. Gaps break the line.
// Its accessible name says what it shows, from the data: "Trend, fiscal 2020 to fiscal 2024: $274.5bn to $391.0bn".
import { MISSING_TEXT, type ResolvedSeries } from "../../renderer/data/resolve";
import { displayValue } from "../ChartFigure/model";

type OkSeries = Extract<ResolvedSeries, { ok: true }>;

const W = 120;
const H = 36;
/** Room for the end dot and its ring inside the box. */
const PAD = 4;

export interface SparklineGeometry {
  path: string;
  dot: { x: number; y: number } | null;
}

/** The line and the latest point's dot in a 120×36 box. The vertical range is the data's own (it shows shape). */
export function sparklineGeometry(values: readonly (number | null)[]): SparklineGeometry {
  const present = values.filter((v): v is number => v !== null);
  if (present.length === 0) return { path: "", dot: null };
  const min = Math.min(...present);
  const max = Math.max(...present);
  const n = values.length;
  const x = (i: number) => (n === 1 ? W - PAD : PAD + (i / (n - 1)) * (W - 2 * PAD));
  const y = (v: number) => (max === min ? H / 2 : PAD + ((max - v) / (max - min)) * (H - 2 * PAD));
  let path = "";
  let pen = false;
  let dot: SparklineGeometry["dot"] = null;
  values.forEach((v, i) => {
    if (v === null) {
      pen = false;
      return;
    }
    path += `${pen ? "L" : "M"}${x(i).toFixed(1)},${y(v).toFixed(1)}`;
    pen = true;
    dot = { x: x(i), y: y(v) };
  });
  return { path, dot };
}

/** "Trend, fiscal 2020 to fiscal 2024: $274.5bn to $391.0bn". */
export function sparklineLabel(series: OkSeries): string {
  const pts = series.points;
  const first = pts[0];
  const last = pts[pts.length - 1];
  if (!first || !last) return `Trend: ${MISSING_TEXT.toLowerCase()}`;
  const say = (p: (typeof pts)[number]) =>
    p.value === null ? MISSING_TEXT.toLowerCase() : displayValue(p.value, series);
  const span = first === last ? first.period.label : `${first.period.label} to ${last.period.label}`;
  return `Trend, ${span}: ${first === last ? say(first) : `${say(first)} to ${say(last)}`}`;
}

export function Sparkline({ series, className }: { series: OkSeries; className?: string }) {
  const { path, dot } = sparklineGeometry(series.points.map((p) => (p.value === null ? null : Number(p.value))));
  if (!dot) return null;
  return (
    <svg
      className={`sparkline${className ? ` ${className}` : ""}`}
      width={W}
      height={H}
      viewBox={`0 0 ${W} ${H}`}
      role="img"
      aria-label={sparklineLabel(series)}
      data-ref={series.ref}
    >
      <path className="sparkline__line" d={path} />
      <circle className="sparkline__dot" cx={dot.x.toFixed(1)} cy={dot.y.toFixed(1)} r="2.25" />
    </svg>
  );
}
