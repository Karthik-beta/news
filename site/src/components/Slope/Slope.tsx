// Slope chart (design/charts.md, "Choosing the form": two points in time, several items; guidance against actual,
// before and after). One line per item from its first value to its second, on one value scale that need not start
// at zero (positions, not lengths). The highlighted item is crimson, or the neutral highlight when its second value
// is below its first (design/charts.md, "When the subject fell"), and the rest context; with nothing highlighted,
// up to 8 items take the categorical colours in order. Values on the left; values and names on the right, nudged
// apart so no two labels overlap. The plot is an island.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { usePageData } from "../../renderer/data/context";
import { MISSING_TEXT, type PageDataResolver, type ResolvedValue } from "../../renderer/data/resolve";
import { Island } from "../../renderer/islands/Island";
import { ChartFigure } from "../ChartFigure/ChartFigure";
import {
  type ColourRole,
  colourRoles,
  displayValue,
  isThin,
  type Measure,
  modelSummary,
  sameAxis,
  seriesFell,
  sourcesOf,
  type TableModel,
  tooltipSource,
} from "../ChartFigure/model";
import { SANKEY, stackLabels, wrapLines } from "../Sankey/layout";
import SlopePlot, { type SlopeItem, type SlopePlotProps } from "./SlopePlot.client";

export interface SlopeItemInput {
  label: string;
  from: string;
  to: string;
  highlight?: boolean | null;
}

export interface SlopeProps {
  title: string;
  subtitle: string;
  /** The two points' names, text with {pd_…} tokens. */
  periods: string[];
  items: SlopeItemInput[];
}

type OkValue = Extract<ResolvedValue, { ok: true }>;
interface Cell {
  ref: string;
  value: OkValue | null;
}

export interface SlopeData {
  periods: [string, string];
  items: { input: SlopeItemInput; from: Cell; to: Cell; role: ColourRole }[];
  measure: Measure | null;
  model: TableModel | null;
}

export function slopeData(props: SlopeProps, data: PageDataResolver): SlopeData {
  const periods: [string, string] = [data.plainText(props.periods[0] ?? ""), data.plainText(props.periods[1] ?? "")];
  const cell = (ref: string): Cell => {
    const v = data.value(ref);
    return { ref, value: v.ok ? v : null };
  };
  const raw = props.items.map((input) => ({ input, from: cell(input.from), to: cell(input.to) }));
  const first = raw.flatMap((r) => [r.from, r.to]).find((c) => c.value !== null)?.value ?? null;
  const measure: Measure | null = first ? { unit: first.unit, currency: first.currency, scale: first.scale } : null;
  // A value in another unit, currency or scale is left out, never converted.
  const keep = (c: Cell): Cell =>
    c.value && measure && !sameAxis([measure, c.value]) ? { ref: c.ref, value: null } : c;
  const subject = raw.find((r) => r.input.highlight);
  const fell = subject ? seriesFell([subject.from.value?.value ?? null, subject.to.value?.value ?? null]) : false;
  const roles = colourRoles(
    raw.map((_, i) => `item-${i}`),
    { highlight: subject ? `item-${raw.indexOf(subject)}` : null, fell },
  );
  const items = raw.map((r, i) => ({
    ...r,
    from: keep(r.from),
    to: keep(r.to),
    // Beyond the 8 categorical slots an item is neutral: never a generated colour (design/color.md).
    role: (!subject && i >= 8 ? "neutral" : (roles[i] ?? "cat-1")) as ColourRole,
  }));
  if (!measure) return { periods, items, measure, model: null };
  const ok = items.flatMap((r) => [r.from, r.to]).flatMap((c) => (c.value ? [c.value] : []));
  const model: TableModel = {
    title: data.plainText(props.title),
    subtitle: data.plainText(props.subtitle),
    head: "Item",
    columns: periods.map((p, i) => ({ key: `period-${i}`, label: p || (i === 0 ? "First" : "Second"), ...measure })),
    rows: items.map((r, i) => ({
      key: `item-${i}`,
      label: r.input.label,
      highlight: Boolean(r.input.highlight),
      fell: Boolean(r.input.highlight) && fell,
      cells: [r.from, r.to].map((c) => ({ ref: c.ref, value: c.value?.value ?? null })),
    })),
    sources: sourcesOf(...ok.map((v) => v.sources)),
  };
  return { periods, items, measure, model };
}

