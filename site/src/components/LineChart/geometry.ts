// Server-side geometry for line and step-line charts: series on one axis, positions per period, direct end labels
// nudged apart, and event markers for step lines. The island only draws what this returns.
import { formatShortDate } from "../../format";
import type { PageDataResolver } from "../../renderer/data/resolve";
import type { LegendItem } from "../ChartFigure/ChartFigure";
import {
  type ColourRole,
  colourRoles,
  displayValue,
  isSubjectRole,
  isThin,
  type Measure,
  modelSummary,
  type OkSeries,
  type PeriodSlot,
  periodSlots,
  pointAt,
  resolveSeries,
  sameAxis,
  seriesFell,
  seriesTable,
  type TableModel,
  tooltipSource,
} from "../ChartFigure/model";
import { axisWidth, displayExponent, niceScale, tightLabels, xLabelPlan } from "../ChartFigure/scale";
import type { EventMarker, LineChartPlotProps, LineSeriesGeometry } from "./LineChartPlot.client";

export interface LineChartData {
  series: OkSeries[];
  model: TableModel | null;
}

/** The series that share the first one's axis (never converted to match), and the table they make. */
export function lineChartData(
  refs: readonly string[],
  text: { title: string; subtitle: string },
  data: PageDataResolver,
): LineChartData {
  const resolved = resolveSeries(data, refs);
  const first = resolved[0];
  const series = first ? resolved.filter((s) => sameAxis([first, s])) : [];
  if (series.length === 0) return { series, model: null };
  return {
    series,
    model: seriesTable(series, { title: data.plainText(text.title), subtitle: data.plainText(text.subtitle) }),
  };
}

const dayOf = (iso: string) => Date.UTC(Number(iso.slice(0, 4)), Number(iso.slice(5, 7)) - 1, Number(iso.slice(8, 10)));

/** X positions, 0-100: evenly spaced, or by date when every period has one (`byDate`, for rate paths). */
export function stopPositions(slots: readonly PeriodSlot[], byDate: boolean): number[] {
  const n = slots.length;
  if (n === 1) return [50];
  const dates = slots.map((s) => (s.date && /^\d{4}-\d{2}-\d{2}/.test(s.date) ? dayOf(s.date) : null));
  if (byDate && dates.every((d) => d !== null)) {
    const ds = dates as number[];
    const t0 = Math.min(...ds);
    const t1 = Math.max(...ds);
    if (t1 > t0) return ds.map((d) => ((d - t0) / (t1 - t0)) * 100);
  }
  return slots.map((_, i) => (i / (n - 1)) * 100);
}

/**
 * Spreads labels vertically so they never overlap: each at least `gap` apart (in % of the plot height), as close
 * to its line's end as that allows, and kept inside the plot.
 */
export function spreadLabels(ys: readonly number[], gap: number): number[] {
  const order = ys.map((y, i) => ({ y, i })).sort((a, b) => a.y - b.y);
  const placed = order.map((o) => o.y);
  for (let k = 1; k < placed.length; k++) placed[k] = Math.max(placed[k] as number, (placed[k - 1] as number) + gap);
  const overflow = (placed[placed.length - 1] ?? 0) - 100;
  if (overflow > 0) {
    for (let k = 0; k < placed.length; k++) placed[k] = (placed[k] as number) - overflow;
    for (let k = 0; k < placed.length; k++) {
      const floor = k === 0 ? 0 : (placed[k - 1] as number) + gap;
      placed[k] = Math.max(placed[k] as number, floor);
    }
  }
  const out = new Array<number>(ys.length);
  order.forEach((o, k) => {
    out[o.i] = placed[k] as number;
  });
  return out;
}

/** "Rate cut, 18 Sep 2024": what changed at a decision, worked out from the series itself. */
export function eventLabel(s: OkSeries, periodRef: string, slot: PeriodSlot): string {
  const i = s.points.findIndex((p) => p.period.ref === periodRef);
  const when = slot.date ? formatShortDate(slot.date) : slot.label;
  const here = s.points[i]?.value ?? null;
  let before: string | null = null;
  for (let k = i - 1; k >= 0; k--) {
    const v = s.points[k]?.value ?? null;
    if (v !== null) {
      before = v;
      break;
    }
  }
  if (here === null) return when;
  if (before === null) return `Rate held, ${when}`;
  const d = Number(here) - Number(before);
  return `${d < 0 ? "Rate cut" : d > 0 ? "Rate rise" : "Rate held"}, ${when}`;
}

