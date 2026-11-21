// Waterfall (design/charts.md, "Waterfall"): revenue down to profit, one row per line of the income statement in
// the company's own words. Totals (net sales, gross margin, operating income) run from zero in comparison blue;
// deductions and additions float between where the running amount was and where it ends, in neutral grey, each
// labelled with its sign; the result (the last total, usually net income) is the highlight, or the neutral
// highlight when it is a loss (design/charts.md, "When the subject fell"). Horizontal, in HTML and CSS, so the
// labels stay full size on phones. The plot is the horizontal bar island, so a waterfall adds no JavaScript.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { withSign } from "../../format";
import { usePageData } from "../../renderer/data/context";
import { MISSING_TEXT, type PageDataResolver, type ResolvedValue } from "../../renderer/data/resolve";
import { Island } from "../../renderer/islands/Island";
import BarChartPlot, { type BarChartPlotProps, type BarRow } from "../BarChart/BarChartPlot.client";
import { ChartFigure } from "../ChartFigure/ChartFigure";
import {
  type ColourRole,
  displayValue,
  type Measure,
  sameAxis,
  sourcesOf,
  subjectRole,
  type TableModel,
  type TableRow,
  tooltipSource,
  valueFell,
} from "../ChartFigure/model";
import { type Mismatch, type Step, type StepKind, waterfallMismatches, waterfallSteps } from "./steps";

export interface WaterfallRowInput {
  /** The line's name as the company writes it ("Cost of sales"). */
  label: string;
  /** dp_… or dv_…, the value as published. */
  value: string;
  /** "total" runs from zero; "deduct" subtracts the value as published; "add" adds it as published. */
  kind: StepKind;
}

export interface WaterfallProps {
  title: string;
  subtitle: string;
  rows: WaterfallRowInput[];
}

type OkValue = Extract<ResolvedValue, { ok: true }>;

export interface ResolvedLine {
  label: string;
  ref: string;
  kind: StepKind | "segment";
  value: OkValue | null;
}

export interface WaterfallData {
  /** The rows on the first value's axis, in the order given. */
  lines: ResolvedLine[];
  measure: Measure | null;
  steps: Step[];
  /** The result: the last total (usually net income), or -1 when there is no total. */
  result: number;
  /** True when the result is a loss: it takes the neutral highlight, never crimson. */
  fell: boolean;
  mismatches: Mismatch[];
  model: TableModel | null;
}

/** Resolves references and keeps those on the first value's axis (another unit, currency or scale is left out). */
export function resolveLines(
  inputs: readonly { label: string; value: string; kind: ResolvedLine["kind"] }[],
  data: PageDataResolver,
  measure: Measure | null = null,
): { lines: ResolvedLine[]; measure: Measure | null } {
  const resolved = inputs.map((r) => {
    const v = data.value(r.value);
    return { label: r.label, ref: r.value, kind: r.kind, value: v.ok ? v : null };
  });
  const first = resolved.find((r) => r.value !== null)?.value ?? null;
  const axis: Measure | null =
    measure ?? (first ? { unit: first.unit, currency: first.currency, scale: first.scale } : null);
  const lines = resolved.filter((r) => r.value === null || (axis !== null && sameAxis([axis, r.value])));
  return { lines, measure: axis };
}

/** The data table: one row per line, the values exactly as published, the result highlighted. */
export function linesModel(
  lines: readonly ResolvedLine[],
  opts: { title: string; subtitle: string; measure: Measure; result: number; fell: boolean },
): TableModel {
  const ok = lines.flatMap((l) => (l.value ? [l.value] : []));
  const periods = new Set(ok.map((v) => v.period?.label ?? ""));
  const period = periods.size === 1 ? [...periods][0] : "";
  const rows: TableRow[] = lines.map((l, i) => ({
    key: `${l.ref}-${i}`,
    label: l.label,
    highlight: i === opts.result,
    fell: i === opts.result && opts.fell,
    cells: [{ ref: l.ref, value: l.value?.value ?? null }],
  }));
  return {
    title: opts.title,
    subtitle: opts.subtitle,
    head: "Item",
    columns: [{ key: "value", label: period || "Value", ...opts.measure }],
    rows,
    sources: sourcesOf(...ok.map((v) => v.sources)),
  };
}

export function waterfallData(props: WaterfallProps, data: PageDataResolver): WaterfallData {
  const { lines, measure } = resolveLines(props.rows, data);
  return waterfallFromLines(lines, measure, {
    title: data.plainText(props.title),
    subtitle: data.plainText(props.subtitle),
  });
}

/** The waterfall of resolved statement rows (the Sankey's fallback builds its waterfall from its own rows). */
export function waterfallFromLines(
  lines: ResolvedLine[],
  measure: Measure | null,
  text: { title: string; subtitle: string },
): WaterfallData {
  const inputs = lines.map((l) => ({ kind: l.kind as StepKind, value: l.value?.value ?? null }));
  const steps = waterfallSteps(inputs);
  let result = -1;
  lines.forEach((l, i) => {
    if (l.kind === "total") result = i;
  });
  const fell = result >= 0 && valueFell(lines[result]?.value?.value);
  const mismatches = waterfallMismatches(inputs);
  const model = measure ? linesModel(lines, { ...text, measure, result, fell }) : null;
  return { lines, measure, steps, result, fell, mismatches, model };
}

