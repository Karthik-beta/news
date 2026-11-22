// Histogram (design/charts.md, "Choosing the form": distribution): how many values fall in each range. Ranges are
// half-open, [a, b), on round numbers (1, 2 or 5 × 10^n), about log2(n) + 1 of them for n values (Sturges), at
// most 12; their edges are worked out from the exact decimals, so a value on an edge always goes up a range. Columns
// from zero; the y axis counts values. No count is written on the plot (a count is ours, not the source's): the axis
// reads it, the tooltip lists the values with their citations, and the data table lists every value. The plot is
// the column chart's own island. Naming one item makes its range crimson (the neutral highlight when the item is
// below zero, the bar rule) and the rest context; otherwise every range is blue.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { decimalToString, parseDecimal, shiftDecimal } from "../../format/decimal";
import { usePageData } from "../../renderer/data/context";
import { MISSING_TEXT, type PageDataResolver, type ResolvedValue } from "../../renderer/data/resolve";
import { Island } from "../../renderer/islands/Island";
import { ChartFigure } from "../ChartFigure/ChartFigure";
import {
  type ColourRole,
  displayValue,
  type Measure,
  sameAxis,
  sourcesOf,
  subjectRole,
  type TableModel,
  tooltipSource,
  valueFell,
} from "../ChartFigure/model";
import { axisWidth, bareNumber, displayExponent, niceScale, stepDecimal, xLabelPlan } from "../ChartFigure/scale";
import type { ColumnBand, ColumnChartPlotProps } from "../ColumnChart/ColumnChartPlot.client";
import ColumnChartPlot from "../ColumnChart/ColumnChartPlot.client";

export interface HistogramItem {
  label: string;
  value: string;
  highlight?: boolean | null;
}

export interface HistogramProps {
  title: string;
  subtitle: string;
  items: HistogramItem[];
}

type OkValue = Extract<ResolvedValue, { ok: true }>;

export interface Bins {
  /** The edges as exact decimals in the source's scale, lowest first; one more than the bins. */
  edges: string[];
  /** How many values fall in each bin. */
  counts: number[];
  /** Each value's bin, in the order given. */
  bin: number[];
  /** The width of a bin, an exact decimal ("5", "0.2"). */
  step: string;
}

/** Sturges' rule: about log2(n) + 1 bins, at least one and at most 12. */
export const binTarget = (n: number) => Math.min(12, Math.max(1, Math.ceil(Math.log2(Math.max(n, 1))) + 1));

/**
 * Bins on round numbers: the finest step of 1, 2 or 5 × 10^n that needs at most `target` half-open bins [a, b).
 * A value's bin is found from its exact decimal (shifted, never divided in floating point), so 0.3 sits at the
 * start of [0.3, 0.4), not at the end of [0.2, 0.3).
 */
export function histogramBins(values: readonly string[], target: number): Bins {
  if (values.length === 0) return { edges: [], counts: [], bin: [], step: "" };
  const nums = values.map(Number);
  const lo = Math.min(...nums);
  const hi = Math.max(...nums);
  const span = hi - lo || Math.abs(hi) || 1;
  // The value in units of the step: value / 10^exp exactly, then over the mantissa (1, 2 or 5).
  const units = (v: string, mantissa: number, exp: number) =>
    Math.floor(Number(decimalToString(shiftDecimal(parseDecimal(v), -exp))) / mantissa);
  const loText = values[nums.indexOf(lo)] as string;
  const hiText = values[nums.indexOf(hi)] as string;
  for (let exp = Math.floor(Math.log10(span / Math.max(target, 1))) - 1; ; exp++) {
    for (const mantissa of [1, 2, 5]) {
      const k0 = units(loText, mantissa, exp);
      const k1 = units(hiText, mantissa, exp) + 1;
      if (k1 - k0 > target) continue;
      const edges = Array.from({ length: k1 - k0 + 1 }, (_, i) => stepDecimal(k0 + i, mantissa, exp));
      const bin = values.map((v) => units(v, mantissa, exp) - k0);
      const counts = edges.slice(1).map((_, i) => bin.filter((b) => b === i).length);
      return { edges, counts, bin, step: stepDecimal(1, mantissa, exp) };
    }
  }
}

export interface HistogramData {
  /** Items with a value in the chart's unit, lowest first, then the rest (no data, or another unit) in given order. */
  rows: { input: HistogramItem; ref: string; value: OkValue | null }[];
  measure: Measure | null;
  model: TableModel | null;
}

