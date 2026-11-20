// Horizontal bar chart (design/charts.md): ranking or comparing categories, sorted by value unless the order is
// natural, bars from zero, 22px thick with 8px between rows, the value just past each bar's end. The highlighted
// bar is crimson with a bold crimson label, or the neutral highlight when it fell (design/charts.md, "When the
// subject fell"); the others are one colour (context, or blue when nothing is highlighted), never chosen by value. HTML and CSS, so long labels fit on phones; the plot is an island.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { usePageData } from "../../renderer/data/context";
import { MISSING_TEXT, type PageDataResolver, type ResolvedValue } from "../../renderer/data/resolve";
import { Island } from "../../renderer/islands/Island";
import { ChartFigure } from "../ChartFigure/ChartFigure";
import {
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
import BarChartPlot, { type BarChartPlotProps, type BarRow } from "./BarChartPlot.client";

export interface BarInput {
  label: string;
  value: string;
  highlight?: boolean | null;
}

export interface BarChartProps {
  title: string;
  subtitle: string;
  bars: BarInput[];
  /** "value" (the default): largest first. "given": the page writer's order, for natural orders (size bands). */
  sort?: "value" | "given" | null;
}

type OkValue = Extract<ResolvedValue, { ok: true }>;

interface ResolvedBar {
  input: BarInput;
  value: OkValue | null;
}

export interface BarChartData {
  bars: ResolvedBar[];
  measure: Measure | null;
  model: TableModel | null;
}

/** Resolves the bars, keeps those on the first value's axis, sorts them, and builds the table model. */
export function barChartData(props: BarChartProps, data: PageDataResolver): BarChartData {
  const resolved = props.bars.map((b) => {
    const v = data.value(b.value);
    return { input: b, value: v.ok ? v : null };
  });
  const firstOk = resolved.find((b) => b.value !== null)?.value ?? null;
  const measure: Measure | null = firstOk
    ? { unit: firstOk.unit, currency: firstOk.currency, scale: firstOk.scale }
    : null;
  // A bar in another unit, currency or scale cannot share the axis; it is left out, never converted.
  const bars = resolved.filter((b) => b.value === null || (measure !== null && sameAxis([measure, b.value])));
  if (props.sort !== "given") {
    bars.sort((a, b) => {
      if (a.value === null) return b.value === null ? 0 : 1;
      if (b.value === null) return -1;
      return Number(b.value.value) - Number(a.value.value);
    });
  }
  if (!measure) return { bars, measure, model: null };
  const okValues = bars.flatMap((b) => (b.value ? [b.value] : []));
  const labels = new Set(okValues.map((v) => v.label));
  const periods = new Set(okValues.map((v) => v.period?.label ?? ""));
  const columnLabel =
    labels.size === 1 ? [...labels][0] : periods.size === 1 && [...periods][0] ? [...periods][0] : "Value";
  const model: TableModel = {
    title: data.plainText(props.title),
    subtitle: data.plainText(props.subtitle),
    head: "Category",
    columns: [{ key: "value", label: columnLabel as string, ...measure }],
    rows: bars.map((b, i) => ({
      key: `${b.input.value}-${i}`,
      label: b.input.label,
      highlight: Boolean(b.input.highlight),
      fell: Boolean(b.input.highlight) && valueFell(b.value?.value),
      cells: [{ ref: b.input.value, value: b.value?.value ?? null }],
    })),
    sources: sourcesOf(...okValues.map((v) => v.sources)),
  };
  return { bars, measure, model };
}

/** Geometry for the island: each bar's start and width along the track, the zero line, room for labels. */
export function barPlot(chart: BarChartData, data: PageDataResolver): BarChartPlotProps {
  const measure = chart.measure as Measure;
  const values = chart.bars.flatMap((b) => (b.value ? [Number(b.value.value)] : []));
  const min = Math.min(0, ...values);
  const max = Math.max(0, ...values);
  const span = max - min || 1;
  const x = (v: number) => ((v - min) / span) * 100;
  const zero = x(0);
  const anyHighlight = chart.bars.some((b) => b.input.highlight);
  const rows: BarRow[] = chart.bars.map((b, i) => {
    // The highlighted bar is crimson unless it fell (below zero): then the neutral highlight, never crimson.
    const role = b.input.highlight ? subjectRole(valueFell(b.value?.value)) : anyHighlight ? "context" : "cat-1";
    const shown = b.value ? displayValue(b.value.value, measure) : MISSING_TEXT;
    const tooltip = [
      {
        value: b.value ? shown : null,
        series: null,
        role,
        ref: b.input.value,
        source: b.value ? tooltipSource(data, b.input.value) : null,
      },
    ];
    if (!b.value) {
      return {
        key: `${b.input.value}-${i}`,
        label: b.input.label,
        role,
        start: null,
        width: null,
        negative: false,
        value: shown,
        highlight: Boolean(b.input.highlight),
        ref: b.input.value,
        tooltip,
      };
    }
    const v = Number(b.value.value);
    const start = Math.min(x(v), zero);
    return {
      key: `${b.input.value}-${i}`,
      label: b.input.label,
      role,
      start,
      width: Math.abs(x(v) - zero),
      negative: v < 0,
      value: shown,
      highlight: Boolean(b.input.highlight),
      ref: b.input.value,
      tooltip,
    };
  });
  const widest = (neg: boolean) =>
    Math.max(0, ...rows.filter((r) => r.negative === neg && r.start !== null).map((r) => r.value.length));
  const missingWidth = rows.some((r) => r.start === null) ? MISSING_TEXT.length : 0;
  return {
    summary: chart.model ? modelSummary(chart.model) : "",
    rows,
    zero,
    room: {
      negative: values.some((v) => v < 0) ? `${widest(true) + 1}ch` : "0ch",
      positive: `${Math.max(widest(false), missingWidth) + 1}ch`,
    },
  };
}

export const BarChart: ComponentRenderer<BarChartProps> = ({ props, statementId }) => {
  const data = usePageData();
  const chart = barChartData(props, data);
  const plotted = chart.bars.some((b) => b.value !== null);
  return (
    <ChartFigure statementId={statementId} title={props.title} subtitle={props.subtitle} model={chart.model}>
      {plotted ? <Island name="BarChartPlot" component={BarChartPlot} props={barPlot(chart, data)} /> : null}
    </ChartFigure>
  );
};
