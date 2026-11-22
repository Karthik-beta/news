// The interactive plot of a column chart: an island. The server renders it complete (gridlines, columns, value
// labels, x labels), so it reads with JavaScript off; once hydrated, each column is a hover and tap target and the
// plot is one tab stop stepped with the arrow keys (design/charts.md, "Interaction"). Geometry arrives as
// percentages, so the plot re-lays out at any width with its text at full size.
import { type CSSProperties, type PointerEvent, useMemo } from "react";
import {
  alignFor,
  ChartLiveRegion,
  ChartTooltip,
  subjectClass,
  type TooltipLine,
  tooltipText,
  useChartStepper,
} from "../ChartFigure/interaction";
import { type AxisTick, ValueAxis, type XLabel, XLabels } from "../ChartFigure/plot";

export interface ColumnMark {
  /** The series the mark belongs to (its key in the band). */
  series: string;
  role: string;
  /** Top of the column and its height, 0-100 of the plot; null for a gap. */
  top: number | null;
  height: number | null;
  negative: boolean;
  /** The bare value label ("391.0"), or null. */
  label: string | null;
  /** "always": the highlighted and latest columns; "room": shown when the chart is sparse enough to fit them. */
  show: "always" | "room" | "never";
  ref: string | null;
}

export interface ColumnBand {
  key: string;
  label: string;
  /** Left edge and width of the band, 0-100 of the marks' area. */
  left: number;
  width: number;
  highlight: boolean;
  marks: ColumnMark[];
  tooltip: TooltipLine[];
}

export interface ColumnChartPlotProps {
  /** The accessible name: the data in a sentence. */
  summary: string;
  axisWidth: string;
  ticks: AxisTick[];
  /** Position of the zero line, 0-100 from the top. */
  zero: number;
  bands: ColumnBand[];
  xLabels: XLabel[];
  /** How many value labels fit: all on phones (≤ 5), from tablets (≤ 8), from desktops (≤ 12), or only the key ones. */
  density: "n5" | "n8" | "n12" | "key";
  /** How x labels at the ends stay inside the plot; by default narrow columns pin the first and last. A histogram's
   * labels are bin edges, so the ones on the plot's edges are pinned by position. */
  pinEnds?: "position" | "index" | false;
}

export default function ColumnChartPlot({
  summary,
  axisWidth,
  ticks,
  zero,
  bands,
  xLabels,
  density,
  pinEnds,
}: ColumnChartPlotProps) {
  const { active, setActive, rootRef, plotRef, onKeyDown, onBlur } = useChartStepper(bands.length);
  const centres = useMemo(() => bands.map((b) => b.left + b.width / 2), [bands]);
  const band = active === null ? null : bands[active];
  // The first and last labels align to the plot's edges when they could overhang it: narrow columns, long labels.
  const edge = (i: number, label: string | null): "start" | "end" | null => {
    if (bands.length < 2 || (bands.length <= 8 && (label?.length ?? 0) <= 5)) return null;
    return i === 0 ? "start" : i === bands.length - 1 ? "end" : null;
  };

  const fromPointer = (e: PointerEvent<HTMLDivElement>) => {
    const target = (e.target as HTMLElement).closest<HTMLElement>("[data-band]");
    if (target) setActive(Number(target.dataset.band));
  };

  const tipTop = band
    ? Math.min(zero, ...band.marks.map((m) => (m.top === null ? zero : m.negative ? zero : m.top)))
    : 0;

  return (
    <div
      className={`chart-plot column-chart__plot column-chart--${density}`}
      ref={rootRef}
      style={{ "--chart-axis-width": axisWidth } as CSSProperties}
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
            {bands.map((b, i) => (
              <div
                key={b.key}
                className={`column-chart__band${i === active ? " is-active" : ""}${b.highlight ? " is-highlight" : ""}`}
                data-band={i}
                style={{ left: `${b.left}%`, width: `${b.width}%` }}
              >
                <div className="column-chart__group">
                  {b.marks.map((m) => (
                    <span key={m.series} className="column-chart__slot">
                      {m.top === null || m.height === null ? (
                        <span
                          className="column-chart__fill chart-hatch"
                          style={{ top: "0%", height: "100%" }}
                          data-ref={m.ref ?? undefined}
                        />
                      ) : (
                        <span
                          className={`column-chart__fill chart-colour--${m.role}`}
                          style={{ top: `${m.top}%`, height: `${m.height}%` }}
                          data-ref={m.ref ?? undefined}
                        />
                      )}
                      {m.label !== null && m.show !== "never" && m.top !== null && m.height !== null ? (
                        <span
                          className={`column-chart__value text-tabular column-chart__value--${m.show}${m.negative ? " is-negative" : ""}${subjectClass(m.role)}${edge(i, m.label) === "start" ? " is-first" : edge(i, m.label) === "end" ? " is-last" : ""}`}
                          style={m.negative ? { top: `${m.top + m.height}%` } : { bottom: `${100 - m.top}%` }}
                          data-ref={m.ref ?? undefined}
                        >
                          {m.label}
                        </span>
                      ) : null}
                    </span>
                  ))}
                </div>
              </div>
            ))}
          </div>
          <div className="chart-plot__overlay">
            {band ? (
              <ChartTooltip
                title={band.label}
                lines={band.tooltip}
                x={`${centres[active ?? 0]}%`}
                y={`${tipTop}%`}
                align={alignFor(centres[active ?? 0] ?? 50)}
              />
            ) : null}
          </div>
        </div>
      </div>
      <XLabels labels={xLabels} pinEnds={pinEnds ?? (bands.length > 8 ? "index" : false)} />
      <ChartLiveRegion text={band ? tooltipText(band.label, band.tooltip) : ""} />
    </div>
  );
}
