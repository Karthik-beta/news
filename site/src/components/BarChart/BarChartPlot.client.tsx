// The interactive plot of a horizontal bar chart: an island, built in HTML and CSS so labels stay full size on
// phones (design/charts.md, "Responsive behaviour"). The server renders every bar, label and value; once
// hydrated, each row is a hover and tap target and the arrow keys step through the bars from one tab stop.
import { type CSSProperties, type PointerEvent, useEffect, useRef } from "react";
import {
  alignFor,
  ChartLiveRegion,
  ChartTooltip,
  type TooltipLine,
  tooltipText,
  useChartStepper,
} from "../ChartFigure/interaction";

export interface BarRow {
  key: string;
  label: string;
  role: string;
  /** Left edge and width of the bar, 0-100 of the track; null for a missing value. */
  start: number | null;
  width: number | null;
  negative: boolean;
  /** The value as readers see it ("$67.0bn"), or "No data". */
  value: string;
  highlight: boolean;
  ref: string | null;
  tooltip: TooltipLine[];
}

export interface BarChartPlotProps {
  summary: string;
  rows: BarRow[];
  /** Position of zero along the track, 0-100. */
  zero: number;
  /** Room for value labels left of the negative bars and right of the positive ones, in ch. */
  room: { negative: string; positive: string };
}

export default function BarChartPlot({ summary, rows, zero, room }: BarChartPlotProps) {
  const { active, setActive, rootRef, plotRef, onKeyDown, onBlur } = useChartStepper(rows.length);
  const rowRefs = useRef<(HTMLDivElement | null)[]>([]);
  const row = active === null ? null : rows[active];

  // Keep the active row in view while stepping with the keys through a long list.
  useEffect(() => {
    if (active !== null) rowRefs.current[active]?.scrollIntoView?.({ block: "nearest" });
  }, [active]);

  const fromPointer = (e: PointerEvent<HTMLDivElement>) => {
    const target = (e.target as HTMLElement).closest<HTMLElement>("[data-row]");
    if (target) setActive(Number(target.dataset.row));
  };

  const style = {
    "--bar-zero": `${zero}%`,
    "--bar-room-negative": room.negative,
    "--bar-room-positive": room.positive,
  } as CSSProperties;

  return (
    <div className="bar-chart__plot" ref={rootRef} style={style}>
      <div
        className="bar-chart__rows"
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
            ref={(el) => {
              rowRefs.current[i] = el;
            }}
            className={`bar-chart__row${i === active ? " is-active" : ""}${r.highlight ? " is-highlight" : ""}`}
            data-row={i}
          >
            <span className="bar-chart__label">{r.label}</span>
            <span className="bar-chart__track">
              {r.start === null || r.width === null ? (
                <>
                  <span className="bar-chart__bar bar-chart__bar--missing chart-hatch" data-ref={r.ref ?? undefined} />
                  <span className="bar-chart__value bar-chart__value--missing">{r.value}</span>
                </>
              ) : (
                <>
                  <span
                    className={`bar-chart__bar chart-colour--${r.role}`}
                    style={{ left: `${r.start}%`, width: `${r.width}%` }}
                    data-ref={r.ref ?? undefined}
                  />
                  <span
                    className={`bar-chart__value text-tabular${r.negative ? " is-negative" : ""}${r.highlight ? " is-highlight" : ""}`}
                    style={r.negative ? { right: `${100 - r.start}%` } : { left: `${r.start + r.width}%` }}
                    data-ref={r.ref ?? undefined}
                  >
                    {r.value}
                  </span>
                </>
              )}
              {i === active && row ? (
                <ChartTooltip
                  title={row.label}
                  lines={row.tooltip}
                  x={r.start === null || r.width === null ? `${zero}%` : `${r.negative ? r.start : r.start + r.width}%`}
                  y="0%"
                  align={alignFor(
                    r.start === null || r.width === null ? zero : r.negative ? r.start : r.start + r.width,
                  )}
                />
              ) : null}
            </span>
          </div>
        ))}
      </div>
      <ChartLiveRegion text={row ? tooltipText(row.label, row.tooltip) : ""} />
    </div>
  );
}
