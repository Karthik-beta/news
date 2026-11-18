import { z } from "zod/v4";
import { defineNewsComponent } from "../../../../components/contract";
import { refPattern } from "../../../bundle/schema";
import { formatValue } from "../../../data/resolve";
import { escapeMarkdown } from "../../../markdown";
import { FixtureFigure } from "./FixtureFigure";

const valueRef = z
  .string()
  .regex(/^(dp|dv)_[a-z0-9]+(_[a-z0-9]+)*$/)
  .describe("dp_… or dv_… reference");

export default defineNewsComponent({
  kind: "component",
  name: "FixtureFigure",
  index: "FixtureFigure — fixture only: key figure — dp_/dv_ value, dv_ change",
  description: "Fixture-only key figure for renderer tests: label, cited value, change in words.",
  dataShape: "value: dp_ or dv_; change: a dv_ change; against: the pd_ period it is measured against.",
  example: `sales = FixtureFigure("Net sales", "dp_aapl_net_sales_fy2024", "dv_aapl_net_sales_growth_fy2024", "pd_fy2023")`,
  props: z.object({
    label: z.string().describe("What the figure is"),
    value: valueRef,
    change: valueRef.nullable().optional(),
    against: z.string().regex(refPattern("pd")).nullable().optional().describe("pd_… reference"),
  }),
  component: FixtureFigure,
  markdown: ({ props, data }) => {
    const v = data.value(props.value);
    return `**${escapeMarkdown(props.label)}:** ${v.ok ? escapeMarkdown(formatValue(v)) : "No data"}`;
  },
});
