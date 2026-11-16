// Plot parts shared by the SVG-and-HTML chart islands (column, line, step line): the value axis with its gridlines
// and the x labels. Client-safe (islands render them), so the server computes the geometry and passes plain JSON.

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
  /** Shown on phones (under 600px). */
  phone: boolean;
  /** Shown from 600px up. */
  wide: boolean;
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

/** X labels under the plot. Lines pin labels at the plot's edges inside it; columns centre every label. */
export function XLabels({ labels, pinEnds }: { labels: readonly XLabel[]; pinEnds: boolean }) {
  const lastIndex = labels.length - 1;
  return (
    <div className="chart-plot__x" aria-hidden="true">
      {labels.map((l) => {
        if (!l.phone && !l.wide) return null;
        const classes = ["chart-plot__x-label"];
        if (!l.phone) classes.push("is-wide-only");
        if (!l.wide) classes.push("is-phone-only");
        if (pinEnds && l.x <= 0.5 && lastIndex > 0) classes.push("is-first");
        if (pinEnds && l.x >= 99.5 && lastIndex > 0) classes.push("is-last");
        return (
          <span key={`${l.label}-${l.x}`} className={classes.join(" ")} style={{ left: `${l.x}%` }}>
            {l.label}
          </span>
        );
      })}
    </div>
  );
}
