// Plot parts shared by the SVG-and-HTML chart islands (column, line, step line): the value axis with its gridlines
// and the x labels. Client-safe (islands render them), so the server computes the geometry and passes plain JSON.
import type { CSSProperties } from "react";

export interface AxisTick {
  /** 0-100 from the top of the plot. */
  y: number;
  label: string;
  baseline: boolean;
}

export interface XLabel {
  /** 0-100 from the left of the marks' area. */
  x: number;
  label: string;
  /** Shown on a plot narrower than 40em of its text (a phone, or any screen at large text sizes). */
  phone: boolean;
  /** Shown on a plot of 40em or more (a tablet or wider at 100% text). */
  wide: boolean;
  /** Shown when the plot is tight (narrower than 20em of its text: a phone at 150-200% text size). */
  tight: boolean;
  /** The widest a tight label may wrap to, as a % of the x axis (0 when not a tight label). */
  room: number;
}

/** Gridlines (solid hairlines, horizontal only) with their labels above them on the left; the baseline in ink. */
export function ValueAxis({ ticks }: { ticks: readonly AxisTick[] }) {
  return (
    <div aria-hidden="true">
      {ticks.map((t) => (
        <div
          key={`${t.label}-${t.y}`}
          className={t.baseline ? "chart-plot__gridline is-baseline" : "chart-plot__gridline"}
          style={{ top: `${t.y}%` }}
        >
          <span className="chart-plot__tick">{t.label}</span>
        </div>
      ))}
    </div>
  );
}

/**
 * X labels under the plot, centred on their stop. Pinned labels stay inside the plot instead: lines pin labels that
 * sit on its edges ("position"); narrow columns pin the first and last labels ("index").
 */
export function XLabels({ labels, pinEnds }: { labels: readonly XLabel[]; pinEnds: "position" | "index" | false }) {
  const lastIndex = labels.length - 1;
  return (
    <div className="chart-plot__x" aria-hidden="true">
      {labels.map((l, i) => {
        if (!l.phone && !l.wide && !l.tight) return null;
        const classes = ["chart-plot__x-label"];
        if (!l.phone) classes.push("is-wide-only");
        if (!l.wide) classes.push("is-phone-only");
        // A tight plot keeps only its tight labels, the first and last pinned inside the plot, the rest wrapping.
        if (l.tight) classes.push("is-tight");
        if (l.tight && i === 0 && lastIndex > 0) classes.push("is-tight-first");
        if (l.tight && i === lastIndex && lastIndex > 0) classes.push("is-tight-last");
        const first = pinEnds === "position" ? l.x <= 0.5 : pinEnds === "index" && i === 0;
        const last = pinEnds === "position" ? l.x >= 99.5 : pinEnds === "index" && i === lastIndex;
        if (first && lastIndex > 0) classes.push("is-first");
        if (last && lastIndex > 0) classes.push("is-last");
        const style = { left: `${l.x}%`, ...(l.tight ? { "--x-label-room": `${l.room}%` } : {}) } as CSSProperties;
        return (
          <span key={`${l.label}-${l.x}`} className={classes.join(" ")} style={style}>
            {l.label}
          </span>
        );
      })}
    </div>
  );
}
