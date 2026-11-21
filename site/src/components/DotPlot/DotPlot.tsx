// Dot plot and dumbbell (design/charts.md, "Choosing the form": distribution; two points in time, several items).
// One row per item. With one value per row it is a dot plot: 8px dots on a shared scale, blue, or the highlighted
// row in crimson and the rest context. With two values (keys name them: "fiscal 2023", "fiscal 2024") it is a
// dumbbell: the first value in context, the second in comparison blue, joined by a line; the highlighted row's
// second dot is crimson, or the neutral highlight when it is below the first (design/charts.md, "When the subject
// fell"). Positions need no zero, so there is no axis: every dot carries its value. HTML and CSS; an island.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { usePageData } from "../../renderer/data/context";
import { MISSING_TEXT, type PageDataResolver, type ResolvedValue } from "../../renderer/data/resolve";
import { Island } from "../../renderer/islands/Island";
import { ChartFigure, type LegendItem } from "../ChartFigure/ChartFigure";
import {
  type ColourRole,
  displayValue,
  type Measure,
  modelSummary,
  sameAxis,
  sourcesOf,
  subjectRole,
  type TableModel,
  tooltipSource,
  valueFell,
} from "../ChartFigure/model";
import DotPlotPlot, { type DotMark, type DotPlotPlotProps, type DotRow } from "./DotPlotPlot.client";

export interface DotRowInput {
  label: string;
  values: string[];
  highlight?: boolean | null;
}

export interface DotPlotProps {
  title: string;
  subtitle: string;
  rows: DotRowInput[];
  /** Names of the two values in a dumbbell ("{pd_fy2023}", "{pd_fy2024}"); omit for one value per row. */
  keys?: string[] | null;
  sort?: "value" | "given" | null;
}

type OkValue = Extract<ResolvedValue, { ok: true }>;

export interface DotData {
  dumbbell: boolean;
  keys: string[];
  rows: { input: DotRowInput; cells: { ref: string; value: OkValue | null }[] }[];
  measure: Measure | null;
  model: TableModel | null;
}

export function dotPlotData(props: DotPlotProps, data: PageDataResolver): DotData {
  const dumbbell = props.rows.some((r) => r.values.length > 1);
  const width = dumbbell ? 2 : 1;
  const keys = Array.from({ length: width }, (_, i) => {
    const k = props.keys?.[i];
    return k ? data.plainText(k) : width === 1 ? "Value" : i === 0 ? "First" : "Second";
  });
  const raw = props.rows.map((r) => ({
    input: r,
    cells: Array.from({ length: width }, (_, i) => {
      const ref = r.values[i] ?? "";
      const v = ref ? data.value(ref) : null;
      return { ref, value: v?.ok ? v : null };
    }),
  }));
  const first = raw.flatMap((r) => r.cells).find((c) => c.value !== null)?.value ?? null;
  const measure: Measure | null = first ? { unit: first.unit, currency: first.currency, scale: first.scale } : null;
  // A value in another unit, currency or scale is left out, never converted.
  const rows = raw.map((r) => ({
    ...r,
    cells: r.cells.map((c) => (c.value && measure && !sameAxis([measure, c.value]) ? { ref: c.ref, value: null } : c)),
  }));
  if (props.sort !== "given") {
    const key = (r: (typeof rows)[number]) => r.cells[width - 1]?.value?.value ?? null;
    rows.sort((a, b) => {
      const ka = key(a);
      const kb = key(b);
      if (ka === null) return kb === null ? 0 : 1;
      if (kb === null) return -1;
      return Number(kb) - Number(ka);
    });
  }
  if (!measure) return { dumbbell, keys, rows, measure, model: null };
  const ok = rows.flatMap((r) => r.cells.flatMap((c) => (c.value ? [c.value] : [])));
  const model: TableModel = {
    title: data.plainText(props.title),
    subtitle: data.plainText(props.subtitle),
    head: "Category",
    columns: keys.map((k, i) => ({ key: `value-${i}`, label: k, ...measure })),
    rows: rows.map((r, i) => ({
      key: `row-${i}`,
      label: r.input.label,
      highlight: Boolean(r.input.highlight),
      fell: Boolean(r.input.highlight) && rowFell(r.cells.map((c) => c.value?.value ?? null)),
      cells: r.cells.map((c) => ({ ref: c.ref || null, value: c.value?.value ?? null })),
    })),
    sources: sourcesOf(...ok.map((v) => v.sources)),
  };
  return { dumbbell, keys, rows, measure, model };
}

/**
 * The highlighted row fell: one value below zero (the bar rule), or two values with the second below the first
 * (the line rule). A missing value never fell.
 */
