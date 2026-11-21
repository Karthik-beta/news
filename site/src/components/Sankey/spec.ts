import { z } from "zod/v4";
import { MISSING_TEXT } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { modelMarkdown, modelToChartTable } from "../ChartFigure/model";
import { defineNewsComponent } from "../contract";
import { statementRows, valueRef } from "../Waterfall/spec";
import { mismatchNote } from "../Waterfall/Waterfall";
import { Sankey, sankeyData } from "./Sankey";

export default defineNewsComponent({
  kind: "component",
  name: "Sankey",
  index: "Sankey — revenue flows to profit — rows as Waterfall, segments?",
  description:
    "Sankey from revenue segments to total revenue, costs and profit (Tier 1 earnings); shows the waterfall of the same rows on phones.",
  dataShape:
    'rows: as Waterfall, 2–16 {label, value, kind} with kind "total", "deduct" or "add", the first a total; segments: optional 1–12 {label, value} revenue lines that add up to the first total. Values dp_… or dv_… in one shared unit.',
  example: `flows = Sankey("About a quarter of Apple's sales became net income in {pd_fy2024}", "Statement of operations, US$ bn", [{label: "Total net sales", value: "dp_aapl_net_sales_fy2024", kind: "total"}, {label: "Total cost of sales", value: "dp_aapl_cost_of_sales_fy2024", kind: "deduct"}, {label: "Gross margin", value: "dp_aapl_gross_margin_fy2024", kind: "total"}], [{label: "Products", value: "dp_aapl_sales_products_fy2024"}, {label: "Services", value: "dp_aapl_sales_services_fy2024"}])`,
  props: z.object({
    title: z.string().describe("The takeaway, a sentence with no full stop"),
    subtitle: z.string().describe("What is plotted, the period and the unit"),
    rows: statementRows,
    segments: z
      .array(
        z.object({
          label: z.string().min(1).describe("The revenue line as the company names it"),
          value: valueRef,
        }),
      )
      .max(12)
      .nullable()
      .optional()
      .describe("Revenue segments that add up to the first row"),
  }),
  component: Sankey,
  markdown: ({ props, data }) => {
    const chart = sankeyData(props, data);
    if (!chart.model) return `### ${escapeMarkdown(data.plainText(props.title))}\n\n${MISSING_TEXT}.`;
    const note = mismatchNote(chart.waterfall);
    return note ? `${modelMarkdown(chart.model)}\n\n${escapeMarkdown(note)}` : modelMarkdown(chart.model);
  },
  csv: ({ props, data }) => {
    const { model } = sankeyData(props, data);
    return model ? modelToChartTable(model) : null;
  },
});
