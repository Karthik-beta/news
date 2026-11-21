import { z } from "zod/v4";
import { MISSING_TEXT } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { modelMarkdown, modelToChartTable } from "../ChartFigure/model";
import { defineNewsComponent } from "../contract";
import { valueRef } from "../Waterfall/spec";
import { Slope, slopeData } from "./Slope";

export default defineNewsComponent({
  kind: "component",
  name: "Slope",
  index: "Slope — two points in time — periods[2], {label, from, to}[]",
  description:
    "Slope chart: each item's line from a first value to a second (before and after, guidance and actual); one item may be the highlight.",
  dataShape:
    "periods: the two points' names, text with {pd_…} tokens. items: 2–12 {label, from, to, highlight?}: from and to dp_… or dv_… in one shared unit (others are left out); highlight true on the one item the title is about.",
  example: `regions = Slope("Greater China was the only region where sales fell", "Net sales by region, US$ bn", ["{pd_fy2023}", "{pd_fy2024}"], [{label: "Greater China", from: "dp_aapl_sales_china_fy2023", to: "dp_aapl_sales_china_fy2024", highlight: true}, {label: "Japan", from: "dp_aapl_sales_japan_fy2023", to: "dp_aapl_sales_japan_fy2024"}])`,
  props: z.object({
    title: z.string().describe("The takeaway, a sentence with no full stop"),
    subtitle: z.string().describe("What is plotted, the period and the unit"),
    periods: z.array(z.string().min(1)).length(2).describe("The two points' names, with {pd_…} tokens"),
    items: z
      .array(
        z.object({
          label: z.string().min(1).describe("The item, as text"),
          from: valueRef,
          to: valueRef,
          highlight: z.boolean().nullable().optional().describe("True for the one item the title is about"),
        }),
      )
      .min(2)
      .max(12)
      .refine((items) => items.filter((i) => i.highlight).length <= 1, "At most one item may be highlighted"),
  }),
  component: Slope,
  markdown: ({ props, data }) => {
    const { model } = slopeData(props, data);
    return model ? modelMarkdown(model) : `### ${escapeMarkdown(data.plainText(props.title))}\n\n${MISSING_TEXT}.`;
  },
  csv: ({ props, data }) => {
    const { model } = slopeData(props, data);
    return model ? modelToChartTable(model) : null;
  },
});
