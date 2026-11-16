import { z } from "zod/v4";
import { refPattern } from "../../renderer/bundle/schema";
import { MISSING_TEXT } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { modelMarkdown, modelToChartTable } from "../ChartFigure/model";
import { defineNewsComponent } from "../contract";
import { lineChartData } from "./geometry";
import { LineChart } from "./LineChart";

const seriesRef = z.string().regex(refPattern("sr"));

export default defineNewsComponent({
  kind: "component",
  name: "LineChart",
  index: "LineChart — change over time, 1-4 lines — sr_[], sr_ highlight",
  description:
    "Line chart for change over time: up to four series on one axis, labelled at their ends; the highlight series is crimson, one comparison blue.",
  dataShape:
    "series: 1–4 sr_… series sharing one unit (others are left out); highlight: the sr_… the title is about; comparison: one sr_… to compare against; fromZero: false lets the axis start near the data. Title and subtitle may hold {ref} tokens; the subtitle names the unit.",
  example: `margins = LineChart("Services earns about twice the gross margin of products", "Gross margin as a share of net sales, fiscal years, %", ["sr_aapl_gm_services", "sr_aapl_gm_products"], "sr_aapl_gm_services", "sr_aapl_gm_products")`,
  props: z.object({
    title: z.string().describe("The takeaway, a sentence with no full stop"),
    subtitle: z.string().describe("What is plotted, the period and the unit"),
    series: z.array(seriesRef).min(1).max(4).describe("sr_… references, 1–4"),
    highlight: seriesRef.nullable().optional().describe("sr_… the title is about (crimson)"),
    comparison: seriesRef.nullable().optional().describe("one sr_… to compare against (blue)"),
    fromZero: z.boolean().nullable().optional().describe("false: the axis starts near the data"),
  }),
  component: LineChart,
  markdown: ({ props, data }) => {
    const { model } = lineChartData(props.series, props, data);
    return model ? modelMarkdown(model) : `### ${escapeMarkdown(data.plainText(props.title))}\n\n${MISSING_TEXT}.`;
  },
  csv: ({ props, data }) => {
    const { model } = lineChartData(props.series, props, data);
    return model ? modelToChartTable(model) : null;
  },
});
