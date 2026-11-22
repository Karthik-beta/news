// Heatmap (design/charts.md, "Heatmaps"): a matrix of two categories, one row per series and one column per period,
// each cell shaded by its value on the sequential blue ramp (design/color.md, "Sequential ramps"). The classes are the
// round-number ranges of the chart's value axis (3 to 5 of them), each a fixed ramp step validated in both themes;
// a legend above the grid names each range. Values are written in the cells when every column has room for them,
// and are always in the tooltip and the data table. Colour here is magnitude, so nothing is crimson and nothing
// is highlighted. A missing value is a hatched cell. HTML and CSS; an island for the tooltips.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { usePageData } from "../../renderer/data/context";
import { MISSING_TEXT, type PageDataResolver } from "../../renderer/data/resolve";
import { Island } from "../../renderer/islands/Island";
import { ChartFigure, hasValues } from "../ChartFigure/ChartFigure";
import {
  displayValue,
  type Measure,
  modelSummary,
  type OkSeries,
  periodSlots,
  pointAt,
  resolveSeries,
  sameAxis,
  type TableModel,
  tooltipSource,
  transposedSeriesTable,
} from "../ChartFigure/model";
import { bareNumber, displayExponent, niceScale, xLabelPlan } from "../ChartFigure/scale";
import HeatmapPlot, { type HeatmapPlotProps } from "./HeatmapPlot.client";

export interface HeatmapProps {
  title: string;
  subtitle: string;
  series: string[];
}

export interface HeatmapData {
  series: OkSeries[];
  model: TableModel | null;
}

/** The series that share the first one's unit (never converted to match); the table has a row per series. */
export function heatmapData(props: HeatmapProps, data: PageDataResolver): HeatmapData {
  const resolved = resolveSeries(data, props.series);
  const first = resolved[0];
  const series = first ? resolved.filter((s) => sameAxis([first, s])) : [];
  if (series.length === 0) return { series, model: null };
  const model = transposedSeriesTable(series, {
    title: data.plainText(props.title),
    subtitle: data.plainText(props.subtitle),
  });
  return { series, model };
}

/** The validated ramp has five steps; fewer classes take steps spread across it, so the ends are always used. */
export function rampSteps(classes: number): number[] {
  if (classes <= 1) return [3];
  return Array.from({ length: classes }, (_, i) => 1 + Math.round((i * 4) / (classes - 1)));
}

export interface HeatmapClasses {
  /** The class edges as exact decimals, lowest first (one more than the classes), and their axis labels. */
  edges: { value: string; label: string }[];
  /** The ramp step (1-5) of each class. */
  steps: number[];
  /** The class of a value: the number of inner edges at or below it, so a value on an edge goes up a class. */
  classOf: (value: string) => number;
}

/** Classes on the round numbers of a value axis over the data (no zero forced in): 3 to 5 equal ranges. */
export function heatmapClasses(values: readonly string[], measure: Measure, exponent: number): HeatmapClasses {
  const scale = niceScale(
    values.map((v) => Number(v)),
    measure,
    { includeZero: false, displayExponent: exponent },
  );
  const edges = scale.ticks.map((t) => ({ value: t.value, label: t.label }));
  const inner = edges.slice(1, -1).map((e) => Number(e.value));
  return {
    edges,
    steps: rampSteps(edges.length - 1),
    classOf: (value) => inner.filter((e) => Number(value) >= e).length,
  };
}

/** The island's props and the classes behind them (the legend's ranges). */
export function heatmapPlot(
  chart: HeatmapData & { model: TableModel },
  data: PageDataResolver,
): { plot: HeatmapPlotProps; classes: HeatmapClasses } {
  const { series, model } = chart;
  const first = series[0] as OkSeries;
  const measure: Measure = { unit: first.unit, currency: first.currency, scale: first.scale };
  const slots = periodSlots(series);
  const values = series.flatMap((s) => s.points.map((p) => p.value).filter((v): v is string => v !== null));
  const exponent = displayExponent(values, measure);
  const classes = heatmapClasses(values, measure, exponent);
  const rows = series.map((s) => ({
    key: s.ref,
    label: s.label,
    cells: slots.map((slot) => {
      const p = pointAt(s, slot.ref);
      const v = p?.value ?? null;
      return {
        key: `${s.ref}-${slot.ref}`,
        step: v === null ? null : (classes.steps[classes.classOf(v)] ?? null),
        label: v === null ? null : bareNumber(v, measure, exponent),
        ref: p?.ref ?? null,
        tooltip: {
          value: v === null ? null : displayValue(v, measure),
          series: null,
          role: "context",
          ref: p?.ref ?? null,
          source: v === null ? null : tooltipSource(data, p?.ref ?? null),
        },
      };
    }),
  }));
  // Values show when every column has room for the longest at the label size (about 0.6em a character, plus
  // padding), after the row names' column. The plot is a query container, so this holds at any text size.
  const longestName = Math.min(14, Math.max(4, ...series.map((s) => s.label.length)));
  const longestValue = Math.max(1, ...rows.flatMap((r) => r.cells.map((c) => c.label?.length ?? 0)));
  const need = longestName * 0.6 + 0.6 + slots.length * (longestValue * 0.6 + 1);
  const fit = FITS.find((f) => need <= f) ?? null;
  const n = slots.length;
  const plot: HeatmapPlotProps = {
    summary: modelSummary(model),
    nameWidth: `${longestName}ch`,
    columns: n,
    rows,
    periods: slots.map((s) => s.label),
    xLabels: xLabelPlan(
      slots.map((s) => s.label),
      slots.map((_, i) => ((i + 0.5) / n) * 100),
    ),
    fit,
  };
  return { plot, classes };
}

/** The plot widths, in em of its text, at which cell values are switched on (matched in Heatmap.css). */
export const FITS = [16, 20, 24, 30, 36, 44, 52] as const;

/** The legend: one key per class with its range, and "No data" when a cell is hatched. */
function HeatmapLegend({ classes, missing }: { classes: HeatmapClasses; missing: boolean }) {
  return (
    <ul className="chart-legend heatmap__legend text-label">
      {classes.steps.map((step, i) => (
        <li key={step} className="chart-legend__item">
          <span className={`chart-key chart-key--square heatmap__step--${step}`} aria-hidden="true" />
          {classes.edges[i]?.label} to {classes.edges[i + 1]?.label}
        </li>
      ))}
      {missing ? (
        <li className="chart-legend__item">
          <span className="chart-key chart-key--square chart-hatch" aria-hidden="true" />
          {MISSING_TEXT}
        </li>
      ) : null}
    </ul>
  );
}

export const Heatmap: ComponentRenderer<HeatmapProps> = ({ props, statementId }) => {
  const data = usePageData();
  const chart = heatmapData(props, data);
  const model = chart.model;
  let body = null;
  if (hasValues(model)) {
    const { plot, classes } = heatmapPlot({ series: chart.series, model }, data);
    const missing = plot.rows.some((r) => r.cells.some((c) => c.step === null));
    body = (
      <div className="heatmap">
        <HeatmapLegend classes={classes} missing={missing} />
        <Island name="HeatmapPlot" component={HeatmapPlot} props={plot} />
      </div>
    );
  }
  return (
    <ChartFigure statementId={statementId} title={props.title} subtitle={props.subtitle} model={model}>
      {body}
    </ChartFigure>
  );
};
