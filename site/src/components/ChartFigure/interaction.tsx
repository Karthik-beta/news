// The interaction every chart island shares (design/charts.md, "Interaction"): the plot is one tab stop; arrow keys
// move between points (or bars); Home and End jump to the ends; the tooltip follows focus; Escape closes it; a
// pointer or a tap shows a mark's tooltip, and a tap outside the chart closes it. No zoom, pan or animation.
// Client-safe: islands import this, so it must never import server-only code (the resolver, the renderer).
import { type KeyboardEvent, type ReactNode, type RefObject, useCallback, useEffect, useRef, useState } from "react";

/** One line of a tooltip: a value, its series and period, and where it comes from. */
export interface TooltipLine {
  /** The value as readers see it ("$391.0bn"), or null for a gap ("No data"). */
  value: string | null;
  /** Series name; null for a single-series chart (the title names it). */
  series: string | null;
  /** The colour role of the series' mark, for the short key line in the tooltip. */
  role: string;
  /** The data reference, so the tooltip carries its citation. */
  ref: string | null;
  /** "Form 10-K, fiscal 2024, p. 23". */
  source: string | null;
}

export interface ChartStepper {
  active: number | null;
  setActive: (i: number | null) => void;
  /** The island's root: marks it hydrated and makes the plot a tab stop only once the keys work. */
  rootRef: RefObject<HTMLDivElement | null>;
  /** The focusable plot. */
  plotRef: RefObject<HTMLDivElement | null>;
  onKeyDown: (e: KeyboardEvent) => void;
  onBlur: () => void;
}

/**
 * Keyboard and outside-tap handling for a chart with `count` stops. The plot gets `tabindex="0"` after hydration,
 * so without JavaScript there is no tab stop that does nothing (the data table holds every value).
 */
export function useChartStepper(count: number): ChartStepper {
  const [active, setActive] = useState<number | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const plotRef = useRef<HTMLDivElement | null>(null);
  const last = count - 1;

  useEffect(() => {
    rootRef.current?.setAttribute("data-hydrated", "true");
    if (count > 0) plotRef.current?.setAttribute("tabindex", "0");
  }, [count]);

  useEffect(() => {
    if (active === null) return;
    const close = (e: PointerEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setActive(null);
    };
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, [active]);

  const onKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (count === 0) return;
      const next = (a: number | null) => Math.min((a ?? -1) + 1, last);
      const prev = (a: number | null) => Math.max((a ?? last + 1) - 1, 0);
      switch (e.key) {
        case "ArrowRight":
        case "ArrowDown":
          setActive(next);
          break;
        case "ArrowLeft":
        case "ArrowUp":
          setActive(prev);
          break;
        case "Home":
          setActive(0);
          break;
        case "End":
          setActive(last);
          break;
        case "Escape":
          setActive(null);
          break;
        default:
          return;
      }
      e.preventDefault();
    },
    [count, last],
  );

  return { active, setActive, rootRef, plotRef, onKeyDown, onBlur: () => setActive(null) };
}

/** The nearest stop to a pointer, from the stops' x positions (0-100) across the element. */
export function nearestStop(xs: readonly number[], clientX: number, rect: DOMRect): number {
  const x = ((clientX - rect.left) / Math.max(rect.width, 1)) * 100;
  let best = 0;
  xs.forEach((p, i) => {
    if (Math.abs(p - x) < Math.abs((xs[best] ?? 0) - x)) best = i;
  });
  return best;
}

/** Plain text of a tooltip, for the live region screen readers hear. */
export function tooltipText(title: string, lines: readonly TooltipLine[]): string {
  return [
    title,
    ...lines.map((l) =>
      [l.series, l.value ?? "No data", l.source ? `Source: ${l.source}` : null].filter(Boolean).join(", "),
    ),
  ].join(". ");
}

/**
 * The visible tooltip (design/charts.md: value first, series and period after, then the source) and a polite live
 * region with the same words. The visible box is hidden from assistive technology, so nothing is read twice.
 */
export function ChartTooltip({
  title,
  lines,
  x,
  y,
  align,
  placement = "above",
}: {
  title: string;
  lines: readonly TooltipLine[];
  /** Position inside the island's positioned container, any CSS length. */
  x: string;
  y: string;
  align: "start" | "center" | "end";
  placement?: "above" | "below";
}): ReactNode {
  return (
    <span
      className={`popover popover--tooltip popover--${placement} popover--align-${align} chart-tooltip`}
      style={{ left: x, top: y }}
      aria-hidden="true"
    >
      {lines.map((l) => (
        <span key={`${l.series ?? ""}-${l.ref ?? "gap"}`} className="chart-tooltip__line" data-ref={l.ref ?? undefined}>
          {lines.length > 1 ? <span className={`chart-key chart-key--line chart-colour--${l.role}`} /> : null}
          <span className="popover__value">{l.value ?? "No data"}</span>
          {l.series ? <span className="chart-tooltip__series">{l.series}</span> : null}
        </span>
      ))}
      <span className="popover__line">{title}</span>
      {[...new Set(lines.map((l) => l.source).filter(Boolean))].map((s) => (
        <span key={s} className="popover__meta">
          {s}
        </span>
      ))}
    </span>
  );
}

/** The live region beside the plot: always in the page, filled while a tooltip shows. */
export function ChartLiveRegion({ text }: { text: string }) {
  return (
    <span className="visually-hidden" aria-live="polite">
      {text}
    </span>
  );
}

/** Tooltip alignment near the plot's edges, so it never runs off the chart. */
export const alignFor = (x: number): "start" | "center" | "end" => (x < 20 ? "start" : x > 80 ? "end" : "center");

/**
 * The class that makes the subject's label bold in its colour: " is-highlight" (crimson) or " is-highlight-neutral"
 * when the subject fell; "" for every other role, whose labels stay in ink.
 */
export const subjectClass = (role: string): string =>
  role === "highlight" ? " is-highlight" : role === "highlight-neutral" ? " is-highlight-neutral" : "";
