import { z } from "zod/v4";
import { MISSING_TEXT } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { modelMarkdown, modelToChartTable } from "../ChartFigure/model";
import { defineNewsComponent } from "../contract";
import { mismatchNote, Waterfall, waterfallData } from "./Waterfall";

export const valueRef = z
  .string()
  .regex(/^(dp|dv)_[a-z0-9]+(_[a-z0-9]+)*$/)
  .describe("dp_… or dv_… reference");

/** One line of an income statement, shared with the Sankey. */
export const statementRow = z.object({
  label: z.string().min(1).describe("The line's name as the company writes it"),
  value: valueRef,
  kind: z
    .enum(["total", "deduct", "add"])
    .describe("total: from zero; deduct: subtracts the value as published; add: adds the value as published"),
});

export const statementRows = z
  .array(statementRow)
  .min(2)
  .max(16)
  .refine((rows) => rows[0]?.kind === "total", "The first row must be a total (net sales, revenue)");

export default defineNewsComponent({
  kind: "component",
  name: "Waterfall",
  index: "Waterfall — revenue to profit — {label, dp_, kind}[] rows",
  description:
    "Horizontal waterfall from revenue down to profit in the company's own line names; totals blue, costs grey, the result highlighted.",
  dataShape:
    'rows: 2–16 objects {label, value, kind}: label the line as the company names it; value a dp_… or dv_… in one shared unit (others are left out); kind "total" (from zero; the first row is one, the last total is the result), "deduct" (a cost as published) or "add" (other income as published, signed).',
  example: `profit = Waterfall("Apple kept about a quarter of its sales as profit in {pd_fy2024}", "Statement of operations, US$ bn", [{label: "Net sales", value: "dp_aapl_net_sales_fy2024", kind: "total"}, {label: "Cost of sales", value: "dp_aapl_cost_of_sales_fy2024", kind: "deduct"}, {label: "Gross margin", value: "dp_aapl_gross_margin_fy2024", kind: "total"}, {label: "Research and development", value: "dp_aapl_rnd_fy2024", kind: "deduct"}, {label: "Selling, general and administrative", value: "dp_aapl_sga_fy2024", kind: "deduct"}, {label: "Operating income", value: "dp_aapl_operating_income_fy2024", kind: "total"}])`,
  props: z.object({
    title: z.string().describe("The takeaway, a sentence with no full stop"),
    subtitle: z.string().describe("What is plotted, the period and the unit"),
    rows: statementRows,
  }),
  component: Waterfall,
  markdown: ({ props, data }) => {
    const chart = waterfallData(props, data);
    if (!chart.model) return `### ${escapeMarkdown(data.plainText(props.title))}\n\n${MISSING_TEXT}.`;
    const note = mismatchNote(chart);
    return note ? `${modelMarkdown(chart.model)}\n\n${escapeMarkdown(note)}` : modelMarkdown(chart.model);
  },
  csv: ({ props, data }) => {
    const { model } = waterfallData(props, data);
    return model ? modelToChartTable(model) : null;
  },
});
