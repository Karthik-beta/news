// Line chart (design/charts.md): change over time, up to four series on one axis with direct labels at their ends.
// The series the title is about is crimson; one comparison may be blue; the rest are pale context. With no
// highlight, one series is blue and several equal series take the categorical colours in the order given.
// The plot is an island for its crosshair tooltip; the frame, legend, source line and data table are static.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { usePageData } from "../../renderer/data/context";
import { Island } from "../../renderer/islands/Island";
import { ChartFigure, hasValues } from "../ChartFigure/ChartFigure";
import { lineChartData, lineLegend, linePlot } from "./geometry";
import LineChartPlot from "./LineChartPlot.client";

export interface LineChartProps {
  title: string;
  subtitle: string;
  series: string[];
  highlight?: string | null;
  comparison?: string | null;
  /** Start the axis at zero (the default); false starts it near the data when the variation is the point. */
  fromZero?: boolean | null;
}

export const LineChart: ComponentRenderer<LineChartProps> = ({ props, statementId }) => {
  const data = usePageData();
  const chart = lineChartData(props.series, props, data);
  const opts = {
    highlight: props.highlight,
    comparison: props.comparison,
    fromZero: props.fromZero !== false,
    curve: "linear" as const,
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
        <Island
          name="LineChartPlot"
          component={LineChartPlot}
          props={linePlot({ series: chart.series, model: chart.model }, opts, data)}
        />
      ) : null}
    </ChartFigure>
  );
};
