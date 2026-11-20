// The interactive plot of a line or step-line chart: an island. The server renders it complete: gridlines, lines,
// end markers, direct end labels, event markers and x labels, so it reads with JavaScript off. Once hydrated, a
// vertical hairline crosshair snaps to the nearest period and one tooltip lists every series there; the plot is
// one tab stop and the arrow keys step through the periods (design/charts.md, "Interaction"). Lines are SVG
// stretched to the plot with non-scaling strokes; every piece of text is HTML at full size.
import type { CSSProperties, PointerEvent } from "react";
import {
  alignFor,
  ChartLiveRegion,
  ChartTooltip,
  nearestStop,
  subjectClass,
  type TooltipLine,
  tooltipText,
  useChartStepper,
} from "../ChartFigure/interaction";
import { type AxisTick, ValueAxis, type XLabel, XLabels } from "../ChartFigure/plot";

export interface LineStop {
  key: string;
  /** 0-100 from the left of the marks' area. */
  x: number;
  label: string;
  tooltip: TooltipLine[];
}

export interface LineSeriesGeometry {
  key: string;
  role: string;
  /** Context lines are 1.5px; the rest 2.5px. */
  thin: boolean;
  /** One y (0-100 from the top) per stop, or null where the series has no value: a gap in the line. */
  ys: (number | null)[];
  /** The direct label at the line's end, or null (no value, or more than four series). */
  end: {
    x: number;
    y: number;
    name: string | null;
    /** The latest point's period, which names the value in the list a tight plot shows below itself. */
    period: string;
    value: string;
    labelY: number;
    highlight: boolean;
    /** The latest point's reference: the label is a cited figure. */
    ref: string | null;
  } | null;
}

export interface EventMarker {
  key: string;
  x: number;
  /** "Rate cut, 18 Sep 2024"; null keeps only the line where labels would collide. */
  label: string | null;
  /** 0 or 1: labels close together stagger onto a second row. */
  row: number;
  /** Labelled on phones too (the latest decision only). */
  phone: boolean;
}

export interface LineChartPlotProps {
  summary: string;
  axisWidth: string;
  ticks: AxisTick[];
  stops: LineStop[];
  series: LineSeriesGeometry[];
  xLabels: XLabel[];
  /** "linear" for line charts; "step" holds each value until the next date (rate paths). */
  curve: "linear" | "step";
  events: EventMarker[];
  /** Room for the end labels to the right of the plot, in ch: phones and from 600px up. */
  endRoom: {
    phone: string;
    wide: string;
    /** False when the labels cannot all fit a phone's plot: phones then leave them out. */
    phoneLabels: boolean;
  };
}

/** An SVG path in the plot's 0-1000 box, broken at gaps; steps hold the value until the next stop. */
export function linePath(xs: readonly number[], ys: readonly (number | null)[], curve: "linear" | "step"): string {
  let d = "";
  let prev: { x: number; y: number } | null = null;
  ys.forEach((y, i) => {
    const x = xs[i] ?? 0;
    if (y === null) {
      prev = null;
      return;
    }
    const X = (x * 10).toFixed(1);
    const Y = (y * 10).toFixed(1);
    if (prev === null) d += `M${X},${Y}`;
    else if (curve === "step") d += `H${X}V${Y}`;
    else d += `L${X},${Y}`;
    prev = { x, y };
  });
  return d;
}

