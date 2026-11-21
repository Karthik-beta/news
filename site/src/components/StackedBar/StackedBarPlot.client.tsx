// The interactive plot of a stacked or segmented bar: an island, in HTML and CSS so labels stay full size on phones.
// The server renders every row and segment, so it reads with JavaScript off. Once hydrated, the plot is one tab stop:
// a stacked bar steps through its rows (one tooltip lists every part), a segmented bar through its parts.
import type { CSSProperties, PointerEvent } from "react";
import {
  alignFor,
  ChartLiveRegion,
  ChartTooltip,
  type TooltipLine,
  tooltipText,
  useChartStepper,
} from "../ChartFigure/interaction";

export interface StackSegment {
  key: string;
  role: string;
  /** Left edge and width along the track, 0-100. */
  left: number;
  width: number;
  ref: string | null;
}

export interface StackRow {
  key: string;
  label: string;
  segments: StackSegment[];
  /** The row's total, written just past the bar's end, when the page gives one. */
  total: { left: number; value: string; ref: string } | null;
  /** True when nothing in the row could be drawn. */
  empty: boolean;
}

export interface StackStop {
  row: number;
  /** The segment, for a segmented bar; null for a whole row. */
  segment: number | null;
  /** Where the tooltip points, 0-100 along the track. */
  x: number;
  title: string;
  tooltip: TooltipLine[];
}

export interface StackedBarPlotProps {
  summary: string;
  variant: "stacked" | "segmented";
  rows: StackRow[];
  stops: StackStop[];
  /** Room right of the bars for the total labels, in ch. */
  room: string;
}

export default function StackedBarPlot({ summary, variant, rows, stops, room }: StackedBarPlotProps) {
  const { active, setActive, rootRef, plotRef, onKeyDown, onBlur } = useChartStepper(stops.length);
  const stop = active === null ? null : stops[active];

  const fromPointer = (e: PointerEvent<HTMLDivElement>) => {
    const target = (e.target as HTMLElement).closest<HTMLElement>("[data-stop]");
    if (target) setActive(Number(target.dataset.stop));
  };
  const stopOf = (row: number, segment: number | null) =>
    stops.findIndex((s) => s.row === row && (s.segment === null || s.segment === segment));

  return (
    <div
      className={`stacked-bar__plot stacked-bar--${variant}`}
      ref={rootRef}
      style={{ "--bar-room-positive": room } as CSSProperties}
    >
      <div
        className="stacked-bar__rows"
        role="img"
        aria-label={summary}
        ref={plotRef}
        onKeyDown={onKeyDown}
        onBlur={onBlur}
        onPointerMove={(e) => {
          if (e.pointerType === "mouse") fromPointer(e);
        }}
        onPointerDown={fromPointer}
        onPointerLeave={(e) => {
          if (e.pointerType === "mouse") setActive(null);
        }}
      >
        {rows.map((r, i) => (
          <div
            key={r.key}
            className={`stacked-bar__row${stop?.row === i && stop.segment === null ? " is-active" : ""}`}
            data-stop={variant === "stacked" ? stopOf(i, null) : undefined}
          >
            {variant === "stacked" ? <span className="stacked-bar__label">{r.label}</span> : null}
            <span className="stacked-bar__track">
              {r.empty ? <span className="stacked-bar__segment stacked-bar__segment--missing chart-hatch" /> : null}
              {r.segments.map((s, j) => (
                <span
                  key={s.key}
                  className={`stacked-bar__segment chart-colour--${s.role}${stop?.row === i && stop.segment === j ? " is-active" : ""}`}
                  style={{ left: `${s.left}%`, width: `${s.width}%` }}
                  data-ref={s.ref ?? undefined}
                  data-stop={variant === "segmented" ? stopOf(i, j) : undefined}
                />
              ))}
              {r.total ? (
                <span
                  className="stacked-bar__total text-tabular"
                  style={{ left: `${r.total.left}%` }}
                  data-ref={r.total.ref}
                >
                  {r.total.value}
                </span>
              ) : null}
              {stop && stop.row === i ? (
                <ChartTooltip
                  title={stop.title}
                  lines={stop.tooltip}
                  x={`${stop.x}%`}
                  y="0%"
                  align={alignFor(stop.x)}
                />
              ) : null}
            </span>
          </div>
        ))}
      </div>
      <ChartLiveRegion text={stop ? tooltipText(stop.title, stop.tooltip) : ""} />
    </div>
  );
}
