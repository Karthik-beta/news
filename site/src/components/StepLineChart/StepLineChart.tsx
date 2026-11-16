// Step-line chart (design/charts.md, "Line charts and step lines"): a policy rate path. Rates change on dates, so
// each value holds flat until the next decision (steps, not slopes), positioned by date. Decisions named in
// `markers` get an event marker: a 1px line with a short label at the top ("Rate cut, 18 Sep 2024"), worked out
// by code from the series. Shares the line chart's island; the frame and data table are static.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { usePageData } from "../../renderer/data/context";
import { Island } from "../../renderer/islands/Island";
import { ChartFigure, hasValues } from "../ChartFigure/ChartFigure";
import { lineChartData, lineLegend, linePlot } from "../LineChart/geometry";
import LineChartPlot from "../LineChart/LineChartPlot.client";

export interface StepLineChartProps {
  title: string;
  subtitle: string;
  series: string[];
  markers?: string[] | null;
  highlight?: string | null;
}

export const StepLineChart: ComponentRenderer<StepLineChartProps> = ({ props, statementId }) => {
  const data = usePageData();
  const chart = lineChartData(props.series, props, data);
  const opts = { highlight: props.highlight, curve: "step" as const, markers: props.markers ?? [], fromZero: true };
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
