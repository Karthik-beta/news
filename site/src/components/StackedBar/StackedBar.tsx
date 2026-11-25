// Horizontal stacked bar (design/charts.md, "Choosing the form": part of a whole): one bar per row, each split into
// the same parts, 22px thick with a 2px paper gap between segments, square ends. Parts take the categorical
// colours in the order given, or, when one part is the highlight, crimson for it and context for the rest; never
// by value. A legend names the parts; each row's total (when the page gives one) is written past the bar's end;
// every part's value is in the tooltip and the data table. HTML and CSS; the plot is an island.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { usePageData } from "../../renderer/data/context";
import type { PageDataResolver, ResolvedValue } from "../../renderer/data/resolve";
import { Island } from "../../renderer/islands/Island";
import { ChartFigure, type LegendItem } from "../ChartFigure/ChartFigure";
import type { TooltipLine } from "../ChartFigure/interaction";
import {
  type ColourRole,
  colourRoles,
  displayValue,
  type Measure,
  modelSummary,
  sameAxis,
  seriesFell,
  sourcesOf,
  type TableModel,
  tooltipSource,
} from "../ChartFigure/model";
import StackedBarPlot, { type StackedBarPlotProps, type StackRow, type StackStop } from "./StackedBarPlot.client";

export interface StackedRowInput {
  /** The row's name; may hold {pd_…} tokens ("{pd_fy2024}"). */
  label: string;
  /** One dp_/dv_ per part, in the order of `parts`. */
  values: string[];
  /** The row's total as published, written past the bar's end. */
  total?: string | null;
}

export interface StackedBarProps {
  title: string;
  subtitle: string;
  parts: string[];
  rows: StackedRowInput[];
  /** The part the title is about: crimson, the rest context. */
  highlight?: string | null;
}

type OkValue = Extract<ResolvedValue, { ok: true }>;

export interface StackCell {
  ref: string;
  value: OkValue | null;
}

export interface StackData {
  parts: string[];
  roles: ColourRole[];
  rows: { label: string; cells: StackCell[]; total: StackCell | null }[];
  measure: Measure | null;
  model: TableModel | null;
}

/**
 * Resolves every value; values off the first value's axis (another unit, currency or scale) are left out, never
 * converted. Parts are coloured by entity: the categorical slots in order, or highlight and context.
 */
export function stackData(
  parts: readonly string[],
  rows: readonly { label: string; values: readonly string[]; total?: string | null }[],
  opts: { title: string; subtitle: string; highlight?: string | null; head: string; fell?: boolean },
  data: PageDataResolver,
): StackData {
  const resolve = (ref: string): StackCell => {
    const v = data.value(ref);
    return { ref, value: v.ok ? v : null };
  };
  const raw = rows.map((r) => ({
    label: data.plainText(r.label),
    cells: parts.map((_, i) => resolve(r.values[i] ?? "")),
    total: r.total ? resolve(r.total) : null,
  }));
  const first = raw.flatMap((r) => r.cells).find((c) => c.value !== null)?.value ?? null;
  const measure: Measure | null = first ? { unit: first.unit, currency: first.currency, scale: first.scale } : null;
  const keep = (c: StackCell): StackCell =>
    c.value && measure && !sameAxis([measure, c.value]) ? { ref: c.ref, value: null } : c;
  const resolved = raw.map((r) => ({
    label: r.label,
    cells: r.cells.map(keep),
    total: r.total ? keep(r.total) : null,
  }));
  // A highlighted part that shrank from the first row to the last (the line rule, across rows) takes the neutral
  // highlight, never crimson (design/charts.md, "When the subject fell").
  const h = opts.highlight ? parts.indexOf(opts.highlight) : -1;
  const fell = opts.fell ?? (h >= 0 && seriesFell(resolved.map((r) => r.cells[h]?.value?.value ?? null)));
  const roles = colourRoles([...parts], { highlight: opts.highlight, fell });
  if (!measure) return { parts: [...parts], roles, rows: resolved, measure, model: null };
  const hasTotal = resolved.some((r) => r.total !== null);
  const okValues = resolved
    .flatMap((r) => [...r.cells, ...(r.total ? [r.total] : [])])
    .flatMap((c) => (c.value ? [c.value] : []));
  const model: TableModel = {
    title: opts.title,
    subtitle: opts.subtitle,
    head: opts.head,
    columns: [
      ...parts.map((p, i) => ({ key: `part-${i}`, label: p, ...measure })),
      ...(hasTotal ? [{ key: "total", label: "Total", ...measure }] : []),
    ],
    rows: resolved.map((r, i) => ({
      key: `row-${i}`,
      label: r.label,
      highlight: false,
      cells: [
        ...r.cells.map((c) => ({ ref: c.ref || null, value: c.value?.value ?? null })),
        ...(hasTotal ? [{ ref: r.total?.ref ?? null, value: r.total?.value?.value ?? null }] : []),
      ],
    })),
    sources: sourcesOf(...okValues.map((v) => v.sources)),
  };
  return { parts: [...parts], roles, rows: resolved, measure, model };
}

