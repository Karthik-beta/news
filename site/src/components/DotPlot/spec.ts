import { z } from "zod/v4";
import { MISSING_TEXT } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { modelMarkdown, modelToChartTable } from "../ChartFigure/model";
import { defineNewsComponent } from "../contract";
import { valueRef } from "../Waterfall/spec";
import { DotPlot, dotPlotData } from "./DotPlot";

export default defineNewsComponent({
  kind: "component",
  name: "DotPlot",
  index: "DotPlot — dots or before/after dumbbells — {label, dp_[1-2]}[]",
  description:
    "Dot plot of one value per item, or a dumbbell of two (before and after, guidance and actual); one row may be the highlight.",
  dataShape:
    'rows: 1–30 {label, values, highlight?}: values one or two dp_… or dv_… in one shared unit (others are left out). keys: the two values\' names, text with {pd_…} tokens, for a dumbbell. sort: "value" (default, by the last value) or "given".',
  example: `regions = DotPlot("Sales rose in every region but Greater China", "Net sales by region, US$ bn", [{label: "Americas", values: ["dp_aapl_sales_americas_fy2023", "dp_aapl_sales_americas_fy2024"]}, {label: "Europe", values: ["dp_aapl_sales_europe_fy2023", "dp_aapl_sales_europe_fy2024"]}], ["{pd_fy2023}", "{pd_fy2024}"])`,
  props: z.object({
    title: z.string().describe("The takeaway, a sentence with no full stop"),
    subtitle: z.string().describe("What is plotted, the period and the unit"),
    rows: z
      .array(
        z.object({
          label: z.string().min(1).describe("The item, as text"),
          values: z.array(valueRef).min(1).max(2).describe("One value, or two for a dumbbell"),
          highlight: z.boolean().nullable().optional().describe("True for the one row the title is about"),
        }),
      )
      .min(1)
      .max(30)
      .refine((rows) => rows.filter((r) => r.highlight).length <= 1, "At most one row may be highlighted"),
    keys: z.array(z.string().min(1)).length(2).nullable().optional().describe("The two values' names"),
    sort: z.enum(["value", "given"]).nullable().optional().describe("value: largest last value first; given"),
  }),
  component: DotPlot,
  markdown: ({ props, data }) => {
    const { model } = dotPlotData(props, data);
    return model ? modelMarkdown(model) : `### ${escapeMarkdown(data.plainText(props.title))}\n\n${MISSING_TEXT}.`;
  },
  csv: ({ props, data }) => {
    const { model } = dotPlotData(props, data);
    return model ? modelToChartTable(model) : null;
  },
});
