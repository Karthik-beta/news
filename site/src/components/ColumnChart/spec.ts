import { z } from "zod/v4";
import { refPattern } from "../../renderer/bundle/schema";
import { MISSING_TEXT } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { modelMarkdown, modelToChartTable } from "../ChartFigure/model";
import { defineNewsComponent } from "../contract";
import { ColumnChart, columnChartData } from "./ColumnChart";

export default defineNewsComponent({
  kind: "component",
  name: "ColumnChart",
  index: "ColumnChart — periods as columns — sr_[] 1-3, pd_ highlight",
  description:
    "Column chart for change over discrete periods (years, quarters): one series in blue, or 2–3 grouped; a pd_ highlight makes that column crimson.",
  dataShape:
    "series: 1–3 sr_… series sharing one unit (periods are the columns); highlight: the pd_… period the title is about (one series only). Title and subtitle may hold {ref} tokens; the subtitle names the unit.",
  example: `services = ColumnChart("Services sales have grown every year since {pd_fy2020}", "Services net sales, fiscal years, US$ bn", ["sr_aapl_services_5y"], "pd_fy2024")`,
  props: z.object({
    title: z.string().describe("The takeaway, a sentence with no full stop"),
    subtitle: z.string().describe("What is plotted, the period and the unit"),
    series: z
      .array(z.string().regex(refPattern("sr")))
      .min(1)
      .max(3)
      .describe("sr_… references, 1–3"),
    highlight: z
      .string()
      .regex(refPattern("pd"))
      .nullable()
      .optional()
      .describe("pd_… of the column the title is about"),
  }),
  component: ColumnChart,
  markdown: ({ props, data }) => {
    const { model } = columnChartData(props, data);
    return model ? modelMarkdown(model) : `### ${escapeMarkdown(data.plainText(props.title))}\n\n${MISSING_TEXT}.`;
  },
  csv: ({ props, data }) => {
    const { model } = columnChartData(props, data);
    return model ? modelToChartTable(model) : null;
  },
});