export function histogramData(props: HistogramProps, data: PageDataResolver): HistogramData {
  const raw = props.items.map((input) => {
    const v = data.value(input.value);
    return { input, ref: input.value, value: v.ok && v.value !== null ? v : null };
  });
  const first = raw.find((r) => r.value !== null)?.value ?? null;
  const measure: Measure | null = first ? { unit: first.unit, currency: first.currency, scale: first.scale } : null;
  // A value in another unit, currency or scale is left out, never converted.
  const rows = raw
    .map((r) => (r.value && measure && !sameAxis([measure, r.value]) ? { ...r, value: null } : r))
    .map((r, i) => ({ r, i }))
    .sort((a, b) => {
      const va = a.r.value?.value ?? null;
      const vb = b.r.value?.value ?? null;
      if (va === null || vb === null) return va === null ? (vb === null ? a.i - b.i : 1) : -1;
      return Number(va) - Number(vb) || a.i - b.i;
    })
    .map(({ r }) => r);
  if (!measure) return { rows, measure, model: null };
  const model: TableModel = {
    title: data.plainText(props.title),
    subtitle: data.plainText(props.subtitle),
    head: "Item",
    columns: [{ key: "value", label: "Value", ...measure }],
    rows: rows.map((r, i) => ({
      key: `row-${i}`,
      label: data.plainText(r.input.label),
      highlight: Boolean(r.input.highlight),
      fell: Boolean(r.input.highlight) && valueFell(r.value?.value),
      cells: [{ ref: r.ref, value: r.value?.value ?? null }],
    })),
    sources: sourcesOf(...rows.flatMap((r) => (r.value ? [r.value.sources] : []))),
  };
  return { rows, measure, model };
}

/** Tooltips list at most this many values; the data table has them all. */
const TOOLTIP_VALUES = 8;

export function histogramPlot(chart: HistogramData, data: PageDataResolver): ColumnChartPlotProps {
  const measure = chart.measure as Measure;
  const present = chart.rows.filter((r) => r.value !== null) as (HistogramData["rows"][number] & {
    value: OkValue;
  })[];
  const bins = histogramBins(
    present.map((r) => r.value.value as string),
    binTarget(present.length),
  );
  const exponent = displayExponent(bins.edges, measure);
  const counts = niceScale(
    // At least 3 on the axis, so its steps are whole numbers: a count is never 0.5.
    [...bins.counts, 3],
    { unit: "", currency: null, scale: null },
    { includeZero: true },
  );
  const subject = present.find((r) => r.input.highlight);
  const highlightBin = subject ? (bins.bin[present.indexOf(subject)] ?? -1) : -1;
  const roleOf = (i: number): ColourRole =>
    !subject ? "cat-1" : i === highlightBin ? subjectRole(valueFell(subject.value.value)) : "context";
  const n = bins.counts.length;
  const edge = (i: number) => displayValue(bins.edges[i] as string, measure);
  const bands: ColumnBand[] = bins.counts.map((count, i) => {
    const role = roleOf(i);
    const inBin = present.filter((_, j) => bins.bin[j] === i);
    const y = counts.y(count);
    return {
      key: `bin-${i}`,
      label: `${edge(i)} to under ${edge(i + 1)}: ${count} ${count === 1 ? "value" : "values"}${count > TOOLTIP_VALUES ? `, ${TOOLTIP_VALUES} listed` : ""}`,
      left: (i / n) * 100,
      width: 100 / n,
      highlight: i === highlightBin,
      marks: [
        {
          series: "count",
          role,
          top: Math.min(y, counts.zero),
          height: Math.abs(counts.zero - y),
          negative: false,
          label: null,
          show: "never",
          ref: null,
        },
      ],
      tooltip: inBin.slice(0, TOOLTIP_VALUES).map((r) => ({
        value: displayValue(r.value.value as string, measure),
        series: data.plainText(r.input.label),
        role,
        ref: r.ref,
        source: tooltipSource(data, r.ref),
      })),
    };
  });
  const missing = chart.rows.length - present.length;
  const step = displayValue(bins.step, measure);
  const summary = [
    `${chart.model?.title ?? ""}.`,
    `${present.length} ${present.length === 1 ? "value" : "values"} in ${n} ${n === 1 ? "range" : "ranges"} of ${step}:`,
    `${bands.map((b) => b.label).join("; ")}.`,
    missing ? `${missing} with no data.` : "",
  ]
    .filter(Boolean)
    .join(" ");
  const edgeXs = bins.edges.map((_, i) => (i / n) * 100);
  return {
    summary,
    axisWidth: axisWidth(counts.ticks),
    ticks: counts.ticks.map((t) => ({ y: t.y, label: t.label, baseline: t.baseline })),
    zero: counts.zero,
    bands,
    xLabels: xLabelPlan(
      bins.edges.map((e) => bareNumber(e, measure, exponent)),
      edgeXs,
    ),
    density: "key",
    pinEnds: "position",
  };
}

export const Histogram: ComponentRenderer<HistogramProps> = ({ props, statementId }) => {
  const data = usePageData();
  const chart = histogramData(props, data);
  const plotted = chart.rows.some((r) => r.value !== null);
  const missing = chart.rows.filter((r) => r.value === null).map((r) => data.plainText(r.input.label));
  return (
    <ChartFigure statementId={statementId} title={props.title} subtitle={props.subtitle} model={chart.model}>
      {plotted ? (
        <div className="histogram">
          <Island name="ColumnChartPlot" component={ColumnChartPlot} props={histogramPlot(chart, data)} />
          {missing.length ? (
            <p className="histogram__missing text-label">
              {MISSING_TEXT}, not counted: {missing.join("; ")}.
            </p>
          ) : null}
        </div>
      ) : null}
    </ChartFigure>
  );
};