export default function LineChartPlot({
  summary,
  axisWidth,
  ticks,
  stops,
  series,
  xLabels,
  curve,
  events,
  endRoom,
}: LineChartPlotProps) {
  const { active, setActive, rootRef, plotRef, onKeyDown, onBlur } = useChartStepper(stops.length);
  const xs = stops.map((s) => s.x);
  const stop = active === null ? null : stops[active];
  const fromPointer = (e: PointerEvent<HTMLDivElement>) =>
    setActive(nearestStop(xs, e.clientX, e.currentTarget.getBoundingClientRect()));
  const activeYs = active === null ? [] : series.map((s) => s.ys[active] ?? null);
  const tipY = Math.min(...activeYs.filter((y): y is number => y !== null), 50);
  const style = {
    "--chart-axis-width": axisWidth,
    "--line-end-room-phone": endRoom.phone,
    "--line-end-room-wide": endRoom.wide,
  } as CSSProperties;
  const rows = Math.max(0, ...events.filter((e) => e.label).map((e) => e.row + 1));

  return (
    <div
      className={`chart-plot line-chart__plot line-chart--events-${rows}${series.some((s) => s.end) ? " line-chart--end-labels" : ""}${endRoom.phoneLabels ? "" : " line-chart--wide-labels-only"}`}
      ref={rootRef}
      style={style}
    >
      <div className="chart-plot__frame">
        <div className="chart-plot__inner">
          <ValueAxis ticks={ticks} />
          <div
            className="chart-plot__area"
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
            {events.map((ev) => (
              <span key={ev.key} className="line-chart__event" style={{ left: `${ev.x}%` }}>
                {ev.label ? (
                  <span
                    className={`line-chart__event-label line-chart__event-label--row-${ev.row}${ev.x > 70 ? " is-end" : ""}${ev.phone ? "" : " is-wide-only"}`}
                    // The share of the plot's width the label has between its line and the plot's edge; a tight
                    // plot wraps the label to it.
                    style={{ "--event-room": Math.max(0, (ev.x > 70 ? ev.x : 100 - ev.x) - 2) } as CSSProperties}
                  >
                    {ev.label}
                  </span>
                ) : null}
              </span>
            ))}
            {stop ? <span className="line-chart__crosshair" style={{ left: `${stop.x}%` }} /> : null}
            <svg className="line-chart__svg" viewBox="0 0 1000 1000" preserveAspectRatio="none" aria-hidden="true">
              {series.map((s) => (
                <path
                  key={s.key}
                  className={`line-chart__line chart-colour--${s.role}${s.thin ? " is-thin" : ""}`}
                  d={linePath(xs, s.ys, curve)}
                />
              ))}
            </svg>
            {series.map((s) => {
              // A lone point (no neighbour on either side) has no line to show it: draw it as a marker.
              const lone = s.ys.flatMap((y, i) =>
                y !== null &&
                (s.ys[i - 1] ?? null) === null &&
                (s.ys[i + 1] ?? null) === null &&
                !(s.end && s.end.x === xs[i])
                  ? [{ x: xs[i] ?? 0, y, i }]
                  : [],
              );
              return lone.map((p) => (
                <span
                  key={`${s.key}-lone-${p.i}`}
                  className={`line-chart__marker chart-colour--${s.role}`}
                  style={{ left: `${p.x}%`, top: `${p.y}%` }}
                />
              ));
            })}
            {series.map((s) =>
              s.end ? (
                <span
                  key={`${s.key}-end`}
                  className={`line-chart__marker chart-colour--${s.role}`}
                  style={{ left: `${s.end.x}%`, top: `${s.end.y}%` }}
                />
              ) : null,
            )}
            {active !== null
              ? series.map((s, i) => {
                  const y = activeYs[i];
                  return y === null || y === undefined ? null : (
                    <span
                      key={`${s.key}-active`}
                      className={`line-chart__marker is-active chart-colour--${s.role}`}
                      style={{ left: `${stop?.x ?? 0}%`, top: `${y}%` }}
                    />
                  );
                })
              : null}
            {series.map((s) =>
              s.end ? (
                <span
                  key={`${s.key}-label`}
                  className={`line-chart__end-label${s.end.highlight ? subjectClass(s.role) : ""}`}
                  style={{ top: `${s.end.labelY}%` }}
                >
                  {s.end.name ? <span className="line-chart__end-name">{s.end.name}</span> : null}
                  <span className="line-chart__end-value text-tabular" data-ref={s.end.ref ?? undefined}>
                    {s.end.value}
                  </span>
                </span>
              ) : null,
            )}
          </div>
          <div className="chart-plot__overlay">
            {stop ? (
              <ChartTooltip
                title={stop.label}
                lines={stop.tooltip}
                x={`${stop.x}%`}
                y={`${tipY}%`}
                align={alignFor(stop.x)}
                placement={tipY < 25 ? "below" : "above"}
              />
            ) : null}
          </div>
        </div>
      </div>
      <XLabels labels={xLabels} pinEnds="position" />
      {series.some((s) => s.end) ? (
        // A tight plot (a phone at 150-200% text) has no room right of the lines: the end labels move here, below
        // the x axis, and the plot takes the whole width. Hidden otherwise; the same text as the labels above.
        <ul className="line-chart__ends" aria-hidden="true">
          {series.map((s) =>
            s.end ? (
              <li
                key={`${s.key}-end-item`}
                className={`line-chart__ends-item${s.end.highlight ? subjectClass(s.role) : ""}`}
                data-ref={s.end.ref ?? undefined}
              >
                {s.end.name ? (
                  <span className="line-chart__end-name">{s.end.name}</span>
                ) : (
                  <span className="line-chart__end-period">{s.end.period}</span>
                )}{" "}
                <span className="line-chart__end-value text-tabular">{s.end.value}</span>
              </li>
            ) : null,
          )}
        </ul>
      ) : null}
      <ChartLiveRegion text={stop ? tooltipText(stop.label, stop.tooltip) : ""} />
    </div>
  );
}
