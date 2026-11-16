import { z } from "zod/v4";
import { refPattern } from "../../renderer/bundle/schema";
import { MISSING_TEXT } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { modelMarkdown, modelToChartTable } from "../ChartFigure/model";
import { defineNewsComponent } from "../contract";
import { lineChartData } from "../LineChart/geometry";
import { StepLineChart } from "./StepLineChart";

const seriesRef = z.string().regex(refPattern("sr"));

export default defineNewsComponent({
  kind: "component",
  name: "StepLineChart",
  index: "StepLineChart — rate path, steps — sr_[] 1-2, pd_[] markers",
  description:
    "Step-line chart for a policy rate path: each rate holds until the next decision date; named decisions get a labelled event marker.",
  dataShape:
    'series: 1–2 sr_… rate series whose periods carry decision dates; markers: pd_… decisions to mark (labels like "Rate cut" are worked out from the data); highlight: the sr_… the title is about.',
  example: `path = StepLineChart("The Fed has cut three times since {pd_fomc_20240918}", "Federal funds target range, upper limit, %", ["sr_fed_funds_upper"], ["pd_fomc_20220316", "pd_fomc_20240918"])`,
  props: z.object({
    title: z.string().describe("The takeaway, a sentence with no full stop"),
    subtitle: z.string().describe("What is plotted, the period and the unit"),
    series: z.array(seriesRef).min(1).max(2).describe("sr_… rate series, 1–2"),
    markers: z
      .array(z.string().regex(refPattern("pd")))
      .max(12)
      .nullable()
      .optional()
      .describe("pd_… decisions to mark"),
    highlight: seriesRef.nullable().optional().describe("sr_… the title is about (crimson)"),
  }),
  component: StepLineChart,
  markdown: ({ props, data }) => {
    const { model } = lineChartData(props.series, props, data);
    return model ? modelMarkdown(model) : `### ${escapeMarkdown(data.plainText(props.title))}\n\n${MISSING_TEXT}.`;
  },
  csv: ({ props, data }) => {
    const { model } = lineChartData(props.series, props, data);
    return model ? modelToChartTable(model) : null;
  },
});