/** A part can be stacked when it has a value of zero or more; a negative part is left out of the bar. */
const stackable = (c: StackCell) => c.value !== null && Number(c.value.value) >= 0;

/** Geometry: every row on one scale (the longest bar, or the largest total), segments in the order of the parts. */
export function stackPlot(
  chart: StackData,
  variant: "stacked" | "segmented",
  data: PageDataResolver,
): StackedBarPlotProps {
  const measure = chart.measure as Measure;
  const sums = chart.rows.map((r) => r.cells.filter(stackable).reduce((s, c) => s + Number(c.value?.value), 0));
  const totals = chart.rows.map((r) => (r.total?.value ? Number(r.total.value.value) : 0));
  const max = Math.max(...sums, ...totals) || 1;
  const line = (c: StackCell, i: number): TooltipLine => ({
    value: c.value ? displayValue(c.value.value, measure) : null,
    series: chart.parts[i] ?? null,
    role: chart.roles[i] ?? "cat-1",
    ref: c.ref || null,
    source: c.value ? tooltipSource(data, c.ref) : null,
  });
  const stops: StackStop[] = [];
  const rows: StackRow[] = chart.rows.map((r, i) => {
    let at = 0;
    const segments = r.cells.flatMap((c, j) => {
      if (!stackable(c)) return [];
      const width = (Number(c.value?.value) / max) * 100;
      const seg = { key: `${i}-${j}`, role: chart.roles[j] ?? "cat-1", left: at, width, ref: c.ref, part: j };
      at += width;
      return [seg];
    });
    const totalValue = r.total?.value ?? null;
    const total =
      totalValue && r.total
        ? {
            left: (Math.max(Number(totalValue.value), sums[i] ?? 0) / max) * 100,
            value: displayValue(totalValue.value, measure),
            ref: r.total.ref,
          }
        : null;
    if (variant === "stacked") {
      stops.push({ row: i, segment: null, x: at, title: r.label, tooltip: r.cells.map(line) });
    } else {
      segments.forEach((s, k) => {
        stops.push({
          row: i,
          segment: k,
          x: s.left + s.width / 2,
          title: chart.parts[s.part] ?? "",
          tooltip: [{ ...line(r.cells[s.part] as StackCell, s.part), series: null }],
        });
      });
    }
    return {
      key: `row-${i}`,
      label: r.label,
      segments: segments.map(({ part: _part, ...s }) => s),
      total,
      empty: segments.length === 0,
    };
  });
  const widest = Math.max(0, ...rows.map((r) => r.total?.value.length ?? 0));
  return {
    summary: chart.model ? modelSummary(chart.model) : "",
    variant,
    rows,
    stops,
    room: widest > 0 ? `${widest + 1}ch` : "0ch",
  };
}

/** The legend: a square key per part, in the order of the parts. */
export const stackLegend = (chart: Pick<StackData, "parts" | "roles">): LegendItem[] =>
  chart.parts.map((p, i) => ({ key: `part-${i}`, label: p, role: chart.roles[i] ?? "cat-1", shape: "square" }));

export function stackedBarData(props: StackedBarProps, data: PageDataResolver): StackData {
  return stackData(
    props.parts,
    props.rows,
    {
      title: data.plainText(props.title),
      subtitle: data.plainText(props.subtitle),
      highlight: props.highlight,
      head: "Category",
    },
    data,
  );
}

export const StackedBar: ComponentRenderer<StackedBarProps> = ({ props, statementId }) => {
  const data = usePageData();
  const chart = stackedBarData(props, data);
  const plotted = chart.rows.some((r) => r.cells.some((c) => c.value !== null));
  return (
    <ChartFigure
      statementId={statementId}
      title={props.title}
      subtitle={props.subtitle}
      legend={stackLegend(chart)}
      model={chart.model}
    >
      {plotted ? (
        <Island name="StackedBarPlot" component={StackedBarPlot} props={stackPlot(chart, "stacked", data)} />
      ) : null}
    </ChartFigure>
  );
};
