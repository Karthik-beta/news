// The wrapper every chart sits in (design/charts.md, "Anatomy"): the FigureFrame (brand rule and tag, takeaway
// title, subtitle, legend, plot, source line, "Show the data table", CSV and JSON) filled from one table model,
// so the visible table, the downloads and the source line all come from the same data. Static: no JavaScript;
// only the plot inside may be an island. Missing data renders as the frame with "No data", never an empty plot.
import { type ReactNode, useId } from "react";
import type { CitedSource } from "../../renderer/data/resolve";
import { downloadsFor } from "../../renderer/exports";
import { TableView } from "../DataTable/TableView";
import { FigureFrame, sourceAnchor } from "../FigureFrame/FigureFrame";
import { RefText } from "../SourceRef/RefText";
import type { ColourRole, TableModel, TableStyle } from "./model";

export interface LegendItem {
  key: string;
  label: string;
  role: ColourRole;
  /** A 10×10 square for bars and columns, a 14×2.5 line for lines (design/charts.md, "Anatomy"), an 8px dot for dots. */
  shape: "square" | "line" | "dot";
}

/** The legend: only for two or more series, above the plot, keys in the series colour and text in ink-2. */
export function ChartLegend({ items }: { items: readonly LegendItem[] }) {
  if (items.length < 2) return null;
  return (
    <ul className="chart-legend text-label">
      {items.map((item) => (
        <li key={item.key} className="chart-legend__item">
          <span className={`chart-key chart-key--${item.shape} chart-colour--${item.role}`} aria-hidden="true" />
          {item.label}
        </li>
      ))}
    </ul>
  );
}

/**
 * "Source: Apple Inc., Form 10-K, fiscal 2024": the sources behind a block, each linked to its entry in the page's
 * sources list. The same words FigureFrame writes under a chart, for blocks without a frame (key figures).
 */
export function SourceLine({ sources, className }: { sources: readonly CitedSource[]; className?: string }) {
  if (sources.length === 0) return null;
  return (
    <p className={`source-line text-source${className ? ` ${className}` : ""}`}>
      {sources.length > 1 ? "Sources: " : "Source: "}
      {sources.map(({ id, source }, i) => (
        <span key={id}>
          {i > 0 ? "; " : ""}
          <a href={`#${sourceAnchor(id)}`}>
            {source.publisher}, {source.short}
          </a>
        </span>
      ))}
    </p>
  );
}

export interface ChartFigureProps {
  /** The statement name in page.oui: the figure's id and the downloads' file names. */
  statementId?: string;
  /** The takeaway, with {ref} tokens. */
  title: string;
  /** What, when and the unit, with {ref} tokens. */
  subtitle: string;
  legend?: readonly LegendItem[];
  /** The data behind the chart; null when nothing resolved (the frame says "No data"). */
  model: TableModel | null;
  /** How the data table writes values. Charts use the source's digits, with the unit in each heading. */
  tableStyle?: TableStyle;
  /** The plot; ignored when the model has no data. */
  children?: ReactNode;
  /** A line before the sources ("Calculated by News from …"). */
  sourceNote?: ReactNode;
}

/** True when a model has at least one value to plot. */
export const hasValues = (model: TableModel | null): model is TableModel =>
  model?.rows.some((r) => r.cells.some((c) => c.value !== null)) ?? false;

/** Ids are the statement name when there is one; otherwise React's own, made safe for fragment links. */
export function useFigureId(statementId: string | undefined): string {
  const fallback = `chart-${useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;
  return statementId ?? fallback;
}

export function ChartFigure({
  statementId,
  title,
  subtitle,
  legend = [],
  model,
  tableStyle = "as-reported",
  children,
  sourceNote,
}: ChartFigureProps) {
  const id = useFigureId(statementId);
  const titleNode = <RefText text={title} cite="plain" />;
  const subtitleNode = subtitle ? <RefText text={subtitle} cite="plain" /> : undefined;
  const plotted = hasValues(model);
  return (
    <FigureFrame
      id={id}
      title={titleNode}
      subtitle={subtitleNode}
      legend={plotted && legend.length >= 2 ? <ChartLegend items={legend} /> : undefined}
      sources={model?.sources ?? []}
      sourceNote={sourceNote}
      table={
        model && model.rows.length > 0 ? (
          <TableView model={model} style={tableStyle} labelledBy={`${id}-title`} />
        ) : undefined
      }
      downloads={model ? downloadsFor(statementId, model.sources) : undefined}
    >
      {plotted ? children : <p className="chart-figure__missing text-label">No data</p>}
    </FigureFrame>
  );
}
