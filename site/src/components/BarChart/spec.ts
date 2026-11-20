import { z } from "zod/v4";
import { MISSING_TEXT } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { modelMarkdown, modelToChartTable } from "../ChartFigure/model";
import { defineNewsComponent } from "../contract";
import { BarChart, barChartData } from "./BarChart";

const valueRef = z
  .string()
  .regex(/^(dp|dv)_[a-z0-9]+(_[a-z0-9]+)*$/)
  .describe("dp_… or dv_… reference");

export default defineNewsComponent({
  kind: "component",
  name: "BarChart",
  index: "BarChart — ranked categories — {label, dp_, highlight?}[]",
  description:
    "Horizontal bar chart for ranking or comparing categories, sorted largest first; one bar may be the highlight.",
  dataShape:
    'bars: 1–30 objects {label, value, highlight?}: label is the category name as text, value a dp_… or dv_… in one shared unit (others are left out), highlight true on the one bar the title is about. sort: "value" (default) or "given" for a natural order.',
  example: `regions = BarChart("The Americas brought in the most sales in {pd_fy2024}", "Net sales by region, fiscal 2024, US$ bn", [{label: "Americas", value: "dp_aapl_sales_americas_fy2024", highlight: true}, {label: "Europe", value: "dp_aapl_sales_europe_fy2024"}, {label: "Greater China", value: "dp_aapl_sales_china_fy2024"}])`,
  props: z.object({
    title: z.string().describe("The takeaway, a sentence with no full stop"),
    subtitle: z.string().describe("What is plotted, the period and the unit"),
    bars: z
      .array(
        z.object({
          label: z.string().min(1).describe("The category, as text"),
          value: valueRef,
          highlight: z.boolean().nullable().optional().describe("True for the one bar the title is about"),
        }),
      )
      .min(1)
      .max(30)
      .refine((bars) => bars.filter((b) => b.highlight).length <= 1, "At most one bar may be highlighted"),
    sort: z.enum(["value", "given"]).nullable().optional().describe("value: largest first; given: as listed"),
  }),
  component: BarChart,
  markdown: ({ props, data }) => {
    const { model } = barChartData(props, data);
    return model ? modelMarkdown(model) : `### ${escapeMarkdown(data.plainText(props.title))}\n\n${MISSING_TEXT}.`;
  },
  csv: ({ props, data }) => {
    const { model } = barChartData(props, data);
    return model ? modelToChartTable(model) : null;
  },
});
