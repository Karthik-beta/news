// The data model every chart and table shares: rows and columns of exact source values with their references.
// One model feeds the visible data table, the CSV and JSON downloads (csv()), index.md (markdown()) and the
// chart's accessible summary, so the four can never disagree. Server-side only: islands get plain JSON from it.
import { formatMoney, formatNumber, formatQuantity, SCALE_EXPONENT, type Scale, unitKind } from "../../format";
import { CURRENCY_PREFIX, CURRENCY_WORD } from "../../format/currency";
import { fractionDigits } from "../../format/decimal";
import { SCALE_WORD } from "../../format/number";
import type { CitedSource, PageDataResolver, ResolvedSeries, SeriesPoint } from "../../renderer/data/resolve";
import { MISSING_TEXT } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import type { ChartTable } from "../contract";
import type { Measure } from "./scale";

export type { Measure } from "./scale";
export type OkSeries = Extract<ResolvedSeries, { ok: true }>;

export interface TableColumn extends Measure {
  key: string;
  /** The series' or value's own label, verbatim. */
  label: string;
}

export interface TableCell {
  /** The dp_/dv_ reference behind the value, for citations; null for a gap. */
  ref: string | null;
  /** The exact decimal as published, or null when the source has no value. */
  value: string | null;
}

export interface TableRow {
  key: string;
  /** The period label as the source writes it, or the category label the page writer gave. */
  label: string;
  highlight: boolean;
  /** The highlighted row's subject fell: its row bar takes the neutral highlight, never crimson. */
  fell?: boolean;
  cells: TableCell[];
}

export interface TableModel {
  title: string;
  subtitle: string;
  /** The first column's heading ("Period", "Region"). */
  head: string;
  columns: TableColumn[];
  rows: TableRow[];
  sources: CitedSource[];
}

/** "As reported" keeps the source's digits and scale (167,045, with "US$ millions" in the header); compact writes $167.0bn. */
export type TableStyle = "as-reported" | "compact";

const sameMeasure = (a: Measure, b: Measure) => a.unit === b.unit && a.currency === b.currency && a.scale === b.scale;

/** The measure every column shares, or null when columns differ. */
export function commonMeasure(columns: readonly Measure[]): Measure | null {
  const first = columns[0];
  if (!first) return null;
  return columns.every((c) => sameMeasure(c, first)) ? first : null;
}

/**
 * The unit as a short header line, written by code from data.json so it always matches the numbers:
 * "US$ millions", "€ billions", "millions of yen", "CHF millions", "%", "percentage points", "Index 2020=100".
 */
export function unitLabel(m: Measure): string {
  const scale = m.scale && m.scale !== "units" ? SCALE_WORD[m.scale] : "";
  const plural = scale ? `${scale}s` : "";
  if (m.currency) {
    const word = CURRENCY_WORD[m.currency];
    if (word) return plural ? `${plural} of ${word}` : word;
    const symbol = m.currency === "USD" ? "US$" : (CURRENCY_PREFIX[m.currency] ?? m.currency);
    return plural ? `${symbol} ${plural}` : symbol;
  }
  const kind = unitKind(m.unit);
  if (kind === "percent") return "%";
  if (kind === "pp") return "percentage points";
  if (kind === "bp") return "basis points";
  return plural ? `${m.unit}, ${plural}` : m.unit;
}

/**
 * A value the way readers see it in chart labels and tooltips: "$391.0bn", "2.4%", "4.50%", "1.9 points". Money
 * and counts are compact; rates and percentages keep the digits the source published (a policy rate of 0.25% is
 * never shown as 0.3%), which never adds precision.
 */
export function displayValue(value: string, m: Measure): string {
  const scale: Scale = m.scale ?? "units";
  if (m.currency) return formatMoney(value, m.currency, { scale });
  const kind = unitKind(m.unit);
  if (kind !== "other") return formatQuantity(value, m.unit, { decimals: fractionDigits(value) });
  return formatQuantity(value, m.unit, { scale });
}