export interface LinePlotOptions {
  highlight?: string | null;
  comparison?: string | null;
  /** Start the axis at zero (the default) or near the data, when the variation is the point. */
  fromZero?: boolean;
  curve: "linear" | "step";
  /** pd_… periods to mark with an event line (step lines). */
  markers?: readonly string[];
  /** Small multiples: every panel's periods and values, so all panels share one x axis and one value axis. */
  shared?: { slots: PeriodSlot[]; values: string[] };
}

/**
 * Each series' colour role. The highlighted series is crimson unless it fell over the shown range (its last value
 * below its first): then it takes the neutral highlight (design/charts.md, "When the subject fell").
 */
export function lineRoles(series: readonly OkSeries[], opts: LinePlotOptions): ColourRole[] {
  const subject = opts.highlight ? series.find((s) => s.ref === opts.highlight) : undefined;
  const fell = subject ? seriesFell(subject.points.map((p) => p.value)) : false;
  return colourRoles(
    series.map((s) => s.ref),
    { highlight: opts.highlight, comparison: opts.comparison, fell },
  );
}

/** Legend items for two or more series: the line keys in each series' colour. */
export function lineLegend(series: readonly OkSeries[], opts: LinePlotOptions): LegendItem[] {
  const roles = lineRoles(series, opts);
  return series.map((s, i) => ({ key: s.ref, label: s.label, role: roles[i] ?? "cat-1", shape: "line" }));
}

