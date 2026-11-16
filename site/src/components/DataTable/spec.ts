import { z } from "zod/v4";
import { REF_PATTERN, refPattern } from "../../renderer/bundle/schema";
import { MISSING_TEXT } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { modelMarkdown, modelToChartTable } from "../ChartFigure/model";
import { defineNewsComponent } from "../contract";
import { DataTable, dataTableData } from "./DataTable";

export default defineNewsComponent({
  kind: "component",
  name: "DataTable",
  index: "DataTable — as-reported table — sr_[] 1-8, pd_/sr_ highlight",
  description:
    "A data table with the chart anatomy: one row per period (or per series), the source's own scale and precision, one highlighted row.",
  dataShape:
    'series: 1–8 sr_… series (each a column; rows are their periods), or rows: "series" for one row per series in one shared unit; highlight: the pd_… row (or sr_… row) to highlight; style: "as-reported" (default) or "compact".',
  example: `segments = DataTable("Services margins have stayed about twice those of products", "Gross margin as a share of net sales, fiscal years, %", ["sr_aapl_gm_products", "sr_aapl_gm_services"], "pd_fy2024")`,
  props: z.object({
    title: z.string().describe("The takeaway, a sentence with no full stop"),
    subtitle: z.string().describe("What is shown, the period and the unit"),
    series: z
      .array(z.string().regex(refPattern("sr")))
      .min(1)
      .max(8)
      .describe("sr_… references, 1–8"),
    highlight: z
      .string()
      .regex(REF_PATTERN)
      .refine((r) => r.startsWith("pd_") || r.startsWith("sr_"), "a pd_… or sr_… reference")
      .nullable()
      .optional()
      .describe("pd_… (or sr_…) of the row to highlight"),
    rows: z.enum(["periods", "series"]).nullable().optional().describe("periods (default) or series"),
    style: z.enum(["as-reported", "compact"]).nullable().optional().describe("as-reported (default) or compact"),
  }),
  component: DataTable,
  markdown: ({ props, data }) => {
    const model = dataTableData(props, data);
    return model ? modelMarkdown(model) : `### ${escapeMarkdown(data.plainText(props.title))}\n\n${MISSING_TEXT}.`;
  },
  csv: ({ props, data }) => {
    const model = dataTableData(props, data);
    return model ? modelToChartTable(model) : null;
  },
});
