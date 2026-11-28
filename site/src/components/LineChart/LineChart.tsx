// Line chart (design/charts.md): change over time, up to four series on one axis with direct labels at their ends.
// The series the title is about is crimson; one comparison may be blue; the rest are pale context. With no
// highlight, one series is blue and several equal series take the categorical colours in the order given.
// Up to three notes (design/charts.md, "Annotations"): each a 1px ink-3 line at its period, labelled with its
// number, and a numbered list under the plot with the period and the writer's words, so notes read on a phone, in
// print and to a screen reader. The plot is an island for its crosshair tooltip; the frame, legend, notes, source
// line and data table are static.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { usePageData } from "../../renderer/data/context";
import { Island } from "../../renderer/islands/Island";
import { ChartFigure, hasValues } from "../ChartFigure/ChartFigure";
import { periodSlots } from "../ChartFigure/model";
import { type ChartNote, lineChartData, lineLegend, linePlot, type NumberedNote, numberedNotes } from "./geometry";
import LineChartPlot from "./LineChartPlot.client";

export interface LineChartProps {
  title: string;
  subtitle: string;
  series: string[];
  highlight?: string | null;
  comparison?: string | null;
  /** Start the axis at zero (the default); false starts it near the data when the variation is the point. */
  fromZero?: boolean | null;
  /** Up to three notes at plotted periods. */
  notes?: ChartNote[] | null;
}

export function ChartNotes({ notes }: { notes: readonly NumberedNote[] }) {
  if (notes.length === 0) return null;
  return (
    <ol className="line-chart__notes" aria-label="Notes on the chart">
      {notes.map((n) => (
        <li key={n.number} className="line-chart__note" data-ref={n.at}>
          <span className="line-chart__note-number" aria-hidden="true">
            {n.number}
          </span>
          <span>
            <span className="line-chart__note-period">{n.period}:</span> {n.text}
          </span>
        </li>
      ))}
    </ol>
  );
}

export const LineChart: ComponentRenderer<LineChartProps> = ({ props, statementId }) => {
  const data = usePageData();
  const chart = lineChartData(props.series, props, data);
  const opts = {
    highlight: props.highlight,
    comparison: props.comparison,
    fromZero: props.fromZero !== false,
    curve: "linear" as const,
    notes: props.notes ?? [],
  };
  return (
    <ChartFigure
      statementId={statementId}
      title={props.title}
      subtitle={props.subtitle}
      legend={chart.series.length > 1 ? lineLegend(chart.series, opts) : []}
      model={chart.model}
    >
      {hasValues(chart.model) ? (
        <>
          <Island
            name="LineChartPlot"
            component={LineChartPlot}
            props={linePlot({ series: chart.series, model: chart.model }, opts, data)}
          />
          <ChartNotes notes={numberedNotes(opts.notes, periodSlots(chart.series))} />
        </>
      ) : null}
    </ChartFigure>
  );
};