export const SLOPE = {
  /** The plot's starting height in units (18rem); it grows when the labels need more. */
  height: 288,
  /** Characters a right-hand label's line holds (its 10em gutter). */
  lineChars: 18,
} as const;

/** Geometry: positions as % of the plot's height, labels nudged apart on each side, the highlight drawn last. */
export function slopePlot(chart: SlopeData, data: PageDataResolver): SlopePlotProps {
  const measure = chart.measure as Measure;
  const values = chart.items.flatMap((r) => [r.from, r.to].flatMap((c) => (c.value ? [Number(c.value.value)] : [])));
  let lo = Math.min(...values);
  let hi = Math.max(...values);
  if (lo === hi) {
    const pad = Math.abs(hi) || 1;
    lo -= pad / 2;
    hi += pad / 2;
  }
  // Items with at least one value, the highlight last so it is drawn on top.
  const shown = chart.items
    .filter((r) => r.from.value || r.to.value)
    .sort((a, b) => Number(Boolean(a.input.highlight)) - Number(Boolean(b.input.highlight)));
  const text = (c: Cell) => (c.value ? displayValue(c.value.value, measure) : MISSING_TEXT);
  const line = SANKEY.lineHeight;
  // Room above the top dot and below the bottom one for half a label.
  const inset = line / 2 + 2;
  let height: number = SLOPE.height;
  let tops: { from: number[]; to: number[] } = { from: [], to: [] };
  const yAt = (v: number, h: number) => inset + ((hi - v) / (hi - lo)) * (h - 2 * inset);
  // A missing end's label sits level with the other end.
  const ends = (h: number) =>
    shown.map((r) => {
      const f = r.from.value ? yAt(Number(r.from.value.value), h) : null;
      const t = r.to.value ? yAt(Number(r.to.value.value), h) : null;
      return { from: f ?? (t as number), to: t ?? (f as number) };
    });
  for (let attempt = 0; attempt < 6; attempt++) {
    const at = ends(height);
    const side = (key: "from" | "to", lines: (i: number) => number) => {
      const order = shown.map((_, i) => i).sort((a, b) => (at[a]?.[key] ?? 0) - (at[b]?.[key] ?? 0));
      const placed = stackLabels(
        order.map((i) => ({ centre: at[i]?.[key] ?? 0, h: lines(i) * line })),
        height,
      );
      const out: number[] = [];
      order.forEach((i, j) => {
        out[i] = placed.tops[j] as number;
      });
      return { tops: out, need: placed.need };
    };
    const left = side("from", () => 1);
    const right = side("to", (i) => {
      const r = shown[i] as (typeof shown)[number];
      return wrapLines(`${text(r.to)} ${r.input.label}`, SLOPE.lineChars);
    });
    tops = { from: left.tops, to: right.tops };
    const need = Math.max(left.need, right.need);
    if (need <= height) break;
    height = Math.ceil(need);
  }
  const at = ends(height);
  const pct = (v: number) => (v / height) * 100;
  const items: SlopeItem[] = shown.map((r, i) => ({
    key: `item-${chart.items.indexOf(r)}`,
    label: r.input.label,
    role: r.role,
    thin: isThin(r.role),
    highlight: Boolean(r.input.highlight),
    from: {
      y: r.from.value ? pct(at[i]?.from ?? 0) : null,
      labelTop: pct(tops.from[i] ?? 0),
      value: text(r.from),
      ref: r.from.ref,
    },
    to: {
      y: r.to.value ? pct(at[i]?.to ?? 0) : null,
      labelTop: pct(tops.to[i] ?? 0),
      value: text(r.to),
      ref: r.to.ref,
    },
    tooltip: [r.from, r.to].map((c, j) => ({
      value: c.value ? displayValue(c.value.value, measure) : null,
      series: chart.periods[j] || null,
      role: r.role,
      ref: c.ref,
      source: c.value ? tooltipSource(data, c.ref) : null,
    })),
  }));
  return { summary: chart.model ? modelSummary(chart.model) : "", height, periods: chart.periods, items };
}

export const Slope: ComponentRenderer<SlopeProps> = ({ props, statementId }) => {
  const data = usePageData();
  const chart = slopeData(props, data);
  const plotted = chart.items.some((r) => r.from.value || r.to.value);
  return (
    <ChartFigure statementId={statementId} title={props.title} subtitle={props.subtitle} model={chart.model}>
      {plotted ? <Island name="SlopePlot" component={SlopePlot} props={slopePlot(chart, data)} /> : null}
    </ChartFigure>
  );
};
