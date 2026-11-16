// Resolves data references (dp_ dv_ sr_ pd_ q_ cx_) against a page's data.json and sources.json. Components never
// see a bare number from the model: they get a reference, ask the resolver, and render what comes back, including
// the honest "missing" result when a reference is unknown or has no value. The resolver records every reference
// and source a page uses (for citations, C2 and the bundle's meta) and every problem it meets.
// Contract: site/src/renderer/README.md.
import {
  type Decimal,
  formatMoney,
  formatNumber,
  formatPercent,
  formatPeriodLabel,
  formatQuantity,
  type Scale,
  unitKind,
} from "../../format";
import {
  type ContextLineT,
  type DataPointT,
  type DerivedValueT,
  type PageDataT,
  type PageSourcesT,
  type PeriodT,
  REF_PATTERN,
  type RefKind,
  type SourceT,
} from "../bundle/schema";

/** `{dp_7kq2}` inside text (headlines, summaries, prose). Only the braced form is a reference in text. */
export const TEXT_REF = /\{((?:dp|dv|sr|pd|q|cx)_[a-z0-9]+(?:_[a-z0-9]+)*)\}/g;
/** A reference-shaped word without braces: almost certainly a mistake, reported as a problem. */
const BARE_REF = /(?<![{\w])((?:dp|dv|sr|pd|q|cx)_[a-z0-9]+(?:_[a-z0-9]+)*)(?![}\w])/g;

export const refKind = (ref: string): RefKind | null =>
  REF_PATTERN.test(ref) ? (ref.slice(0, ref.indexOf("_")) as RefKind) : null;
export const isRef = (value: unknown): value is string => typeof value === "string" && REF_PATTERN.test(value);

export type ProblemCode =
  | "unknown-ref" // the reference is not in data.json (C2)
  | "wrong-kind" // e.g. a series where a single value is needed
  | "no-value" // the data point exists, but the source published no value
  | "unknown-source" // a data point or quote cites a source missing from sources.json
  | "broken-link" // a series, derived value or context line points at a missing reference
  | "bare-ref"; // a reference-shaped word in text without braces

export interface Problem {
  code: ProblemCode;
  ref?: string;
  message: string;
}

/** The source behind a value, as components show it. */
export interface CitedSource {
  id: string;
  source: SourceT;
}

export interface ResolvedPeriod {
  ref: string;
  label: string;
  period: PeriodT;
}

export type ResolvedValue =
  | {
      ok: true;
      ref: string;
      kind: "dp" | "dv";
      label: string;
      value: Decimal;
      /** As published, for dp_ values. */
      valueText: string | null;
      unit: string;
      currency: string | null;
      scale: Scale | null;
      /** Digits after the point the source is precise to, in the value's scale, when known. */
      sourceDecimals: number | null;
      period: ResolvedPeriod | null;
      sources: CitedSource[];
      /** For dv_ values: how code calculated it ("Calculated by News from …"). */
      derived: { formula: DerivedValueT["formula"]; inputs: string[] } | null;
      /** XBRL tag, JSON path or PDF page, for dp_ values. */
      locator: DataPointT["locator"] | null;
    }
  | { ok: false; ref: string; reason: "unknown-ref" | "wrong-kind" | "no-value"; label: string | null };

export interface SeriesPoint {
  period: ResolvedPeriod | { ref: string; label: null };
  /** The data point, or null for a gap. */
  ref: string | null;
  value: Decimal | null;
  sources: CitedSource[];
}

export type ResolvedSeries =
  | {
      ok: true;
      ref: string;
      label: string;
      unit: string;
      currency: string | null;
      scale: Scale | null;
      points: SeriesPoint[];
      sources: CitedSource[];
    }
  | { ok: false; ref: string; reason: "unknown-ref" | "wrong-kind" };

export type TextSegment =
  | { type: "text"; text: string }
  | { type: "value"; ref: string; value: Extract<ResolvedValue, { ok: true }> }
  | { type: "period"; ref: string; label: string }
  | { type: "context"; ref: string; text: string }
  | { type: "quote"; ref: string; text: string }
  | { type: "missing"; ref: string };

