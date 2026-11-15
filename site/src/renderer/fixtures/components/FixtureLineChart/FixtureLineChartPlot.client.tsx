// The interactive part of the fixture line chart: an island. It renders complete on the server (gridlines, line,
// markers, labels) and gains its tooltip once hydrated: hover or tap a period, or focus the plot and use the arrow
// keys; Escape closes it. Geometry comes from the server as percentages, so the plot re-lays out at any width
// with text at full size (labels are HTML, the line is a stretched SVG with a non-scaling stroke).
import { type KeyboardEvent, type PointerEvent, useEffect, useRef, useState } from "react";
import { Tooltip } from "../../../../components/Popover/Popover";

export interface PlotPoint {
  label: string;
  /** The formatted value, or null for a gap. */
  display: string | null;
  /** 0-100, from the left of the plot area. */
  x: number;
  /** 0-100, from the top of the plot area; null for a gap. */
  y: number | null;
  /** Hidden on phones to leave room (first and last labels always show). */
  mid: boolean;
}

export interface FixtureLineChartPlotProps {
  /** The accessible name: the data in a sentence (design/content-style.md). */
  summary: string;
  seriesLabel: string;
  points: PlotPoint[];
  ticks: { y: number; label: string; baseline: boolean }[];
}

function linePath(points: PlotPoint[]): string {
  let d = "";
  let pen = false;
  for (const p of points) {
    if (p.y === null) {
      pen = false;
      continue;
    }
    d += `${pen ? "L" : "M"}${(p.x * 10).toFixed(1)},${(p.y * 10).toFixed(1)}`;
    pen = true;
  }
  return d;
}

export default function FixtureLineChartPlot({ summary, seriesLabel, points, ticks }: FixtureLineChartPlotProps) {
  const [active, setActive] = useState<number | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const last = points.length - 1;

  // Marks the island live, so tests can tell server HTML from a hydrated chart.
  useEffect(() => {
    root.current?.setAttribute("data-hydrated", "true");
  }, []);

  // Tapping outside the chart closes the tooltip.
  useEffect(() => {
    if (active === null) return;
    const close = (e: globalThis.PointerEvent) => {
      if (!root.current?.contains(e.target as Node)) setActive(null);
    };
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, [active]);

  const nearest = (e: PointerEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const x = ((e.clientX - rect.left) / Math.max(rect.width, 1)) * 100;
    let best = 0;
    points.forEach((p, i) => {
      if (Math.abs(p.x - x) < Math.abs((points[best] as PlotPoint).x - x)) best = i;
    });
    setActive(best);
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "ArrowRight") setActive((a) => Math.min((a ?? -1) + 1, last));
    else if (e.key === "ArrowLeft") setActive((a) => Math.max((a ?? last + 1) - 1, 0));
    else if (e.key === "Home") setActive(0);
    else if (e.key === "End") setActive(last);
    else if (e.key === "Escape") setActive(null);
    else return;
    e.preventDefault();
  };

  const point = active === null ? null : points[active];
  const align = point && point.x < 15 ? "start" : point && point.x > 85 ? "end" : "center";

  return (
    <div className="fixture-plot" ref={root}>
      <div
        className="fixture-plot__area"
        role="img"
        aria-label={summary}
        // biome-ignore lint/a11y/noNoninteractiveTabindex: the plot is one tab stop stepped with arrow keys (design/charts.md)
        tabIndex={0}
        onKeyDown={onKeyDown}
        onBlur={() => setActive(null)}
        onPointerMove={nearest}
        onPointerDown={nearest}
        onPointerLeave={(e) => {
          if (e.pointerType === "mouse") setActive(null);
        }}
      >
        <svg className="fixture-plot__svg" viewBox="0 0 1000 1000" preserveAspectRatio="none" aria-hidden="true">
          {ticks.map((t) => (
            <line
              key={t.label}
              className={t.baseline ? "fixture-plot__baseline" : "fixture-plot__grid"}
              x1="0"
              x2="1000"
              y1={t.y * 10}
              y2={t.y * 10}
            />
          ))}
          {active !== null && point ? (
            <line className="fixture-plot__crosshair" x1={point.x * 10} x2={point.x * 10} y1="0" y2="1000" />
          ) : null}
          <path className="fixture-plot__line" d={linePath(points)} />
        </svg>
        {ticks.map((t) => (
          <span key={t.label} className="fixture-plot__tick text-tabular" style={{ top: `${t.y}%` }}>
            {t.label}
          </span>
        ))}
        {points.map((p, i) =>
          p.y === null ? null : (
            <span
              key={p.label}
              className={i === active ? "fixture-plot__marker is-active" : "fixture-plot__marker"}
              style={{ left: `${p.x}%`, top: `${p.y}%` }}
            />
          ),
        )}
      </div>
      <div className="fixture-plot__overlay">
        {point ? (
          <Tooltip x={`${point.x}%`} y={`${point.y ?? 50}%`} align={align}>
            <span className="popover__value">{point.display ?? "No data"}</span>
            <span className="popover__line">
              {seriesLabel}, {point.label}
            </span>
          </Tooltip>
        ) : null}
      </div>
      <div className="fixture-plot__x" aria-hidden="true">
        {points.map((p, i) => (
          <span
            key={p.label}
            className={`fixture-plot__x-label text-tabular${p.mid ? " is-mid" : ""}${i === 0 ? " is-first" : ""}${i === last && i > 0 ? " is-last" : ""}`}
            style={{ left: `${p.x}%` }}
          >
            {p.label}
          </span>
        ))}
      </div>
    </div>
  );
}
