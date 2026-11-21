// Sankey (design/charts.md, "Sankey", Tier 1 earnings): revenue segments flow into total revenue, which splits into
// costs and the profit carried down to the result. Revenue flows in context, costs in neutral, the profit path in
// the highlight. Built from the same rows as the waterfall, which it falls back to where it cannot be read: when its
// plot is narrower than its columns need (every phone), the waterfall shows instead, by CSS alone. When the rows
// cannot be drawn as flows (a loss, a missing value, lines that do not add up) the waterfall shows at every width.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { usePageData } from "../../renderer/data/context";
import type { PageDataResolver } from "../../renderer/data/resolve";
import { Island } from "../../renderer/islands/Island";
import { ChartFigure } from "../ChartFigure/ChartFigure";
import { displayValue, type Measure, type TableModel, tooltipSource } from "../ChartFigure/model";
import {
  linesModel,
  linesSummary,
  mismatchNote,
  type ResolvedLine,
  resolveLines,
  type WaterfallData,
  WaterfallIsland,
  type WaterfallRowInput,
  waterfallFromLines,
} from "../Waterfall/Waterfall";
import { columnX, flowPath, type SankeyLineInput, type SankeyNode, type SankeyResult, sankeyLayout } from "./layout";
import SankeyPlot, { type SankeyPlotProps } from "./SankeyPlot.client";

export interface SankeySegmentInput {
  label: string;
  value: string;
}

export interface SankeyProps {
  title: string;
  subtitle: string;
  rows: WaterfallRowInput[];
  segments?: SankeySegmentInput[] | null;
}

export interface SankeyData {
  measure: Measure | null;
  segments: ResolvedLine[];
  rows: ResolvedLine[];
  /** The fallback: the rows as a waterfall. */
  waterfall: WaterfallData;
  layout: SankeyResult | null;
  /** Segments and rows, the result highlighted: the data table, CSV and JSON. */
  model: TableModel | null;
}

export function sankeyData(props: SankeyProps, data: PageDataResolver): SankeyData {
  const text = { title: data.plainText(props.title), subtitle: data.plainText(props.subtitle) };
  const inputs = [
    ...(props.segments ?? []).map((s) => ({ ...s, kind: "segment" as const })),
    ...props.rows.map((r) => ({ label: r.label, value: r.value, kind: r.kind })),
  ];
  const { lines, measure } = resolveLines(inputs, data);
  const segments = lines.filter((l) => l.kind === "segment");
  const rows = lines.filter((l) => l.kind !== "segment");
  const waterfall = waterfallFromLines(rows, measure, text);
  if (!measure) return { measure, segments, rows, waterfall, layout: null, model: null };
  const all = [...segments, ...rows];
  const model = linesModel(all, {
    ...text,
    measure,
    result: waterfall.result < 0 ? -1 : segments.length + waterfall.result,
    fell: waterfall.fell,
  });
  // Every input must have resolved for the flows to add up: a line left out or unknown means the waterfall only.
  const complete = lines.length === inputs.length;
  const layout = complete
    ? sankeyLayout(
        all.map(
          (l): SankeyLineInput => ({
            label: l.label,
            ref: l.ref,
            kind: l.kind,
            value: l.value?.value ?? null,
            shown: l.value ? displayValue(l.value.value, measure) : "",
          }),
        ),
      )
    : ({ ok: false, reason: "missing" } as const);
  return { measure, segments, rows, waterfall, layout, model };
}

/** The island's props: percentages of the plot for nodes and labels, SVG paths for the flows. */
export function sankeyPlot(chart: SankeyData, data: PageDataResolver): SankeyPlotProps | null {
  if (!chart.layout?.ok || !chart.model) return null;
  const { columns, height, nodes: laid, links } = chart.layout.layout;
  const pct = (v: number) => (v / height) * 100;
  // Stops in reading order: column by column, top to bottom.
  const order = laid
    .map((_, i) => i)
    .sort((a, b) => {
      const na = laid[a] as SankeyNode;
      const nb = laid[b] as SankeyNode;
      return na.column - nb.column || na.y - nb.y;
    });
  const position = new Map(order.map((old, i) => [old, i]));
  const nodes = order.map((i) => laid[i] as SankeyNode);
  return {
    summary: linesSummary(chart.model),
    height,
    nodes: nodes.map((n) => {
      const single = n.folded ? null : (n.items[0] ?? null);
      return {
        key: n.key,
        label: n.label,
        role: n.role,
        x: columnX(n.column, columns) / 10,
        edge: n.column === 0 ? "start" : n.column === columns - 1 ? "end" : "middle",
        top: pct(n.y),
        height: pct(n.height),
        labelTop: pct(n.labelTop),
        value: single ? single.shown : null,
        ref: single ? single.ref : null,
        result: n.result,
        tooltip: n.items.map((it) => ({
          value: it.shown,
          series: n.folded ? it.label : null,
          role: n.role,
          ref: it.ref,
          source: tooltipSource(data, it.ref),
        })),
      };
    }),
    flows: links.map((l) => {
      const s = laid[l.source];
      const t = laid[l.target];
      return {
        key: l.key,
        role: l.role,
        d: flowPath(l, columnX(s?.column ?? 0, columns), columnX(t?.column ?? 0, columns)),
        owner: position.get(l.owner) ?? 0,
      };
    }),
  };
}

export const Sankey: ComponentRenderer<SankeyProps> = ({ props, statementId }) => {
  const data = usePageData();
  const chart = sankeyData(props, data);
  const plot = sankeyPlot(chart, data);
  const plotted = chart.rows.some((l) => l.value !== null);
  const note = mismatchNote(chart.waterfall);
  const columns = chart.layout?.ok ? chart.layout.layout.columns : 0;
  return (
    <ChartFigure
      statementId={statementId}
      title={props.title}
      subtitle={props.subtitle}
      model={chart.model}
      sourceNote={note ? <span className="waterfall__note">{note}</span> : undefined}
    >
      {plotted ? (
        <div className={plot ? `sankey sankey--c${Math.min(columns, 8)}` : "sankey sankey--fallback-only"}>
          {plot ? (
            <div className="sankey__flows">
              <Island name="SankeyPlot" component={SankeyPlot} props={plot} />
            </div>
          ) : null}
          <div className="sankey__fallback">
            <WaterfallIsland chart={chart.waterfall} data={data} />
          </div>
        </div>
      ) : null}
    </ChartFigure>
  );
};