export type ValueStyle = "compact" | "as-reported";

/** Shown wherever a value is missing. Never a zero, never a blank that reads as one. */
export const MISSING_TEXT = "No data";

export class PageDataResolver {
  readonly problems: Problem[] = [];
  readonly usedRefs = new Set<string>();
  readonly usedSources = new Set<string>();
  private readonly seen = new Set<string>();

  constructor(
    readonly data: PageDataT,
    readonly sources: PageSourcesT,
  ) {}

  private problem(p: Problem) {
    const key = `${p.code}:${p.ref ?? ""}:${p.message}`;
    if (this.seen.has(key)) return;
    this.seen.add(key);
    this.problems.push(p);
  }

  private cite(ids: Iterable<string>): CitedSource[] {
    const out: CitedSource[] = [];
    for (const id of new Set(ids)) {
      const source = this.sources.sources[id];
      if (!source) {
        this.problem({ code: "unknown-source", ref: id, message: `Source ${id} is not in sources.json` });
        continue;
      }
      this.usedSources.add(id);
      out.push({ id, source });
    }
    return out;
  }

  private unknown(ref: string, what: string) {
    this.problem({ code: "unknown-ref", ref, message: `${ref} is not in data.json (${what})` });
  }

  source(id: string): SourceT | null {
    return this.sources.sources[id] ?? null;
  }

  period(ref: string): ResolvedPeriod | null {
    this.usedRefs.add(ref);
    if (refKind(ref) !== "pd") {
      this.problem({ code: "wrong-kind", ref, message: `${ref} is not a period (pd_…)` });
      return null;
    }
    const period = this.data.periods[ref];
    if (!period) {
      this.unknown(ref, "period");
      return null;
    }
    return { ref, label: formatPeriodLabel(period.label), period };
  }

  /** A single value: dp_… or dv_…. */
  value(ref: string): ResolvedValue {
    this.usedRefs.add(ref);
    const kind = refKind(ref);
    if (kind === "dp") {
      const point = this.data.points[ref];
      if (!point) {
        this.unknown(ref, "data point");
        return { ok: false, ref, reason: "unknown-ref", label: null };
      }
      return this.fromPoint(ref, point);
    }
    if (kind === "dv") {
      const derived = this.data.derived[ref];
      if (!derived) {
        this.unknown(ref, "derived value");
        return { ok: false, ref, reason: "unknown-ref", label: null };
      }
      const inputSources: string[] = [];
      for (const input of derived.inputs) {
        const point = this.data.points[input];
        if (point) inputSources.push(point.source);
        else
          this.problem({ code: "broken-link", ref, message: `${ref} is calculated from ${input}, not in data.json` });
      }
      return {
        ok: true,
        ref,
        kind: "dv",
        label: derived.label,
        value: derived.value,
        valueText: null,
        unit: derived.unit,
        currency: derived.currency,
        scale: derived.scale,
        sourceDecimals: derived.rounding,
        period: null,
        sources: this.cite(inputSources),
        derived: { formula: derived.formula, inputs: derived.inputs },
        locator: null,
      };
    }
    this.problem({ code: "wrong-kind", ref, message: `${ref} is not a single value (dp_… or dv_…)` });
    return { ok: false, ref, reason: kind ? "wrong-kind" : "unknown-ref", label: null };
  }

  private fromPoint(ref: string, point: DataPointT): ResolvedValue {
    const sources = this.cite([point.source]);
    if (point.value === null) {
      this.problem({ code: "no-value", ref, message: `${ref} has no value: the source did not publish one` });
      return { ok: false, ref, reason: "no-value", label: point.label };
    }
    return {
      ok: true,
      ref,
      kind: "dp",
      label: point.label,
      value: point.value,
      valueText: point.value_text,
      unit: point.unit,
      currency: point.currency,
      scale: point.scale,
      sourceDecimals: point.decimals ?? null,
      period: this.period(point.period),
      sources,
      derived: null,
      locator: point.locator,
    };
  }