/** A table cell: the source's digits (as reported) or the compact form; "No data" for a gap. */
export function formatCell(value: string | null, m: Measure, style: TableStyle): string {
  if (value === null) return MISSING_TEXT;
  return style === "as-reported" ? formatNumber(value) : displayValue(value, m);
}

/** The CSV and JSON download for a table model: exact decimals, verbatim labels, the unit in each heading. */
export function modelToChartTable(model: TableModel): ChartTable {
  const common = commonMeasure(model.columns);
  return {
    title: model.title,
    subtitle: model.subtitle,
    columns: [
      { key: "label", label: model.head },
      ...model.columns.map((c) => ({ key: c.key, label: `${c.label} (${unitLabel(c)})` })),
    ],
    rows: model.rows.map((r) => {
      const row: Record<string, string | null> = { label: r.label };
      model.columns.forEach((c, i) => {
        row[c.key] = r.cells[i]?.value ?? null;
      });
      return row;
    }),
    unit: common?.unit,
    currency: common ? common.currency : null,
    scale: common ? common.scale : null,
    sources: model.sources.map((s) => s.id),
  };
}

/** index.md for a chart or table: its title, subtitle and the data as a Markdown table of exact values. */
export function modelMarkdown(model: TableModel): string {
  const header = [model.head, ...model.columns.map((c) => `${c.label} (${unitLabel(c)})`)];
  const lines = [
    `### ${escapeMarkdown(model.title)}`,
    "",
    ...(model.subtitle ? [escapeMarkdown(model.subtitle), ""] : []),
    `| ${header.map(escapeMarkdown).join(" | ")} |`,
    `| --- | ${model.columns.map(() => "---:").join(" | ")} |`,
    ...model.rows.map(
      (r) =>
        `| ${escapeMarkdown(r.label)} | ${r.cells.map((c) => (c.value === null ? MISSING_TEXT : c.value)).join(" | ")} |`,
    ),
  ];
  if (model.rows.length === 0) lines.push("", `${MISSING_TEXT}.`);
  return lines.join("\n");
}

/**
 * The chart's data in a sentence, for its accessible name (design/content-style.md): every value when a column has
 * 12 or fewer rows; otherwise the count, the endpoints and the extremes.
 */
export function modelSummary(model: TableModel): string {
  const parts = model.columns.map((c, i) => {
    const values = model.rows.map((r) => ({ label: r.label, value: r.cells[i]?.value ?? null }));
    const say = (v: { label: string; value: string | null }) =>
      `${v.label}, ${v.value === null ? MISSING_TEXT.toLowerCase() : displayValue(v.value, c)}`;
    if (values.length <= 12) return `${c.label}: ${values.map(say).join("; ")}`;
    const present = values.filter((v) => v.value !== null) as { label: string; value: string }[];
    const first = values[0] as (typeof values)[number];
    const last = values[values.length - 1] as (typeof values)[number];
    if (present.length === 0) return `${c.label}, ${values.length} periods, ${MISSING_TEXT.toLowerCase()}`;
    const hi = present.reduce((a, b) => (Number(b.value) > Number(a.value) ? b : a));
    const lo = present.reduce((a, b) => (Number(b.value) < Number(a.value) ? b : a));
    return `${c.label}, ${values.length} periods, from ${say(first)} to ${say(last)}; highest ${displayValue(hi.value, c)} in ${hi.label}; lowest ${displayValue(lo.value, c)} in ${lo.label}`;
  });
  return `${model.title}. ${parts.join(". ")}.`;
}

/** Periods across several series, in first-seen order: the rows of a time table and the x positions of a chart. */
export interface PeriodSlot {
  ref: string;
  label: string;
  /** ISO date for time-scaled charts (step lines), when the period has one. */
  date: string | null;
}

