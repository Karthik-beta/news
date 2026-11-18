import { useEffect, useRef, useState } from "react";

// The one interactive part of the fixture page: a small SVG line chart. It renders complete on the server
// (line, points, axis labels, a text summary) and gains the hover / keyboard tooltip once hydrated.
// Spike-quality: geometry is minimal, the real ChartFigure lands with the component library (Phase 3).

export interface LineChartProps {
  title: string;
  subtitle: string;
  labels: string[];
  values: number[];
  source: string;
}

const W = 600;
const H = 240;
const PAD = { top: 16, right: 16, bottom: 32, left: 16 };

export function LineChart({ title, subtitle, labels, values, source }: LineChartProps) {
  const [active, setActive] = useState<number | null>(null);
  const figure = useRef<HTMLElement>(null);

  // Marks the island as live, so tests (and the curious) can tell server HTML from a hydrated chart.
  useEffect(() => {
    figure.current?.setAttribute("data-hydrated", "true");
  }, []);

  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = max - min || 1;
  const x = (i: number) => PAD.left + (i * (W - PAD.left - PAD.right)) / Math.max(values.length - 1, 1);
  const y = (v: number) => PAD.top + (1 - (v - min) / span) * (H - PAD.top - PAD.bottom);
  const path = values.map((v, i) => `${i === 0 ? "M" : "L"}${x(i).toFixed(1)},${y(v).toFixed(1)}`).join(" ");
  const last = values.length - 1;
  const summary = `${title}, ${labels[0]} to ${labels[last]}: from ${values[0]} to ${values[last]}.`;

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowRight") setActive((a) => Math.min((a ?? -1) + 1, last));
    else if (e.key === "ArrowLeft") setActive((a) => Math.max((a ?? last + 1) - 1, 0));
    else if (e.key === "Escape") setActive(null);
    else return;
    e.preventDefault();
  };

  return (
    <figure className="oui-chart" ref={figure}>
      <figcaption>
        <span className="oui-chart-title">{title}</span>
        <span className="oui-chart-subtitle">{subtitle}</span>
      </figcaption>
      <div
        className="oui-chart-plot"
        role="img"
        aria-label={summary}
        // biome-ignore lint/a11y/noNoninteractiveTabindex: the plot is one tab stop with arrow keys (design/charts.md)
        tabIndex={0}
        onKeyDown={onKeyDown}
        onBlur={() => setActive(null)}
        onMouseLeave={() => setActive(null)}
      >
        <svg viewBox={`0 0 ${W} ${H}`} aria-hidden="true" focusable="false">
          <line
            className="oui-chart-baseline"
            x1={PAD.left}
            x2={W - PAD.right}
            y1={H - PAD.bottom}
            y2={H - PAD.bottom}
          />
          <path className="oui-chart-line" d={path} />
          {values.map((v, i) => (
            // biome-ignore lint/a11y/noStaticElementInteractions: hover only mirrors the plot's keyboard navigation
            <circle
              key={labels[i]}
              className={i === active ? "oui-chart-point is-active" : "oui-chart-point"}
              cx={x(i)}
              cy={y(v)}
              r={i === active ? 6 : 4}
              onMouseEnter={() => setActive(i)}
            />
          ))}
          {labels.map((label, i) => (
            <text
              key={label}
              className="oui-chart-label"
              x={x(i)}
              y={H - 8}
              textAnchor={i === 0 ? "start" : i === last ? "end" : "middle"}
            >
              {label}
            </text>
          ))}
        </svg>
        {active !== null && (
          <p className="oui-chart-tooltip" role="status">
            <strong>{values[active]}</strong> {labels[active]}
          </p>
        )}
      </div>
      <p className="oui-chart-source">Source: {source}</p>
    </figure>
  );
}