export function rowFell(values: readonly (string | null)[]): boolean {
  if (values.length === 1) return valueFell(values[0]);
  const [a, b] = values;
  if (a === null || b === null || a === undefined || b === undefined) return false;
  return Number(b) < Number(a);
}

/** Roles by position, never by value: one value blue (or highlight and context); two, context then comparison. */
function roleOf(chart: DotData, row: DotData["rows"][number], i: number, anyHighlight: boolean): ColourRole {
  const highlighted = Boolean(row.input.highlight);
  const last = i === chart.keys.length - 1;
  if (highlighted && last) return subjectRole(rowFell(row.cells.map((c) => c.value?.value ?? null)));
  if (chart.dumbbell) return last ? "comparison" : "context";
  return anyHighlight ? "context" : "cat-1";
}

export function dotPlot(chart: DotData, data: PageDataResolver): DotPlotPlotProps {
  const measure = chart.measure as Measure;
  const values = chart.rows.flatMap((r) => r.cells.flatMap((c) => (c.value ? [Number(c.value.value)] : [])));
  let lo = Math.min(...values);
  let hi = Math.max(...values);
  if (lo === hi) {
    const pad = Math.abs(hi) || 1;
    lo -= pad / 2;
    hi += pad / 2;
  }
  const x = (v: number) => ((v - lo) / (hi - lo)) * 100;
  const anyHighlight = chart.rows.some((r) => r.input.highlight);
  const rows: DotRow[] = chart.rows.map((r, ri) => {
    const xs = r.cells.map((c) => (c.value ? x(Number(c.value.value)) : null));
    const present = xs.filter((v): v is number => v !== null);
    // "No data" sits past the end of the track away from the other dot, in that side's label room, and the other
    // dot's value goes on its far side, so the two never meet however narrow the track.
    const missingSide: "left" | "right" = present[0] !== undefined && present[0] < 50 ? "right" : "left";
    const anyMissing = present.length < xs.length;
    const dots: DotMark[] = r.cells.map((c, i) => {
      const role = roleOf(chart, r, i, anyHighlight);
      const xi = xs[i] ?? null;
      if (xi === null) {
        return { key: `${ri}-${i}`, role, x: null, value: MISSING_TEXT, side: missingSide, ref: c.ref || null };
      }
      // Two dots: the smaller value's label on its left, the larger's on its right. One dot: on its right, unless
      // the row's other value is missing (then away from the "No data").
      const otherX = xs[1 - i];
      const side: "left" | "right" = anyMissing
        ? missingSide === "right"
          ? "left"
          : "right"
        : chart.dumbbell && otherX !== null && otherX !== undefined && (xi < otherX || (xi === otherX && i === 0))
          ? "left"
          : "right";
      return {
        key: `${ri}-${i}`,
        role,
        x: xi,
        value: displayValue((c.value as OkValue).value, measure),
        side,
        ref: c.ref,
      };
    });
    const line =
      chart.dumbbell && present.length === 2
        ? { left: Math.min(...present), width: Math.abs((present[1] as number) - (present[0] as number)) }
        : null;
    return {
      key: `row-${ri}`,
      label: r.input.label,
      highlight: Boolean(r.input.highlight),
      dots,
      line,
      tooltip: r.cells.map((c, i) => ({
        value: c.value ? displayValue(c.value.value, measure) : null,
        series: chart.dumbbell ? (chart.keys[i] ?? null) : null,
        role: roleOf(chart, r, i, anyHighlight),
        ref: c.ref || null,
        source: c.value ? tooltipSource(data, c.ref) : null,
      })),
    };
  });
  const widest = (side: "left" | "right") =>
    Math.max(0, ...rows.flatMap((r) => r.dots.filter((d) => d.side === side).map((d) => d.value.length)));
  return {
    summary: chart.model ? modelSummary(chart.model) : "",
    rows,
    room: { left: `${widest("left") + 1}ch`, right: `${widest("right") + 1}ch` },
  };
}

export function dotLegend(chart: DotData): LegendItem[] {
  if (!chart.dumbbell) return [];
  return chart.keys.map((k, i) => ({
    key: `key-${i}`,
    label: k,
    role: i === chart.keys.length - 1 ? "comparison" : "context",
    shape: "dot",
  }));
}

export const DotPlot: ComponentRenderer<DotPlotProps> = ({ props, statementId }) => {
  const data = usePageData();
  const chart = dotPlotData(props, data);
  const plotted = chart.rows.some((r) => r.cells.some((c) => c.value !== null));
  return (
    <ChartFigure
      statementId={statementId}
      title={props.title}
      subtitle={props.subtitle}
      legend={dotLegend(chart)}
      model={chart.model}
    >
      {plotted ? <Island name="DotPlotPlot" component={DotPlotPlot} props={dotPlot(chart, data)} /> : null}
    </ChartFigure>
  );
};