export function linePlot(
  chart: LineChartData & { model: TableModel },
  opts: LinePlotOptions,
  data: PageDataResolver,
): LineChartPlotProps {
  const { series, model } = chart;
  const first = series[0] as OkSeries;
  const measure: Measure = { unit: first.unit, currency: first.currency, scale: first.scale };
  const slots = opts.shared?.slots ?? periodSlots(series);
  const xs = stopPositions(slots, opts.curve === "step");
  const values =
    opts.shared?.values ?? series.flatMap((s) => s.points.map((p) => p.value).filter((v): v is string => v !== null));
  const exponent = displayExponent(values, measure);
  const scale = niceScale(
    values.map((v) => Number(v)),
    measure,
    { includeZero: opts.fromZero !== false, displayExponent: exponent },
  );
  const roles = lineRoles(series, opts);
  const multi = series.length > 1;
  const labelled = series.length <= 4;

  const geometry: LineSeriesGeometry[] = series.map((s, j) => {
    const ys = slots.map((slot) => {
      const p = pointAt(s, slot.ref);
      return p?.value == null ? null : scale.y(Number(p.value));
    });
    let lastIndex = -1;
    for (let i = ys.length - 1; i >= 0; i--) {
      if (ys[i] !== null) {
        lastIndex = i;
        break;
      }
    }
    const lastSlot = slots[lastIndex];
    const lastPoint = lastSlot ? pointAt(s, lastSlot.ref) : null;
    const role = roles[j] ?? "cat-1";
    return {
      key: s.ref,
      role,
      thin: isThin(role),
      ys,
      end:
        labelled && lastPoint?.value != null
          ? {
              x: xs[lastIndex] ?? 100,
              y: ys[lastIndex] as number,
              name: multi ? s.label : null,
              period: lastSlot?.label ?? "",
              value: displayValue(lastPoint.value, s),
              labelY: ys[lastIndex] as number,
              highlight: isSubjectRole(role),
              ref: lastPoint.ref,
            }
          : null,
    };
  });

  // End labels sit in a column right of the plot. Names wrap at about 11 characters on phones and 16 wider; a line
  // of label text is about 17.6px, against a plot 240px tall on phones and 288px wider. Labels are nudged apart by
  // the tallest one's height. When a phone cannot fit them all, phones drop them (the legend above, the tooltip
  // and the data table still name every series) and give their room back to the plot.
  const ended = geometry.filter((g) => g.end);
  const lines = (perLine: number) => (g: LineSeriesGeometry) =>
    (g.end?.name ? Math.ceil(g.end.name.length / perLine) : 0) + 1;
  const heights = {
    phone: ended.map((g) => ((lines(11)(g) * 17.6) / 240) * 100),
    wide: ended.map((g) => ((lines(16)(g) * 17.6) / 288) * 100),
  };
  const sum = (xs: number[]) => xs.reduce((a, b) => a + b, 0);
  const phoneLabels = sum(heights.phone) <= 100;
  const gap = Math.max(0, ...(phoneLabels ? heights.phone : heights.wide)) + 1;
  const spread = spreadLabels(
    ended.map((g) => g.end?.y ?? 0),
    gap,
  );
  ended.forEach((g, k) => {
    if (g.end) g.end.labelY = spread[k] as number;
  });

  // Room for the end labels, in ch of the label font.
  const valueChars = Math.max(0, ...ended.map((g) => g.end?.value.length ?? 0));
  const nameChars = Math.max(0, ...ended.map((g) => g.end?.name?.length ?? 0));
  const room = {
    phone: ended.length && phoneLabels ? Math.max(valueChars, Math.min(nameChars, 11)) + 1 : 0,
    wide: ended.length ? Math.max(valueChars, Math.min(nameChars, 16)) + 1 : 0,
  };
  const endRoom = { phone: `${room.phone}ch`, wide: `${room.wide}ch`, phoneLabels };

  // Event markers: labels stagger onto a second row when they would touch; a third is left as a bare line. A label
  // starts at its line, or ends there past 70% of the width; its width is estimated for the narrowest wide plot
  // (about 520px of 13.5px text, 7.5px a character), so labels never touch from 600px up.
  const events: EventMarker[] = [];
  const rowEnds = [Number.NEGATIVE_INFINITY, Number.NEGATIVE_INFINITY];
  const markers = (opts.markers ?? [])
    .map((ref) => ({ ref, i: slots.findIndex((s) => s.ref === ref) }))
    .filter((m) => m.i >= 0)
    .sort((a, b) => (xs[a.i] ?? 0) - (xs[b.i] ?? 0));
  for (const { ref, i } of markers) {
    const slot = slots[i] as PeriodSlot;
    const x = xs[i] ?? 0;
    const label = eventLabel(first, ref, slot);
    const width = ((label.length * 7.5 + 8) / 520) * 100;
    const [from, to] = x > 70 ? [x - width, x] : [x, x + width];
    const row = rowEnds.findIndex((end) => from > end + 1);
    if (row >= 0) rowEnds[row] = to;
    events.push({ key: ref, x, label: row >= 0 ? label : null, row: Math.max(row, 0), phone: false });
  }
  // A phone has room for one label: the latest decision's. The other lines stay, unlabelled.
  const lastLabelled = events.findLast((e) => e.label !== null);
  if (lastLabelled) lastLabelled.phone = true;

  // A date axis (step lines) is labelled by year, at each 1 January inside the range; otherwise by period.
  const yearLabels = (() => {
    if (opts.curve !== "step") return null;
    const dated = slots.map((s) => (s.date ? dayOf(s.date) : null));
    if (dated.some((d) => d === null) || xs.length < 2) return null;
    const ds = dated as number[];
    const t0 = Math.min(...ds);
    const t1 = Math.max(...ds);
    if (t1 <= t0) return null;
    const years: { label: string; x: number }[] = [{ label: String(new Date(t0).getUTCFullYear()), x: 0 }];
    for (let y = new Date(t0).getUTCFullYear() + 1; Date.UTC(y, 0, 1) <= t1; y++) {
      years.push({ label: String(y), x: ((Date.UTC(y, 0, 1) - t0) / (t1 - t0)) * 100 });
    }
    return years.length >= 2 ? years : null;
  })();

  return {
    summary: modelSummary(model),
    axisWidth: axisWidth(scale.ticks),
    ticks: scale.ticks.map((t) => ({ y: t.y, label: t.label, baseline: t.baseline })),
    stops: slots.map((slot, i) => ({
      key: slot.ref,
      x: xs[i] ?? 0,
      label: slot.label,
      tooltip: series.map((s, j) => {
        const p = pointAt(s, slot.ref);
        return {
          value: p?.value == null ? null : displayValue(p.value, s),
          series: multi ? s.label : null,
          role: roles[j] ?? "cat-1",
          ref: p?.ref ?? null,
          source: p?.value == null ? null : tooltipSource(data, p.ref),
        };
      }),
    })),
    series: geometry,
    xLabels: yearLabels
      ? (() => {
          const tight = tightLabels(
            yearLabels.map((y) => y.label),
            yearLabels.map((y) => y.x),
          );
          return yearLabels.map((y, i) => ({
            ...y,
            phone: true,
            wide: true,
            tight: tight.has(i),
            room: tight.get(i) ?? 0,
          }));
        })()
      : xLabelPlan(
          slots.map((s) => s.label),
          xs,
          room,
        ),
    curve: opts.curve,
    events,
    endRoom,
  };
}
