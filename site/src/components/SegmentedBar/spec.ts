import { z } from "zod/v4";
import { MISSING_TEXT } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { modelMarkdown, modelToChartTable } from "../ChartFigure/model";
import { defineNewsComponent } from "../contract";
import { valueRef } from "../Waterfall/spec";
import { SegmentedBar, segmentedBarData } from "./SegmentedBar";

export default defineNewsComponent({
  kind: "component",
  name: "SegmentedBar",
  index: "SegmentedBar — one whole in 2-6 parts — {label, dp_, highlight?}[]",
  description:
    "One tall bar split into 2–6 parts of a whole, with a key giving each part's value; one part may be the highlight.",
  dataShape:
    "parts: 2–6 objects {label, value, highlight?}: label the part's name; value a dp_… or dv_… in one shared unit (others are left out); highlight true on the one part the title is about.",
  example: `mix = SegmentedBar("iPhone brought in half of Apple's sales in {pd_fy2024}", "Net sales by category, US$ bn", [{label: "iPhone", value: "dp_aapl_sales_iphone_fy2024", highlight: true}, {label: "Services", value: "dp_aapl_sales_services_fy2024"}, {label: "Mac", value: "dp_aapl_sales_mac_fy2024"}])`,
  props: z.object({
    title: z.string().describe("The takeaway, a sentence with no full stop"),
    subtitle: z.string().describe("What is plotted, the period and the unit"),
    parts: z
      .array(
        z.object({
          label: z.string().min(1).describe("The part's name"),
          value: valueRef,
          highlight: z.boolean().nullable().optional().describe("True for the one part the title is about"),
        }),
      )
      .min(2)
      .max(6)
      .refine((parts) => parts.filter((p) => p.highlight).length <= 1, "At most one part may be highlighted"),
  }),
  component: SegmentedBar,
  markdown: ({ props, data }) => {
    const { model } = segmentedBarData(props, data);
    return model ? modelMarkdown(model) : `### ${escapeMarkdown(data.plainText(props.title))}\n\n${MISSING_TEXT}.`;
  },
  csv: ({ props, data }) => {
    const { model } = segmentedBarData(props, data);
    return model ? modelToChartTable(model) : null;
  },
});