/**
 * The accessible name: the title, then every line with its value as published ("Total cost of sales, $210.4bn").
 * A statement is read line by line, so every line is listed, however many.
 */
export function linesSummary(model: TableModel): string {
  const column = model.columns[0];
  const say = (r: TableRow) => {
    const v = r.cells[0]?.value ?? null;
    return `${r.label}, ${v === null || !column ? MISSING_TEXT.toLowerCase() : displayValue(v, column)}`;
  };
  return `${model.title}. ${model.rows.map(say).join("; ")}.`;
}

/** A line's label on the plot: a total as it is; a change with its sign ("−$210.4bn", "+$0.3bn"). */
export function stepLabel(step: Step, value: string, measure: Measure): string {
  if (step.kind === "total" || step.effect === null) return displayValue(value, measure);
  return withSign(displayValue(step.effect, measure), step.effect);
}

/** Geometry for the horizontal bar island: each line's span along the track, the zero line, room for labels. */
export function waterfallPlot(chart: WaterfallData, data: PageDataResolver): BarChartPlotProps {
  const measure = chart.measure as Measure;
  const ends = chart.steps.flatMap((s) => [s.start, s.end]).filter((v): v is string => v !== null);
  const values = ends.map(Number);
  const min = Math.min(0, ...values);
  const max = Math.max(0, ...values);
  const span = max - min || 1;
  const x = (v: number) => ((v - min) / span) * 100;
  const zero = x(0);
  const rows: BarRow[] = chart.lines.map((line, i) => {
    const step = chart.steps[i] as Step;
    const isResult = i === chart.result;
    const role: ColourRole = isResult ? subjectRole(chart.fell) : line.kind === "total" ? "comparison" : "neutral";
    const key = `${line.ref}-${i}`;
    const shown = line.value ? stepLabel(step, line.value.value, measure) : MISSING_TEXT;
    const tooltip = [
      {
        value: line.value ? shown : null,
        series: null,
        role,
        ref: line.ref,
        source: line.value ? tooltipSource(data, line.ref) : null,
      },
    ];
    const base = { key, label: line.label, role, value: shown, highlight: isResult, ref: line.ref, tooltip };
    if (step.start === null || step.end === null) {
      // No value, or a line after a missing one that cannot be placed: hatched, with its value (or "No data").
      return { ...base, start: null, width: null, negative: false };
    }
    const a = x(Number(step.start));
    const b = x(Number(step.end));
    // A total below zero is labelled left of its bar; every other label sits just past the bar's right end.
    const negative = step.kind === "total" && Number(step.end) < 0;
    return { ...base, start: Math.min(a, b), width: Math.abs(b - a), negative };
  });
  const widest = (neg: boolean) =>
    Math.max(0, ...rows.filter((r) => r.negative === neg && r.start !== null).map((r) => r.value.length));
  const unplaced = Math.max(0, ...rows.filter((r) => r.start === null).map((r) => r.value.length));
  return {
    summary: chart.model ? linesSummary(chart.model) : "",
    rows,
    zero,
    room: {
      negative: rows.some((r) => r.negative) ? `${widest(true) + 1}ch` : "0ch",
      positive: `${Math.max(widest(false), unplaced) + 1}ch`,
    },
  };
}

/** A line under the plot when the lines shown do not add up to a total: the source has lines left out. */
export function mismatchNote(chart: Pick<WaterfallData, "mismatches" | "lines">): string | null {
  if (chart.mismatches.length === 0) return null;
  const names = chart.mismatches.map((m) => chart.lines[m.index]?.label ?? "").filter(Boolean);
  return `The lines shown do not add up to ${names.join(" or ")}: the source has lines not shown here.`;
}

/** The waterfall's plot, for the Waterfall chart and the Sankey's narrow-screen fallback. */
export function WaterfallIsland({ chart, data }: { chart: WaterfallData; data: PageDataResolver }) {
  return <Island name="BarChartPlot" component={BarChartPlot} props={waterfallPlot(chart, data)} />;
}

export const Waterfall: ComponentRenderer<WaterfallProps> = ({ props, statementId }) => {
  const data = usePageData();
  const chart = waterfallData(props, data);
  const plotted = chart.lines.some((l) => l.value !== null);
  const note = mismatchNote(chart);
  return (
    <ChartFigure
      statementId={statementId}
      title={props.title}
      subtitle={props.subtitle}
      model={chart.model}
      sourceNote={note ? <span className="waterfall__note">{note}</span> : undefined}
    >
      {plotted ? (
        <div className="waterfall">
          <WaterfallIsland chart={chart} data={data} />
        </div>
      ) : null}
    </ChartFigure>
  );
};