  series(ref: string): ResolvedSeries {
    this.usedRefs.add(ref);
    if (refKind(ref) !== "sr") {
      this.problem({ code: "wrong-kind", ref, message: `${ref} is not a series (sr_…)` });
      return { ok: false, ref, reason: "wrong-kind" };
    }
    const series = this.data.series[ref];
    if (!series) {
      this.unknown(ref, "series");
      return { ok: false, ref, reason: "unknown-ref" };
    }
    const all: string[] = [];
    const points: SeriesPoint[] = series.points.map(({ period, point }) => {
      const resolvedPeriod = this.period(period) ?? { ref: period, label: null };
      if (point === null) return { period: resolvedPeriod, ref: null, value: null, sources: [] };
      this.usedRefs.add(point);
      const dp = this.data.points[point];
      if (!dp) {
        this.problem({ code: "broken-link", ref, message: `${ref} lists ${point}, which is not in data.json` });
        return { period: resolvedPeriod, ref: point, value: null, sources: [] };
      }
      all.push(dp.source);
      return { period: resolvedPeriod, ref: point, value: dp.value, sources: this.cite([dp.source]) };
    });
    return {
      ok: true,
      ref,
      label: series.label,
      unit: series.unit,
      currency: series.currency,
      scale: series.scale,
      points,
      sources: this.cite(all),
    };
  }

  quote(ref: string): { ok: true; ref: string; text: string; speaker: string | null; sources: CitedSource[] } | null {
    this.usedRefs.add(ref);
    const quote = refKind(ref) === "q" ? this.data.quotes[ref] : undefined;
    if (!quote) {
      if (refKind(ref) === "q") this.unknown(ref, "quote");
      else this.problem({ code: "wrong-kind", ref, message: `${ref} is not a quote (q_…)` });
      return null;
    }
    return { ok: true, ref, text: quote.text, speaker: quote.speaker ?? null, sources: this.cite([quote.source]) };
  }

  contextLine(ref: string): ContextLineT | null {
    this.usedRefs.add(ref);
    const line = refKind(ref) === "cx" ? this.data.context[ref] : undefined;
    if (!line) {
      if (refKind(ref) === "cx") this.unknown(ref, "context line");
      else this.problem({ code: "wrong-kind", ref, message: `${ref} is not a context line (cx_…)` });
      return null;
    }
    return line;
  }

  /** Splits text with `{ref}` tokens into segments a component can render (cited values become SourceRefs). */
  text(template: string): TextSegment[] {
    for (const m of template.matchAll(BARE_REF)) {
      this.problem({ code: "bare-ref", ref: m[1], message: `"${m[1]}" in text needs braces: {${m[1]}}` });
    }
    const out: TextSegment[] = [];
    let last = 0;
    for (const m of template.matchAll(TEXT_REF)) {
      const ref = m[1] as string;
      if (m.index > last) out.push({ type: "text", text: template.slice(last, m.index) });
      last = m.index + m[0].length;
      out.push(this.segment(ref));
    }
    if (last < template.length) out.push({ type: "text", text: template.slice(last) });
    return out;
  }

  private segment(ref: string): TextSegment {
    const kind = refKind(ref);
    if (kind === "dp" || kind === "dv") {
      const value = this.value(ref);
      return value.ok ? { type: "value", ref, value } : { type: "missing", ref };
    }
    if (kind === "pd") {
      const period = this.period(ref);
      return period ? { type: "period", ref, label: period.label } : { type: "missing", ref };
    }
    if (kind === "cx") {
      const line = this.contextLine(ref);
      return line ? { type: "context", ref, text: line.text } : { type: "missing", ref };
    }
    if (kind === "q") {
      const quote = this.quote(ref);
      return quote ? { type: "quote", ref, text: quote.text } : { type: "missing", ref };
    }
    this.usedRefs.add(ref);
    this.problem({ code: "wrong-kind", ref, message: `${ref} is a series and cannot sit inside text` });
    return { type: "missing", ref };
  }

