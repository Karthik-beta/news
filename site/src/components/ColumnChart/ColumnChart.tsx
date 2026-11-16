// Column chart (design/charts.md): change over time in discrete periods, columns from zero, one value axis, solid
// horizontal gridlines. One series is blue; naming a period as the highlight makes that column crimson with a bold
// label and the rest context. Two or three series are grouped columns in the categorical colours, with a legend.
// The plot is an island for its tooltips; the frame, legend, source line and data table are static HTML.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { usePageData } from "../../renderer/data/context";
import type { PageDataResolver } from "../../renderer/data/resolve";
import { Island } from "../../renderer/islands/Island";
import { ChartFigure, type LegendItem } from "../ChartFigure/ChartFigure";
import {
  colourRoles,
  displayValue,
  type Measure,
  modelSummary,
  type OkSeries,
  periodSlots,
  pointAt,
  resolveSeries,
  sameAxis,
  seriesTable,
  type TableModel,
  tooltipSource,
} from "../ChartFigure/model";
import { axisWidth, bareNumber, displayExponent, niceScale, xLabelPlan } from "../ChartFigure/scale";
import ColumnChartPlot, { type ColumnBand, type ColumnChartPlotProps } from "./ColumnChartPlot.client";

export interface ColumnChartProps {
  title: string;
  subtitle: string;
  series: string[];
  highlight?: string | null;
}

export interface ColumnChartData {
  series: OkSeries[];
  model: TableModel | null;
}

/** The series that share the first one's axis (never converted to match), and the table they make. */
export function columnChartData(props: ColumnChartProps, data: PageDataResolver): ColumnChartData {
  const resolved = resolveSeries(data, props.series);
  const first = resolved[0];
  const series = first ? resolved.filter((s) => sameAxis([first, s])) : [];
  if (series.length === 0) return { series, model: null };
  const model = seriesTable(series, {
    title: data.plainText(props.title),
    subtitle: data.plainText(props.subtitle),
    highlight: series.length === 1 ? props.highlight : null,
  });
  return { series, model };
}

/** Geometry and labels for the island, all computed here so the client only draws and responds. */
export function columnPlot(
  { series, model }: { series: OkSeries[]; model: TableModel },
  props: ColumnChartProps,
  data: PageDataResolver,
): ColumnChartPlotProps {
  const first = series[0] as OkSeries;
  const measure: Measure = { unit: first.unit, currency: first.currency, scale: first.scale };
  const slots = periodSlots(series);
  const values = series.flatMap((s) => s.points.map((p) => p.value).filter((v): v is string => v !== null));
  const exponent = displayExponent(values, measure);
  const scale = niceScale(
    values.map((v) => Number(v)),
    measure,
    { includeZero: true, displayExponent: exponent },
  );
  const single = series.length === 1;
  const highlight = single ? (props.highlight ?? null) : null;
  // Grouped columns: categorical colours by series, in the order given. One series: blue, or the highlighted
  // period in crimson and the rest context. Never chosen by the value.
  const roles = colourRoles(series.map((s) => s.ref));
  const roleOf = (j: number, isHighlight: boolean) =>
    !single ? (roles[j] ?? "cat-1") : highlight === null ? "cat-1" : isHighlight ? "highlight" : "context";
  const n = slots.length;
  const lastWithValue = (() => {
    for (let i = n - 1; i >= 0; i--) {
      const slot = slots[i];
      if (slot && series.some((s) => pointAt(s, slot.ref)?.value != null)) return i;
    }
    return -1;
  })();
  const bands: ColumnBand[] = slots.map((slot, i) => {
    const isHighlight = highlight !== null && slot.ref === highlight;
    return {
      key: slot.ref,
      label: slot.label,
      left: (i / n) * 100,
      width: 100 / n,
      highlight: isHighlight,
      marks: series.map((s, j) => {
        const p = pointAt(s, slot.ref);
        const role = roleOf(j, isHighlight);
        if (!p || p.value === null) {
          return {
            series: s.ref,
            role,
            top: null,
            height: null,
            negative: false,
            label: null,
            show: "never" as const,
            ref: p?.ref ?? null,
          };
        }
        const v = Number(p.value);
        const yv = scale.y(v);
        return {
          series: s.ref,
          role,
          top: Math.min(yv, scale.zero),
          height: Math.abs(yv - scale.zero),
          negative: v < 0,
          label: bareNumber(p.value, measure, exponent),
          show: !single
            ? ("never" as const)
            : isHighlight || i === lastWithValue
              ? ("always" as const)
              : ("room" as const),
          ref: p.ref,
        };
      }),
      tooltip: series.map((s, j) => {
        const p = pointAt(s, slot.ref);
        return {
          value: p?.value == null ? null : displayValue(p.value, s),
          series: single ? null : s.label,
          role: roleOf(j, isHighlight),
          ref: p?.ref ?? null,
          source: p?.value == null ? null : tooltipSource(data, p.ref),
        };
      }),
    };
  });
  const density = n <= 5 ? "n5" : n <= 8 ? "n8" : n <= 12 ? "n12" : "key";
  return {
    summary: modelSummary(model),
    axisWidth: axisWidth(scale.ticks),
    ticks: scale.ticks.map((t) => ({ y: t.y, label: t.label, baseline: t.baseline })),
    zero: scale.zero,
    bands,
    xLabels: xLabelPlan(
      slots.map((s) => s.label),
      slots.map((_, i) => ((i + 0.5) / n) * 100),
    ),
    density,
  };
}

/** Legend items: only for grouped columns (two or more series). */
export function columnLegend(series: OkSeries[]): LegendItem[] {
  const roles = colourRoles(series.map((s) => s.ref));
  return series.map((s, i) => ({ key: s.ref, label: s.label, role: roles[i] ?? "cat-1", shape: "square" }));
}

export const ColumnChart: ComponentRenderer<ColumnChartProps> = ({ props, statementId }) => {
  const data = usePageData();
  const chart = columnChartData(props, data);
  const plotted = chart.model?.rows.some((r) => r.cells.some((c) => c.value !== null));
  return (
    <ChartFigure
      statementId={statementId}
      title={props.title}
      subtitle={props.subtitle}
      legend={chart.series.length > 1 ? columnLegend(chart.series) : []}
      model={chart.model}
    >
      {plotted && chart.model ? (
        <Island
          name="ColumnChartPlot"
          component={ColumnChartPlot}
          props={columnPlot({ series: chart.series, model: chart.model }, props, data)}
        />
      ) : null}
    </ChartFigure>
  );
};
