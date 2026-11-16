// Data table as a figure (design/charts.md, "Tables as charts"): the chart anatomy (rule and tag, title, subtitle,
// source line, CSV and JSON) around a real table. "As reported" (the default) keeps the source's scale and
// precision, with the unit under each heading written by code; the highlighted row is bold with a crimson bar.
// Wide tables scroll sideways inside their own focusable region with the first column fixed. Static: 0 KB JS.
import type { ComponentRenderer } from "@openuidev/react-lang";
import type { ReactNode } from "react";
import { usePageData } from "../../renderer/data/context";
import type { PageDataResolver } from "../../renderer/data/resolve";
import { downloadsFor } from "../../renderer/exports";
import { useFigureId } from "../ChartFigure/ChartFigure";
import { resolveSeries, sameAxis, seriesTable, type TableModel, transposedSeriesTable } from "../ChartFigure/model";
import { FigureFrame } from "../FigureFrame/FigureFrame";
import { RefText } from "../SourceRef/RefText";
import { TableView } from "./TableView";

export interface DataTableProps {
  title: string;
  subtitle: string;
  series: string[];
  /** The row to highlight: a pd_… period (rows are periods) or an sr_… series (rows are series). */
  highlight?: string | null;
  /** "periods" (the default): one row per period, one column per series. "series": one row per series. */
  rows?: "periods" | "series" | null;
  style?: "as-reported" | "compact" | null;
}

/** The table model, or null when no series resolved. Rows of series must share one unit (never converted). */
export function dataTableData(props: DataTableProps, data: PageDataResolver): TableModel | null {
  const resolved = resolveSeries(data, props.series);
  const first = resolved[0];
  if (!first) return null;
  const opts = {
    title: data.plainText(props.title),
    subtitle: data.plainText(props.subtitle),
    highlight: props.highlight ?? null,
  };
  if (props.rows === "series") {
    return transposedSeriesTable(
      resolved.filter((s) => sameAxis([first, s])),
      opts,
    );
  }
  return seriesTable(resolved, opts);
}

/**
 * A table that may be wider than the screen: it scrolls inside its own region, which takes focus so keyboard users
 * can scroll it too (WCAG 2.1.1), and is named by the figure's title.
 */
export function TableScroll({ labelledBy, children }: { labelledBy: string; children: ReactNode }) {
  return (
    // biome-ignore lint/a11y/noNoninteractiveTabindex: a scrollable region must be reachable by keyboard (axe: scrollable-region-focusable)
    <section className="data-table__scroll" aria-labelledby={labelledBy} tabIndex={0}>
      {children}
    </section>
  );
}

export const DataTable: ComponentRenderer<DataTableProps> = ({ props, statementId }) => {
  const data = usePageData();
  const id = useFigureId(statementId);
  const model = dataTableData(props, data);
  const titleId = `${id}-title`;
  return (
    <FigureFrame
      id={id}
      title={<RefText text={props.title} cite="plain" />}
      subtitle={props.subtitle ? <RefText text={props.subtitle} cite="plain" /> : undefined}
      sources={model?.sources ?? []}
      downloads={model ? downloadsFor(statementId, model.sources) : undefined}
    >
      {model && model.rows.length > 0 ? (
        <TableScroll labelledBy={titleId}>
          <TableView model={model} style={props.style ?? "as-reported"} labelledBy={titleId} />
        </TableScroll>
      ) : (
        <p className="chart-figure__missing text-label">No data</p>
      )}
    </FigureFrame>
  );
};
