import { z } from "zod/v4";
import { MISSING_TEXT } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { defineNewsComponent } from "../contract";
import { ComparisonTable, comparisonCsv } from "./ComparisonTable";

const valueRef = z
  .string()
  .regex(/^(dp|dv)_[a-z0-9]+(_[a-z0-9]+)*$/)
  .describe("dp_… or dv_… reference");

export default defineNewsComponent({
  kind: "component",
  name: "ComparisonTable",
  index: "ComparisonTable — A vs B, change last — {label, a, b, change?}[]",
  description:
    "A table comparing two columns side by side (guidance against actual, this year against last) with the change last, never coloured.",
  dataShape:
    'columns: the two headings as text (may hold {pd_} tokens); rows: 1–20 objects {label, a, b, change?, highlight?}: a and b are dp_… or dv_…, change a dv_… change; changeStyle: "signed" (default, +7.7%) or "words" (Up 7.7%).',
  example: `regions = ComparisonTable("Greater China was the only region where sales fell", "Net sales by region, US$", ["{pd_fy2023}", "{pd_fy2024}"], [{label: "Americas", a: "dp_aapl_sales_americas_fy2023", b: "dp_aapl_sales_americas_fy2024", change: "dv_aapl_sales_americas_change_fy2024"}, {label: "Greater China", a: "dp_aapl_sales_china_fy2023", b: "dp_aapl_sales_china_fy2024", change: "dv_aapl_sales_china_change_fy2024"}])`,
  props: z.object({
    title: z.string().describe("The takeaway, a sentence with no full stop"),
    subtitle: z.string().describe("What is compared and the unit"),
    columns: z.array(z.string()).length(2).describe("The two compared columns' headings"),
    rows: z
      .array(
        z.object({
          label: z.string().min(1).describe("What the row measures, as text"),
          a: valueRef,
          b: valueRef,
          change: valueRef.nullable().optional(),
          highlight: z.boolean().nullable().optional(),
        }),
      )
      .min(1)
      .max(20),
    changeStyle: z.enum(["signed", "words"]).nullable().optional().describe("signed (default) or words"),
  }),
  component: ComparisonTable,
  markdown: ({ props, data }) => {
    const t = comparisonCsv(props, data);
    const lines = [
      `### ${escapeMarkdown(t.title)}`,
      "",
      ...(t.subtitle ? [escapeMarkdown(t.subtitle), ""] : []),
      `| ${t.columns.map((c) => escapeMarkdown(c.label)).join(" | ")} |`,
      `| --- | ${t.columns
        .slice(1)
        .map((c) => (c.key === "unit" ? "---" : "---:"))
        .join(" | ")} |`,
      ...t.rows.map(
        (r) =>
          `| ${t.columns.map((c) => escapeMarkdown(r[c.key] ?? (c.key === "unit" ? "" : MISSING_TEXT))).join(" | ")} |`,
      ),
    ];
    return lines.join("\n");
  },
  csv: ({ props, data }) => comparisonCsv(props, data),
});