export function periodSlots(all: readonly OkSeries[]): PeriodSlot[] {
  const seen = new Map<string, PeriodSlot>();
  for (const s of all) {
    for (const p of s.points) {
      if (seen.has(p.period.ref)) continue;
      const period = "period" in p.period ? p.period.period : null;
      seen.set(p.period.ref, {
        ref: p.period.ref,
        label: p.period.label ?? "Unknown period",
        date: period?.start ?? period?.end ?? null,
      });
    }
  }
  return [...seen.values()];
}

/** The point of a series at a period, or null when the series has nothing there. */
export function pointAt(s: OkSeries, periodRef: string): SeriesPoint | null {
  return s.points.find((p) => p.period.ref === periodRef) ?? null;
}

/** Resolves several sr_ references; unknown ones are dropped (the resolver has recorded the problem). */
export function resolveSeries(data: PageDataResolver, refs: readonly string[]): OkSeries[] {
  return refs.map((r) => data.series(r)).filter((s): s is OkSeries => s.ok);
}

/** Every source behind some series, once each, in order. */
export function sourcesOf(...lists: CitedSource[][]): CitedSource[] {
  const out = new Map<string, CitedSource>();
  for (const list of lists) for (const s of list) if (!out.has(s.id)) out.set(s.id, s);
  return [...out.values()];
}

/**
 * A time table: one row per period, one column per series (design/charts.md, "Tables as charts").
 * `highlight` is the pd_ of the row to highlight.
 */
export function seriesTable(
  all: readonly OkSeries[],
  opts: { title: string; subtitle: string; highlight?: string | null; head?: string },
): TableModel {
  const slots = periodSlots(all);
  return {
    title: opts.title,
    subtitle: opts.subtitle,
    head: opts.head ?? "Period",
    columns: all.map((s) => ({ key: s.ref, label: s.label, unit: s.unit, currency: s.currency, scale: s.scale })),
    rows: slots.map((slot) => {
      const highlight = Boolean(opts.highlight) && slot.ref === opts.highlight;
      const cells = all.map((s) => {
        const p = pointAt(s, slot.ref);
        return { ref: p?.ref ?? null, value: p?.value ?? null };
      });
      return { key: slot.ref, label: slot.label, highlight, fell: highlight && periodFell(cells), cells };
    }),
    sources: sourcesOf(...all.map((s) => s.sources)),
  };
}

/** A highlighted period fell when it has values and every one is below zero (the column rule, row by row). */
function periodFell(cells: readonly TableCell[]): boolean {
  const present = cells.filter((c) => c.value !== null);
  return present.length > 0 && present.every((c) => valueFell(c.value));
}

/** The same table turned so each series is a row and each period a column (segments by year). */
export function transposedSeriesTable(
  all: readonly OkSeries[],
  opts: { title: string; subtitle: string; highlight?: string | null; head?: string },
): TableModel {
  const slots = periodSlots(all);
  const first = all[0];
  const measure: Measure = first
    ? { unit: first.unit, currency: first.currency, scale: first.scale }
    : { unit: "", currency: null, scale: null };
  return {
    title: opts.title,
    subtitle: opts.subtitle,
    head: opts.head ?? "Series",
    columns: slots.map((slot) => ({ key: slot.ref, label: slot.label, ...measure })),
    rows: all.map((s) => {
      const highlight = Boolean(opts.highlight) && s.ref === opts.highlight;
      const cells = slots.map((slot) => {
        const p = pointAt(s, slot.ref);
        return { ref: p?.ref ?? null, value: p?.value ?? null };
      });
      // A series as a row fell by the line rule: its last value below its first.
      return { key: s.ref, label: s.label, highlight, fell: highlight && seriesFell(cells.map((c) => c.value)), cells };
    }),
    sources: sourcesOf(...all.map((s) => s.sources)),
  };
}

/** True when two series can share one value axis: same unit, currency and scale (never converted to match). */
export function sameAxis(all: readonly Measure[]): boolean {
  return commonMeasure(all) !== null;
}

