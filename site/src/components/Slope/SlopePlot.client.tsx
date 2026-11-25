// The interactive plot of a slope chart: an island. Lines are SVG stretched to the plot with non-scaling strokes;
// dots and every label are HTML at full size: values on the left, values and names on the right. The server
// renders it complete, so it reads with JavaScript off. Once hydrated, a pointer picks the nearest line, each item
// is a tap target, and the arrow keys step through the items from one tab stop (design/charts.md, "Interaction").
import type { CSSProperties, PointerEvent } from "react";
import {
  ChartLiveRegion,
  ChartTooltip,
  subjectClass,
  type TooltipLine,
  tooltipText,
  useChartStepper,
} from "../ChartFigure/interaction";

export interface SlopeEnd {
  /** The dot's position, % of the plot's height from the top; null for a missing value. */
  y: number | null;
  /** The label's top, % of the plot's height. */
  labelTop: number;
  value: string;
  ref: string | null;
}

export interface SlopeItem {
  key: string;
  label: string;
  role: string;
  thin: boolean;
  highlight: boolean;
  from: SlopeEnd;
  to: SlopeEnd;
  /** The right-hand label's top in a tight plot, where it is the value alone: % of the plot's height. */
  tightTop: number;
  tooltip: TooltipLine[];
}

export interface SlopePlotProps {
  summary: string;
  /** The plot's height in units (px at a 16px root), set in rem. */
  height: number;
  /** The two periods' names, written above the two ends. */
  periods: [string, string];
  /** Items in drawing order: the highlight last, so it is drawn on top. */
  items: SlopeItem[];
  /** The widest right-hand value, in characters: a tight plot's right gutter. */
  endRoom: number;
}

export default function SlopePlot({ summary, height, periods, items, endRoom }: SlopePlotProps) {
  const { active, setActive, rootRef, plotRef, onKeyDown, onBlur } = useChartStepper(items.length);
  const item = active === null ? null : items[active];

  // The nearest line to the pointer, by its height where the pointer is across the plot.
  const fromPointer = (e: PointerEvent<HTMLDivElement>) => {
    const target = (e.target as HTMLElement).closest<HTMLElement>("[data-item]");
    if (target) {
      setActive(Number(target.dataset.item));
      return;
    }
    const rect = e.currentTarget.getBoundingClientRect();
    const fx = Math.min(Math.max((e.clientX - rect.left) / Math.max(rect.width, 1), 0), 1);
    const fy = ((e.clientY - rect.top) / Math.max(rect.height, 1)) * 100;
    let best: number | null = null;
    let bestD = Number.POSITIVE_INFINITY;
    items.forEach((it, i) => {
      const a = it.from.y ?? it.to.y;
      const b = it.to.y ?? it.from.y;
      if (a === null || b === null) return;
      const d = Math.abs(a + (b - a) * fx - fy);
      if (d < bestD) {
        bestD = d;
        best = i;
      }
    });
    setActive(best);
  };

  const tipY = item ? Math.min(item.from.y ?? 100, item.to.y ?? 100) : 0;
  // A tight plot's right gutter: the widest right-hand value and a character to spare.
  const style = { "--slope-end-room": `${endRoom + 1}ch` } as CSSProperties;
  // The items a tight plot lists below itself, in the order of the right-hand values, top to bottom.
  const named = [...items].sort((a, b) => a.tightTop - b.tightTop);

  return (
    <div className="slope__plot" ref={rootRef} style={style}>
      <div className="slope__periods" aria-hidden="true">
        <span className="slope__period">{periods[0]}</span>
        <span className="slope__period slope__period--end">{periods[1]}</span>
      </div>
      <div className="slope__frame" style={{ height: `${height / 16}rem` }}>
        <div
          className="slope__area"
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
          <svg
            className="slope__svg"
            viewBox="0 0 100 100"
            preserveAspectRatio="none"
            aria-hidden="true"
            focusable="false"
          >
            {items.map((it, i) =>
              it.from.y !== null && it.to.y !== null ? (
                <line
                  key={it.key}
                  className={`slope__line chart-colour--${it.role}${it.thin ? " is-thin" : ""}${i === active ? " is-active" : ""}`}
                  x1="0"
                  y1={it.from.y}
                  x2="100"
                  y2={it.to.y}
                  vectorEffect="non-scaling-stroke"
                />
              ) : null,
            )}
          </svg>
          {items.map((it, i) => (
            <span key={`${it.key}-marks`} data-item={i}>
              {it.from.y !== null ? (
                <span
                  className={`slope__dot chart-colour--${it.role}`}
                  style={{ left: "0%", top: `${it.from.y}%` }}
                  data-ref={it.from.ref ?? undefined}
                />
              ) : null}
              {it.to.y !== null ? (
                <span
                  className={`slope__dot chart-colour--${it.role}`}
                  style={{ left: "100%", top: `${it.to.y}%` }}
                  data-ref={it.to.ref ?? undefined}
                />
              ) : null}
              <span
                className={`slope__label slope__label--start text-tabular${subjectClass(it.role)}${it.from.y === null ? " is-missing" : ""}`}
                style={{ top: `${it.from.labelTop}%` }}
                data-ref={it.from.y === null ? undefined : (it.from.ref ?? undefined)}
              >
                {it.from.value}
              </span>
              <span
                className={`slope__label slope__label--end${it.highlight ? " is-highlight-item" : ""}`}
                style={
                  { "--slope-label-top": `${it.to.labelTop}%`, "--slope-tight-top": `${it.tightTop}%` } as CSSProperties
                }
              >
                <span
                  className={`slope__value text-tabular${subjectClass(it.role)}${it.to.y === null ? " is-missing" : ""}`}
                  data-ref={it.to.y === null ? undefined : (it.to.ref ?? undefined)}
                >
                  {it.to.value}
                </span>{" "}
                <span className="slope__name">{it.label}</span>
              </span>
            </span>
          ))}
          {item ? <ChartTooltip title={item.label} lines={item.tooltip} x="50%" y={`${tipY}%`} align="center" /> : null}
        </div>
      </div>
      {/* A tight plot (a phone at 150-200% text) has room for the right-hand values only: the names and the
          left-hand values move here, below the plot, each item with both its values. Hidden otherwise; the same
          text as the labels. */}
      <ul className="slope__names" aria-hidden="true">
        {named.map((it) => (
          <li key={`${it.key}-name`} className={`slope__names-item${it.highlight ? " is-highlight-item" : ""}`}>
            <span className="slope__names-name">{it.label}</span>{" "}
            <span
              className={`slope__names-value text-tabular${it.from.y === null ? " is-missing" : ""}`}
              data-ref={it.from.y === null ? undefined : (it.from.ref ?? undefined)}
            >
              {it.from.value}
            </span>{" "}
            to{" "}
            <span
              className={`slope__names-value text-tabular${subjectClass(it.role)}${it.to.y === null ? " is-missing" : ""}`}
              data-ref={it.to.y === null ? undefined : (it.to.ref ?? undefined)}
            >
              {it.to.value}
            </span>
          </li>
        ))}
      </ul>
      <ChartLiveRegion text={item ? tooltipText(item.label, item.tooltip) : ""} />
    </div>
  );
}
