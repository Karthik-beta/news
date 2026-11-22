// The interactive plot of a heatmap: an island in HTML and CSS. The server renders every row name, cell, value and
// period label, so it reads with JavaScript off; once hydrated each cell is a hover and tap target, and the grid is
// one tab stop: left and right step through the cells, up and down move a row, Home and End jump to the ends
// (design/charts.md, "Interaction").
import type { CSSProperties, KeyboardEvent, PointerEvent } from "react";
import {
  alignFor,
  ChartLiveRegion,
  ChartTooltip,
  type TooltipLine,
  tooltipText,
  useChartStepper,
} from "../ChartFigure/interaction";
import { type XLabel, XLabels } from "../ChartFigure/plot";

export interface HeatmapCell {
  key: string;
  /** The ramp step, 1 (least) to 5 (most); null for no data (a hatched cell). */
  step: number | null;
  /** The bare value written in the cell ("73.9"), or null. */
  label: string | null;
  ref: string | null;
  tooltip: TooltipLine;
}

export interface HeatmapPlotProps {
  summary: string;
  /** The row names' column, in ch. */
  nameWidth: string;
  columns: number;
  rows: { key: string; label: string; cells: HeatmapCell[] }[];
  periods: string[];
  xLabels: XLabel[];
  /** The plot width (em) from which every value fits in its cell; null when they never do. */
  fit: number | null;
}

export default function HeatmapPlot({ summary, nameWidth, columns, rows, periods, xLabels, fit }: HeatmapPlotProps) {
  const count = rows.length * columns;
  const { active, setActive, rootRef, plotRef, onKeyDown, onBlur } = useChartStepper(count);
  const at = (i: number | null) => {
    if (i === null) return null;
    const row = rows[Math.floor(i / columns)];
    const cell = row?.cells[i % columns];
    return row && cell ? { row, cell, column: i % columns } : null;
  };
  const current = at(active);
  const title = current ? `${current.row.label}, ${periods[current.column] ?? ""}` : "";

  const onKey = (e: KeyboardEvent) => {
    if ((e.key === "ArrowUp" || e.key === "ArrowDown") && active !== null) {
      const next = active + (e.key === "ArrowDown" ? columns : -columns);
      if (next >= 0 && next < count) setActive(next);
      e.preventDefault();
      return;
    }
    onKeyDown(e);
  };
  const fromPointer = (e: PointerEvent<HTMLDivElement>) => {
    const target = (e.target as HTMLElement).closest<HTMLElement>("[data-cell]");
    if (target) setActive(Number(target.dataset.cell));
  };

  return (
    <div
      className={`chart-plot heatmap__plot${fit === null ? "" : ` heatmap--fit-${fit}`}`}
      ref={rootRef}
      style={{ "--chart-axis-width": nameWidth, "--heatmap-columns": columns } as CSSProperties}
    >
      <div
        className="heatmap__grid"
        role="img"
        aria-label={summary}
        ref={plotRef}
        onKeyDown={onKey}
        onBlur={onBlur}
        onPointerMove={(e) => {
          if (e.pointerType === "mouse") fromPointer(e);
        }}
        onPointerDown={fromPointer}
        onPointerLeave={(e) => {
          if (e.pointerType === "mouse") setActive(null);
        }}
      >
        {rows.map((r, ri) => (
          <div key={r.key} className="heatmap__row">
            <span className="heatmap__name">{r.label}</span>
            <span className="heatmap__cells">
              {r.cells.map((c, ci) => {
                const i = ri * columns + ci;
                return (
                  <span
                    key={c.key}
                    className={`heatmap__cell ${c.step === null ? "chart-hatch" : `heatmap__step--${c.step}`}${i === active ? " is-active" : ""}`}
                    data-cell={i}
                    data-ref={c.ref ?? undefined}
                  >
                    {c.label === null ? null : <span className="heatmap__value text-tabular">{c.label}</span>}
                    {i === active && current ? (
                      <ChartTooltip
                        title={title}
                        lines={[current.cell.tooltip]}
                        x="50%"
                        y="0%"
                        align={alignFor(((ci + 0.5) / columns) * 100)}
                      />
                    ) : null}
                  </span>
                );
              })}
            </span>
          </div>
        ))}
      </div>
      {/* As in the column chart: many columns pin the first and last labels inside the plot. */}
      <XLabels labels={xLabels} pinEnds={columns > 8 ? "index" : false} />
      <ChartLiveRegion text={current ? tooltipText(title, [current.cell.tooltip]) : ""} />
    </div>
  );
}
