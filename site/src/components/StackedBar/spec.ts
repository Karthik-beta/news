import { z } from "zod/v4";
import { MISSING_TEXT } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { modelMarkdown, modelToChartTable } from "../ChartFigure/model";
import { defineNewsComponent } from "../contract";
import { valueRef } from "../Waterfall/spec";
import { StackedBar, stackedBarData } from "./StackedBar";

export default defineNewsComponent({
  kind: "component",
  name: "StackedBar",
  index: "StackedBar — parts of a whole by row — parts[], {label, dp_[]}[]",
  description:
    "Horizontal stacked bars: each row split into the same 2–8 parts, coloured by part; one part may be the highlight.",
  dataShape:
    "parts: 2–8 part names, in order. rows: 1–12 {label, values, total?}: label text (may hold {pd_…}); values one dp_… or dv_… per part, in one shared unit (others are left out); total an optional dp_… written past the bar. highlight: a part name.",
  example: `regions = StackedBar("The Americas made up the largest share of sales in both years", "Net sales by region, US$ bn", ["Americas", "Europe", "Greater China"], [{label: "{pd_fy2023}", values: ["dp_aapl_sales_americas_fy2023", "dp_aapl_sales_europe_fy2023", "dp_aapl_sales_china_fy2023"]}, {label: "{pd_fy2024}", values: ["dp_aapl_sales_americas_fy2024", "dp_aapl_sales_europe_fy2024", "dp_aapl_sales_china_fy2024"]}])`,
  props: z.object({
    title: z.string().describe("The takeaway, a sentence with no full stop"),
    subtitle: z.string().describe("What is plotted, the period and the unit"),
    parts: z.array(z.string().min(1)).min(2).max(8).describe("The parts' names, in stacking order"),
    rows: z
      .array(
        z.object({
          label: z.string().min(1).describe("The row's name; may hold {pd_…} tokens"),
          values: z.array(valueRef).min(1).max(8).describe("One value per part, in the order of parts"),
          total: valueRef.nullable().optional().describe("The row's total as published"),
        }),
      )
      .min(1)
      .max(12),
    highlight: z.string().nullable().optional().describe("The part the title is about"),
  }),
  component: StackedBar,
  markdown: ({ props, data }) => {
    const { model } = stackedBarData(props, data);
    return model ? modelMarkdown(model) : `### ${escapeMarkdown(data.plainText(props.title))}\n\n${MISSING_TEXT}.`;
  },
  csv: ({ props, data }) => {
    const { model } = stackedBarData(props, data);
    return model ? modelToChartTable(model) : null;
  },
});
