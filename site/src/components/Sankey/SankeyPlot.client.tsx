// The interactive plot of a Sankey: an island. The server renders it complete (flows, nodes and every label), so
// it reads with JavaScript off. Flows are SVG stretched to the plot (no outlines); nodes and labels are HTML at full
// size. Once hydrated, each node (and the flows it owns) is a hover and tap target, and the plot is one tab stop
// stepped with the arrow keys through the nodes, column by column (design/charts.md, "Interaction").
import { Fragment, type PointerEvent, type ReactNode } from "react";
import {
  alignFor,
  ChartLiveRegion,
  ChartTooltip,
  subjectClass,
  type TooltipLine,
  tooltipText,
  useChartStepper,
} from "../ChartFigure/interaction";

export interface SankeyPlotNode {
  key: string;
  label: string;
  role: string;
  /** Its column's position across the flows' area, 0-100. */
  x: number;
  edge: "start" | "middle" | "end";
  /** Top and height of the node, and the top of its label, as % of the plot's height. */
  top: number;
  height: number;
  labelTop: number;
  /** The value as readers see it, or null for "Other" (its tooltip lists the lines folded into it). */
  value: string | null;
  ref: string | null;
  result: boolean;
  tooltip: TooltipLine[];
}

export interface SankeyPlotFlow {
  key: string;
  role: string;
  /** The band in the 0-1000 × 0-height box. */
  d: string;
  /** The node whose tooltip a pointer on the flow shows. */
  owner: number;
}

export interface SankeyPlotProps {
  summary: string;
  /** The plot's height in units (px at a 16px root); set in rem so it grows with the reader's text size. */
  height: number;
  nodes: SankeyPlotNode[];
  flows: SankeyPlotFlow[];
}

/** A line name that may break after a slash ("Other income/(expense), net"), as well as between words. */
function breakable(label: string): ReactNode {
  // Each piece keyed by the text up to it, which is unique within the label.
  const pieces = label.split("/").map((text, i, all) => ({
    text,
    key: all.slice(0, i + 1).join("/"),
    slash: i < all.length - 1,
  }));
  return pieces.map((p) => (
    <Fragment key={p.key}>
      {p.text}
      {p.slash ? (
        <>
          /<wbr />
        </>
      ) : null}
    </Fragment>
  ));
}

export default function SankeyPlot({ summary, height, nodes, flows }: SankeyPlotProps) {
  const { active, setActive, rootRef, plotRef, onKeyDown, onBlur } = useChartStepper(nodes.length);
  const node = active === null ? null : nodes[active];

  const fromPointer = (e: PointerEvent<HTMLDivElement>) => {
    const target = (e.target as Element).closest<HTMLElement | SVGElement>("[data-node]");
    if (target) setActive(Number(target.dataset.node));
  };

  return (
    <div className="sankey__plot" ref={rootRef} style={{ height: `${height / 16}rem` }}>
      <div
        className="sankey__area"
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
          className="sankey__svg"
          viewBox={`0 0 1000 ${height}`}
          preserveAspectRatio="none"
          aria-hidden="true"
          focusable="false"
        >
          {flows.map((f) => (
            <path
              key={f.key}
              className={`sankey__flow chart-colour--${f.role}${f.owner === active ? " is-active" : ""}`}
              d={f.d}
              data-node={f.owner}
            />
          ))}
        </svg>
        {nodes.map((n, i) => (
          <span
            key={n.key}
            className={`sankey__node sankey__node--${n.edge} chart-colour--${n.role}`}
            style={{ left: `${n.x}%`, top: `${n.top}%`, height: `${n.height}%` }}
            data-node={i}
            data-ref={n.ref ?? undefined}
          />
        ))}
        {nodes.map((n, i) => (
          <span
            key={`${n.key}-label`}
            className={`sankey__label sankey__label--${n.edge}${n.result ? " is-result" : ""}${i === active ? " is-active" : ""}`}
            style={{ left: `${n.x}%`, top: `${n.labelTop}%` }}
            data-node={i}
          >
            <span className="sankey__name">{breakable(n.label)}</span>
            {n.value !== null ? (
              <span className={`sankey__value text-tabular${subjectClass(n.role)}`} data-ref={n.ref ?? undefined}>
                {n.value}
              </span>
            ) : null}
          </span>
        ))}
        {node ? (
          <ChartTooltip
            title={node.label}
            lines={node.tooltip}
            x={`${node.x}%`}
            y={`${node.top}%`}
            align={alignFor(node.x)}
          />
        ) : null}
      </div>
      <ChartLiveRegion text={node ? tooltipText(node.label, node.tooltip) : ""} />
    </div>
  );
}
