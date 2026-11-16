import { formatValue, MISSING_TEXT } from "../../renderer/data/resolve";
import { escapeMarkdown } from "../../renderer/markdown";
import { defineNewsComponent } from "../contract";
import { changeWords, KeyFigure } from "./KeyFigure";
import { keyFigureProps } from "./schema";

export default defineNewsComponent({
  kind: "component",
  name: "KeyFigure",
  index: "KeyFigure — one headline number — dp_, dv_ change, pd_, sr_ trend",
  description:
    "One headline number with its label, its change in words and an optional sparkline; alone, or inside KeyFigures.",
  dataShape:
    "value: a dp_… or dv_…; change: a dv_… change (written in words, never coloured); against: the pd_… the change is measured against; trend: an sr_… for a 120×36 sparkline. The label is text.",
  example: `sales = KeyFigure("Total net sales", "dp_aapl_net_sales_fy2024", "dv_aapl_net_sales_growth_fy2024", "pd_fy2023", "sr_aapl_net_sales_5y")`,
  props: keyFigureProps,
  component: KeyFigure,
  markdown: ({ props, data }) => {
    const v = data.value(props.value);
    const change = changeWords(data, props);
    const value = v.ok ? escapeMarkdown(formatValue(v)) : MISSING_TEXT;
    return `- **${escapeMarkdown(data.plainText(props.label))}:** ${value}${change ? `, ${escapeMarkdown(change.charAt(0).toLowerCase() + change.slice(1))}` : ""}`;
  },
});
