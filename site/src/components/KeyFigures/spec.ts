import { z } from "zod/v4";
import { escapeMarkdown } from "../../renderer/markdown";
import { sourcesOf } from "../ChartFigure/model";
import { defineNewsComponent } from "../contract";
import { figureSources, type KeyFigureProps } from "../KeyFigure/KeyFigure";
import { keyFigureProps } from "../KeyFigure/schema";
import { KeyFigures } from "./KeyFigures";

const figuresOf = (children: unknown[]) =>
  children.filter(
    (c): c is { props: KeyFigureProps } =>
      typeof c === "object" && c !== null && (c as { typeName?: string }).typeName === "KeyFigure",
  );

export default defineNewsComponent({
  kind: "component",
  name: "KeyFigures",
  index: "KeyFigures — row of 2-4 headline numbers — KeyFigure[]",
  description: "A row of two to four key figures with one source line under the row.",
  dataShape: "children: 2–4 KeyFigure elements, in reading order.",
  example: `figures = KeyFigures([sales, services])
sales = KeyFigure("Total net sales", "dp_aapl_net_sales_fy2024", "dv_aapl_net_sales_growth_fy2024", "pd_fy2023")
services = KeyFigure("Services net sales", "dp_aapl_sales_services_fy2024", "dv_aapl_services_change_fy2024", "pd_fy2023", "sr_aapl_services_5y")`,
  props: z.object({
    children: z.array(keyFigureProps).min(2).max(4).describe("2–4 KeyFigure elements"),
  }),
  component: KeyFigures,
  markdown: ({ props, data, children }) => {
    const figures = figuresOf(props.children);
    const lines = figures.map((f) => children(f)).filter(Boolean);
    const sources = sourcesOf(...figures.map((f) => figureSources(data, f.props)));
    const source = sources.length
      ? `\n\n${sources.length > 1 ? "Sources" : "Source"}: ${sources.map((s) => escapeMarkdown(`${s.source.publisher}, ${s.source.short}`)).join("; ")}.`
      : "";
    return `${lines.join("\n")}${source}`;
  },
});