/** Scale exponent, re-exported for chart geometry. */
export const scaleExponent = (m: Measure) => SCALE_EXPONENT[m.scale ?? "units"];

/**
 * Colour roles (design/color.md, "Chart colours"). Assigned by entity, from the page writer's choices, never by
 * rank. Crimson never marks a subject that fell (Karthik, 16 Nov 2026): a highlighted subject that fell takes the
 * neutral highlight ("highlight-neutral") instead. What "fell" means per chart is in design/charts.md, "When the
 * subject fell", and in `valueFell` and `seriesFell` below.
 *   highlight given: the subject is crimson (neutral if it fell), one comparison may be blue, the rest are context.
 *   no highlight: one series is blue (single series); 2-8 equal series take the categorical slots in order.
 */
export type ColourRole =
  | "highlight"
  | "highlight-neutral"
  | "comparison"
  | "context"
  | "neutral"
  | "cat-1"
  | "cat-2"
  | "cat-3"
  | "cat-4"
  | "cat-5"
  | "cat-6"
  | "cat-7"
  | "cat-8";

export function colourRoles(
  keys: readonly string[],
  {
    highlight,
    comparison,
    fell = false,
  }: { highlight?: string | null; comparison?: string | null; fell?: boolean } = {},
): ColourRole[] {
  const hasHighlight = Boolean(highlight) && keys.includes(highlight as string);
  const hasComparison = Boolean(comparison) && keys.includes(comparison as string) && comparison !== highlight;
  if (hasHighlight || hasComparison) {
    return keys.map((k) => (k === highlight ? subjectRole(fell) : k === comparison ? "comparison" : "context"));
  }
  if (keys.length === 1) return ["cat-1"];
  return keys.map((_, i) => `cat-${Math.min(i + 1, 8)}` as ColourRole);
}

/** The highlighted subject's role: crimson, or the neutral highlight when it fell. */
export const subjectRole = (fell: boolean): ColourRole => (fell ? "highlight-neutral" : "highlight");

/** True for either highlight role: the subject's label is bold in its colour. */
export const isSubjectRole = (role: string) => role === "highlight" || role === "highlight-neutral";

/**
 * Bars and columns: the subject fell when its value is below zero (a change, a loss) or, when the chart names a
 * comparison value, below that value. Exact decimals are compared as numbers; they are never shown from here.
 */
export function valueFell(value: string | null | undefined, comparison?: string | null): boolean {
  if (value === null || value === undefined) return false;
  const v = Number(value);
  if (!Number.isFinite(v)) return false;
  if (comparison !== null && comparison !== undefined && Number.isFinite(Number(comparison))) {
    return v < Number(comparison);
  }
  return v < 0;
}

/**
 * Lines: the subject fell when its last value in the shown range is below its first (gaps skipped). A flat line, a
 * single point or no values never fell.
 */
export function seriesFell(values: readonly (string | null)[]): boolean {
  const present = values.filter((v): v is string => v !== null && Number.isFinite(Number(v)));
  if (present.length < 2) return false;
  return Number(present[present.length - 1]) < Number(present[0]);
}

/** Line width role: highlight (either), comparison and categorical lines are 2.5px; context lines 1.5px. */
export const isThin = (role: ColourRole) => role === "context" || role === "neutral";

/**
 * The source line for one value's tooltip (design/charts.md, "Interaction"): the document's short form and, for a
 * page in a PDF, the page ("Form 10-K, fiscal 2024, p. 23"). Null when the value has no source.
 */
export function tooltipSource(data: PageDataResolver, ref: string | null): string | null {
  if (!ref) return null;
  const v = data.value(ref);
  if (!v.ok) return null;
  const s = v.sources[0];
  if (!s) return null;
  const loc = v.locator as Record<string, unknown> | null;
  const page = loc?.kind === "pdf" && (typeof loc.page === "number" || typeof loc.page === "string") ? loc.page : null;
  return page === null ? s.source.short : `${s.source.short}, p. ${page}`;
}
