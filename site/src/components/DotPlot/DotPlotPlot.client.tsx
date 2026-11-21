// The interactive plot of a dot plot or dumbbell: an island in HTML and CSS, one row per item, its label above the
// track on phones and in a column on the left from 600px. The server renders every dot, connector and value label,
// so it reads with JavaScript off; once hydrated each row is a hover and tap target and the arrow keys step
// through the rows from one tab stop (design/charts.md, "Interaction").
import type { CSSProperties, PointerEvent } from "react";
import {
  alignFor,
  ChartLiveRegion,
  ChartTooltip,
  subjectClass,
  type TooltipLine,
  tooltipText,
  useChartStepper,
} from "../ChartFigure/interaction";

export interface DotMark {
  key: string;
  role: string;
  /** Position along the track, 0-100; null for a missing value. */
  x: number | null;
  /** The value as readers see it, or "No data". */
  value: string;
  /** Where the value's label sits: left of the dot or right of it. */
  side: "left" | "right";
  ref: string | null;
}

export interface DotRow {
  key: string;
  label: string;
  highlight: boolean;
  dots: DotMark[];
  /** The dumbbell's connector between the two dots, 0-100; null for one dot or a missing value. */
  line: { left: number; width: number } | null;
  tooltip: TooltipLine[];
}

export interface DotPlotPlotProps {
  summary: string;
  rows: DotRow[];
  /** Room either side of the track for the value labels, in ch. */
  room: { left: string; right: string };
}

export default function DotPlotPlot({ summary, rows, room }: DotPlotPlotProps) {
  const { active, setActive, rootRef, plotRef, onKeyDown, onBlur } = useChartStepper(rows.length);
  const row = active === null ? null : rows[active];

  const fromPointer = (e: PointerEvent<HTMLDivElement>) => {
    const target = (e.target as HTMLElement).closest<HTMLElement>("[data-row]");
    if (target) setActive(Number(target.dataset.row));
  };
  const style = { "--bar-room-negative": room.left, "--bar-room-positive": room.right } as CSSProperties;
  const tipX = (r: DotRow) => {
    const xs = r.dots.flatMap((d) => (d.x === null ? [] : [d.x]));
    return xs.length ? Math.max(...xs) : 0;
  };

  return (
    <div className="dot-plot__plot" ref={rootRef} style={style}>
      <div
        className="dot-plot__rows"
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
            className={`dot-plot__row${i === active ? " is-active" : ""}${r.highlight ? " is-highlight" : ""}`}
            data-row={i}
          >
            <span className="dot-plot__label">{r.label}</span>
            <span className="dot-plot__track">
              {r.line ? (
                <span className="dot-plot__line" style={{ left: `${r.line.left}%`, width: `${r.line.width}%` }} />
              ) : null}
              {r.dots.map((d) =>
                d.x === null ? (
                  <span
                    key={d.key}
                    className="dot-plot__value dot-plot__value--missing"
                    style={d.side === "left" ? { left: "0%" } : { right: "0%" }}
                  >
                    {d.value}
                  </span>
                ) : (
                  <span key={d.key}>
                    <span
                      className={`dot-plot__dot chart-colour--${d.role}`}
                      style={{ left: `${d.x}%` }}
                      data-ref={d.ref ?? undefined}
                    />
                    <span
                      className={`dot-plot__value dot-plot__value--${d.side} text-tabular${subjectClass(d.role)}`}
                      style={d.side === "left" ? { right: `${100 - d.x}%` } : { left: `${d.x}%` }}
                      data-ref={d.ref ?? undefined}
                    >
                      {d.value}
                    </span>
                  </span>
                ),
              )}
              {i === active && row ? (
                <ChartTooltip
                  title={row.label}
                  lines={row.tooltip}
                  x={`${tipX(row)}%`}
                  y="0%"
                  align={alignFor(tipX(row))}
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