  /** Text with every reference filled in, as plain text (titles, Markdown, accessible names). */
  plainText(template: string, style: ValueStyle = "compact"): string {
    return (
      this.text(template)
        // biome-ignore lint/suspicious/useIterableCallbackReturn: the switch is exhaustive over TextSegment
        .map((s) => {
          switch (s.type) {
            case "text":
              return s.text;
            case "value":
              return formatValue(s.value, style);
            case "period":
              return s.label;
            case "context":
              return s.text;
            case "quote":
              return `“${s.text}”`;
            case "missing":
              return MISSING_TEXT.toLowerCase();
          }
        })
        .join("")
    );
  }

  /**
   * Cross-reference checks over the whole bundle, independent of what the page uses: every point, quote and
   * period link resolves. The renderer runs it before rendering.
   */
  checkIntegrity(): Problem[] {
    const found: Problem[] = [];
    const add = (code: ProblemCode, ref: string, message: string) => found.push({ code, ref, message });
    const { points, derived, series, periods, quotes, context } = this.data;
    const all = new Set([
      ...Object.keys(points),
      ...Object.keys(derived),
      ...Object.keys(series),
      ...Object.keys(periods),
      ...Object.keys(quotes),
      ...Object.keys(context),
    ]);
    for (const [ref, p] of Object.entries(points)) {
      if (!this.sources.sources[p.source]) add("unknown-source", ref, `${ref} cites ${p.source}, not in sources.json`);
      if (!periods[p.period]) add("broken-link", ref, `${ref} has period ${p.period}, not in data.json`);
    }
    for (const [ref, d] of Object.entries(derived)) {
      for (const input of d.inputs) if (!points[input]) add("broken-link", ref, `${ref} input ${input} is missing`);
    }
    for (const [ref, s] of Object.entries(series)) {
      for (const { period, point } of s.points) {
        if (!periods[period]) add("broken-link", ref, `${ref} has period ${period}, not in data.json`);
        if (point && !points[point]) add("broken-link", ref, `${ref} lists ${point}, not in data.json`);
      }
    }
    for (const [ref, q] of Object.entries(quotes)) {
      if (!this.sources.sources[q.source]) add("unknown-source", ref, `${ref} cites ${q.source}, not in sources.json`);
    }
    for (const [ref, c] of Object.entries(context)) {
      for (const input of c.inputs ?? [])
        if (!all.has(input)) add("broken-link", ref, `${ref} input ${input} is missing`);
    }
    for (const p of found) this.problem(p);
    return found;
  }
}

/**
 * The house format for a resolved value: compact for key figures and prose ("$391.0bn", "2.4%"), or as reported
 * for citation popovers ("$391,035 million"). Goes through the shared formatters, which never convert.
 */
export function formatValue(
  v: Extract<ResolvedValue, { ok: true }>,
  style: ValueStyle = "compact",
  options: { currencyNamed?: boolean } = {},
): string {
  const scale = v.scale ?? "units";
  const sourceDecimals = v.sourceDecimals ?? undefined;
  if (v.currency) return formatMoney(v.value, v.currency, { scale, sourceDecimals, style, ...options });
  const kind = unitKind(v.unit);
  if (style === "as-reported") {
    if (kind === "percent") return formatPercent(v.value, { decimals: v.value.split(".")[1]?.length ?? 0 });
    if (kind !== "other") return formatQuantity(v.value, v.unit, { decimals: v.value.split(".")[1]?.length ?? 0 });
    const word = {
      units: "",
      thousands: " thousand",
      millions: " million",
      billions: " billion",
      trillions: " trillion",
    };
    return `${formatNumber(v.value)}${word[scale]} ${v.unit}`;
  }
  return formatQuantity(v.value, v.unit, { scale, sourceDecimals });
}

/** Every `{ref}` token in a piece of text, in order. */
export function refsInText(text: string): string[] {
  return [...text.matchAll(TEXT_REF)].map((m) => m[1] as string);
}
